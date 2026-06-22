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
pub mod firewall;
pub mod oversight;
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

/// A CRISPR edit expressed at **species** granularity (invariant #6): which Cas variant, which locus on
/// the species genome, and the guide. It carries **no organism handle** — it edits the one shared
/// species genome, never an individual.
///
/// Resolved through [`crispr::apply_edit`] against the env's Cas-variant table and the species genome.
///
/// Serde-(de)serializable so it can be logged to `actions.ndjson` and replayed bit-identically (SPEC
/// §5/§6): `cas`/`target` ride as their integer ids; `guide` as its validated ACGT string (a malformed
/// guide in a log fails to deserialize — see [`crispr::GuideSequence`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditAction {
    /// Which Cas variant performs the edit (resolved by id against the variant table).
    pub cas: CasVariantId,
    /// The species-genome locus to target (resolved against `genome.loci` by id).
    pub target: LocusId,
    /// The guide (spacer) sequence.
    pub guide: GuideSequence,
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
        }
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
        let mut sim = match &self.species {
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
                }],
            ),
            None => Simulation::reset_with_env(&cfg, &self.env),
        };
        let obs = sim.observe();
        self.sim = Some(sim);
        obs
    }

    fn step(&mut self, action: Action) -> StepResult<Observation> {
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
                // Apply the edit to the SPECIES genome, threading the run's own RNG (inv. #3, #6): the
                // edit draws ONLY from the single seeded stream handed in here. `with_genome_and_rng`
                // re-expresses phenotype afterwards, so the edit changes subsequent selection dynamics.
                let crispr_edit = Edit {
                    cas: edit.cas,
                    target: edit.target,
                    guide: edit.guide,
                };
                let outcome = sim.with_genome_and_rng(|g, rng| {
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
        }

        let obs = sim.observe();
        let reward = Self::reward_of(&obs);
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
                }),
                Action::Advance(20),
                Action::ApplyEdit(EditAction {
                    cas: cas_id("AsCas12a"),
                    target: LocusId(1),
                    guide: GuideSequence::new(*b"TTTACCGGTTTAGGGCAAAC").unwrap(),
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
        });
        match a {
            Action::Advance(_) => {}
            // EditAction targets a species LocusId — never an organism/entity id (inv. #6).
            Action::ApplyEdit(EditAction { target: _, .. }) => {}
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
        });
        let j = serde_json::to_string(&edit).unwrap();
        // The guide rides as its validated ACGT string; ids as bare integers.
        assert!(
            j.contains("\"ACGTGGACGTTTTAGGCCGG\""),
            "guide string missing: {j}"
        );
        assert_eq!(serde_json::from_str::<Action>(&j).unwrap(), edit);
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
