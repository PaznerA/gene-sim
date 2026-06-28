//! gene-sim AI harness — a gym-like environment over the headless sim core (SPEC §2.2, §8 Stage 3; S3.1).
//!
//! This is the [`Env`] surface a player or an LLM agent drives: [`reset`](Env::reset) /
//! [`step`](Env::step) / [`seed`](Env::seed), shaped after the Gymnasium/PettingZoo **API** (not its
//! training stack). [`GeneSimEnv`] wraps [`sim_core::Simulation`] so all of the determinism guarantees
//! flow straight through.
//!
//! ## Action granularity (invariant #6 — the load-bearing rule of this slice)
//! Actions advance generations ([`Action::Advance`]), edit the **species genome** ([`Action::ApplyEdit`]),
//! or apply a CRISPR edit to a **cell region** ([`Action::ApplyEditRegion`], ADR-011 S-D — the selective
//! brush). There is **no per-organism action** — individual organisms are ECS entities, never RL agents. The
//! type system enforces it: [`Action`] carries no organism handle, an [`EditAction`] targets a
//! [`genome::LocusId`], and a [`RegionSpec`] targets CELLS (centre + radius), never a specific entity. Per the
//! ADR-011 human ruling, the region edit is sub-species but cell-scoped (a minimum radius keeps it from being
//! de-facto per-organism) and is allowed in an AI policy's action space.
//!
//! ## Determinism (invariant #3)
//! One seeded `rand_chacha::ChaCha8Rng` is created once per [`reset`](Env::reset) inside the wrapped
//! [`sim_core::Simulation`] and threaded through every subsequent `step` — generation advances **and**
//! the species edit (which draws via [`sim_core::Simulation::with_genome_and_rng`]). No thread/global RNG
//! is used and no `HashMap` is iterated in sim logic. A fixed `(seed, action-sequence)` reproduces an
//! identical sequence of [`Observation`]s.

#![forbid(unsafe_code)]

use crispr::{
    apply_edit, default_cas_variants, evaluate_region_edit, CasVariant, CasVariantId,
    DefaultOffTargetScore, DefaultOnTargetScore, Edit, EditOutcome, EditThresholds, GuideSequence,
    RegionEditOutcome,
};
use genome::spec::BuiltSpecies;
use genome::LocusId;
use serde::{Deserialize, Serialize};
use sim_core::gp::{trait_map_for, OntologyMap};
use sim_core::{EnvParams, Observation, SimConfig, Simulation};

pub mod campaign;
pub mod capture;
pub mod discover;
pub mod firewall;
pub mod oversight;
pub mod promote;
pub mod replay;
pub mod species;

/// The result of one [`Env::step`]: the new observation plus a scalar reward and an episode-`done` flag
/// (Gymnasium `step` shape, SPEC §2.2).
#[derive(Debug, Clone, PartialEq)]
pub struct StepResult<Obs> {
    /// The observation after the action was applied.
    pub obs: Obs,
    /// Scalar reward for this step (see [`GeneSimEnv`] for the concrete definition).
    pub reward: f64,
    /// Whether the episode has terminated.
    pub done: bool,
}

/// A minimal, gym-shaped environment surface: `reset` / `step` / `seed` (SPEC §2.2).
///
/// Kept generic over the action and observation types so alternate envs can reuse the shape, while the
/// concrete [`GeneSimEnv`] pins them to species-granular [`Action`]s and [`Observation`]s.
pub trait Env {
    /// The (species/operator-granular) action type.
    type Action;
    /// The observation type returned by `reset`/`step`.
    type Obs;

    /// Start a fresh episode from `seed` and return the initial observation.
    fn reset(&mut self, seed: u64) -> Self::Obs;

    /// Apply one action and return the resulting [`StepResult`].
    fn step(&mut self, action: Self::Action) -> StepResult<Self::Obs>;

    /// Set the master seed used by the **next** [`reset`](Env::reset). Does not disturb a run in progress.
    fn seed(&mut self, seed: u64);
}

/// A CRISPR edit expressed at **species** granularity (invariant #6): which species' genome, which Cas
/// variant, which locus on that genome, and the guide. It carries **no organism handle** — it edits one
/// shared species genome, never an individual (Variant-Lab A: the species is CHOSEN, default the primary).
///
/// Resolved through [`crispr::apply_edit`] against the env's Cas-variant table and the chosen species genome.
///
/// Serde-(de)serializable so it can be logged to `actions.ndjson` and replayed bit-identically (SPEC
/// §5/§6): `cas`/`target`/`species` ride as their integer ids; `guide` as its validated ACGT string (a
/// malformed guide in a log fails to deserialize — see [`crispr::GuideSequence`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditAction {
    /// Which Cas variant performs the edit (resolved by id against the variant table).
    pub cas: CasVariantId,
    /// The species-genome locus to target (resolved against `genome.loci` by id).
    pub target: LocusId,
    /// The guide (spacer) sequence.
    pub guide: GuideSequence,
    /// Which species' genome to edit (operator/species granularity — inv #6; never a per-organism handle).
    /// Raw registry ordinal → [`sim_core::SpeciesId`] at the env boundary, the SAME way the SP-3 interventions
    /// resolve `species: u16`. `#[serde(default)]` makes this `0` = the resident PRIMARY species when absent,
    /// so a pre-Variant-Lab `actions.ndjson` line (without the field) deserializes to EXACTLY today's
    /// primary-species edit and replays byte-identically (the recorded-episode golden + the R2 round-trip + the
    /// pinned config are all unmoved).
    #[serde(default)]
    pub species: u16,
}

/// The species/operator-granular action space (invariant #6). There is deliberately **no** per-organism
/// variant: the agent only advances time or edits the shared species genome.
///
/// Serde-(de)serializable so an action sequence can be logged one-per-line to `actions.ndjson` and
/// replayed bit-identically (SPEC §5/§6). Encoded as an externally-tagged enum, e.g.
/// `{"Advance":10}` / `{"ApplyEdit":{ ... }}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// Advance the simulation by `N` generations using the run's single seeded RNG.
    Advance(u64),
    /// Apply a CRISPR edit to the **species** genome (then re-express phenotype so it changes dynamics).
    ApplyEdit(EditAction),
    /// Apply a CRISPR edit to only the organisms inside a CELL region (the selective brush, ADR-011 S-D).
    /// The [`RegionSpec`] names cells, never an organism — the invariant-#6 type guard at the action level.
    ApplyEditRegion(EditAction, RegionSpec),

    /// **INERT SCAFFOLDING (ADR-017 S5 design; not yet load-bearing).** The player spends earned credit to
    /// request a deep, real-E. coli edit. The non-deterministic FBA solve is the PRODUCER (off-thread, outside
    /// the hash); this action only *journals* the request at a deterministic position in the stream. It targets
    /// a `species` (operator/species granularity, inv #6 — never a per-organism handle) + a `locus`, and names
    /// the future `due_epoch` the resulting impact must commit at. `req_id` is a deterministic monotonic
    /// occurrence index into the request stream (NEVER wall-clock/UUID — replay-stable).
    ///
    /// At S4/S5 the step arm is a strict NO-OP: it draws ZERO `SimRng` words and mutates no hashed component
    /// (modeled on `Advance(0)`, NOT on `ApplyEdit` — `ApplyEdit` DRAWS from the stream). This is what keeps
    /// the pinned literal `0x47a0_3c8f_6701_f240` unchanged. Round-trips through `actions.ndjson`; on replay it
    /// is a no-op for the sim (only the paired [`Action::CommitEcoliImpact`] carries effect, and that is also
    /// journaled). `species` is a raw `u16` here (not the core's `SpeciesId`) until S5 promotes the core type to
    /// serde — see `docs/llm/proposals/ecoli-oversight-gameloop-draft.md`.
    RequestEcoliEdit {
        /// Target species (operator/species granularity — inv #6). Raw `u16` scaffold; → `SpeciesId` at S5.
        species: u16,
        /// Target locus on that species' genome (the gene the deep edit perturbs).
        locus: LocusId,
        /// How the deep edit acts on transcription (reuses the landed `crispr::EditKind`, commit 41a7f48).
        edit_kind: crispr::EditKind,
        /// The generation epoch at which the resulting impact is due to commit (a function of the Tick stream,
        /// NOT wall-clock). The firewall buffers the impact until this epoch.
        due_epoch: u32,
        /// Deterministic monotonic occurrence index into the request stream (replay-stable ordering key).
        req_id: u32,
    },

    /// **The CONSUMER side of the firewall (ADR-017 S6 — LOAD-BEARING).** The harness journals the QUANTIZED
    /// integer result of a background FBA solve, committed at a fixed epoch. The payload (`growth_ratio_q` +
    /// ordered `exchange_deltas`) is carried INLINE so replay reads the impact straight from `actions.ndjson` and
    /// NEVER re-runs the deep compute; `slipped_from` makes a deterministic epoch slip self-describing in the
    /// journal. `content_hash` binds the quantized bytes (NOT the floats, NOT the FBA model-version string — that
    /// belongs in provenance). Floats never cross this boundary: everything is quantized via `fixed::to_unit_u16`
    /// inside the subprocess before it is journaled.
    ///
    /// At S6 the step arm READS `growth_ratio_q`: it draws ZERO `SimRng` (the committed integer is read straight
    /// from the journal) but routes the integer to [`sim_core::Simulation::commit_species_edit`], which maps it
    /// to a strictly-positive `[0.5,1.5]` per-species DEMAND + MINERALIZATION factor consumed by the NEXT
    /// `Advance`. A wild-type ratio (1000) maps to exactly neutral (a no-op → hash-unchanged); a committed KO
    /// throttles the edited species (the load-bearing wire). The `exchange_deltas` are carried for the future
    /// ordered-`ResourceField` tap; the single growth-ratio factor is the wire this slice lands.
    CommitEcoliImpact {
        /// Target species (matches the paired request). Raw `u16` scaffold; → `SpeciesId` at S5.
        species: u16,
        /// The request this commit answers (the `(species, req_id)` pair is the deterministic drain key).
        req_id: u32,
        /// The epoch this impact actually commits at (after any deterministic slip).
        due_epoch: u32,
        /// If the commit slipped past its originally-scheduled epoch, the epoch it was originally due at
        /// (self-describing slip — replay is exact). `None` = committed on its first scheduled epoch.
        slipped_from: Option<u32>,
        /// Content hash over the quantized bytes (`growth_ratio_q` + index-ordered `exchange_deltas`). Binds
        /// the committed integers so a tampered journal cannot inject a divergent impact silently (S5 rejects
        /// a mismatch on replay as `InvalidData`).
        content_hash: u64,
        /// Quantized growth-ratio factor (permille; `1000` = wild-type). READ at S6: mapped to a strictly-
        /// positive `[0.5,1.5]` per-species DEMAND + MINERALIZATION factor by `sim_core::edit_factor_q`.
        growth_ratio_q: u16,
        /// Quantized exchange-flux deltas as `(exchange_index, signed_delta)`, in canonical exchange-index
        /// order. Carried for the future ordered-`ResourceField` mineralize tap; not yet consumed by selection
        /// (the single `growth_ratio_q` factor is the wire this slice lands).
        exchange_deltas: Vec<(u16, i16)>,
    },

    /// **CONTAMINATION / IMMIGRATION (ADR-019 S1 — the SP-3-deferred seed/inoculate tool).** Drop a baked
    /// contaminant `SpeciesSpec` (resolved by `species_key`, the `data/species/<key>.json` file stem) onto the
    /// substrate: spawn `count` organisms inside the `region` disc, each endowed with `endow_j` joules MINTED
    /// from a NAMED `immigration` influx tap (conserved — the arrival is accounted, never conjured). RNG-FREE
    /// (deterministic cell-fill placement, OrgIds from the monotonic `NextOrgId`), region-scoped (cells, never
    /// an organism — inv #6), journaled into `actions.ndjson` so a contaminated run replays bit-identically.
    ///
    /// Establish / displace / die-out is NOT coded — it EMERGES from the ADR-013 metabolism→trophic→
    /// reproduce_or_die joule economy (a poorly-adapted immigrant starves; a well-adapted one out-harvests the
    /// resident). At `step` the contaminant species is registered into the running roster (if new) and the
    /// orgs are spawned. Externally-tagged serde-additive: every EXISTING `actions.ndjson` line is unchanged,
    /// so the pinned config (which issues no `RegionInoculate`) keeps `0x47a0_3c8f_6701_f240`.
    RegionInoculate {
        /// The contaminant species key (== the `data/species/<key>.json` file stem) to inoculate.
        species_key: String,
        /// The disc region (cells, centre + radius) the propagule lands in — inv #6 (no organism handle).
        region: RegionSpec,
        /// Number of organisms to spawn.
        count: u32,
        /// Per-organism starting joule reserve, MINTED from the `immigration` ledger tap (conserved).
        endow_j: i64,
    },

    /// **SP-3 PCR-AMPLIFY** — the faithful local-clone tool: spawn `count` FAITHFUL clones of an ALREADY-RESIDENT
    /// species inside the `region` disc, each endowed with `endow_j` joules MINTED from the named `intervention`
    /// ledger tap (conserved — a PCR reaction ADDS copies, never conjured, never halves the template). Unlike
    /// [`Action::RegionInoculate`] (a neutral baked contaminant), each clone COPIES its heritable state VERBATIM
    /// from a deterministically-chosen LOCAL resident template (no mutation, daughter-cell placement). RNG-FREE,
    /// region-scoped (cells, never an organism — inv #6), journaled into `actions.ndjson`. A clean no-op if the
    /// species has no in-region template. `species` is a raw [`sim_core::SpeciesId`] ordinal (the
    /// `RegionInoculate`/`RequestEcoliEdit` scaffold convention; resolved at the step boundary). Externally-tagged
    /// serde-additive: every existing `actions.ndjson` line is byte-identical, so the pinned config keeps
    /// `0x47a0_3c8f_6701_f240`.
    RegionPcrAmplify {
        /// Target RESIDENT species ordinal (operator/species granularity — inv #6). Raw `u16` scaffold.
        species: u16,
        /// The disc region (cells) to amplify into — inv #6 (no organism handle).
        region: RegionSpec,
        /// Number of clones to spawn.
        count: u32,
        /// Per-clone starting joule reserve, MINTED from the `intervention` ledger tap (conserved).
        endow_j: i64,
    },

    /// **SP-3 ANTIBIOTIC CULL** — the selective-kill tool: deterministically kill a `strength`-permille kill
    /// FRACTION of one species' LIVING orgs inside the `region` disc; each culled org's residual `J` deposits to
    /// detritus (carcass→detritus, accounted EXACTLY like a starvation death — a paired bucket move, NO tap
    /// minted). RNG-FREE (a largest-remainder apportioned SUBSET of the canonical census, NOT a per-org coin
    /// flip), region-scoped (cells — inv #6), journaled. `strength` is a permille kill-fraction in `[0, 1000]`.
    RegionCull {
        /// Target species ordinal (operator/species granularity — inv #6). Raw `u16` scaffold.
        species: u16,
        /// The disc region (cells) to cull within — inv #6 (no organism handle).
        region: RegionSpec,
        /// Permille kill FRACTION `[0, 1000]` applied DETERMINISTICALLY to the canonical in-region census.
        strength: u16,
    },

    /// **SP-3 NUTRIENT FEED** — the substrate-feed tool: deposit `amount_j` joules into one [`sim_core`]
    /// `PoolStock` plane (`channel` ∈ {0 light, 1 free_nutrient, 2 detritus}) across the `region` disc, MINTED
    /// from the named `intervention` ledger tap (conserved; per-cell `POOL_CAP` spill → overflow). Species-
    /// AGNOSTIC (it feeds the substrate, not an organism — no `species` field). RNG-FREE, region-scoped (cells —
    /// inv #6), journaled.
    RegionNutrient {
        /// Pool channel selector: `0` light · `1` free_nutrient · `2` detritus (read by ordinal — inv #3).
        channel: u8,
        /// The disc region (cells) to feed — inv #6 (no organism handle).
        region: RegionSpec,
        /// Joules to deposit, MINTED from the `intervention` ledger tap (conserved).
        amount_j: i64,
    },

    /// **SP-3 TOXIN SPIKE** — the chemical-spike tool: deposit `amount_milli` (== `J` 1:1) into one [`sim_core`]
    /// `ChemField` plane (`channel` ∈ {0 toxin, 1 kin, 2 alarm}) across the `region` disc, MINTED from the named
    /// `intervention` ledger tap (conserved; per-cell `CHEM_CAP` spill → overflow). The channel selector makes
    /// kin/alarm reachable, not only toxin. RNG-FREE, region-scoped (cells — inv #6), journaled.
    RegionToxin {
        /// Chem channel selector: `0` toxin · `1` kin · `2` alarm (read by ordinal — inv #3).
        channel: u8,
        /// The disc region (cells) to spike — inv #6 (no organism handle).
        region: RegionSpec,
        /// Milli-J (== `J` 1:1) to deposit, MINTED from the `intervention` ledger tap (conserved).
        amount_milli: i64,
    },
}

/// A spatial brush region for [`Action::ApplyEditRegion`]: a disc of world cells (centre + radius). Serde so
/// it journals to `actions.ndjson` for bit-identical replay; converts to a `sim_core::Region` at apply time.
/// Carries NO organism handle (invariant #6 — the edit targets cells, not individuals).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionSpec {
    /// Disc centre cell x on the world grid.
    pub cx: u32,
    /// Disc centre cell y on the world grid.
    pub cy: u32,
    /// Disc radius in cells.
    pub radius: u32,
}

impl RegionSpec {
    /// Convert to the core's [`sim_core::Region`] (disc cells). Public so the campaign-grader can read a
    /// scenario's target zone via [`GeneSimEnv::region_allele`].
    #[must_use]
    pub fn to_region(self) -> sim_core::Region {
        sim_core::Region {
            cx: self.cx,
            cy: self.cy,
            radius: self.radius,
        }
    }
}

/// A gym-like environment over the deterministic headless core (SPEC §2.2; S3.1).
///
/// Wraps a [`sim_core::Simulation`]; `reset` seeds it once, `step` applies a species-granular [`Action`].
/// The Cas-variant table and scoring/gating policy are fixed at construction so an `ApplyEdit` is a pure
/// function of `(species genome, action, RNG state)` — preserving determinism (inv. #3).
pub struct GeneSimEnv {
    /// Master seed used by the next `reset`.
    seed: u64,
    /// Per-run generation budget / spawn count handed to the core (the edit/advance loop runs on top).
    entity_count: u32,
    /// The player-set climate the next `reset` builds the world under (ADR-012 Phase E). Default = neutral.
    env: EnvParams,
    /// The live simulation, present once `reset` has been called.
    sim: Option<Simulation>,
    /// Cas-variant table the `ApplyEdit` action resolves against (data, not code — SPEC §4).
    variants: Vec<CasVariant>,
    /// On-target scorer for edit gating (pluggable behind a trait, inv. #5; in-core default here).
    on: DefaultOnTargetScore,
    /// Off-target scorer for edit gating.
    off: DefaultOffTargetScore,
    /// Gating thresholds for [`crispr::apply_edit`].
    thresholds: EditThresholds,
    /// Outcome of the most recent `ApplyEdit` (so callers/tests can inspect success vs failure).
    last_edit: Option<EditOutcome>,
    /// Outcome + covered-organism count of the most recent `ApplyEditRegion` (ADR-011 S-D).
    last_region_edit: Option<(RegionEditOutcome, u32)>,
    /// The species the **next** `reset` runs (ADR-017 "RUN E. coli"). `None` = the default plant genome +
    /// map (byte-identical). `Some(built)` runs `built.genome` through `trait_map_for(built.key)`.
    species: Option<BuiltSpecies>,
    /// The MULTI-SPECIES ROSTER the **next** `reset` spawns (SP-2, ADR-020). An ordered `Vec` of
    /// `(BuiltSpecies, starting_count)` pairs — the player's composed consortium. When non-empty it takes
    /// PRECEDENCE over [`species`](Self::set_species): each entry becomes a [`sim_core::RosterEntry`] (same
    /// inline construction as the single-species path, but with the per-species count) and the run spawns via
    /// [`Simulation::reset_with_roster`], seeding the single `SimRng` ONCE over the full population (inv #3).
    /// Empty by default → the pinned config never enters this arm (hash-neutral; the literal is untouched).
    /// An ordered `Vec`, never a `HashMap` iterated in sim logic — the ROW ORDER is the load-bearing spawn key.
    roster: Vec<(BuiltSpecies, u32)>,
    /// Available CONTAMINANT species, keyed by `species_key` (ADR-019 S1 — the loaded consortium). The
    /// boundary (renderer/CLI) loads each baked `SpeciesSpec` JSON into a [`BuiltSpecies`] and registers it
    /// here so a journaled [`Action::RegionInoculate`] can resolve its key to a genome at `step`. Empty by
    /// default → the pinned config never inoculates (an unresolved key is a logged no-op, not a panic). An
    /// ordered `Vec`, never a `HashMap` iterated in sim logic (inv #3).
    consortium: Vec<BuiltSpecies>,
    /// The CONTAINMENT knob the **next** `reset` builds its immigration schedule under (ADR-019 S2). Default
    /// [`sim_core::ContainmentLevel::Sealed`] (OFF) → an empty schedule → the pinned config issues no events
    /// (hash-neutral). Paired with [`consortium_config`](Self::set_containment) below.
    containment: sim_core::ContainmentLevel,
    /// The consortium config (menu set + pressure params) the schedule draws from (ADR-019 S2). Default empty
    /// → no events regardless of the knob.
    consortium_config: sim_core::ConsortiumConfig,
    /// The expanded `(due_epoch, RegionInoculate)` schedule for the CURRENT run, sorted by `due_epoch` (built
    /// at `reset` off the off-stream `IMMG_STREAM_BASE` family — ZERO `SimRng` draws). Drained in order as the
    /// env advances generations; each fired event is journaled like a hand-issued action. Empty when the knob
    /// is Sealed (the default) — so the pinned config carries an empty schedule.
    schedule: Vec<sim_core::ScheduledInoculation>,
    /// How many of `schedule`'s events have already fired (the drain cursor). Reset to `0` at `reset`.
    schedule_cursor: usize,
    /// Cumulative generations advanced since `reset` (the Tick clock the schedule fires against). The env
    /// advances via `Action::Advance`, so this mirrors the core's generation counter for schedule timing.
    generation: u64,
    /// The OVERSIGHT earned-credit economy state (ADR-017 S4–S6 — the renderer-driven earn→spend loop the
    /// godot-sim oversight `#[func]`s marshal). Off-hash by construction (the `edits_used` / `CreditLedger`
    /// precedent): accrual is a pure integer fold over RNG-free read-only projections, so it adds 0 bytes to
    /// `hash_world` and the pinned literal `0x47a0_3c8f_6701_f240` is untouched. DISABLED by default → the
    /// existing headless callers (discovery / campaign / `OversightEpisode`) pay zero overhead and step
    /// byte-AND-perf-identically; [`LiveSim`](../../godot-sim) enables it at `reset` so the renderer's run accrues.
    oversight: OversightState,
}

/// A single committed deep-edit (ADR-017 S6 — the renderer's INSPECT row). Mirrors the journaled
/// [`Action::CommitEcoliImpact`] payload that crossed the firewall: the target species, its `req_id`, the epoch
/// it committed at, the quantized growth ratio, and the strictly-positive `[0.5,1.5]` demand factor it mapped to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommittedEdit {
    /// Target species ordinal (operator/species granularity — inv #6).
    pub species: u16,
    /// The deterministic monotonic occurrence index of the committed request.
    pub req_id: u32,
    /// The epoch the impact committed at (Tick-counted, never wall-clock).
    pub due_epoch: u32,
    /// The committed quantized growth-ratio permille (`1000` = wild-type).
    pub growth_ratio_q: u16,
    /// The strictly-positive `[500,1500]` permille demand factor the ratio mapped to via [`sim_core::edit_factor_q`].
    pub factor_q: u16,
}

/// A read-only snapshot of the OVERSIGHT ledger for the renderer's INSPECT view (ADR-017 S4). Pure data — drawn
/// without touching the sim RNG, never folded into the determinism hash (inv #3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OversightStatus {
    /// Spendable credit currently held.
    pub credit: u64,
    /// Total credit ever accrued (monotonic).
    pub accrued_total: u64,
    /// The per-generation accrual cap (the economy's accrual-rate ceiling).
    pub per_gen_cap: u64,
    /// The cost of one deep edit (the spend gate).
    pub edit_cost: u64,
    /// Whether a deep edit can be afforded right now (`credit >= edit_cost`).
    pub affordable: bool,
    /// Requests buffered but not yet committed (always `0` in the immediate-commit renderer path; kept for the
    /// firewall-buffered future).
    pub pending: u32,
    /// Every deep edit committed so far this run, in commit order.
    pub committed: Vec<CommittedEdit>,
}

/// A read-only PREVIEW of a deep edit (ADR-017 S6) — the predicted KO/growth outcome WITHOUT committing. Drawn
/// with zero `SimRng` and no mutation (modeled on [`GeneSimEnv::observe_all`]), so previewing never perturbs the
/// run or the hash (inv #3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OversightPreview {
    /// The quantized growth-ratio permille being previewed (`1000` = wild-type).
    pub growth_ratio_q: u16,
    /// The strictly-positive `[500,1500]` permille demand factor the ratio WOULD map to (loss-of-function map).
    pub predicted_factor_q: u16,
    /// The factor currently in effect for that species (`1000` = unedited/neutral).
    pub current_factor_q: u16,
    /// Whether a deep edit can be afforded right now.
    pub affordable: bool,
}

/// The outcome of a renderer-driven [`GeneSimEnv::commit_ecoli_edit`]: whether the spend gate accepted, the epoch
/// the impact committed at, and the `RequestEcoliEdit`/`CommitEcoliImpact` pair to append to the renderer journal
/// (empty when REJECTED). The pair is the SAME journaled action stream `OversightEpisode` produces, so a saved
/// session replays the committed edit bit-identically from `actions.ndjson` (inv #3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OversightCommit {
    /// `true` if the spend gate accepted and the impact was committed; `false` if refused (insufficient credit).
    pub applied: bool,
    /// A short human-readable reason (`"applied"` / `"rejected: insufficient credit"` / `"not reset"`).
    pub reason: &'static str,
    /// The epoch the impact committed at (Tick-counted; `0` when rejected).
    pub due_epoch: u32,
    /// The committed request's `req_id` (`0` when rejected).
    pub req_id: u32,
    /// The strictly-positive `[500,1500]` permille demand factor applied (`1000` neutral when rejected/none).
    pub factor_q: u16,
    /// The journaled `[RequestEcoliEdit, CommitEcoliImpact]` pair to record for save/load (empty when rejected).
    pub journaled: Vec<Action>,
}

/// The neutral per-species demand factor permille (`1000` = `1.0×`, an unedited/no-op species). Mirrors
/// sim-core's crate-private `EDIT_FACTOR_NEUTRAL_Q`; a committed wild-type ratio maps here (hash-neutral).
const NEUTRAL_FACTOR_Q: u16 = 1000;

/// The default OBJECTIVE region the credit Term A reads — a whole-world disc over the `RESOURCE_DIMS` grid (the
/// same zone the `firewall_determinism` economy tests read). Pure config, off-hash.
const DEFAULT_OVERSIGHT_REGION: sim_core::Region = sim_core::Region {
    cx: 16,
    cy: 16,
    radius: 64,
};

/// The OVERSIGHT earned-credit economy state held on [`GeneSimEnv`] (ADR-017 S4–S6). Off-hash: the harness-layer
/// `CreditLedger` + `EditFirewall` (the `edits_used` precedent) add 0 bytes to `hash_world`. Disabled by default.
#[derive(Debug)]
struct OversightState {
    /// Whether accrual runs at all (set by [`GeneSimEnv::enable_oversight`]). Off by default → zero overhead and
    /// byte-AND-perf-identical stepping for the headless callers that never enable it.
    enabled: bool,
    /// The earned-credit ledger (S4) — recomputed deterministically from the per-advance stats fold.
    ledger: oversight::CreditLedger,
    /// The credit-economy tuning (S4).
    policy: oversight::CreditPolicy,
    /// The firewall `req_id` allocator (S5) — a deterministic monotonic occurrence index, reset per episode.
    firewall: firewall::EditFirewall,
    /// Every committed deep edit this run, in commit order (the INSPECT view).
    committed: Vec<CommittedEdit>,
    /// The objective region the credit Term A reads, and the grid it is read on. Set by `enable_oversight`.
    region: sim_core::Region,
    grid: (u32, u32),
    /// The previous advance's stats sample (for the quantize-each-then-difference accrual fold). `None` until the
    /// gen-0 baseline is taken at `reset`.
    prev_sample: Option<oversight::GenSample>,
}

impl Default for OversightState {
    fn default() -> Self {
        Self {
            enabled: false,
            ledger: oversight::CreditLedger::new(),
            policy: oversight::CreditPolicy::default(),
            firewall: firewall::EditFirewall::new(),
            committed: Vec::new(),
            region: DEFAULT_OVERSIGHT_REGION,
            grid: sim_core::resource::RESOURCE_DIMS,
            prev_sample: None,
        }
    }
}

impl GeneSimEnv {
    /// Build an env with the default Cas-variant table and in-core scorers. `entity_count` is the
    /// population spawned at each `reset`.
    #[must_use]
    pub fn new(entity_count: u32) -> Self {
        Self {
            seed: 42,
            entity_count,
            env: EnvParams::default(),
            sim: None,
            variants: default_cas_variants(),
            on: DefaultOnTargetScore,
            off: DefaultOffTargetScore::default(),
            thresholds: EditThresholds::default(),
            last_edit: None,
            last_region_edit: None,
            species: None,
            roster: Vec::new(),
            consortium: Vec::new(),
            containment: sim_core::ContainmentLevel::default(),
            consortium_config: sim_core::ConsortiumConfig::default(),
            schedule: Vec::new(),
            schedule_cursor: 0,
            generation: 0,
            oversight: OversightState::default(),
        }
    }

    /// Set the CONTAINMENT knob + consortium config the **next** `reset` builds its immigration schedule under
    /// (ADR-019 S2). The schedule expands at `reset` off the off-stream `IMMG_STREAM_BASE` family (ZERO
    /// `SimRng` draws), so this is hash-neutral while `level` is [`Sealed`](sim_core::ContainmentLevel::Sealed)
    /// (the default → an empty schedule). The boundary loads the named consortium species as contaminants (see
    /// [`register_contaminant`](Self::register_contaminant)) so a scheduled event can resolve its key. Does not
    /// disturb a run in progress.
    pub fn set_containment(
        &mut self,
        level: sim_core::ContainmentLevel,
        config: sim_core::ConsortiumConfig,
    ) {
        self.containment = level;
        self.consortium_config = config;
    }

    /// The CURRENT run's expanded immigration schedule (ADR-019 S2 — for the panel + tests). Read-only.
    #[must_use]
    pub fn immigration_schedule(&self) -> &[sim_core::ScheduledInoculation] {
        &self.schedule
    }

    /// Cumulative generations advanced since `reset` (the Tick clock the immigration schedule fires against).
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The cumulative `J` minted into the run via the `immigration` tap so far (ADR-019 — for the panel +
    /// tests). `0` on a run that never inoculated. Panics if called before `reset`.
    #[must_use]
    pub fn immigration_minted(&self) -> i64 {
        self.sim
            .as_ref()
            .expect("GeneSimEnv::immigration_minted called before reset")
            .ledger()
            .immigration
    }

    /// The cumulative `J` minted into the run via the `intervention` tap so far (SP-3 — for the panel + tests):
    /// the sum of PCR-clone endowments + nutrient-feed + toxin-spike J. `0` on a run that issues no SP-3
    /// intervention. (An antibiotic CULL mints NOTHING — it never touches this tap.) Panics if called before
    /// `reset`.
    #[must_use]
    pub fn intervention_minted(&self) -> i64 {
        self.sim
            .as_ref()
            .expect("GeneSimEnv::intervention_minted called before reset")
            .ledger()
            .intervention
    }

    /// Drain every scheduled inoculation whose `due_epoch < up_to_generation` (ADR-019 S2), advancing the
    /// schedule cursor, and return them as journaled [`Action::RegionInoculate`]s in schedule order. The
    /// DRIVER (renderer/oversight/replay loop) calls this around its `Advance` cadence and JOURNALS + `step`s
    /// each returned action — so a scheduled arrival is byte-identical to a hand-fired one and a contaminated
    /// run replays from `actions.ndjson` alone (the schedule is a SOURCE of journaled actions, never a hidden
    /// side-effect inside `step`). Tick-clocked: `due_epoch` is a generation count, never wall-clock. Returns
    /// an empty `Vec` for a Sealed run (empty schedule). The cursor only advances forward (idempotent across a
    /// monotonically rising `up_to_generation`).
    #[must_use]
    pub fn drain_due_inoculations(&mut self, up_to_generation: u64) -> Vec<Action> {
        let mut out = Vec::new();
        while self.schedule_cursor < self.schedule.len() {
            let due = &self.schedule[self.schedule_cursor];
            if u64::from(due.due_epoch) >= up_to_generation {
                break; // schedule is sorted by due_epoch — nothing further is due yet
            }
            out.push(Action::RegionInoculate {
                species_key: due.event.species_key.clone(),
                region: RegionSpec {
                    cx: due.event.region.cx,
                    cy: due.event.region.cy,
                    radius: due.event.region.radius,
                },
                count: due.event.count,
                endow_j: due.event.endow_j,
            });
            self.schedule_cursor += 1;
        }
        out
    }

    /// Register a CONTAMINANT species (ADR-019 S1) so a journaled [`Action::RegionInoculate`] keyed on its
    /// `built.key` can resolve a genome at `step`. The boundary loads the baked `SpeciesSpec` JSON (via
    /// [`species::load_species_file`] / [`species::build_species_from_str`]) and hands the [`BuiltSpecies`]
    /// here. Re-registering the same key REPLACES the prior built (the latest wins); the order of distinct
    /// keys is insertion order (an ordered `Vec`, inv #3). Does not disturb a run in progress — it only seeds
    /// the resolver the NEXT inoculation reads. The default env has an empty consortium → the pinned config
    /// never inoculates.
    pub fn register_contaminant(&mut self, built: BuiltSpecies) {
        if let Some(slot) = self.consortium.iter_mut().find(|b| b.key == built.key) {
            *slot = built;
        } else {
            self.consortium.push(built);
        }
    }

    /// The registered contaminant CONSORTIUM in insertion order (ADR-019 S1 + R2 — read-only). The boundary
    /// (renderer SAVE) reads this so a saved session can PERSIST the keys + genomes a journaled
    /// [`Action::RegionInoculate`] resolves against, and a LOAD/replay re-`register_contaminant`s them BEFORE
    /// replaying the journal (without that, a journaled inoculate resolves nothing on replay and the hash
    /// diverges — the R2 break). An ordered `Vec`, never iterated as a `HashMap` in sim logic (inv #3).
    #[must_use]
    pub fn registered_consortium(&self) -> &[BuiltSpecies] {
        &self.consortium
    }

    /// Set the climate the **next** `reset` builds the world under (ADR-012 Phase E). Does not disturb a run in
    /// progress. The renderer/CLI feeds this from the main menu; default is the neutral world.
    pub fn set_environment(&mut self, env: EnvParams) {
        self.env = env;
    }

    /// Set the species the **next** `reset` runs (ADR-017 "RUN E. coli"). The renderer/menu feeds a
    /// [`BuiltSpecies`] (loaded via [`species::load_species_file`]); the env runs its genome through the
    /// per-species trait map (E. coli → gltA growth) and adopts its niche `entity_count` (when non-zero). Pass
    /// the default species (or never call this) to keep the historical plant run. Does not disturb a run in
    /// progress.
    pub fn set_species(&mut self, built: BuiltSpecies) {
        if built.entity_count > 0 {
            self.entity_count = built.entity_count;
        }
        self.species = Some(built);
    }

    /// Set the MULTI-SPECIES ROSTER the **next** `reset` spawns (SP-2, ADR-020). The boundary (renderer/CLI)
    /// loads each baked `SpeciesSpec` JSON into a [`BuiltSpecies`] and pairs it with the player's chosen
    /// STARTING COUNT; this stores the ordered `(built, count)` pairs verbatim. A non-empty roster takes
    /// PRECEDENCE over [`set_species`](Self::set_species) at `reset`, mapping each entry to a
    /// [`sim_core::RosterEntry`] (with `entity_count` = the per-species count) and spawning via
    /// [`Simulation::reset_with_roster`] — one `SimRng` seeded once over the full population (inv #3).
    ///
    /// CRITICAL (hash-neutrality guard): unlike `set_species`, this does **NOT** mutate `self.entity_count`.
    /// The per-species count flows straight into each `RosterEntry`; `self.entity_count` stays the
    /// legacy/snapshot-grid fallback for the non-roster paths, so calling `set_roster` then resetting WITHOUT
    /// a roster (e.g. after `clear_roster`) is byte-identical to never having called it. Does not disturb a
    /// run in progress.
    pub fn set_roster(&mut self, entries: Vec<(BuiltSpecies, u32)>) {
        self.roster = entries;
    }

    /// Clear the multi-species roster (SP-2) — the **next** `reset` falls back to the
    /// [`set_species`](Self::set_species) / default-plant precedence. Leaves `self.entity_count` untouched.
    pub fn clear_roster(&mut self) {
        self.roster.clear();
    }

    /// The outcome of the most recent [`Action::ApplyEdit`], if any (for inspection / tests).
    #[must_use]
    pub fn last_edit(&self) -> Option<&EditOutcome> {
        self.last_edit.as_ref()
    }

    /// The outcome + covered-organism count of the most recent [`Action::ApplyEditRegion`] (ADR-011 S-D).
    #[must_use]
    pub fn last_region_edit(&self) -> Option<&(RegionEditOutcome, u32)> {
        self.last_region_edit.as_ref()
    }

    /// The current observation without taking a step (panics if called before `reset`).
    #[must_use]
    pub fn observe(&mut self) -> Observation {
        self.sim
            .as_mut()
            .expect("GeneSimEnv::observe called before reset")
            .observe()
    }

    /// Observe EVERY species in the roster (delegates to [`sim_core::Simulation::observe_all`]; panics if
    /// called before `reset`). A read-only per-species display projection for the renderer's specimen view —
    /// pure w.r.t. the run (no RNG draw, no mutation, never folded into the determinism hash, inv #2/#3).
    #[must_use]
    pub fn observe_all(&self) -> Vec<sim_core::SpeciesObservation> {
        self.sim
            .as_ref()
            .expect("GeneSimEnv::observe_all called before reset")
            .observe_all()
    }

    /// The MEASURED per-generation FlowMatrix as `(s, flat_row_major_i64)` (ADR-013 F4 — delegates to
    /// [`sim_core::Simulation::flow_matrix`]; panics if called before `reset`). Read-only: a pure projection of
    /// the recorded matrix (no RNG draw, no mutation). The renderer's relations heatmap reads this contract.
    #[must_use]
    pub fn flow_matrix(&self) -> (usize, Vec<i64>) {
        self.sim
            .as_ref()
            .expect("GeneSimEnv::flow_matrix called before reset")
            .flow_matrix()
    }

    /// The read-only per-species relations **signatures** as `(s, D, flat s*D u16, roles s u8)` (ADR-014
    /// re-grounded — delegates to [`sim_core::Simulation::species_signatures`]; panics if called before
    /// `reset`). A PURE off-hash projection (Block A cached Strategy, Block B measured FlowMatrix) — no RNG
    /// draw, no mutation, never folded into the determinism hash (inv #2/#3). The boundary
    /// `relations-index` k-NN / guild clustering consumes this; the output is VIEW-ONLY and never re-enters
    /// the sim.
    #[must_use]
    pub fn species_signatures(&self) -> (usize, usize, Vec<u16>, Vec<u8>) {
        self.sim
            .as_ref()
            .expect("GeneSimEnv::species_signatures called before reset")
            .species_signatures()
    }

    /// Export species `species`'s CURRENT (post-edit) genome + niche as a `SpeciesSpec` JSON STRING (Variant-Lab
    /// Slice B — the "save the edited variant" boundary). Delegates to
    /// [`sim_core::Simulation::export_species_spec`] (the single biology→spec mapping, inv #2) then serializes
    /// with `serde_json`. Read-only: draws ZERO `SimRng`, mutates nothing, never folded into the determinism
    /// hash (inv #3 — modeled on [`observe_all`](Self::observe_all) / [`species_signatures`](Self::species_signatures)).
    /// The JSON re-loads through [`species::build_species_from_str`] to the SAME expressed phenotype (the
    /// save→reseed contract). Returns `None` before `reset`, for an out-of-range `species`, or on the
    /// (impossible-by-construction) serialize error — the boundary maps `None` to an empty string + an error.
    #[must_use]
    pub fn export_species_json(&self, species: u16) -> Option<String> {
        let spec = self
            .sim
            .as_ref()?
            .export_species_spec(sim_core::SpeciesId::new(species))?;
        serde_json::to_string_pretty(&spec).ok()
    }

    /// A read-only, derived per-cell [`sim_core::GridSnapshot`] of the current state (delegates to
    /// [`sim_core::Simulation::snapshot`]; panics if called before `reset`).
    ///
    /// Read-only & ADDITIVE (invariant #3): `snapshot` draws no RNG and mutates nothing, so taking one
    /// mid-episode cannot change the determinism hash. The renderer reads these to draw the ecosystem;
    /// stepping the env through the same action sequence keeps a snapshot's `generation` aligned with the
    /// journaled `Advance` cumulative (so injection markers land on the right frame).
    #[must_use]
    pub fn snapshot(&mut self, width: u32, height: u32) -> sim_core::GridSnapshot {
        self.sim
            .as_mut()
            .expect("GeneSimEnv::snapshot called before reset")
            .snapshot(width, height)
    }

    /// Read the mean allele frequency over the populated cells of a disc `region` on a `grid_w × grid_h`
    /// grid (campaign-grader). Delegates to [`sim_core::Simulation::region_allele`] — read-only, RNG-free
    /// (panics if called before `reset`). This is the headless equivalent of the renderer's `_eval_mission`
    /// zone reading, so the campaign scorer grades in Rust instead of GDScript (invariant #2).
    #[must_use]
    pub fn region_allele(
        &mut self,
        region: sim_core::Region,
        grid_w: u32,
        grid_h: u32,
    ) -> sim_core::RegionReadout {
        self.sim
            .as_mut()
            .expect("GeneSimEnv::region_allele called before reset")
            .region_allele(region, grid_w, grid_h)
    }

    /// Commit a deep-edit impact to the core (ADR-017 S6 — the load-bearing OVERSIGHT wire). Routes a firewall
    /// [`Action::CommitEcoliImpact`]'s `(species, growth_ratio_q)` to [`sim_core::Simulation::commit_species_edit`]
    /// so the NEXT `Advance` makes the core throttle that species' uptake + mineralization by the strictly-
    /// positive `[0.5,1.5]` factor [`sim_core::edit_factor_q`] derives.
    ///
    /// The committed `growth_ratio_q` already encodes the `EditKind` grading (the `oracle-fba` frozen-table
    /// lookup bakes Knockout/Knockdown/Activate INTO the permille before it crosses the firewall), so the core
    /// maps it via the loss-of-function direction ([`sim_core::EditEffect::Knockout`]): a `<1000` ratio is a
    /// penalty toward `0.5×`, `1000` is exactly neutral (a no-op → hash-unchanged). The committed INTEGER read
    /// straight from the journal on replay is the only thing crossing into the hashed sim (the firewall's one-way
    /// quantized crossing). Panics if called before `reset`.
    pub fn commit_species_edit(&mut self, species: u16, growth_ratio_q: u16) {
        self.sim
            .as_mut()
            .expect("GeneSimEnv::commit_species_edit called before reset")
            .commit_species_edit(
                sim_core::SpeciesId::new(species),
                growth_ratio_q,
                sim_core::EditEffect::Knockout,
            );
    }

    // ── OVERSIGHT earned-credit loop (ADR-017 S4–S6) — the renderer-driven earn→spend surface ──────────────────
    // All of the economy/biology lives HERE in the harness (inv #2); the godot-sim `#[func]`s only marshal these
    // returns into `VarDictionary`s. Accrual is a pure integer fold over RNG-free read-only projections, so the
    // whole surface is OFF-hash — the pinned literal `0x47a0_3c8f_6701_f240` is never moved by it.

    /// ENABLE the OVERSIGHT earned-credit economy for subsequent runs under `policy` (ADR-017 S4). Off by default
    /// so the headless callers (discovery / campaign / `OversightEpisode`) stay byte-AND-perf-identical; the
    /// renderer's binding calls this before `reset`. The objective region the credit Term A reads is the
    /// whole-world disc over `RESOURCE_DIMS`. The per-run ledger / firewall / committed list are (re)initialized at
    /// the next [`reset`](Env::reset). Does not disturb a run already in progress.
    pub fn enable_oversight(&mut self, policy: oversight::CreditPolicy) {
        self.oversight.enabled = true;
        self.oversight.policy = policy;
        self.oversight.region = DEFAULT_OVERSIGHT_REGION;
        self.oversight.grid = sim_core::resource::RESOURCE_DIMS;
    }

    /// Read the OVERSIGHT ledger (ADR-017 S4) — balance + accrual cap + the committed deep edits. Pure read-only
    /// (no RNG, no mutation, off-hash). The renderer's INSPECT panel marshals this; returns the zero ledger before
    /// any run.
    #[must_use]
    pub fn oversight_status(&self) -> OversightStatus {
        let o = &self.oversight;
        OversightStatus {
            credit: o.ledger.credit,
            accrued_total: o.ledger.accrued_total,
            per_gen_cap: o.policy.per_gen_cap,
            edit_cost: o.policy.ecoli_edit_cost,
            affordable: o.ledger.can_afford(&o.policy),
            // Immediate-commit renderer path never buffers, so nothing is pending; `is_empty()` confirms it.
            pending: u32::from(!o.firewall.is_empty()),
            committed: o.committed.clone(),
        }
    }

    /// PREVIEW a deep edit (ADR-017 S6) — the predicted KO/growth outcome WITHOUT committing. Maps the
    /// already-quantized FBA `growth_ratio_q` to the strictly-positive `[0.5,1.5]` demand factor via the SAME
    /// core [`sim_core::edit_factor_q`] the commit path applies (loss-of-function direction), reports the factor
    /// currently in effect for `species`, and whether the spend gate can afford it. READ-ONLY: zero `SimRng`, no
    /// mutation, never folded into the hash (inv #3 — modeled on [`observe_all`](Self::observe_all)). Returns the
    /// neutral preview before `reset`.
    #[must_use]
    pub fn preview_ecoli_edit(&self, species: u16, growth_ratio_q: u16) -> OversightPreview {
        let predicted_factor_q =
            sim_core::edit_factor_q(growth_ratio_q, sim_core::EditEffect::Knockout);
        let current_factor_q = self
            .sim
            .as_ref()
            .map(|s| s.species_edit_factor_q(sim_core::SpeciesId::new(species)))
            .unwrap_or(NEUTRAL_FACTOR_Q);
        OversightPreview {
            growth_ratio_q,
            predicted_factor_q,
            current_factor_q,
            affordable: self.oversight.ledger.can_afford(&self.oversight.policy),
        }
    }

    /// COMMIT a deep edit (ADR-017 S6 — the load-bearing OVERSIGHT wire). Runs the credit SPEND gate; on accept it
    /// allocates the deterministic `req_id`, journals the `RequestEcoliEdit` + `CommitEcoliImpact` pair (the SAME
    /// stream `OversightEpisode` produces), and applies the committed quantized `growth_ratio_q` through the
    /// existing [`Action::CommitEcoliImpact`] `step` arm (which sets the per-species `[0.5,1.5]` demand factor the
    /// NEXT `Advance` consumes). The pair is RETURNED for the renderer to append to its journal so a saved session
    /// replays the edit bit-identically (inv #3); a REFUSED request journals nothing and applies nothing (the
    /// hash-neutral baseline). The commit epoch is Tick-counted (`max(due_epoch, epoch_of(gen) + EPOCH_LEAD)`),
    /// never wall-clock. `growth_ratio_q` is the already-quantized FBA answer the off-thread oracle produced (the
    /// firewall's one-way integer crossing — floats never reach here). Rejected (not reset) before `reset`.
    pub fn commit_ecoli_edit(
        &mut self,
        species: u16,
        growth_ratio_q: u16,
        due_epoch: u32,
    ) -> OversightCommit {
        if self.sim.is_none() {
            return OversightCommit {
                applied: false,
                reason: "not reset",
                due_epoch: 0,
                req_id: 0,
                factor_q: NEUTRAL_FACTOR_Q,
                journaled: Vec::new(),
            };
        }
        // Spend gate (the two-tier rule): a borderline credit accepts-or-refuses HERE; the decision is journaled
        // (a refused request emits no action) so replay never re-decides on a recomputed credit.
        let policy = self.oversight.policy;
        if !self.oversight.ledger.try_spend(&policy) {
            return OversightCommit {
                applied: false,
                reason: "rejected: insufficient credit",
                due_epoch: 0,
                req_id: 0,
                factor_q: NEUTRAL_FACTOR_Q,
                journaled: Vec::new(),
            };
        }
        let req_id = self.oversight.firewall.alloc_req_id();
        // The commit epoch is decided by epoch-counting off the Tick stream (never wall-clock); honour a caller's
        // later request but never earlier than the lead window.
        let commit_epoch =
            due_epoch.max(oversight::epoch_of(self.generation) + oversight::EPOCH_LEAD);
        // Build the firewall payload + its content hash over the quantized bytes (the same binding replay checks).
        let impact = firewall::EcoliImpact {
            growth_ratio_q,
            exchange_deltas: Vec::new(),
        };
        let content_hash = impact.content_hash();
        let request = Action::RequestEcoliEdit {
            species,
            locus: LocusId(0),
            edit_kind: crispr::EditKind::Knockout,
            due_epoch: commit_epoch,
            req_id,
        };
        let commit = Action::CommitEcoliImpact {
            species,
            req_id,
            due_epoch: commit_epoch,
            slipped_from: None,
            content_hash,
            growth_ratio_q,
            exchange_deltas: Vec::new(),
        };
        // Apply through the existing step arms: the request is inert (no RNG), the commit sets the per-species
        // demand factor (no RNG — the integer is read straight from the action, exactly as replay reads it).
        let _ = self.step(request.clone());
        let _ = self.step(commit.clone());
        let factor_q = sim_core::edit_factor_q(growth_ratio_q, sim_core::EditEffect::Knockout);
        self.oversight.committed.push(CommittedEdit {
            species,
            req_id,
            due_epoch: commit_epoch,
            growth_ratio_q,
            factor_q,
        });
        OversightCommit {
            applied: true,
            reason: "applied",
            due_epoch: commit_epoch,
            req_id,
            factor_q,
            journaled: vec![request, commit],
        }
    }

    /// Accrue ONE advance's earned credit (ADR-017 S4): sample the RNG-free objective region + FlowMatrix-health
    /// projections, quantize-each-then-difference against the previous sample, and fold the improvement into the
    /// ledger. Called after each `Advance` when oversight is enabled. Pure read-only sampling → off-hash (inv #3).
    fn accrue_oversight(&mut self) {
        let now = self.sample_oversight();
        if let Some(prev) = self.oversight.prev_sample {
            let policy = self.oversight.policy;
            self.oversight.ledger.accrue_gen(&prev, &now, &policy);
        }
        self.oversight.prev_sample = Some(now);
    }

    /// Read the RNG-free OVERSIGHT credit sample at the current env state (a pure read-only projection — no RNG, no
    /// mutation). Mirrors `oversight::sample_now`.
    fn sample_oversight(&mut self) -> oversight::GenSample {
        let region = self.oversight.region;
        let (gw, gh) = self.oversight.grid;
        let readout = self.region_allele(region, gw, gh);
        let (s, flat) = self.flow_matrix();
        oversight::GenSample::from_projections(&readout, s, &flat)
    }

    /// Apply a journaled [`Action::RegionInoculate`] (ADR-019 S1): resolve the contaminant `species_key` to a
    /// registered [`BuiltSpecies`], register it into the running roster (idempotent — no duplicate), then spawn
    /// `count` orgs RNG-FREE into the region disc with J minted from the `immigration` tap. An UNRESOLVED key
    /// (the consortium was never loaded) is a clean NO-OP — never a panic — so a journaled stream stays robust.
    /// Establish/displace/die-out then EMERGES from the ADR-013 economy; nothing is scripted here.
    fn step_region_inoculate(
        &mut self,
        species_key: &str,
        region: RegionSpec,
        count: u32,
        endow_j: i64,
    ) {
        // Resolve + clone the built out of the consortium first so the `sim` borrow below is independent.
        let resolved = self
            .consortium
            .iter()
            .find(|b| b.key == species_key)
            .cloned();
        let Some(built) = resolved else {
            self.last_edit = None;
            return; // unresolved key → no-op (the consortium was never loaded)
        };
        let sim = self
            .sim
            .as_mut()
            .expect("GeneSimEnv::step called before reset");
        let role = sim_core::gp::role_from_override(built.trophic_role.as_deref(), &built.key);
        let sid = sim.register_species(
            built.name,
            built.key.clone(),
            built.genome,
            OntologyMap::new(trait_map_for(&built.key)),
            role,
            // ADR-019 S5: carry the declared host_key through so an obligate symbiont resolves its host SpeciesId
            // at register (the host must already be registered — the region_inoculate host-presence gate enforces it).
            built.host_key.as_deref(),
        );
        sim.region_inoculate(sid, region.to_region(), count, endow_j);
        self.last_edit = None;
    }

    /// The deterministic [`sim_core::RunStats`] of the episode so far — its `hash` is the bit-identical
    /// replay artifact (SPEC §6). Folds in the same final RNG word as the one-shot path, so it must be
    /// called once at the **end** of an episode (panics if called before `reset`).
    #[must_use]
    pub fn run_stats(&mut self) -> sim_core::RunStats {
        self.sim
            .as_mut()
            .expect("GeneSimEnv::run_stats called before reset")
            .run_stats()
    }

    /// Reward for an observation: the population `allele_freq` (the trait under selection), in `[0, 1]`.
    ///
    /// Documented choice (SPEC §2.2 allows a simple scalar): higher allele frequency of the favored
    /// genotype = more reward, so an agent that picks edits/advances driving the population toward the
    /// selected trait is rewarded. Bounded `[0, 1]` and deterministic.
    fn reward_of(obs: &Observation) -> f64 {
        obs.allele_freq
    }
}

impl Env for GeneSimEnv {
    type Action = Action;
    type Obs = Observation;

    fn reset(&mut self, seed: u64) -> Observation {
        self.seed = seed;
        self.last_edit = None;
        self.last_region_edit = None;
        let cfg = SimConfig {
            seed,
            // `generations` here is only metadata for the stats hash; the env advances via `Advance`.
            generations: 0,
            entity_count: self.entity_count,
        };
        // Build the world under the player's climate (ADR-012 Phase E; default env = byte-identical to before).
        // With a selected species (ADR-017), run ITS genome through ITS trait map; otherwise the default plant.
        // A selected species goes through a 1-entry ROSTER so the species' KEY + trophic ROLE reach the registry
        // (read by the read-only `observe_all` so the renderer can show the right glyph). The roster path is
        // byte-identical to `reset_with_genome_and_map` for a single entry (name/key/role are display metadata,
        // never folded into the determinism hash) — so determinism is preserved (inv #3).
        // SP-2 (ADR-020): the MULTI-SPECIES ROSTER takes PRECEDENCE (roster > species > default plant). When the
        // player has composed a consortium, map each `(built, count)` to a `RosterEntry` — the EXACT per-entry
        // construction the single-species path below inlines, GENERALIZED from 1 to N, with `entity_count: *n`
        // (the per-species starting count, NOT `cfg.entity_count`) — and spawn the whole roster through ONE
        // `reset_with_roster` (the single `SimRng` seeded once over the full population, inv #3). Empty by default
        // → this arm is skipped → the pinned config falls through to the IDENTICAL `Some`/`None` arms (hash-neutral).
        let mut sim = if !self.roster.is_empty() {
            let roster: Vec<sim_core::RosterEntry> = self
                .roster
                .iter()
                .map(|(b, n)| sim_core::RosterEntry {
                    name: b.name.clone(),
                    key: b.key.clone(),
                    genome: b.genome.clone(),
                    gp_map: OntologyMap::new(trait_map_for(&b.key)),
                    entity_count: *n,
                    role: sim_core::gp::role_from_override(b.trophic_role.as_deref(), &b.key),
                    host_key: b.host_key.clone(),
                })
                .collect();
            Simulation::reset_with_roster(&cfg, &self.env, roster)
        } else {
            match &self.species {
                Some(b) => Simulation::reset_with_roster(
                    &cfg,
                    &self.env,
                    vec![sim_core::RosterEntry {
                        name: b.name.clone(),
                        key: b.key.clone(),
                        genome: b.genome.clone(),
                        gp_map: OntologyMap::new(trait_map_for(&b.key)),
                        entity_count: cfg.entity_count,
                        // ADR-013 F4: honour the spec's `niche.trophic_role` override (E. coli → Decomposer),
                        // falling back to `role_for(key)` when absent/unrecognized (the DATA-driven role seam).
                        role: sim_core::gp::role_from_override(b.trophic_role.as_deref(), &b.key),
                        host_key: b.host_key.clone(),
                    }],
                ),
                None => Simulation::reset_with_env(&cfg, &self.env),
            }
        };
        let obs = sim.observe();
        self.sim = Some(sim);
        // ADR-019 S2: EXPAND the containment-knob immigration schedule for THIS run, off the off-stream
        // `IMMG_STREAM_BASE` family (ZERO `SimRng` draws). Sealed (the default) → an empty schedule → the
        // pinned config carries no events (hash-neutral). The world grid is `RESOURCE_DIMS` (the disc-bound).
        let (ww, wh) = sim_core::resource::RESOURCE_DIMS;
        self.schedule = sim_core::immigration::expand_schedule(
            seed,
            self.containment,
            &self.consortium_config,
            ww,
            wh,
        );
        self.schedule_cursor = 0;
        self.generation = 0;
        // ADR-017 S4: (re)initialize the OVERSIGHT per-run ledger / firewall / committed list and take the gen-0
        // credit baseline (a pure read-only sample). Skipped entirely when oversight is disabled (the default), so
        // a non-renderer reset is byte-AND-perf-identical. Off-hash: the sample draws no `SimRng`.
        if self.oversight.enabled {
            self.oversight.ledger = oversight::CreditLedger::new();
            self.oversight.firewall = firewall::EditFirewall::new();
            self.oversight.committed.clear();
            self.oversight.prev_sample = Some(self.sample_oversight());
        }
        obs
    }

    fn step(&mut self, action: Action) -> StepResult<Observation> {
        // ADR-019 S1: RegionInoculate is dispatched FIRST because it reads BOTH `self.consortium` (the loaded
        // contaminant resolver) and `self.sim` — borrowing them through one `&mut self.sim` local would
        // conflict. Handling it here keeps the rest of the match on a single `sim` borrow unchanged.
        if let Action::RegionInoculate {
            species_key,
            region,
            count,
            endow_j,
        } = &action
        {
            self.step_region_inoculate(species_key, *region, *count, *endow_j);
            let obs = self
                .sim
                .as_mut()
                .expect("GeneSimEnv::step called before reset")
                .observe();
            let reward = Self::reward_of(&obs);
            return StepResult {
                obs,
                reward,
                done: false,
            };
        }

        // ADR-019 S2: track the cumulative generation the schedule fires against (the Tick clock). Captured
        // before the `sim` borrow so it can be folded in after the borrow ends.
        let advance_by = if let Action::Advance(n) = &action {
            *n
        } else {
            0
        };

        let variants = &self.variants;
        let (on, off, thresholds) = (&self.on, &self.off, &self.thresholds);
        let sim = self
            .sim
            .as_mut()
            .expect("GeneSimEnv::step called before reset");

        match action {
            Action::Advance(n) => {
                // Advance N generations on the single seeded stream (inv. #3).
                sim.step(n);
                self.last_edit = None;
            }
            Action::ApplyEdit(edit) => {
                // Apply the edit to the CHOSEN species' genome, threading the run's own RNG (inv. #3, #6): the
                // edit draws ONLY from the single seeded stream handed in here, and draws the SAME way for any
                // species (so a `species: 0` edit is byte-identical to the pre-Variant-Lab behavior — the
                // pinned literal is unmoved). `edit.species` resolves to a `SpeciesId` at THIS boundary, the
                // SAME `species: u16 → SpeciesId` mapping the SP-3 interventions use (default 0 = the resident
                // primary). `with_species_genome_and_rng` re-expresses THAT species' phenotype afterwards, so
                // the edit changes only its subsequent selection dynamics (inv #6 species-granular).
                let sid = sim_core::SpeciesId::new(edit.species);
                let crispr_edit = Edit {
                    cas: edit.cas,
                    target: edit.target,
                    guide: edit.guide,
                };
                let outcome = sim.with_species_genome_and_rng(sid, |g, rng| {
                    apply_edit(g, &crispr_edit, variants, on, off, thresholds, rng)
                });
                self.last_edit = Some(outcome);
                self.last_region_edit = None;
            }
            Action::ApplyEditRegion(edit, region) => {
                // Region-scoped edit (ADR-011 S-D): the SAME gate as ApplyEdit, but it does NOT mutate the
                // genome — it returns a signed allele delta that sim-core adds to every in-region organism.
                // RNG cost is fixed (≤1 draw), independent of the brushed area (inv #3); region targets cells
                // only (inv #6).
                let crispr_edit = Edit {
                    cas: edit.cas,
                    target: edit.target,
                    guide: edit.guide,
                };
                let (outcome, covered) = sim.apply_edit_region(region.to_region(), |g, rng| {
                    let oc =
                        evaluate_region_edit(g, &crispr_edit, variants, on, off, thresholds, rng);
                    let delta = match oc {
                        RegionEditOutcome::Applied { genotype_delta, .. } => genotype_delta,
                        RegionEditOutcome::Failed { .. } => 0.0,
                    };
                    (oc, delta)
                });
                self.last_region_edit = Some((outcome, covered));
                self.last_edit = None;
            }
            // ── ADR-017 S6 OVERSIGHT actions at the bare `step` level ─────────────────────────────────────
            // The firewall BUFFERING + the off-thread oracle dispatch + the epoch-boundary drain live in the
            // harness DRIVER (`oversight::OversightEpisode`), NOT here in the single-threaded `World` step — so
            // the dispatch concurrency is in the env layer (inv #2), and `step` itself stays a pure CONSUMER of
            // the journaled action stream. `RequestEcoliEdit` draws ZERO `SimRng` (modeled on `Advance(0)`);
            // `CommitEcoliImpact` reads the committed INTEGER from the journal and sets the per-species edit
            // factor (still zero `SimRng` — the selection effect lands on the NEXT `Advance`). See
            // `docs/llm/proposals/ecoli-oversight-gameloop-draft.md`.
            Action::RequestEcoliEdit { .. } => {
                // The driver buffers the request + dispatches the oracle; the bare step is inert (no RNG, no
                // hashed mutation) so a journaled stream containing a request replays consistently.
            }
            Action::CommitEcoliImpact {
                species,
                growth_ratio_q,
                ..
            } => {
                // ADR-017 S6 (the load-bearing wire): the firewall's committed quantized `growth_ratio_q` crosses
                // into the hashed sim as a strictly-positive `[0.5,1.5]` per-species DEMAND + MINERALIZATION
                // factor (the core maps + clamps it). A wild-type ratio (1000) maps to exactly neutral (a no-op →
                // hash-unchanged — the pinned single-species PLANT run never commits a non-neutral factor); a
                // committed KO throttles the edited species. NO RNG draw (the committed integer is read straight
                // from the journal; selection consumes it next `Advance`). This is the one-way quantized crossing
                // the firewall pins — replay reads the integer from `actions.ndjson`, never re-solving FBA.
                sim.commit_species_edit(
                    sim_core::SpeciesId::new(species),
                    growth_ratio_q,
                    sim_core::EditEffect::Knockout,
                );
            }
            // ── SP-3 intervention tools — all read ONLY `self.sim` (a RESIDENT species/the substrate), so they
            // sit in the MAIN match on the single `sim` borrow (UNLIKE RegionInoculate, which also reads
            // `self.consortium`). Each resolves a raw `species: u16` → `SpeciesId` at the boundary, is RNG-free,
            // and books its J move through the core's named `intervention` tap (or, for Cull, a paired carcass
            // move). last_edit/last_region_edit reset to None (these are not CRISPR-edit outcomes).
            Action::RegionPcrAmplify {
                species,
                region,
                count,
                endow_j,
            } => {
                sim.region_pcr_amplify(
                    sim_core::SpeciesId::new(species),
                    region.to_region(),
                    count,
                    endow_j,
                );
                self.last_edit = None;
                self.last_region_edit = None;
            }
            Action::RegionCull {
                species,
                region,
                strength,
            } => {
                sim.region_cull(
                    sim_core::SpeciesId::new(species),
                    region.to_region(),
                    strength,
                );
                self.last_edit = None;
                self.last_region_edit = None;
            }
            Action::RegionNutrient {
                channel,
                region,
                amount_j,
            } => {
                sim.region_nutrient(channel, region.to_region(), amount_j);
                self.last_edit = None;
                self.last_region_edit = None;
            }
            Action::RegionToxin {
                channel,
                region,
                amount_milli,
            } => {
                sim.region_toxin(channel, region.to_region(), amount_milli);
                self.last_edit = None;
                self.last_region_edit = None;
            }
            // ADR-019 S1: RegionInoculate is dispatched ABOVE (it reads self.consortium + self.sim together);
            // it can never reach this match arm. The `unreachable!` documents that + keeps the match total.
            Action::RegionInoculate { .. } => {
                unreachable!("RegionInoculate dispatched before the sim borrow")
            }
        }

        let obs = sim.observe();
        let reward = Self::reward_of(&obs);
        // Advance the schedule's Tick clock after the `sim` borrow ends (ADR-019 S2 — `advance_by` is 0 for a
        // non-Advance action). The schedule itself is drained by the driver via `drain_due_inoculations`.
        self.generation += advance_by;
        // ADR-017 S4: fold this advance's earned credit into the OVERSIGHT ledger (off-hash — a pure integer fold
        // over RNG-free read-only projections). Only when enabled (the renderer path) and only for an actual
        // advance; the `sim` borrow above has ended, so `accrue_oversight` can re-borrow self.
        if self.oversight.enabled && advance_by > 0 {
            self.accrue_oversight();
        }
        StepResult {
            obs,
            reward,
            // Single-episode PoC env: never auto-terminates; the driver decides when to stop.
            done: false,
        }
    }

    fn seed(&mut self, seed: u64) {
        // Sets the master seed for the NEXT reset (does not disturb a run in progress).
        self.seed = seed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Look up a seed Cas variant id by name (the seed table is a build invariant).
    fn cas_id(name: &str) -> CasVariantId {
        default_cas_variants()
            .into_iter()
            .find(|v| v.name == name)
            .unwrap_or_else(|| panic!("seed table missing {name}"))
            .id
    }

    #[test]
    fn reset_step_advance_observe_cycle() {
        // AC: one reset → step(Advance) → observe cycle.
        let mut env = GeneSimEnv::new(200);
        let o0 = env.reset(7);
        assert_eq!(o0.generation, 0);
        assert_eq!(o0.population_size, 200);

        let r = env.step(Action::Advance(25));
        assert_eq!(r.obs.generation, 25);
        assert!((0.0..=1.0).contains(&r.obs.allele_freq));
        assert!((0.0..=1.0).contains(&r.reward));
        assert!(!r.done);
        // reward is defined as the allele frequency.
        assert_eq!(r.reward, r.obs.allele_freq);
    }

    #[test]
    fn set_species_runs_ecoli_off_gltacitrate() {
        // ADR-017: GeneSimEnv::set_species runs the E. coli genome through its trait map — population from the
        // niche (800), GrowthRate from gltA wild-type (1.0) — while a default env (no set_species) stays plant.
        use sim_core::Trait;
        let built = crate::species::load_species_file(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/species/ecoli.json"
        ))
        .expect("ecoli loads");

        let mut env = GeneSimEnv::new(500);
        env.set_species(built);
        let o = env.reset(7);
        assert_eq!(
            o.population_size, 800,
            "E. coli niche entity_count governs the population"
        );
        assert_eq!(
            o.phenotype.get(Trait::GrowthRate),
            Some(1.0),
            "E. coli GrowthRate comes from gltA wild-type"
        );

        let mut plant = GeneSimEnv::new(500);
        let op = plant.reset(7);
        assert_eq!(op.population_size, 500);
        assert!((op.phenotype.get(Trait::GrowthRate).unwrap() - 0.6).abs() < 1e-9);
    }

    /// Load a baked species spec from `data/species/<stem>.json` (the byte-mover boundary used in tests).
    fn load_stem(stem: &str) -> BuiltSpecies {
        crate::species::load_species_file(format!(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/species/{}.json"),
            stem
        ))
        .unwrap_or_else(|e| panic!("{stem}.json loads: {e}"))
    }

    #[test]
    fn one_entry_plant_roster_equals_single_species_path() {
        // SP-2.1 hash-neutrality / degenerate-case identity (ADR-020): a 1-row Plant/N roster is byte-identical
        // to the single-species plant path of the SAME N — name/key/role are display metadata, never hashed.
        let plant = load_stem("default");

        // Both envs carry the SAME fallback `entity_count` (500) so the metadata `config.entity_count` folded
        // into `run_stats().hash` matches; the roster's per-species count (500) equals it, isolating the test
        // to the SPAWN path (RosterEntry-from-N vs RosterEntry-from-1). 500 < default.json's niche (1000), so
        // `set_species` would adopt 1000 — force it back to 500 to drive an identical population on both paths.
        let mut roster_env = GeneSimEnv::new(500);
        roster_env.set_roster(vec![(plant.clone(), 500)]);
        roster_env.reset(7);
        let roster_hash = roster_env.run_stats().hash;

        let mut single_env = GeneSimEnv::new(500);
        single_env.set_species(plant);
        single_env.entity_count = 500; // override the adopted niche count back to 500 to match the roster entry
        single_env.reset(7);
        let single_hash = single_env.run_stats().hash;

        assert_eq!(
            roster_hash, single_hash,
            "a 1-entry plant/500 roster must be byte-identical to the single-species plant/500 path"
        );
    }

    #[test]
    fn set_roster_does_not_mutate_entity_count() {
        // SP-2.1 CRITICAL guard: unlike set_species, set_roster must NOT copy a per-species count into
        // self.entity_count — so a subsequent NON-roster reset is unchanged. Clearing the roster restores the
        // legacy fallback path exactly.
        let ecoli = load_stem("ecoli"); // niche count = 800

        let mut env = GeneSimEnv::new(300);
        env.set_roster(vec![(ecoli, 800)]);
        // The fallback count is untouched by set_roster.
        assert_eq!(
            env.entity_count, 300,
            "set_roster must not mutate entity_count"
        );

        // After clear_roster a default reset spawns the legacy 300, not 800.
        env.clear_roster();
        let o = env.reset(7);
        assert_eq!(
            o.population_size, 300,
            "clearing the roster restores the legacy entity_count fallback"
        );
    }

    #[test]
    fn three_species_roster_is_deterministic_same_seed() {
        // SP-2.1 determinism (inv #3): a fixed multi-species roster reset TWICE from one seed → equal hash.
        let roster = || {
            vec![
                (load_stem("default"), 500u32),
                (load_stem("ecoli"), 300u32),
                (load_stem("bdellovibrio"), 100u32),
            ]
        };
        let run = || {
            let mut env = GeneSimEnv::new(200);
            env.set_roster(roster());
            env.reset(2024);
            env.step(Action::Advance(20));
            env.run_stats().hash
        };
        assert_eq!(
            run(),
            run(),
            "a composed multi-species run must be deterministic"
        );
    }

    #[test]
    fn roster_order_and_count_are_load_bearing() {
        // SP-2.1: the roster is the load-bearing spawn key — a DIFFERENT order OR count yields a DIFFERENT hash,
        // proving set_roster actually drives the RNG stream (not a metadata no-op).
        let base = || {
            let mut env = GeneSimEnv::new(200);
            env.set_roster(vec![
                (load_stem("default"), 500u32),
                (load_stem("ecoli"), 300u32),
                (load_stem("bdellovibrio"), 100u32),
            ]);
            env.reset(2024);
            env.step(Action::Advance(20));
            env.run_stats().hash
        };
        let reordered = || {
            let mut env = GeneSimEnv::new(200);
            env.set_roster(vec![
                (load_stem("ecoli"), 300u32),
                (load_stem("default"), 500u32),
                (load_stem("bdellovibrio"), 100u32),
            ]);
            env.reset(2024);
            env.step(Action::Advance(20));
            env.run_stats().hash
        };
        let recounted = || {
            let mut env = GeneSimEnv::new(200);
            env.set_roster(vec![
                (load_stem("default"), 400u32), // 500 -> 400
                (load_stem("ecoli"), 300u32),
                (load_stem("bdellovibrio"), 100u32),
            ]);
            env.reset(2024);
            env.step(Action::Advance(20));
            env.run_stats().hash
        };
        let h = base();
        assert_ne!(
            h,
            reordered(),
            "a reordered roster must yield a different hash"
        );
        assert_ne!(
            h,
            recounted(),
            "a different per-species count must yield a different hash"
        );
    }

    #[test]
    fn empty_roster_reset_is_unchanged_default_plant() {
        // SP-2.1 hash-neutrality: with an empty roster the env reset is byte-identical to a plain default reset
        // (the pinned single-species-plant path is never perturbed by the additive roster field).
        let mut env = GeneSimEnv::new(200);
        env.reset(7);
        let baseline = env.run_stats().hash;

        let mut env2 = GeneSimEnv::new(200);
        env2.set_roster(vec![(load_stem("ecoli"), 800)]);
        env2.clear_roster(); // back to empty
        env2.reset(7);
        assert_eq!(
            env2.run_stats().hash,
            baseline,
            "an empty roster must leave the default-plant reset byte-identical"
        );
    }

    #[test]
    fn reset_step_apply_edit_observe_cycle() {
        // AC: one reset → step(ApplyEdit) → observe cycle. The edit targets the SPECIES genome (locus 0).
        let mut env = GeneSimEnv::new(100);
        env.reset(11);

        // A guide present in the growth locus with an adjacent NGG PAM → a clean targetable edit.
        let edit = EditAction {
            cas: cas_id("SpCas9"),
            target: LocusId(0),
            guide: GuideSequence::new(*b"ACGTGGACGTTTTAGGCCGG").unwrap(),
            species: 0,
        };
        let r = env.step(Action::ApplyEdit(edit));
        assert!(!r.done);
        // The action produced an explicit edit outcome (success or failure — never a silent no-op).
        assert!(env.last_edit().is_some());
        // The species genome stays valid after the edit (SPEC §10.4 — no invalid genome).
        assert!(env
            .observe()
            .phenotype
            .values
            .iter()
            .all(|(_, v)| (0.0..=1.0).contains(v)));
    }

    #[test]
    fn same_seed_and_actions_reproduce_observations() {
        // Determinism AC (inv. #3): same seed + same action sequence ⇒ identical observation sequence.
        let actions = || {
            vec![
                Action::Advance(10),
                Action::ApplyEdit(EditAction {
                    cas: cas_id("SpCas9"),
                    target: LocusId(0),
                    guide: GuideSequence::new(*b"ACGTGGACGTTTTAGGCCGG").unwrap(),
                    species: 0,
                }),
                Action::Advance(20),
                Action::ApplyEdit(EditAction {
                    cas: cas_id("AsCas12a"),
                    target: LocusId(1),
                    guide: GuideSequence::new(*b"TTTACCGGTTTAGGGCAAAC").unwrap(),
                    species: 0,
                }),
                Action::Advance(15),
            ]
        };

        let run = || {
            let mut env = GeneSimEnv::new(300);
            let mut seq = vec![env.reset(2024)];
            for a in actions() {
                seq.push(env.step(a).obs);
            }
            seq
        };

        let a = run();
        let b = run();
        assert_eq!(
            a, b,
            "same seed + actions must yield identical observations"
        );
    }

    #[test]
    fn seed_sets_the_next_reset_master_seed() {
        // `seed()` changes the master seed used by the next reset; different seeds diverge.
        let mut env = GeneSimEnv::new(200);
        env.reset(1);
        let o1 = env.step(Action::Advance(50)).obs;

        env.seed(2);
        env.reset(env.seed); // reset honours the seed set via `seed()`
        let o2 = env.step(Action::Advance(50)).obs;
        assert_ne!(
            o1.allele_freq.to_bits(),
            o2.allele_freq.to_bits(),
            "different seeds should diverge"
        );
    }

    /// Action-space granularity guard (invariant #6 — the load-bearing rule of S3.1).
    ///
    /// This is a compile-time assertion in test form: the only ways to construct an [`Action`] are
    /// [`Action::Advance`] (time) and [`Action::ApplyEdit`] (species genome). Neither carries an
    /// organism handle, so the type system makes a per-organism action *unrepresentable*. If anyone
    /// later adds a per-organism variant, this match stops compiling — forcing a review against inv. #6.
    #[test]
    fn action_space_is_species_granular() {
        let a = Action::ApplyEdit(EditAction {
            cas: cas_id("SpCas9"),
            target: LocusId(0),
            guide: GuideSequence::new(*b"ACGTGG").unwrap(),
            species: 0,
        });
        match a {
            Action::Advance(_) => {}
            // EditAction targets a species LocusId + a `species: u16` registry ordinal — never an
            // organism/entity id (inv. #6). The destructure names `species` so a future organism-handle
            // field would stop this compiling and force a review.
            Action::ApplyEdit(EditAction {
                target: _,
                species: _,
                ..
            }) => {}
            // ApplyEditRegion targets a species LocusId + a CELL region (cx/cy/radius) — still no organism
            // handle, so per-organism targeting stays unrepresentable (ADR-011 invariant-#6 ruling).
            Action::ApplyEditRegion(EditAction { target: _, .. }, RegionSpec { .. }) => {}
            // ADR-017 S5: both oversight variants target a SPECIES (`species: u16`, → `SpeciesId` at S5) + a
            // species LocusId — never a per-organism handle, so the deep-edit request stays at the
            // operator/species granularity ceiling (inv. #6). The destructure names every field so a future
            // organism-handle field would stop this compiling and force a review.
            Action::RequestEcoliEdit {
                species: _,
                locus: _,
                edit_kind: _,
                due_epoch: _,
                req_id: _,
            } => {}
            Action::CommitEcoliImpact {
                species: _,
                req_id: _,
                due_epoch: _,
                slipped_from: _,
                content_hash: _,
                growth_ratio_q: _,
                exchange_deltas: _,
            } => {}
            // ADR-019 S1: RegionInoculate targets a SPECIES (`species_key`) + a CELL region (`region`) — never
            // a per-organism handle, so the contamination tool stays at the operator/species granularity
            // ceiling (inv #6). The destructure names every field so a future organism-handle field would stop
            // this compiling and force a review.
            Action::RegionInoculate {
                species_key: _,
                region: RegionSpec { .. },
                count: _,
                endow_j: _,
            } => {}
            // SP-3: the four intervention tools target a SPECIES (`species: u16`, → `SpeciesId` at the boundary)
            // or the SUBSTRATE (a `channel`) + a CELL region (`region`) — never a per-organism handle, so they
            // stay at the operator/species granularity ceiling (inv #6). The destructure names every field so a
            // future organism-handle field (e.g. an `OrgId`) would stop this compiling and force a review.
            Action::RegionPcrAmplify {
                species: _,
                region: RegionSpec { .. },
                count: _,
                endow_j: _,
            } => {}
            Action::RegionCull {
                species: _,
                region: RegionSpec { .. },
                strength: _,
            } => {}
            Action::RegionNutrient {
                channel: _,
                region: RegionSpec { .. },
                amount_j: _,
            } => {}
            Action::RegionToxin {
                channel: _,
                region: RegionSpec { .. },
                amount_milli: _,
            } => {}
        }
    }

    #[test]
    fn region_edit_covers_organisms_and_is_deterministic() {
        // ADR-011 S-D: a region edit covers a nonzero set of organisms, reproduces bit-for-bit for the same
        // (seed, edit, region), and (on a passing gate) shifts the population allele_freq vs an un-edited control.
        let mk = || {
            let mut e = GeneSimEnv::new(600);
            e.reset(7);
            e
        };
        let action = || {
            Action::ApplyEditRegion(
                EditAction {
                    cas: cas_id("SpCas9"),
                    target: LocusId(0),
                    guide: GuideSequence::new(*b"ACGTGGACGTTTTAGGCCGG").unwrap(),
                    species: 0,
                },
                RegionSpec {
                    cx: 16,
                    cy: 16,
                    radius: 8,
                },
            )
        };
        let mut a = mk();
        a.step(action());
        let mut b = mk();
        b.step(action());
        assert_eq!(
            a.observe().allele_freq,
            b.observe().allele_freq,
            "region edit must be deterministic for a fixed seed/edit/region"
        );
        let (_outcome, covered) = a
            .last_region_edit()
            .expect("region edit should be recorded");
        assert!(*covered > 0, "the brush should cover some organisms");
        // A passing region edit moves the field allele_freq away from the un-edited control.
        let control = mk().observe().allele_freq;
        assert_ne!(
            a.observe().allele_freq,
            control,
            "a covered edit should change allele_freq"
        );
    }

    #[test]
    fn action_and_edit_action_round_trip_through_serde() {
        // AC (S3.2): an Action / EditAction (incl. an ApplyEdit with a real guide) survives a JSON
        // round-trip — the `actions.ndjson` line encoding (SPEC §5).
        let advance = Action::Advance(42);
        let j = serde_json::to_string(&advance).unwrap();
        assert_eq!(j, "{\"Advance\":42}");
        assert_eq!(serde_json::from_str::<Action>(&j).unwrap(), advance);

        let edit = Action::ApplyEdit(EditAction {
            cas: cas_id("SpCas9"),
            target: LocusId(0),
            guide: GuideSequence::new(*b"ACGTGGACGTTTTAGGCCGG").unwrap(),
            species: 3,
        });
        let j = serde_json::to_string(&edit).unwrap();
        // The guide rides as its validated ACGT string; ids as bare integers; the chosen species rides too.
        assert!(
            j.contains("\"ACGTGGACGTTTTAGGCCGG\""),
            "guide string missing: {j}"
        );
        assert!(j.contains("\"species\":3"), "chosen species missing: {j}");
        assert_eq!(serde_json::from_str::<Action>(&j).unwrap(), edit);

        // BACK-COMPAT (Variant-Lab A): a pre-Variant-Lab `actions.ndjson` line has NO `species` field. The
        // `#[serde(default)]` makes it deserialize to `species: 0` (the resident primary) — EXACTLY today's
        // behavior — so an old recorded journal replays byte-identically.
        let old_line = r#"{"ApplyEdit":{"cas":0,"target":0,"guide":"ACGTGGACGTTTTAGGCCGG"}}"#;
        let parsed = serde_json::from_str::<Action>(old_line).unwrap();
        match parsed {
            Action::ApplyEdit(e) => assert_eq!(
                e.species, 0,
                "an old line without `species` must default to the primary (0)"
            ),
            other => panic!("expected ApplyEdit, got {other:?}"),
        }
    }

    #[test]
    fn export_species_json_round_trips_to_the_live_phenotype() {
        // Variant-Lab Slice B (the save→reseed contract): exporting species S's CURRENT genome + niche as
        // SpeciesSpec JSON, then rebuilding through the SAME res:// loader (build_species_from_str), yields a
        // BuiltSpecies whose EXPRESSED PHENOTYPE is identical to the live species' phenotype — across the full
        // JSON boundary, after an edit step (export reflects the CURRENT genome, whatever the gate decided).
        use sim_core::gp::{trait_map_for, GenotypePhenotypeMap, OntologyMap, Trait};

        let mut env = GeneSimEnv::new(300);
        env.set_species(load_stem("ecoli")); // key "ecoli-core", role Decomposer
        env.reset(7);

        // Edit the live species genome (the export must read whatever the genome is NOW).
        env.step(Action::ApplyEdit(EditAction {
            cas: cas_id("SpCas9"),
            target: LocusId(0),
            guide: GuideSequence::new(*b"ACGTGGACGTTTTAGGCCGG").unwrap(),
            species: 0,
        }));

        // The LIVE phenotype the renderer shows for species 0.
        let live = env.observe_all()[0].phenotype.clone();

        // EXPORT → rebuild through the res:// boundary the loader uses.
        let json = env.export_species_json(0).expect("export species 0");
        let rebuilt =
            crate::species::build_species_from_str(&json).expect("rebuild from exported JSON");

        // The niche carried the key + role, so the reseed resolves the SAME trait map + role.
        assert_eq!(rebuilt.key, "ecoli-core");
        assert_eq!(rebuilt.trophic_role.as_deref(), Some("decomposer"));

        // Expressed through ITS key's trait map, the rebuilt genome reproduces the live phenotype EXACTLY.
        let reseeded = OntologyMap::new(trait_map_for(&rebuilt.key)).express(&rebuilt.genome);
        assert_eq!(
            reseeded, live,
            "save→reseed must reproduce the live species' expressed phenotype"
        );
        assert!(
            reseeded.get(Trait::GrowthRate).is_some(),
            "the E. coli GrowthRate trait expresses under the reseeded map"
        );
    }

    #[test]
    fn export_species_json_is_hash_neutral_and_guarded() {
        // Read-only (inv #3): exporting between reset and advance draws ZERO SimRng and mutates nothing, so the
        // subsequent run hashes IDENTICALLY to a run that never exported. Plus the boundary guards: None before
        // reset and for an out-of-range species id (the renderer maps those to an empty GString + godot_error).
        let baseline = {
            let mut env = GeneSimEnv::new(200);
            env.reset(7);
            env.step(Action::Advance(20));
            env.run_stats().hash
        };
        let with_export = {
            let mut env = GeneSimEnv::new(200);
            env.reset(7);
            let _ = env.export_species_json(0).expect("export mid-run");
            env.step(Action::Advance(20));
            env.run_stats().hash
        };
        assert_eq!(
            baseline, with_export,
            "export must be hash-neutral (zero SimRng, no mutation)"
        );

        // Guards.
        let mut fresh = GeneSimEnv::new(50);
        assert!(
            fresh.export_species_json(0).is_none(),
            "None before reset (the boundary returns an empty GString)"
        );
        fresh.reset(1);
        assert!(
            fresh.export_species_json(0).is_some(),
            "the primary species exports after reset"
        );
        assert!(
            fresh.export_species_json(7).is_none(),
            "an out-of-range species id → None"
        );
    }

    #[test]
    fn ecoli_oversight_actions_round_trip_and_are_back_compat() {
        // ADR-017 S5 INERT SCAFFOLDING. The two new variants journal to `actions.ndjson` and survive a
        // serde round-trip; the externally-tagged enum keeps every EXISTING line byte-identical (purely
        // additive). Proves the back-compat discipline before the firewall is wired.

        // (1) The new variants round-trip exactly (including the inline quantized payload + Option<u32>).
        let req = Action::RequestEcoliEdit {
            species: 1,
            locus: LocusId(7),
            edit_kind: crispr::EditKind::Knockdown,
            due_epoch: 42,
            req_id: 0,
        };
        let jr = serde_json::to_string(&req).unwrap();
        assert!(jr.starts_with("{\"RequestEcoliEdit\":"), "tag wrong: {jr}");
        assert_eq!(serde_json::from_str::<Action>(&jr).unwrap(), req);

        let commit = Action::CommitEcoliImpact {
            species: 1,
            req_id: 0,
            due_epoch: 44,
            slipped_from: Some(42),
            content_hash: 0xdead_beef_0000_0001,
            growth_ratio_q: 30_000,
            exchange_deltas: vec![(3, -120), (11, 88)],
        };
        let jc = serde_json::to_string(&commit).unwrap();
        assert!(jc.starts_with("{\"CommitEcoliImpact\":"), "tag wrong: {jc}");
        assert_eq!(serde_json::from_str::<Action>(&jc).unwrap(), commit);

        // (2) BACK-COMPAT: a pre-S5 `actions.ndjson` line still deserializes to the SAME existing variant —
        // adding variants changes nothing about how old lines parse (the load-bearing serde discipline).
        assert_eq!(
            serde_json::from_str::<Action>("{\"Advance\":10}").unwrap(),
            Action::Advance(10)
        );

        // (3) The new variants are STRICT NO-OPS in `step`: stepping them draws zero RNG and leaves the
        // observation generation unchanged (modeled on `Advance(0)`, not on `ApplyEdit`).
        let mut env = GeneSimEnv::new(64);
        env.reset(7);
        let gen_before = env.observe().generation;
        env.step(req);
        env.step(commit);
        assert_eq!(
            env.observe().generation,
            gen_before,
            "inert oversight actions must not advance the sim"
        );
    }

    // ── ADR-019 contamination & immigration ────────────────────────────────────────────────────────────

    /// A synthetic CONTAMINANT [`BuiltSpecies`] (key `"contaminant"`, decomposer role) built off the wired
    /// sample genome — no data-agent JSON needed for the harness-level wiring tests.
    fn contaminant_built() -> BuiltSpecies {
        use genome::spec::SpeciesSpec;
        let mut spec =
            SpeciesSpec::from_genome(&genome::sample_genome(), "contaminant", "Contaminant");
        spec.niche.trophic_role = Some("decomposer".to_string());
        spec.build().expect("contaminant builds")
    }

    fn inoculate_action() -> Action {
        Action::RegionInoculate {
            species_key: "contaminant".to_string(),
            region: RegionSpec {
                cx: 16,
                cy: 16,
                radius: 6,
            },
            count: 10,
            endow_j: 800_000,
        }
    }

    #[test]
    fn region_inoculate_action_round_trips_through_serde_back_compat() {
        // ADR-019 S1: the new externally-tagged variant round-trips, and a pre-ADR-019 line still parses
        // unchanged (the additive-serde discipline that keeps existing actions.ndjson byte-identical).
        let inoc = inoculate_action();
        let j = serde_json::to_string(&inoc).unwrap();
        assert!(j.starts_with("{\"RegionInoculate\":"), "tag wrong: {j}");
        assert_eq!(serde_json::from_str::<Action>(&j).unwrap(), inoc);
        // Back-compat: an existing Advance line is unaffected by adding the variant.
        assert_eq!(
            serde_json::from_str::<Action>("{\"Advance\":10}").unwrap(),
            Action::Advance(10)
        );
    }

    #[test]
    fn region_inoculate_conserves_j_and_is_replay_reproducible() {
        // ADR-019 S1: a journaled RegionInoculate (resolved against a loaded contaminant) lifts the immigration
        // tap and is replay-reproducible — the SAME (seed, register, inoculate, advance) yields an identical
        // hash. An UNRESOLVED key (no contaminant loaded) is a clean no-op (J unchanged).
        let run = |load: bool| -> (i64, u64) {
            let mut env = GeneSimEnv::new(300);
            if load {
                env.register_contaminant(contaminant_built());
            }
            env.reset(2024);
            env.step(inoculate_action());
            let immig = env.immigration_minted();
            env.step(Action::Advance(20));
            (immig, env.run_stats().hash)
        };
        let (immig_loaded, hash_a) = run(true);
        let (immig_unloaded, _hash_b) = run(false);
        assert!(
            immig_loaded > 0,
            "a resolved inoculation mints from the immigration tap"
        );
        assert_eq!(
            immig_unloaded, 0,
            "an unresolved key is a clean no-op (nothing minted)"
        );
        // Replay-reproducible: the SAME loaded sequence reproduces the hash bit-for-bit.
        let (_immig2, hash_a2) = run(true);
        assert_eq!(
            hash_a, hash_a2,
            "an inoculated run must replay bit-identically"
        );
    }

    // ── SP-3 intervention Actions (PCR-amplify / cull / nutrient / toxin) ──────────────────────────────────

    fn sp3_region() -> RegionSpec {
        RegionSpec {
            cx: 16,
            cy: 16,
            radius: 20,
        }
    }

    #[test]
    fn sp3_actions_round_trip_through_serde_back_compat() {
        // SP-3: each new externally-tagged variant round-trips, and a pre-SP-3 line still parses unchanged (the
        // additive-serde discipline that keeps existing actions.ndjson byte-identical).
        let cases: Vec<(Action, &str)> = vec![
            (
                Action::RegionPcrAmplify {
                    species: 0,
                    region: sp3_region(),
                    count: 8,
                    endow_j: 900_000,
                },
                "{\"RegionPcrAmplify\":",
            ),
            (
                Action::RegionCull {
                    species: 0,
                    region: sp3_region(),
                    strength: 500,
                },
                "{\"RegionCull\":",
            ),
            (
                Action::RegionNutrient {
                    channel: 2,
                    region: sp3_region(),
                    amount_j: 8_000_000,
                },
                "{\"RegionNutrient\":",
            ),
            (
                Action::RegionToxin {
                    channel: 0,
                    region: sp3_region(),
                    amount_milli: 5_000_000,
                },
                "{\"RegionToxin\":",
            ),
        ];
        for (action, tag) in cases {
            let j = serde_json::to_string(&action).unwrap();
            assert!(j.starts_with(tag), "tag wrong: {j}");
            assert_eq!(serde_json::from_str::<Action>(&j).unwrap(), action);
        }
        // Back-compat: existing lines are unaffected by adding the variants.
        assert_eq!(
            serde_json::from_str::<Action>("{\"Advance\":10}").unwrap(),
            Action::Advance(10)
        );
        assert_eq!(
            serde_json::from_str::<Action>("{\"RegionInoculate\":{\"species_key\":\"bacillus\",\"region\":{\"cx\":1,\"cy\":2,\"radius\":3},\"count\":4,\"endow_j\":5}}").unwrap(),
            Action::RegionInoculate {
                species_key: "bacillus".to_string(),
                region: RegionSpec { cx: 1, cy: 2, radius: 3 },
                count: 4,
                endow_j: 5,
            }
        );
    }

    #[test]
    fn sp3_interventions_are_hash_neutral_when_inert() {
        // SP-3 hash-neutrality (the lib.rs:1197 inoculation template): a run that issues NO SP-3 intervention is
        // byte-identical to a plain run — the four Actions are inert until invoked, the intervention tap is zero
        // at rest. Mirrors `inoculation_system_is_hash_neutral_when_inert`.
        let plain = || {
            let mut env = GeneSimEnv::new(400);
            env.reset(13_679_457_532_755_275_413);
            env.step(Action::Advance(40));
            (env.run_stats().hash, env.intervention_minted())
        };
        let (h, tap) = plain();
        let (h2, tap2) = plain();
        assert_eq!(h, h2, "an inert intervention surface must be reproducible");
        assert_eq!(tap, 0, "no intervention fired → zero intervention tap");
        assert_eq!(tap2, 0);
    }

    #[test]
    fn sp3_pcr_amplify_conserves_j_and_is_replay_reproducible() {
        // SP-3 PCR twin (the lib.rs:1124 template): a journaled RegionPcrAmplify lifts the intervention tap and
        // is replay-reproducible — the SAME (seed, amplify, advance) yields an identical hash.
        let run = || -> (i64, u64) {
            let mut env = GeneSimEnv::new(300);
            env.reset(2024);
            env.step(Action::RegionPcrAmplify {
                species: 0, // the resident primary species (always present after reset)
                region: sp3_region(),
                count: 12,
                endow_j: 700_000,
            });
            let tap = env.intervention_minted();
            env.step(Action::Advance(20));
            (tap, env.run_stats().hash)
        };
        let (tap, hash) = run();
        assert!(
            tap > 0,
            "a PCR amplification mints from the intervention tap"
        );
        assert_eq!(
            run().1,
            hash,
            "a PCR-amplified run must replay bit-identically"
        );
    }

    #[test]
    fn sp3_cull_conserves_j_and_is_replay_reproducible() {
        // SP-3 cull twin: a journaled RegionCull mints NOTHING (carcass→detritus is a paired bucket move) and is
        // replay-reproducible.
        let run = || -> (i64, u64) {
            let mut env = GeneSimEnv::new(300);
            env.reset(2024);
            env.step(Action::RegionCull {
                species: 0,
                region: sp3_region(),
                strength: 400,
            });
            let tap = env.intervention_minted();
            env.step(Action::Advance(20));
            (tap, env.run_stats().hash)
        };
        let (tap, hash) = run();
        assert_eq!(
            tap, 0,
            "an antibiotic cull mints no J (it never touches the intervention tap)"
        );
        assert_eq!(run().1, hash, "a culled run must replay bit-identically");
    }

    #[test]
    fn sp3_nutrient_conserves_j_and_is_replay_reproducible() {
        // SP-3 nutrient twin: a journaled RegionNutrient lifts the intervention tap and is replay-reproducible.
        let run = || -> (i64, u64) {
            let mut env = GeneSimEnv::new(300);
            env.reset(2024);
            env.step(Action::RegionNutrient {
                channel: 2, // detritus
                region: sp3_region(),
                amount_j: 8_000_000,
            });
            let tap = env.intervention_minted();
            env.step(Action::Advance(20));
            (tap, env.run_stats().hash)
        };
        let (tap, hash) = run();
        assert_eq!(
            tap, 8_000_000,
            "a nutrient feed mints exactly amount_j from the intervention tap"
        );
        assert_eq!(run().1, hash, "a fed run must replay bit-identically");
    }

    #[test]
    fn sp3_toxin_conserves_j_and_is_replay_reproducible() {
        // SP-3 toxin twin: a journaled RegionToxin lifts the intervention tap and is replay-reproducible.
        let run = || -> (i64, u64) {
            let mut env = GeneSimEnv::new(300);
            env.reset(2024);
            env.step(Action::RegionToxin {
                channel: 0, // toxin
                region: sp3_region(),
                amount_milli: 5_000_000,
            });
            let tap = env.intervention_minted();
            env.step(Action::Advance(20));
            (tap, env.run_stats().hash)
        };
        let (tap, hash) = run();
        assert_eq!(
            tap, 5_000_000,
            "a toxin spike mints exactly amount_milli from the intervention tap"
        );
        assert_eq!(run().1, hash, "a spiked run must replay bit-identically");
    }

    #[test]
    fn containment_schedule_is_identical_for_same_seed_knob_config() {
        // ADR-019 S2: the schedule is a PURE function of (seed, ContainmentLevel, ConsortiumConfig) — expanded
        // off the off-stream IMMG family with ZERO SimRng draws. Same inputs → identical schedule; a different
        // seed diverges.
        let cfg = sim_core::ConsortiumConfig {
            species_keys: vec!["bacillus".to_string(), "pseudomonas".to_string()],
            radius: 4,
            endow_j: 500_000,
            horizon: 100,
        };
        let schedule = |seed: u64| -> Vec<sim_core::ScheduledInoculation> {
            let mut env = GeneSimEnv::new(200);
            env.set_containment(sim_core::ContainmentLevel::Lab, cfg.clone());
            env.reset(seed);
            env.immigration_schedule().to_vec()
        };
        assert!(!schedule(7).is_empty(), "Lab containment schedules events");
        assert_eq!(
            schedule(7),
            schedule(7),
            "same seed+knob+config → identical schedule"
        );
        assert_ne!(schedule(7), schedule(8), "a different seed must diverge");
    }

    #[test]
    fn default_containment_is_sealed_and_schedule_empty() {
        // The default (Sealed) → an EMPTY schedule → the env issues no immigration events (hash-neutral path).
        let mut env = GeneSimEnv::new(200);
        env.reset(42);
        assert!(
            env.immigration_schedule().is_empty(),
            "default Sealed containment must carry no scheduled events"
        );
        // Draining at any generation yields nothing.
        assert!(env.drain_due_inoculations(1000).is_empty());
    }

    #[test]
    fn inoculation_system_is_hash_neutral_when_inert() {
        // ADR-019 hash-neutrality: a run that issues NO RegionInoculate (and whose containment is the default
        // Sealed) is byte-identical to a plain run — the new Action is inert until invoked, the immigration tap
        // is zero at rest, and the empty schedule fires nothing. Registering a contaminant that is never
        // inoculated is ALSO inert (it only seeds the resolver). Mirrors the SP-3 inert-until-invoked argument.
        let plain = || {
            let mut env = GeneSimEnv::new(400);
            env.reset(13_679_457_532_755_275_413);
            env.step(Action::Advance(40));
            env.run_stats().hash
        };
        let with_inert_consortium = || {
            let mut env = GeneSimEnv::new(400);
            // Load a contaminant + arm the knob, but Sealed → empty schedule, and never fire an action.
            env.register_contaminant(contaminant_built());
            env.set_containment(
                sim_core::ContainmentLevel::Sealed,
                sim_core::ConsortiumConfig::default_mode_a(),
            );
            env.reset(13_679_457_532_755_275_413);
            assert!(
                env.immigration_schedule().is_empty(),
                "Sealed → empty schedule"
            );
            env.step(Action::Advance(40));
            assert_eq!(
                env.immigration_minted(),
                0,
                "no inoculation → zero immigration tap"
            );
            env.run_stats().hash
        };
        assert_eq!(
            plain(),
            with_inert_consortium(),
            "an inert (un-invoked) contamination system must not move the run hash"
        );
    }

    #[test]
    fn scheduled_events_drain_at_their_epochs_in_order() {
        // ADR-019 S2: the schedule is drained as the env advances — every event whose due_epoch has passed is
        // returned as a journaled RegionInoculate, in schedule order, exactly once.
        let cfg = sim_core::ConsortiumConfig {
            species_keys: vec!["bacillus".to_string()],
            radius: 3,
            endow_j: 400_000,
            horizon: 50,
        };
        let mut env = GeneSimEnv::new(200);
        env.set_containment(sim_core::ContainmentLevel::Open, cfg);
        env.reset(99);
        let total = env.immigration_schedule().len();
        assert!(total > 0);
        // Drain progressively; each event fires once, in due_epoch order, never before its epoch.
        let mut fired = 0usize;
        for gen in 1..=50u64 {
            let due = env.drain_due_inoculations(gen);
            for a in &due {
                assert!(matches!(a, Action::RegionInoculate { .. }));
            }
            fired += due.len();
        }
        assert_eq!(fired, total, "every scheduled event drains exactly once");
        // Draining again past the end yields nothing (the cursor is exhausted).
        assert!(env.drain_due_inoculations(10_000).is_empty());
    }

    #[cfg(feature = "proptest")]
    mod prop {
        use super::*;
        use proptest::prelude::*;

        /// Drive a fixed action sequence off `seed`, returning the `(reward, observation)` of each step
        /// plus the initial observation. Pure helper — no proptest macros inside (so it stays a plain fn).
        fn drive(seed: u64) -> (Observation, Vec<(f64, Observation)>) {
            let mut env = GeneSimEnv::new(150);
            let initial = env.reset(seed);
            let steps = [
                Action::Advance(8),
                Action::ApplyEdit(EditAction {
                    cas: cas_id("SpCas9"),
                    target: LocusId(0),
                    guide: GuideSequence::new(*b"ACGTGGACGTTTTAGGCCGG").unwrap(),
                    species: 0,
                }),
                Action::Advance(12),
            ]
            .into_iter()
            .map(|a| {
                let r = env.step(a);
                (r.reward, r.obs)
            })
            .collect();
            (initial, steps)
        }

        proptest! {
            // For ANY seed, replaying the same (seed, action-sequence) twice yields identical
            // observations, and every reward / allele_freq stays in [0, 1] (inv. #3; SPEC §10.4).
            #[test]
            fn replay_is_deterministic_for_any_seed(seed in any::<u64>()) {
                let a = drive(seed);
                let b = drive(seed);
                for (reward, obs) in &a.1 {
                    prop_assert!((0.0..=1.0).contains(reward));
                    prop_assert!((0.0..=1.0).contains(&obs.allele_freq));
                }
                prop_assert_eq!(a, b);
            }
        }
    }
}
