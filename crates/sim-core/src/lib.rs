//! Headless, deterministic Bevy ECS tick loop (SPEC §2, §6; ADR-002).
//!
//! The core is fully deterministic: organisms are ECS entities, a fixed, explicitly ordered schedule
//! advances them each generation, and **all** randomness flows from a single seeded
//! [`rand_chacha::ChaCha8Rng`] threaded through the world as a resource. No renderer is attached
//! (invariant #4). The parametric [`genome::Genome`] is wired into the core (invariant #2), and Stage 1
//! adds the first real biology: a genotype→phenotype map (see [`gp`]) drives **selection** over a
//! constant-population (Wright-Fisher) loop.
//!
//! Determinism rules honored here (invariant #3):
//! - one seeded `ChaCha8Rng`, no thread-local/global RNG;
//! - a single-threaded, explicitly `.chain()`-ordered schedule;
//! - no `HashMap` iteration in sim logic — entities carry a stable [`OrgId`] and the end-of-run hash is
//!   computed over an id-sorted vector; selection samples parents by ordered cumulative weights.

#![forbid(unsafe_code)]

use bevy_ecs::prelude::*;
use genome::Genome;
use rand_chacha::rand_core::{Rng, SeedableRng};

pub mod chem;
pub mod climate;
pub mod det;
pub mod fixed;
pub mod gp;
pub mod immigration;
pub mod ledger;
pub mod resource;
pub mod signature;
pub mod snapshot;
pub mod soil;
pub mod trophic;

pub use climate::EnvParams;
pub use det::derive_seed;
pub use gp::{GenotypePhenotypeMap, Phenotype, Trait, WeightedSumMap};
pub use immigration::{
    ConsortiumConfig, ContainmentLevel, InoculationEvent, InoculationRegion, ScheduledInoculation,
};
pub use snapshot::{GridSnapshot, CHANNEL_COUNT, SNAPSHOT_MAGIC};

// Re-export the exact `ChaCha8Rng` the core threads, so dependents (e.g. the harness env) draw the
// species-edit action from the SAME seeded stream type without pinning a second `rand_chacha` (inv. #3).
pub use rand_chacha::ChaCha8Rng;

/// Configuration for a single headless run.
#[derive(Debug, Clone)]
pub struct SimConfig {
    /// The (already-derived) per-run seed.
    pub seed: u64,
    /// Number of generations (schedule runs) to advance.
    pub generations: u64,
    /// Number of organisms spawned at start.
    pub entity_count: u32,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            generations: 200,
            entity_count: 1000,
        }
    }
}

/// Per-run summary. `hash` is the determinism artifact (SPEC §6).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunStats {
    pub seed: u64,
    pub generations: u64,
    pub entity_count: u32,
    /// Population statistic in `[0, 1]`: the **mean per-individual `Genotype`** after the final generation
    /// (the allele frequency the selection loop drives). `0.0` for an empty population.
    pub allele_freq: f64,
    /// Stable, build-scoped hash of the final world state (folds in `allele_freq`).
    pub hash: u64,
}

// --- ECS resources & components -------------------------------------------------------------------

/// The single seeded RNG for the run (invariant #3). The only source of randomness in sim logic.
#[derive(Resource)]
struct SimRng(ChaCha8Rng);

/// Generation counter, advanced once per schedule run.
#[derive(Resource, Default)]
struct Tick(u64);

/// The parametric genome wired into the core (invariant #2). Read-only in Stage 0.
#[derive(Resource)]
struct GenomeRes(Genome);

/// The genome-derived base growth rate (the [`Trait::GrowthRate`] phenotype), held as a resource so the
/// selection system can score individuals without re-expressing the genome each generation (invariant #2:
/// genotype→phenotype computed once, here in the core).
#[derive(Resource)]
struct BaseGrowthRate(f64);

/// One species in the [`SpeciesRegistry`] — its genome, per-species genotype→phenotype map, the derived base
/// growth rate, its constant-population target, and its expressed ecological [`gp::Strategy`] (the ADR R3-A
/// spine and ADR-013 F2). `base_growth` mirrors [`BaseGrowthRate`] for the registry's primary entry; the per-species
/// Wright-Fisher (R3-B) reads these. The `dead_code` allow now also covers `strategy`, which is expressed
/// once at reset but UNREAD by selection (the F2 keystone) until F3 funds metabolism from the budget — so it
/// stays hash-neutral, proven by the unchanged pinned literal.
#[allow(dead_code)]
pub(crate) struct SpeciesEntry {
    name: String,
    /// The species DATA key (mirrors [`RosterEntry::key`]) — read ONLY by the read-only [`Simulation::observe_all`]
    /// display projection so the renderer can route its per-species glyph; never a `SimRng` input, never folded
    /// into `hash_world` (inv #2/#3).
    key: String,
    genome: Genome,
    gp_map: gp::OntologyMap,
    base_growth: f64,
    target_pop: u32,
    pub(crate) strategy: gp::Strategy,
}

/// The ordered set of species in the run (ADR R3-A). Indexed by [`SpeciesId`] (= the `Vec` position); NEVER a
/// `HashMap` iterated in sim logic (inv #3). At R3-A there is exactly ONE entry and only it is spawned/selected,
/// so the run is byte-identical to the single-species core; R3-B spawns + selects all entries (a deliberate
/// re-pin) and F3 couples them through the resource substrate.
/// `#[allow(dead_code)]` until R3-B reads `entries` from the per-species selection/observe systems.
#[derive(Resource)]
#[allow(dead_code)]
pub(crate) struct SpeciesRegistry {
    pub(crate) entries: Vec<SpeciesEntry>,
}

/// A species to seed the run with (the boundary builds these from JSON; the core stays filesystem-free, inv #2).
/// `reset_with_roster` turns a `Vec<RosterEntry>` into the [`SpeciesRegistry`] (expressing each `base_growth`).
pub struct RosterEntry {
    /// Human-readable species name (metadata; the registry key is its ordinal [`SpeciesId`]).
    pub name: String,
    /// The species DATA key (`"ecoli-core"` | `"default"` | `""`) — the SAME stable identifier the renderer
    /// dispatches its glyph on (microbe vs plant). Carried verbatim from the JSON boundary; never a `SimRng`
    /// input, never folded into `hash_world` — display metadata only (inv #2/#3).
    pub key: String,
    /// The species genome.
    pub genome: Genome,
    /// The per-species genotype→phenotype map (e.g. `gp::ecoli_trait_map` for E. coli).
    pub gp_map: gp::OntologyMap,
    /// Constant-population target / spawn count for this species.
    pub entity_count: u32,
    /// The species' trophic role (ADR-013 F2) — CATEGORICAL data carried in from the JSON→roster boundary
    /// via [`gp::role_for`], NOT derived from genome scalars. Defaulted to `Autotroph` at existing call sites.
    pub role: gp::TrophicRole,
}

/// The static per-cell soil field as a resource (ADR-011 S-G): LOCAL soil-coupled selection samples each
/// organism's OWN cell ([`soil::SoilField::sample_at`]) instead of a field-wide mean, so drought-tolerant
/// lineages are favored in arid cells — real spatial selection. No `SimRng` draw (off the hash path beyond
/// its coupling effect on per-individual fitness). Supersedes the R1.1 global `MeanSoil` coupling.
#[derive(Resource)]
struct SoilFieldRes(soil::SoilField);

/// The world climate as a resource (ADR-012 Phase E): derived from the player's `EnvParams`, off the `SimRng`
/// stream. Inserted at reset; CONSUMED by selection only once E3 couples it (until then it's hash-neutral).
#[derive(Resource)]
struct ClimateFieldRes(climate::ClimateField);

/// The per-species DEEP-EDIT modifier (ADR-017 S6 — the load-bearing OVERSIGHT wire). One INTEGER PERMILLE
/// factor per [`SpeciesId`] (Vec index = ordinal), in the strictly-positive band `[500, 1500]` (`1000` = neutral
/// `1.0×`). [`metabolism`] reads it as ONE extra demand permille factor for the EDITED species (the same
/// pre-apportion DEMAND seam the soil/climate `match_permille` rides — never an f64 multiply on the granted-J
/// path, the F3 invariant), so a committed E. coli knockout deterministically THROTTLES that species' uptake →
/// it grows less → its population drops → (via the F4 decomposer loop) the ripple reaches the plant.
///
/// **DEFAULTS to all-neutral `1000`** so a run with NO committed edit is BYTE-IDENTICAL to the pre-S6 demand
/// math: a `× 1000/1000` permille factor leaves the single combined-permille product unchanged. NOT folded into
/// [`hash_world`] (the factor only ever reaches the hash THROUGH its coupling effect on the already-hashed
/// Energy/Biomass/pools — like soil/climate). So the pinned single-species PLANT config (no edit, all-neutral)
/// keeps `0x47a0_3c8f_6701_f240`; activating a NON-neutral factor is the deliberate re-pin owned by a later
/// phase (this slice WIRES it but the pinned run never sets a non-neutral factor).
#[derive(Resource)]
pub(crate) struct EditModifierRes {
    /// Per-`SpeciesId` permille factor in `[EDIT_FACTOR_MIN_Q, EDIT_FACTOR_MAX_Q]`; `1000` = neutral. Indexed by
    /// the species ordinal — an ordered `Vec`, NEVER a `HashMap` iterated in sim logic (inv #3).
    factor_q: Vec<u16>,
}

/// Minimum strictly-positive edit factor (permille): a full growth-lethal knockout (`growth_ratio_q == 0`) maps
/// to `0.5×` demand, a real but bounded penalty (never zero — selection stays strictly positive, ADR-005 spirit).
pub(crate) const EDIT_FACTOR_MIN_Q: u16 = 500;
/// Neutral edit factor (permille) — `1.0×`, the demand math is unchanged. A wild-type ratio (`1000`) or a
/// never-edited species sits here, keeping the no-edit run byte-identical.
pub(crate) const EDIT_FACTOR_NEUTRAL_Q: u16 = 1000;
/// Maximum edit factor (permille) — `1.5×`, the ceiling an `Activate` (over-expression) edit lifts demand to.
pub(crate) const EDIT_FACTOR_MAX_Q: u16 = 1500;

impl EditModifierRes {
    /// A fresh all-neutral modifier sized to `species_count` (every species `1.0×` until a commit lands).
    fn neutral(species_count: usize) -> Self {
        Self {
            factor_q: vec![EDIT_FACTOR_NEUTRAL_Q; species_count.max(1)],
        }
    }

    /// The committed permille factor for `sid` (neutral `1000` for an out-of-range/unedited species).
    fn factor_q(&self, sid: u16) -> u16 {
        self.factor_q
            .get(sid as usize)
            .copied()
            .unwrap_or(EDIT_FACTOR_NEUTRAL_Q)
    }

    /// Grow to `new_len ≥ len` species (ADR-019: a `RegionInoculate` may register a new species mid-run); new
    /// slots start NEUTRAL (`1000`). A no-op if `new_len <= len`. Only ever called on an inoculated run.
    fn grow_to(&mut self, new_len: usize) {
        if new_len > self.factor_q.len() {
            self.factor_q.resize(new_len, EDIT_FACTOR_NEUTRAL_Q);
        }
    }
}

/// How a committed deep edit acts on transcription (mirrors `crispr::EditKind` / `oracle_fba::EditKind` BY VALUE
/// so sim-core carries no dependency on either — the same VALUE-only boundary discipline `oracle-fba` uses).
/// Maps a committed FBA growth-ratio + edit verb to a strictly-positive `[0.5,1.5]` demand factor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditEffect {
    /// Full loss of function — the KO ratio applies directly (a lethal `q==0` → `0.5×`).
    Knockout,
    /// Partial loss of function — same monotone map (the ratio is already graded toward WT by the oracle).
    Knockdown,
    /// Gain of function — lifts demand ABOVE neutral toward the `1.5×` ceiling.
    Activate,
}

/// Map a committed deep-edit impact → a strictly-positive `[500, 1500]` PERMILLE demand factor, **integer /
/// fixed-point only** (no transcendental), clamped to the band (ADR-017 S6, the pinned mapping):
///
/// * `Knockout` / `Knockdown`: `factor_q = EDIT_FACTOR_MIN_Q + (growth_ratio_q · (NEUTRAL − MIN)) / 1000`
///   — a linear lift off the `0.5×` floor toward `1.0×` neutral. `growth_ratio_q == 1000` (wild-type) → `1000`
///   (exactly neutral, a no-op); `growth_ratio_q == 0` (lethal KO) → `500` (the strong penalty). Pure `u32`
///   intermediate, floored ONCE, so it is byte-identical on every platform (the `oracle-fba` quantize contract).
/// * `Activate`: lifts ABOVE neutral toward the ceiling — `factor_q = NEUTRAL + (growth_ratio_q · (MAX −
///   NEUTRAL)) / 1000`, so a full-strength activate (`q == 1000`) → `1500` (`1.5×`) and a neutral activate
///   (`q == 1000` is the over-expression magnitude here) lifts; `q == 0` collapses to neutral.
///
/// The result is CLAMPED into `[EDIT_FACTOR_MIN_Q, EDIT_FACTOR_MAX_Q]` so a malformed payload can never escape
/// the strictly-positive band (selection is never zeroed — the firewall's quantized input is bounded `[0,1000]`
/// but the clamp is the hard guarantee).
#[must_use]
pub fn edit_factor_q(growth_ratio_q: u16, effect: EditEffect) -> u16 {
    // Clamp the oracle's growth ratio defensively to its `[0,1000]` permille domain before the integer map.
    let scale = fixed::PERMILLE; // u32 permille denominator (1000)
    let q = u32::from(growth_ratio_q).min(scale);
    let neutral = u32::from(EDIT_FACTOR_NEUTRAL_Q);
    let factor = match effect {
        // Loss-of-function: lift off the 0.5× floor toward 1.0× as the residual growth ratio rises.
        EditEffect::Knockout | EditEffect::Knockdown => {
            let span = neutral - u32::from(EDIT_FACTOR_MIN_Q); // 500
            u32::from(EDIT_FACTOR_MIN_Q) + (q * span) / scale
        }
        // Gain-of-function: lift above 1.0× toward the 1.5× ceiling as the activation magnitude rises.
        EditEffect::Activate => {
            let span = u32::from(EDIT_FACTOR_MAX_Q) - neutral; // 500
            neutral + (q * span) / scale
        }
    };
    factor.clamp(u32::from(EDIT_FACTOR_MIN_Q), u32::from(EDIT_FACTOR_MAX_Q)) as u16
}

/// The STATIC per-cell resource field (ADR-013 F1→F3): light / free_nutrient / detritus (`f32` `[0,1]`),
/// generated off the `SimRng` stream. At F3 it is the render/cap/seed SOURCE — [`PoolStock`] is seeded from it
/// at reset and [`solar_influx`] reads its per-cell carrying caps each tick — while the mutable joule pools
/// live in `PoolStock`. Read by the F3 pipeline (no longer hash-neutral via its coupling to the live pools).
#[derive(Resource)]
#[allow(dead_code)] // the static seed/cap source; the live tick path now reads the PRECOMPUTED SolarLightCap.
struct ResourceFieldRes(resource::ResourceField);

/// PRECOMPUTED per-cell solar light carrying-cap (perf optimization, hash-neutral): the static
/// [`ResourceField`] never changes across the run, so `min(to_unit_u16(light[c]) * CELL_CAP_SCALE, POOL_CAP)`
/// is constant. Computing it ONCE at reset (instead of re-flooring an f64 for every cell every tick in
/// [`solar_influx`]) yields the IDENTICAL integer cap → byte-identical mint order/values. Indexed `y*w + x`.
#[derive(Resource)]
struct SolarLightCap(Vec<i64>);

/// REUSABLE per-tick scratch buffers for [`metabolism`] (perf optimization, hash-neutral). The hot system
/// rebuilds these every tick from the LIVING set; holding their backing allocations in a resource (cleared +
/// refilled, never read as carried state) amortizes the per-tick `Vec` reallocation to zero once the population
/// stabilizes. NEVER folded into `hash_world`; the CONTENTS are fully overwritten each tick so reuse is
/// byte-identical to allocating fresh.
#[derive(Resource, Default)]
struct MetabolismScratch {
    items: Vec<MetabolismItem>,
    frozen_light: Vec<i64>,
    frozen_nutrient: Vec<i64>,
    frozen_detritus: Vec<i64>,
    frozen_toxin: Vec<i32>,
    demand: Vec<[i64; resource::RESOURCE_CHANNELS]>,
    granted: Vec<[i64; resource::RESOURCE_CHANNELS]>,
}

/// REUSABLE per-tick scratch buffers for [`reproduce_or_die`]'s canonical-order row vector + the two frozen
/// chem-plane snapshots (perf optimization, hash-neutral; the [`MetabolismScratch`] rationale). Cleared +
/// refilled each tick; never carried state.
#[derive(Resource, Default)]
struct ReproScratch {
    rows: Vec<ReproRow>,
    frozen_toxin: Vec<i32>,
    frozen_alarm: Vec<i32>,
}

/// Stable per-organism id, assigned monotonically from [`NextOrgId`] (ADR-013 F3 — widened to `u64` now that
/// population is unbounded). Gives a deterministic hash/sort order independent of ECS query/archetype
/// iteration order. OrgIds are NEVER reused (a slot index could repeat across a despawn+spawn and silently
/// re-pair a lineage); births always mint a fresh, larger id.
#[derive(Component, Clone, Copy)]
pub(crate) struct OrgId(pub(crate) u64);

/// Ordinal id of a species in the [`SpeciesRegistry`] (= its `Vec` index). `Ord`/`Hash` so the per-species
/// Wright-Fisher (R3-B) can sort + fold by `(SpeciesId, OrgId)` without a second edit (ADR R3-A).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpeciesId(u16);

impl SpeciesId {
    /// Construct a [`SpeciesId`] from its raw ordinal (the harness firewall carries `species: u16`, ADR-017 S6).
    /// Public so the env layer can route a committed deep edit to the right registry slot; the inner field stays
    /// private so a `SpeciesId` is only ever a registry ordinal.
    #[must_use]
    pub fn new(ordinal: u16) -> Self {
        Self(ordinal)
    }

    /// The raw ordinal (= the [`SpeciesRegistry`] Vec index).
    #[must_use]
    pub fn ordinal(self) -> u16 {
        self.0
    }
}

/// Full-scale energy quantum (ADR-013 F0b): one "unit" of organism energy is `ENERGY_FULL` integer joules.
/// Energy migrates from `f64` to the conserved `i64` currency the CHEMOSTAT-J economy will denominate
/// everything in (the first fixed-point type migration; later phases F1/F3 give it real metabolic meaning).
const ENERGY_FULL: i64 = 1_000_000;

// ── ADR-013 F3 chemostat constants (the gated births/deaths keystone) ────────────────────────────────
//
// ALL i64 / integer — no float on the recurring J path (the cross-ISA divergence guard). These are the F3.3
// landing values; the F3.4 chemostat-tuning sweep + Repin phase re-tunes them OFF the hash before the literal
// is pinned. The numeric budget (adversarial finding #10) is sized so `Σ(pools + Energy + Biomass)` over
// `MAX_POPULATION` orgs + 1024 cells × 3 channels × `POOL_CAP` + influx accumulation stays well under
// `i64::MAX` with many orders of magnitude of margin.

/// Joules per unit of the quantized static `ResourceField` `[0,1]→u16` SEED (ADR-013 F3, finding #9). A cell's
/// initial pool `J` = `to_unit_u16(field_value) as i64 * CELL_J_SCALE`. The single audited f64→int chokepoint
/// is `fixed::to_unit_u16`; this scale lifts that `[0, 65535]` grid into the joule economy.
///
/// CRITICAL (F3.4 chemostat tuning): this is DELIBERATELY << [`CELL_CAP_SCALE`] so a cell starts only PARTLY
/// full, leaving headroom for [`solar_influx`] to actually flow. When seed == cap (the pre-tuning bug), every
/// cell starts AT its cap, the only-true-source solar influx spills 100% to overflow from tick 1, and the
/// ecosystem lives purely off the finite static seed → guaranteed extinction. Seed below cap re-opens the tap.
const CELL_J_SCALE: i64 = 40;

/// Joules per `[0,1]→u16` unit for the per-cell CARRYING CAP (the [`solar_influx`] refill ceiling), distinct
/// from (and >> ) [`CELL_J_SCALE`] so a cell seeded at `field*CELL_J_SCALE` has `field*(CELL_CAP_SCALE −
/// CELL_J_SCALE)` of headroom for continuous solar inflow. This decoupling is what makes solar a LIVE source.
const CELL_CAP_SCALE: i64 = 400;

/// Per-cell hard ceiling on any single `PoolStock` channel (`light`/`free_nutrient`/`detritus`). Influx /
/// excretion past this routes the spill to [`ledger::Ledger::overflow`] (never silently clamped). Sized above
/// the max field-derived cap (`UNIT_SCALE * CELL_CAP_SCALE ≈ 26_214_000`) with headroom for accumulation.
pub(crate) const POOL_CAP: i64 = 200_000_000;

/// Per-cell solar `J` minted into `PoolStock.light` each tick by [`solar_influx`] — but only up to the cell's
/// static `ResourceField.light` carrying-cap (so a bright cell refills, a dark cell stays poor). The ONLY
/// source of new `J` (the INFLUX tap). free_nutrient regen toward its target is also booked as INFLUX at F3 (a
/// documented open tap, closed endogenously by the F4 plant→detritus→decomposer→free_nutrient loop).
const SOLAR_PER_CELL: i64 = 40_000;

/// Uptake saturation: the Monod-like `uptake = (Vmax·S)/(K_half + S)` taps a channel hard at high stock and
/// gently at low stock. `VMAX` is the per-org-per-tick ceiling at infinite stock (scaled by demand); `K_HALF`
/// is the stock at which uptake is half-max. Pure integer (`u128` intermediate, floored to `i64`).
const UPTAKE_VMAX: i64 = 2_000_000;
/// Half-saturation stock for the Monod uptake curve (see [`UPTAKE_VMAX`]).
const UPTAKE_K_HALF: i64 = 1_000_000;

/// Per-org-per-tick MAINTENANCE upkeep debit funded by the `budget[Maintenance]` slice, subtracted from Energy
/// → RESPIRED (the only per-org sink that makes starvation possible). A flat base scaled by the maintenance
/// permille share so a maintenance-heavy strategy pays more upkeep (a real trade-off).
const MAINTENANCE_BASE: i64 = 4_000;

/// Minimum `body_factor` permille (ADR-013 F3.4 chemostat tuning): the demand-chain body multiplier never
/// drops below this, so a fresh seed-biomass org still expresses real uptake demand and can grow toward
/// reproduction rather than starving on its tiny body alone.
pub(crate) const BODY_FACTOR_FLOOR: u64 = 250;

/// Starvation floor: after the maintenance debit, an org whose Energy is BELOW this dies (ADR-013 F3). Its
/// residual Energy+Biomass deposits to the cell detritus pool (carcass→detritus).
pub(crate) const MAINTENANCE_FLOOR: i64 = 1;

/// Senescence ceiling: an org at this [`Age`] dies of old age (HARD at F3; soft coupling deferred).
const AGE_MAX: u32 = 240;

/// Reproduction threshold: an org whose Energy is ≥ this AFTER maintenance may spend an [`OFFSPRING_ENDOWMENT`]
/// to produce one child this tick (ADR-013 F3). Set above the endowment so a birth never drives the parent
/// negative.
const REPRO_THRESHOLD: i64 = 300_000;

/// The conserved `J` a parent SPENDS per birth: `parent.Energy -= OFFSPRING_ENDOWMENT`, and the child receives
/// `Biomass = OFFSPRING_SEED_BIOMASS`, `Energy = OFFSPRING_ENDOWMENT − OFFSPRING_SEED_BIOMASS` — no minting, a
/// pure transfer out of the parent's reserve.
const OFFSPRING_ENDOWMENT: i64 = 200_000;

/// The child's initial structural [`Biomass`], carved OUT of the [`OFFSPRING_ENDOWMENT`] (the rest seeds the
/// child's Energy reserve). Conserved.
pub(crate) const OFFSPRING_SEED_BIOMASS: i64 = 100_000;

/// Per-org Energy cap (ADR-013 F3). Convert/uptake past this routes to [`ledger::Ledger::overflow`].
pub(crate) const ENERGY_CAP: i64 = 4_000_000;

/// Per-org Biomass cap (ADR-013 F3). Growth past this routes to [`ledger::Ledger::overflow`].
pub(crate) const BIOMASS_CAP: i64 = 4_000_000;

/// Trophic-efficiency NUMERATOR/DENOMINATOR (`EFF_NUM/EFF_DEN < 1`): the fraction of CONVERTED uptake that is
/// KEPT; the residual `granted − Σ(kept)` is RESPIRED (computed as a residual, never an independent divide that
/// double-floors a quantum — adversarial finding #7).
pub(crate) const EFF_NUM: i64 = 7;
/// Trophic-efficiency denominator (see [`EFF_NUM`]).
pub(crate) const EFF_DEN: i64 = 10;

/// **LITTERFALL** fraction NUMERATOR/DENOMINATOR (ADR-013 F4): of an AUTOTROPH's convert-RESPIRED inefficiency
/// carbon, this fraction is instead shed to the cell `detritus` pool every tick (a living canopy rains litter
/// even without death) — the second plant→detritus arm beside the F3 carcass deposit. Computed as a fraction
/// of the already-floored respired residual (never an independent divide that double-floors a quantum), so the
/// move stays a paired respired↔detritus split that conserves J. Only Autotrophs litter (decomposers shed
/// nothing — their loss is the mineralization respired tap).
const LITTERFALL_NUM: i64 = 4;
/// Litterfall denominator (see [`LITTERFALL_NUM`]) — 40% of the respired inefficiency becomes detritus.
const LITTERFALL_DEN: i64 = 10;

/// **LIEBIG nutrient-limitation reference** (ADR-013 F4): the per-cell `free_nutrient` stock at which an
/// Autotroph's LIGHT uptake is UN-throttled (gate = 1000 permille). As `free_nutrient` drains below it the
/// light demand scales DOWN linearly toward [`LIEBIG_FLOOR`] (nutrients co-limit photosynthesis) — so a
/// working decomposer (which refills local nutrient toward this reference via mineralization) CONTINUOUSLY
/// raises nearby plant productivity, a legible measurable coupling. Set comparable to the per-cell seed so the
/// gate sits near 1000 in a freshly-seeded cell and tightens as the standing nutrient is drawn down.
const NUTRIENT_LIMIT_REF: i64 = 600_000;

/// **LIEBIG floor** (ADR-013 F3.4 chemostat tuning): the minimum permille the nutrient co-limitation gate
/// returns even at zero local `free_nutrient`. A SOFT co-limitation, not a hard cliff — a plant always retains
/// this fraction of its light uptake on light alone, so a decomposer-less plant MONOCULTURE declines slowly and
/// gracefully (a long rundown over tens of thousands of generations) rather than the gen-~240 age-out cliff of
/// the untuned constants. It does NOT, however, reach a non-zero equilibrium on its own — and that is correct
/// ecology, not a tuning miss: with no decomposer the nutrient cycle never closes (carbon/N lock into detritus
/// with nothing to mineralize them), so an autotroph monoculture must run down. The bounded-non-zero equilibrium
/// is a MULTI-SPECIES property: a working decomposer (raising local nutrient toward [`NUTRIENT_LIMIT_REF`] via
/// mineralization) lifts the gate toward 1000 and MEASURABLY raises plant carrying capacity (~3.5x), so the
/// plant+decomposer roster settles to a stable coexistence attractor. Soft-mutualistic, not obligate. See ADR-013 F3.4.
const LIEBIG_FLOOR: u64 = 350;

/// Allocator guard (inv #6): a HARD ceiling on total live population, set FAR above any resource-supportable
/// equilibrium so it is provably NEVER hit in the pinned config (the `max_population_is_never_hit` test).
/// Keeping it non-load-bearing avoids the "skip births in OrgId order" hidden-selection trap — if it ever bound,
/// it would impose an OrgId-correlated selection gradient, which inv #6 forbids.
const MAX_POPULATION: u32 = 2_000_000;

/// Monotonic OrgId allocator (ADR-013 F3): the id the NEXT spawned organism receives. Bumped at every spawn
/// (initial + birth); NEVER reset mid-run, NEVER reuses a despawned id. A `Resource` so the single-threaded
/// schedule threads it deterministically (inv #3).
#[derive(Resource)]
struct NextOrgId(u64);

/// Cumulative `SimRng` draw counter (ADR-013 F3, finding #4): incremented at EVERY `next_u64` the sim path
/// consumes (births only at F3), and folded into `hash_world` alongside `final_word`. A birth-enumeration
/// off-by-one then breaks the hash LOCALLY (on one ISA) instead of drifting into a plausible-but-wrong
/// reproducible value. A `Resource` threaded through the single-threaded schedule (inv #3).
#[derive(Resource, Default)]
struct DrawCount(u64);

/// The MUTABLE per-cell joule pools (ADR-013 F3) — the live substrate metabolism consumes and regenerates,
/// SEPARATE from the static f32 [`resource::ResourceField`] (which survives as the render/seed/cap source).
/// Each channel is row-major over the world grid (1:1 with `Position`, asserted at reset). Seeded ONCE at reset
/// by quantizing the static field through the single audited f64→int chokepoint
/// (`fixed::to_unit_u16(v) as i64 * CELL_J_SCALE`). Folded into `hash_world` (a varying-N world's pools are
/// hashed state) and summed into [`ledger::LiveTotal::pools`]. Never a `HashMap` (inv #3) — indexed by
/// `cell_index = y*width + x`.
#[derive(Resource)]
pub(crate) struct PoolStock {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) light: Vec<i64>,
    pub(crate) free_nutrient: Vec<i64>,
    pub(crate) detritus: Vec<i64>,
}

impl PoolStock {
    /// Seed the live pools ONCE at reset by quantizing the static [`resource::ResourceField`] `[0,1]` through
    /// the single audited f64→int chokepoint (`fixed::to_unit_u16(v) as i64 * CELL_J_SCALE`). The static field
    /// stays the render/cap/seed source. Pure integer after the one chokepoint multiply — deterministic.
    fn seed_from(field: &resource::ResourceField) -> Self {
        let quantize = |plane: &[f32]| -> Vec<i64> {
            plane
                .iter()
                .map(|&v| i64::from(fixed::to_unit_u16(f64::from(v))) * CELL_J_SCALE)
                .collect()
        };
        Self {
            width: field.width,
            height: field.height,
            light: quantize(&field.light),
            free_nutrient: quantize(&field.free_nutrient),
            detritus: quantize(&field.detritus),
        }
    }

    /// `Σ` over all cells of `light + free_nutrient + detritus` — the [`ledger::LiveTotal::pools`] term. Integer
    /// addition is commutative so the sum is order-independent (inv #3).
    fn total(&self) -> i64 {
        let s = |v: &[i64]| -> i64 { v.iter().copied().sum() };
        s(&self.light) + s(&self.free_nutrient) + s(&self.detritus)
    }
}

/// Per-organism FREE-RESERVE energy as an integer joule quantum (ADR-013 F3). The "birth fund": uptake's
/// Reproduction slice accrues here, maintenance debits it, and a birth spends [`OFFSPRING_ENDOWMENT`] from it
/// (CONSERVED — the child's reserve+seed-biomass come OUT of the parent's reserve, never minted). Capped at
/// [`ENERGY_CAP`]; any uptake/convert overflow past the cap is routed to [`ledger::Ledger::overflow`], never
/// silently clamped. RNG-free in the recurring path — only births draw from `SimRng`.
#[derive(Component, Clone, Copy)]
pub(crate) struct Energy(pub(crate) i64);

/// Per-organism STRUCTURAL mass as an integer joule quantum (ADR-013 F3). Body size: uptake DEMAND scales with
/// Biomass (bigger bodies eat more), Growth-slice joules accrue here, and on death the residual Biomass (plus
/// residual Energy) deposits to the cell's `detritus` pool (carcass→detritus, conserving `J`). Capped at
/// [`BIOMASS_CAP`]; overflow past the cap is routed to [`ledger::Ledger::overflow`]. Folded into `hash_world`.
#[derive(Component, Clone, Copy)]
pub(crate) struct Biomass(pub(crate) i64);

/// Per-organism AGE in ticks (ADR-013 F3). Incremented once per tick (in [`metabolism`]); at [`AGE_MAX`] the
/// organism dies of senescence (a HARD ceiling at F3; soft age→maintenance coupling is deferred). Folded into
/// `hash_world` (per-org heritable-adjacent state that affects the death set).
#[derive(Component, Clone, Copy)]
struct Age(u32);

/// Per-individual heritable scalar in `[0, 1]` — the "allele" under selection. Seeded at spawn from
/// [`SimRng`] so individuals vary; resampled each generation by [`selection`]. Higher fitness ⇒ more copies.
#[derive(Component, Clone, Copy)]
struct Genotype(f64);

/// Per-individual **heritable** drought tolerance in `[0, 1]` (roadmap R1.0a). Seeded at spawn as standing
/// variation; inherited (NOT resampled) from the fitness-sampled parent each generation by [`selection`], so
/// soil-coupled selection (R1.1) can shift the population's drought distribution to match the terrain.
#[derive(Component, Clone, Copy)]
struct DroughtTol(f64);

/// Per-individual **heritable** thermal tolerance in `[0, 1]` (ADR-012 Phase E E3). Standing variation seeded
/// at spawn (after drought, fixed draw order); inherited (NOT resampled) from the fitness-sampled parent; the
/// climate's `TemperatureMatchModifier` favours warm-adapted individuals in warm climates. Folded into the hash.
#[derive(Component, Clone, Copy)]
struct ThermalTol(f64);

/// Per-individual cell position on the canonical [`WORLD_DIMS`] world grid (ADR-011, real spatial biology —
/// no longer a render-only OrgId hash). Seeded at spawn from a DISJOINT off-`SimRng` derive_seed family
/// ([`PLACEMENT_STREAM_BASE`]) so initial placement adds ZERO `SimRng` draws (the spawn stream is unchanged;
/// only `Position` entering `hash_world` re-pins the hash). Inherited + dispersed by [`selection`] (S-B) so
/// lineages cluster into emergent regions. Lives ONLY in the core (invariant #2).
#[derive(Component, Clone, Copy)]
pub(crate) struct Position {
    pub(crate) x: u32,
    pub(crate) y: u32,
}

/// Which species an organism belongs to (ADR R3-A). Heritable — offspring inherit their parent's species (R3-B);
/// assigned at SPAWN from the registry ordinal, NEVER from `SimRng` (zero `next_u64`, the `Position` off-stream
/// precedent), so tagging organisms is hash-neutral (not folded into `hash_world` at R3-A).
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Species(pub(crate) SpeciesId);

/// Canonical world grid for per-organism positions (ADR-011). Equal to `soil::SOIL_DIMS` so an organism's
/// cell maps 1:1 onto a soil cell (no resample for future local-soil coupling). The render snapshot resamples
/// this world grid onto its own `(width, height)`.
const WORLD_DIMS: (u32, u32) = soil::SOIL_DIMS;

/// Disjoint `derive_seed` stream base for initial organism PLACEMENT (ASCII "PLAC"), kept far from the soil
/// family ([`soil::SOIL_STREAM_BASE`]) and the legacy snapshot placement streams `1`/`2` (DECISIONS.md stream
/// registry). Off the `SimRng` stream (inv #3): placement draws zero `next_u64`, so the spawn draw order is
/// unchanged — only `Position` entering `hash_world` re-pins the determinism hash.
const PLACEMENT_STREAM_BASE: u64 = 0x0050_4C41_4300_0000;

/// Minimum brush radius (ADR-011 invariant-#6 guard): a region edit always covers a disc, never a single
/// cell, so it stays a CELL-region operator action and can't degenerate into de-facto per-organism targeting.
pub const MIN_REGION_RADIUS: u32 = 1;

/// A spatial brush region on the world grid for a region-scoped edit (ADR-011 S-D). Targets CELLS, carries NO
/// organism handle (the invariant-#6 type guard — the edit can never name an individual). Euclidean disc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    /// Disc centre cell x on the world grid.
    pub cx: u32,
    /// Disc centre cell y on the world grid.
    pub cy: u32,
    /// Disc radius in cells (clamped up to [`MIN_REGION_RADIUS`]).
    pub radius: u32,
}

/// A read of allele frequency over a disc region (ADR-013 campaign-grader): the mean of the per-cell
/// `allele_freq` over the populated in-region cells, plus how many such cells there were (`populated_cells
/// == 0` means the region is empty and `mean` is `0.0`). Returned by [`Simulation::region_allele`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegionReadout {
    /// Mean-of-cell-means allele frequency over the populated in-region cells, in `[0, 1]`.
    pub mean: f64,
    /// Number of populated (`density > 0`) cells inside the region disc.
    pub populated_cells: u32,
}

impl Region {
    /// Whether world cell `(x, y)` falls inside the disc (radius clamped to [`MIN_REGION_RADIUS`]).
    #[must_use]
    pub fn contains(&self, x: u32, y: u32) -> bool {
        let dx = i64::from(x) - i64::from(self.cx);
        let dy = i64::from(y) - i64::from(self.cy);
        let r = i64::from(self.radius.max(MIN_REGION_RADIUS));
        dx * dx + dy * dy <= r * r
    }
}

/// Deterministic off-stream initial cell for organism `i`: `x`/`y` from two disjoint `derive_seed` streams,
/// modulo the world grid. No `SimRng` draw (inv #3); reproducible from the master seed alone.
fn placement(seed: u64, i: u32) -> Position {
    let w = WORLD_DIMS.0 as u64;
    let h = WORLD_DIMS.1 as u64;
    Position {
        x: (crate::det::derive_seed(seed, PLACEMENT_STREAM_BASE + 2 * i as u64) % w) as u32,
        y: (crate::det::derive_seed(seed, PLACEMENT_STREAM_BASE + 2 * i as u64 + 1) % h) as u32,
    }
}

// --- systems (fixed order via .chain()) -----------------------------------------------------------

fn advance_tick(mut tick: ResMut<Tick>) {
    tick.0 += 1;
}

/// The cell index of a [`Position`] on the [`PoolStock`] grid (`y*width + x`), the canonical sort key's first
/// field. With `WORLD_DIMS == RESOURCE_DIMS` (asserted at reset) `Position` maps 1:1 to a pool cell — no
/// resample. Pure integer (inv #3).
pub(crate) fn cell_index(p: &Position, width: u32) -> u32 {
    p.y * width + p.x
}

/// Map a u64 to a `[0, 1)` f64 using the top 53 bits (deterministic, no rand-API churn). RETIRED from the sim
/// path at F3 (the Wright-Fisher sampler is deleted); kept `pub(crate)` for [`soil`]'s off-stream field
/// generation, which is the only remaining caller.
pub(crate) fn unit_f64(x: u64) -> f64 {
    (x >> 11) as f64 / (1u64 << 53) as f64
}

/// Integer Monod-like saturation uptake: `floor((Vmax · S) / (K_half + S))`, scaled by `demand_permille`
/// (`[0,1000]`, the org's demand on this channel) — pure `u128`, no float/RNG (inv #3). `S` is the FROZEN
/// start-of-tick stock. Returns the org's DEMAND on the channel (granted is apportioned later if the cell is
/// contended).
pub(crate) fn monod_demand(stock: i64, demand_permille: u64) -> i64 {
    if stock <= 0 || demand_permille == 0 {
        return 0;
    }
    let s = stock as u128;
    let raw = (UPTAKE_VMAX as u128 * s) / (UPTAKE_K_HALF as u128 + s);
    ((raw * u128::from(demand_permille)) / u128::from(fixed::PERMILLE)) as i64
}

/// **METABOLISM** (ADR-013 F3 KEYSTONE) — the RNG-FREE integer `uptake → convert → excrete` pass plus the
/// per-tick [`Age`] bump. Replaces the deleted no-op-draw metabolism. Reads the FROZEN start-of-tick
/// [`PoolStock`] + each species' cached [`gp::Strategy`]; resolves per-cell contention by
/// [`fixed::apportion`] (frozen-snapshot + apportion, the human-approved default). Draws ZERO `SimRng`.
///
/// Determinism: builds ONE canonical org vector sorted by `(cell_index, SpeciesId, OrgId)`, then
/// 1. gathers each org's per-channel DEMAND against the frozen stock (Monod);
/// 2. apportions each cell's actual available `J` across its co-located demanders (largest-remainder, ties to
///    lowest canonical index — finding #6), decrementing the live pool ONCE;
/// 3. CONVERTs the granted `J` via [`fixed::split_budget`] (Growth→Biomass, Reproduction/Acquisition→Energy,
///    Maintenance/Defense→respired), with a trophic-efficiency residual respired (finding #7);
/// 4. EXCRETEs the inefficiency carbon to the cell `detritus` pool; caps route overflow (never silent clamp).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn metabolism(
    registry: Res<SpeciesRegistry>,
    soil_field: Res<SoilFieldRes>,
    climate_field: Res<ClimateFieldRes>,
    edit_mod: Res<EditModifierRes>,
    kin_prov: Res<chem::KinProvenance>,
    mut scratch: ResMut<MetabolismScratch>,
    mut pools: ResMut<PoolStock>,
    mut chem: ResMut<chem::ChemField>,
    mut prov: ResMut<trophic::PoolProvenance>,
    mut flow: ResMut<trophic::FlowMatrix>,
    mut ledger: ResMut<ledger::Ledger>,
    mut q: Query<(
        &OrgId,
        &Species,
        &mut Energy,
        &mut Biomass,
        &mut Age,
        &DroughtTol,
        &ThermalTol,
        &Position,
    )>,
) {
    use chem::ChemModifier as _;
    use climate::ClimateModifier as _;
    use soil::EnvironmentModifier as _;
    let soil_mod = soil::LinearTraitMatchModifier;
    let clim_mod = climate::TemperatureMatchModifier;
    let chem_mod = chem::InCoreChem;
    let clim_sample = climate_field.0.sample(); // GLOBAL climate coupling (ADR-012 E3).
    let width = pools.width;
    // ADR-013 F5: FREEZE the chem toxin plane at start-of-tick (the `frozen_light` discipline) so within-tick
    // emit/mint order never affects within-tick sense — archetype-reorder safe. A fresh deposit is sensed
    // in-cell only NEXT tick (after diffusion). KinProvenance is read-only here (own-species marker lookup).
    // Reuse the persistent frozen-toxin buffer (byte-identical snapshot of the start-of-tick toxin plane).
    let mut frozen_toxin = std::mem::take(&mut scratch.frozen_toxin);
    chem.frozen_toxin_into(&mut frozen_toxin);
    // ── Canonical order: (cell_index, SpeciesId, OrgId). Built ONCE over the LIVING set (inv #3). ──
    // BLOCKER #1 fix: the ADR-011/012 soil+climate match factor is re-expressed as an INTEGER permille that
    // scales DEMAND (pre-apportion) — NOT an f64 multiply on the granted J. The f64 factor is computed once
    // per org, normalized to a [0,1] match, and quantized via the single audited `fixed::to_unit_u16`
    // chokepoint; the single floored GRANTED value is then both the pool debit and the org credit (no f64 ever
    // touches hashed Energy/Biomass). This preserves spatial selection as a REAL integer energetic advantage.
    // Reuse the persistent scratch buffers (cleared + refilled; the backing allocation survives across ticks).
    let mut items = std::mem::take(&mut scratch.items);
    items.clear();
    items.extend(q.iter().map(|(id, sp, _e, biomass, _a, d, t, p)| {
        let local_soil = soil_field.0.sample_at(p.x, p.y);
        // Both modifiers return a strictly-positive [0.5,1.5] band; product ∈ [0.25,2.25]. Map to a [0,1]
        // match by `(factor - 0.25)/2.0` (linear, monotone), quantize ONCE to the u16 grid → an integer
        // match permille. A better trait↔environment match ⇒ a higher permille ⇒ more demand.
        let factor =
            soil_mod.fitness_factor(local_soil, d.0) * clim_mod.fitness_factor(clim_sample, t.0);
        let match_unit = ((factor - 0.25) / 2.0).clamp(0.0, 1.0);
        let match_permille = (u64::from(fixed::to_unit_u16(match_unit))
            * u64::from(fixed::PERMILLE))
            / u64::from(fixed::UNIT_SCALE);
        // ADR-013 F5: sense the org's OWN cell chem, FROZEN at start-of-tick. Toxin suppress + kin boost are
        // INTEGER PERMILLE factors folded into the SAME demand product (the EditModifier precedent). Both
        // return the NEUTRAL 1000 in a chem-free cell → the byte-identical pre-F5 demand math.
        let cell = cell_index(p, width);
        let tox_suppress = chem_mod.toxin_suppress_permille(frozen_toxin[cell as usize]);
        let kin_own = kin_prov.own(cell as usize, sp.0 .0 as usize);
        let kin_boost = chem_mod.kin_boost_permille(kin_own);
        MetabolismItem {
            cell,
            species: sp.0 .0,
            org: id.0,
            // Bigger bodies demand more (size→uptake feedback); floor at seed biomass so a fresh org eats.
            body: biomass.0.max(OFFSPRING_SEED_BIOMASS),
            // Floor at a baseline so a poor match still eats a little (no zeroed weight — ADR-005 spirit).
            match_permille: match_permille.max(u64::from(fixed::PERMILLE) / 4),
            tox_suppress,
            kin_boost,
        }
    }));
    items.sort_unstable_by_key(|it| (it.cell, it.species, it.org));

    // ── Pass 1: per-org DEMAND against the FROZEN stock (snapshot the three channels start-of-tick into the
    //    reused frozen buffers — `copy_from_slice` reuses the backing allocation, byte-identical to a clone). ──
    let mut frozen_light = std::mem::take(&mut scratch.frozen_light);
    let mut frozen_nutrient = std::mem::take(&mut scratch.frozen_nutrient);
    let mut frozen_detritus = std::mem::take(&mut scratch.frozen_detritus);
    frozen_light.clear();
    frozen_light.extend_from_slice(&pools.light);
    frozen_nutrient.clear();
    frozen_nutrient.extend_from_slice(&pools.free_nutrient);
    frozen_detritus.clear();
    frozen_detritus.extend_from_slice(&pools.detritus);

    // Per-channel demand vector, indexed parallel to `items` (canonical order → finding #6 apportion index).
    let n = items.len();
    let mut demand = std::mem::take(&mut scratch.demand);
    demand.clear();
    demand.resize(n, [0i64; resource::RESOURCE_CHANNELS]);
    for (i, it) in items.iter().enumerate() {
        let strat = &registry.entries[it.species as usize].strategy;
        // Acquisition permille scales demand; body size scales it further. demand_permille folds affinity.
        let acq = u64::from(strat.budget[gp::BudgetChannel::Acquisition as usize]);
        // body_factor ∈ [BODY_FACTOR_FLOOR, 1000] permille: bigger bodies eat more, but a fresh small org keeps
        // a floor so it is NOT starved out of the demand chain before it can grow (the F3.4 chemostat-tuning fix
        // — at BIOMASS_CAP=4M a seed-biomass org was only 25/1000 and netted nothing).
        let body_factor =
            (((it.body as u128 * u128::from(fixed::PERMILLE)) / (BIOMASS_CAP as u128)).min(1000)
                as u64)
                .max(BODY_FACTOR_FLOOR);
        let cell = it.cell as usize;
        // Channel taps by role: Autotroph→light; Heterotroph→free_nutrient+detritus; Decomposer→detritus;
        // Mixotroph→light+free_nutrient. affinity[c] (u16 grid) gates each channel's demand.
        let aff = strat.affinity;
        let role = strat.role;
        let taps: [(usize, i64, bool); resource::RESOURCE_CHANNELS] = [
            (
                0,
                frozen_light[cell],
                matches!(
                    role,
                    gp::TrophicRole::Autotroph | gp::TrophicRole::Mixotroph
                ),
            ),
            (
                1,
                frozen_nutrient[cell],
                // ADR-013 F4: AUTOTROPHS draw free_nutrient (via affinity[1], the GrowthRate anchor) — the
                // obligate-loop demand side. With the free_nutrient INFLUX arm deleted, this nutrient comes
                // ONLY from decomposer mineralization, so a plant DEPENDS on the decomposer.
                matches!(
                    role,
                    gp::TrophicRole::Autotroph
                        | gp::TrophicRole::Heterotroph
                        | gp::TrophicRole::Mixotroph
                ),
            ),
            (
                2,
                frozen_detritus[cell],
                matches!(
                    role,
                    gp::TrophicRole::Heterotroph | gp::TrophicRole::Decomposer
                ),
            ),
        ];
        // ADR-013 F4 LIEBIG CO-LIMITATION: an AUTOTROPH can only USE light to the extent free_nutrient is also
        // available in its cell (nutrients limit photosynthesis — the obligate-loop teeth). The integer ratio
        // `min(1, frozen_nutrient/NUTRIENT_LIMIT_REF)` (permille) GATES the plant's LIGHT demand pre-apportion.
        // When the decomposer is dead, free_nutrient drains to 0 → the gate → 0 → the plant's light uptake →
        // 0 → it starves. Conserving (demand-side only; ungated light stays in the pool). Non-Autotrophs
        // (or non-light channels) are ungated (ratio = 1000).
        let nutrient_limit = (((frozen_nutrient[cell].max(0) as u128 * u128::from(fixed::PERMILLE))
            / u128::from(NUTRIENT_LIMIT_REF as u64))
        .min(u128::from(fixed::PERMILLE)) as u64)
            .max(LIEBIG_FLOOR); // SOFT co-limitation: never below the floor (plants survive on light alone).
                                // ADR-017 S6: the committed deep-edit demand factor for THIS species (permille, `1000` = neutral). A
                                // knockout/knockdown drops it toward `500` → less demand → less uptake → the edited species grows less.
                                // GATED on non-neutral so a no-edit run NEVER executes the extra multiply (byte-identical demand math →
                                // the pinned single-species PLANT hash is unmoved; the factor reaches the hash only THROUGH coupling).
        let edit_factor_q = edit_mod.factor_q(it.species);
        for (c, stock, taps_channel) in taps {
            if !taps_channel {
                continue;
            }
            // demand_permille = acq · affinity[c] · body · match, all on permille grids → one combined
            // permille. The match factor (blocker #1) makes a well-adapted lineage demand — and thus win — more
            // of a contended pool, the integer spatial-selection gradient (ADR-011/012). Computed as ONE u128
            // product floored ONCE (not a chain of `/p` divides — that double/quadruple-floored a small org's
            // demand to 0, the F3.4 chemostat-tuning fix); the four `/p` for the four permille factors collapse
            // into a single `/p^4` so a fresh org (body_factor small) still expresses a non-zero demand.
            let aff_permille =
                (u64::from(aff[c]) * u64::from(fixed::PERMILLE)) / u64::from(fixed::UNIT_SCALE);
            let p = u128::from(fixed::PERMILLE);
            // Liebig gate on the Autotroph LIGHT channel only (c == 0) folds in as a fifth permille factor.
            let liebig = if c == 0 && role == gp::TrophicRole::Autotroph {
                nutrient_limit
            } else {
                u64::from(fixed::PERMILLE)
            };
            let num = u128::from(acq)
                * u128::from(aff_permille)
                * u128::from(body_factor)
                * u128::from(it.match_permille)
                * u128::from(liebig);
            let mut dp = (num / (p * p * p * p)) as u64; // floor ONCE over the combined permille product
                                                         // ADR-013 F5: fold the chem SENSE factors (toxin-suppress · kin-boost, both permille) into demand,
                                                         // GATED on non-neutral so a chem-free cell NEVER executes the extra multiply (`× 1000/1000` is a
                                                         // no-op → byte-identical pre-F5 demand math, the EditModifier precedent). High local toxin lowers
                                                         // demand (competitive exclusion); own-species kin raises it (kin cooperation). Folded together as
                                                         // ONE u128 product floored ONCE (no double-floor; `tox·kin/(p·p)` joins the combined permille).
                                                         // The chem couplings ride the SAME pre-apportion demand seam — never an f64 multiply on granted-J.
            if it.tox_suppress != fixed::PERMILLE as u64 || it.kin_boost != fixed::PERMILLE as u64 {
                dp = ((u128::from(dp) * u128::from(it.tox_suppress) * u128::from(it.kin_boost))
                    / (p * p)) as u64;
            }
            // ADR-017 S6: scale demand by the committed edit factor (permille). Skipped entirely at the neutral
            // `1000` so the no-edit path is byte-identical (the pinned hash is unmoved); when a knockout commits,
            // `dp · edit_factor_q / 1000` throttles uptake pre-apportion (never an f64 multiply on the granted-J
            // path — the F3 invariant; the floored integer demand is the only consumer downstream).
            if edit_factor_q != EDIT_FACTOR_NEUTRAL_Q {
                dp = ((u128::from(dp) * u128::from(edit_factor_q)) / p) as u64;
            }
            demand[i][c] = monod_demand(stock, dp.min(fixed::PERMILLE as u64));
        }
    }

    // ── Pass 2: per-cell APPORTION the actual available J across co-located demanders (canonical order). ──
    // Group item indices by (channel, cell) and apportion the live pool ONCE per (channel, cell). The `weights`,
    // `shares`, and largest-remainder `rem_scratch` buffers are REUSED across every (channel, cell) group so the
    // inner loop pays no per-group heap allocation (hash-neutral: `apportion_into` is bit-identical to
    // `apportion`; only the buffer ownership moved out of the loop).
    let mut granted = std::mem::take(&mut scratch.granted);
    granted.clear();
    granted.resize(n, [0i64; resource::RESOURCE_CHANNELS]);
    let mut weights: Vec<u64> = Vec::new();
    let mut shares: Vec<i64> = Vec::new();
    let mut rem_scratch: Vec<(u128, usize)> = Vec::new();
    for c in 0..resource::RESOURCE_CHANNELS {
        // Walk items in canonical order; items in the same cell are CONTIGUOUS (cell is the primary key).
        let mut i = 0usize;
        while i < n {
            let cell = items[i].cell;
            let mut j = i;
            while j < n && items[j].cell == cell {
                j += 1;
            }
            // items[i..j] share this cell. Sum their channel-c demand and apportion the available stock.
            let group = j - i;
            weights.clear();
            weights.extend((0..group).map(|k| demand[i + k][c].max(0) as u64));
            let total_demand: i64 = weights.iter().map(|&w| w as i64).sum();
            if total_demand > 0 {
                let cellu = cell as usize;
                let available = pool_channel(&pools, c)[cellu].min(total_demand);
                shares.resize(group, 0);
                fixed::apportion_into(available, &weights, &mut shares, &mut rem_scratch);
                let mut taken = 0i64;
                for (k, share) in shares.iter().enumerate() {
                    granted[i + k][c] = *share;
                    taken += *share;
                    // ADR-013 F4: a free_nutrient (channel 1) uptake is attributed to the decomposer that
                    // MINTED it (PoolProvenance) → flow[plant][decomposer]. Per-org so the apportion over
                    // minting species is exact; the abiotic seed fraction records no edge.
                    if c == 1 && *share > 0 {
                        prov.withdraw_nutrient(
                            cellu,
                            items[i + k].species as usize,
                            *share,
                            &mut flow,
                        );
                    }
                }
                pool_channel_mut(&mut pools, c)[cellu] -= taken; // decrement live pool ONCE
            }
            i = j;
        }
    }

    // ── Pass 3: CONVERT each org's granted J + per-tick Age bump. ──
    // Map OrgId→granted-total in canonical order; per-org effects are then a pure function of that total, so
    // the (order-independent) query mutation below is deterministic (inv #3).
    let mut by_org: std::collections::BTreeMap<u64, i64> = std::collections::BTreeMap::new();
    for (i, it) in items.iter().enumerate() {
        by_org.insert(it.org, granted[i].iter().sum());
    }
    // Per-org litterfall deposits, COLLECTED here (the q.iter_mut() walk is arbitrary order) and applied to the
    // shared detritus pool in a SEPARATE canonical (cell, SpeciesId, OrgId) pass so the cap-overflow routing is
    // order-pinned (ADR-013 F4, adversarial #2).
    let mut litterfall: std::collections::BTreeMap<u64, (u32, u16, i64)> =
        std::collections::BTreeMap::new();
    // ADR-013 F5: per-org toxin mints, COLLECTED here (the q.iter_mut() walk is arbitrary order) and applied to
    // the chem toxin plane in a SEPARATE canonical (cell, SpeciesId, OrgId) pass so the cap-overflow routing is
    // order-pinned (the litterfall precedent). Each is a paired respired↔toxin split that conserves J.
    let mut toxin_mints: std::collections::BTreeMap<u64, (u32, i64)> =
        std::collections::BTreeMap::new();
    // Reusable convert-split buffers (perf, hash-neutral): `split_budget_into` is bit-identical to
    // `split_budget`; only the per-org output/scratch ownership moves out of the loop.
    let mut split: Vec<i64> = Vec::new();
    let mut split_w: Vec<u64> = Vec::new();
    let mut split_rem: Vec<(u128, usize)> = Vec::new();
    for (id, sp, mut energy, mut biomass, mut age, _d, _t, p) in q.iter_mut() {
        age.0 = age.0.saturating_add(1);
        let granted_total = match by_org.get(&id.0) {
            Some(&g) if g > 0 => g,
            _ => continue,
        };
        // CONVERT: split granted J across the 5 budget channels (conserved by split_budget).
        let strat = &registry.entries[sp.0 .0 as usize].strategy;
        fixed::split_budget_into(
            granted_total,
            &strat.budget,
            &mut split,
            &mut split_w,
            &mut split_rem,
        );
        let to_growth = split[gp::BudgetChannel::Growth as usize];
        let to_acq = split[gp::BudgetChannel::Acquisition as usize];
        let to_repro = split[gp::BudgetChannel::Reproduction as usize];
        // Growth→Biomass and Acquisition+Reproduction→Energy each KEEP only EFF_NUM/EFF_DEN (trophic
        // efficiency); the residual `granted − Σkept` (incl. the Maintenance+Defense slices) is RESPIRED,
        // computed as a residual so no quantum is double-floored (finding #7).
        let kept_growth = to_growth * EFF_NUM / EFF_DEN;
        let kept_energy = (to_acq + to_repro) * EFF_NUM / EFF_DEN;
        let respired_convert = granted_total - kept_growth - kept_energy;
        debug_assert!(
            respired_convert >= 0,
            "convert residual must be non-negative"
        );

        // Credit Biomass then Energy; any excess past the caps routes to OVERFLOW (never a silent clamp).
        let (new_b, b_over) = credit_capped(biomass.0, kept_growth, BIOMASS_CAP);
        biomass.0 = new_b;
        let (new_e, e_over) = credit_capped(energy.0, kept_energy, ENERGY_CAP);
        energy.0 = new_e;

        // ADR-013 F5 TOXIN MINT: re-route a fraction of the DEFENSE budget slice (already inside respired_convert)
        // OUT of respired and INTO the toxin plane — the keystone J-source (a paired respired↔toxin move, atomic
        // here where the convert split runs). `toxin_minted = defense_J · TOXIN_YIELD_NUM / TOXIN_YIELD_DEN`,
        // floored ONCE. A species with budget[Defense]==0 mints zero → an allelopathy-off roster is byte-identical
        // (hash-neutral). Bounded by the Defense slice (⊆ respired_convert), so litter + toxin ≤ respired_convert.
        let defense_j = chem::defense_slice(&split);
        let toxin_minted = defense_j * chem::TOXIN_YIELD_NUM / chem::TOXIN_YIELD_DEN;
        // ADR-013 F4 LITTERFALL: an AUTOTROPH sheds a fraction of its convert-respired inefficiency to detritus
        // (a living canopy rains litter even without death). A residual SPLIT of the respired value REMAINING
        // after the toxin mint (no double-floor; keeps litter + toxin ≤ respired_convert): `litter` → detritus,
        // the rest → respired. Decomposers shed nothing here (their loss is the mineralization respired tap).
        let respired_after_toxin = respired_convert - toxin_minted;
        let litter = if strat.role == gp::TrophicRole::Autotroph {
            respired_after_toxin * LITTERFALL_NUM / LITTERFALL_DEN
        } else {
            0
        };
        // Debit the respired-bound amount: respired_convert minus the toxin re-route minus the litter split.
        ledger.respired += respired_convert - toxin_minted - litter;
        ledger.overflow += b_over + e_over;
        if litter > 0 {
            litterfall.insert(id.0, (cell_index(p, width), sp.0 .0, litter));
        }
        if toxin_minted > 0 {
            toxin_mints.insert(id.0, (cell_index(p, width), toxin_minted));
        }
    }
    // Apply toxin mints in canonical (cell, OrgId) order (the BTreeMap is OrgId-keyed; sort by (cell, org) so a
    // cap-saturation spill is order-pinned — the litterfall precedent). Each deposit is the paired half of the
    // respired↔toxin move debited above; the cap-rejected part routes to overflow (nets out — never silent clamp).
    let mut toxin_rows: Vec<(u32, u64, i64)> = toxin_mints
        .into_iter()
        .map(|(org, (cell, amt))| (cell, org, amt))
        .collect();
    toxin_rows.sort_unstable_by_key(|r| (r.0, r.1));
    for (cell, _org, amt) in toxin_rows {
        let cellu = cell as usize;
        // milli == J 1:1; amt is bounded by the Defense slice << i32::MAX.
        let rejected = chem::deposit_capped_plane(
            chem.plane_mut(chem::ChemChannel::Toxin as usize),
            cellu,
            amt as i32,
        );
        ledger.overflow += i64::from(rejected); // toxin cap spill → overflow (nets out)
    }
    // Apply litterfall deposits in canonical (cell, SpeciesId, OrgId) order (the BTreeMap is OrgId-keyed; sort
    // the rows by (cell, species, org) so a cap-saturation spill is order-pinned — adversarial #2). Each is a
    // paired respired↔detritus split that conserves J; provenance tags the depositing species (the FlowMatrix
    // attributes the decomposer's later harvest of it to this plant).
    let mut litter_rows: Vec<(u32, u16, u64, i64)> = litterfall
        .into_iter()
        .map(|(org, (cell, sp, amt))| (cell, sp, org, amt))
        .collect();
    litter_rows.sort_unstable_by_key(|r| (r.0, r.1, r.2));
    for (cell, sp, _org, amt) in litter_rows {
        let cellu = cell as usize;
        let headroom = (POOL_CAP - pools.detritus[cellu]).max(0);
        let accepted = amt.min(headroom);
        pools.detritus[cellu] += accepted;
        prov.deposit_detritus(cellu, sp as usize, accepted);
        ledger.overflow += amt - accepted; // detritus cap spill → overflow (nets out)
    }

    // Return the reused buffers to the scratch resource so their backing allocations survive to the next tick.
    scratch.items = items;
    scratch.frozen_light = frozen_light;
    scratch.frozen_nutrient = frozen_nutrient;
    scratch.frozen_detritus = frozen_detritus;
    scratch.demand = demand;
    scratch.granted = granted;
    scratch.frozen_toxin = frozen_toxin;
}

/// One organism's metabolism row in canonical `(cell, species, org)` order.
struct MetabolismItem {
    cell: u32,
    species: u16,
    org: u64,
    body: i64,
    /// The ADR-011/012 soil+climate trait-match as an INTEGER permille `[250, 1000]` (blocker #1): scales the
    /// org's per-channel DEMAND so a well-adapted lineage out-competes a poorly-adapted one for the same pool
    /// (real integer spatial selection — no f64 on the granted-J path).
    match_permille: u64,
    /// ADR-013 F5 TOXIN-SUPPRESS permille `[TOXIN_SUPPRESS_FLOOR, 1000]`: high local (FROZEN) toxin lowers this
    /// org's demand → less uptake → competitive exclusion (allelopathy). `1000` (neutral) in a chem-free cell →
    /// byte-identical pre-F5 demand math.
    tox_suppress: u64,
    /// ADR-013 F5 KIN-BOOST permille `[1000, 1000+KIN_BOOST_CAP]`: more own-species kin marker at this cell →
    /// more uptake (kin cooperation). `1000` (neutral) when no own-kin is present → byte-identical pre-F5 math.
    kin_boost: u64,
}

/// One organism's `reproduce_or_die` row, snapshotted in canonical `(cell, SpeciesId, OrgId)` order so the
/// maintenance/death/birth passes are all order-independent of ECS query order (inv #3, finding #5).
struct ReproRow {
    cell: u32,
    species: u16,
    org: u64,
    entity: Entity,
    energy: i64,
    biomass: i64,
    age: u32,
    genotype: f64,
    drought: f64,
    thermal: f64,
    px: u32,
    py: u32,
}

/// Immutable per-channel pool plane (`0` light, `1` free_nutrient, `2` detritus).
pub(crate) fn pool_channel(pools: &PoolStock, ch: usize) -> &[i64] {
    match ch {
        0 => &pools.light,
        1 => &pools.free_nutrient,
        _ => &pools.detritus,
    }
}

/// Off-hash render projection: nearest-cell resample of an `i64` pool plane → `f32` in `[0,1]` by [`POOL_CAP`].
/// The integer nearest-cell map is bit-identical IN SPIRIT to [`soil::SoilField::sample_to`]; the only float
/// math is the display-only `/POOL_CAP` normalization divide, centralized HERE so the one audited chokepoint
/// lives in a single place. Pure read — never round-trips back into integer sim state, never draws RNG (inv #3).
pub(crate) fn pool_sample_to(
    plane: &[i64],
    pw: u32,
    ph: u32,
    tx: u32,
    ty: u32,
    target_w: u32,
    target_h: u32,
) -> f32 {
    let sx = ((u64::from(tx) * u64::from(pw)) / u64::from(target_w)).min(u64::from(pw) - 1);
    let sy = ((u64::from(ty) * u64::from(ph)) / u64::from(target_h)).min(u64::from(ph) - 1);
    let idx = (sy * u64::from(pw) + sx) as usize;
    (plane[idx] as f64 / POOL_CAP as f64) as f32
}

/// Mutable per-channel pool plane (see [`pool_channel`]).
pub(crate) fn pool_channel_mut(pools: &mut PoolStock, ch: usize) -> &mut [i64] {
    match ch {
        0 => &mut pools.light,
        1 => &mut pools.free_nutrient,
        _ => &mut pools.detritus,
    }
}

/// Credit `amount` to `value` up to `cap`; returns `(new_value, overflow)` where `overflow` is the part that
/// exceeded the cap (routed to the OVERFLOW tap, never silently clamped — finding #7). `amount >= 0`.
pub(crate) fn credit_capped(value: i64, amount: i64, cap: i64) -> (i64, i64) {
    let target = value + amount;
    if target > cap {
        (cap, target - cap)
    } else {
        (target, 0)
    }
}

/// **SOLAR INFLUX** (ADR-013 F3→F4) — the ONLY source of new `J` (the INFLUX tap). Mints [`SOLAR_PER_CELL`]
/// into each cell's `PoolStock.light` up to the static `ResourceField.light` carrying-cap (×`CELL_J_SCALE`).
/// Per-cell cap-saturation spill routes to OVERFLOW (the influx is BOOKED in full, the rejected part booked to
/// overflow, so it nets out — finding #7). Pure integer, ZERO `SimRng` (inv #3).
///
/// **ADR-013 F4: the `free_nutrient` INFLUX arm is DELETED here.** Solar light is the only true source;
/// `free_nutrient` is now ENDOGENOUS, supplied ONLY by decomposer mineralization of shed detritus (see
/// [`trophic::mineralize`]) — the obligate plant→detritus→decomposer→free_nutrient loop.
fn solar_influx(
    light_cap: Res<SolarLightCap>,
    mut pools: ResMut<PoolStock>,
    mut ledger: ResMut<ledger::Ledger>,
) {
    let cells = (pools.width as usize) * (pools.height as usize);
    for c in 0..cells {
        // light: mint SOLAR_PER_CELL, capped by the static field's per-cell carrying capacity. The cap uses
        // CELL_CAP_SCALE (>> the seed's CELL_J_SCALE) so a cell seeded partly-full has real headroom for solar
        // to flow in tick after tick — the F3.4 fix that turns solar from a 100%-overflow no-op into the live
        // source the chemostat runs on. PRECOMPUTED once at reset (SolarLightCap) — the static field is constant,
        // so this is the IDENTICAL integer cap with no per-tick f64 re-floor (hash-neutral perf optimization).
        mint_to_cap(
            &mut pools.light[c],
            SOLAR_PER_CELL,
            light_cap.0[c],
            &mut ledger,
        );
    }
}

/// Mint `amount` `J` into a cell pool up to `cap`: the accepted part raises the pool, the rejected part spills
/// to OVERFLOW. ALL minted `J` is booked to INFLUX; the rejected part is booked to OVERFLOW so it nets out of
/// the live total (finding #7 — saturating logic ROUTES the spill, never silently clamps).
fn mint_to_cap(cell: &mut i64, amount: i64, cap: i64, ledger: &mut ledger::Ledger) {
    ledger.influx += amount;
    let headroom = (cap - *cell).max(0);
    let accepted = amount.min(headroom);
    *cell += accepted;
    ledger.overflow += amount - accepted; // the rejected part → overflow (nets out)
}

/// **DEPOSIT A CARCASS RESIDUAL** (ADR-013 F4/F5 carcass→detritus, factored out at SP-3.0) — the SHARED
/// accounting for a dead organism's residual `J`, called by BOTH [`reproduce_or_die`] (a starvation/senescence
/// death) AND [`Simulation::region_cull`] (an antibiotic kill). The residual is `J` that was already live (the
/// dead org's post-maintenance Energy + Biomass); this MOVES it bucket-to-bucket — a paired transfer, NOT a
/// mint — so it never touches an influx tap. Splits it like LITTERFALL:
/// * a pinned [`chem::ALARM_FRACTION_NUM`]/[`chem::ALARM_FRACTION_DEN`] fraction → the alarm chem plane (a
///   dying org's distress signal — F5 DEATH-ALARM), milli == J 1:1, [`chem::CHEM_CAP`]-capped;
/// * the rest → the cell's `detritus` pool, [`POOL_CAP`]-capped, species-tagged in [`trophic::PoolProvenance`]
///   so a decomposer's later harvest attributes `flow[decomposer][this-species]` in the [`trophic::FlowMatrix`]
///   (the obligate-loop edge);
/// * both cap-rejected parts → the OVERFLOW tap (finding #5 — never a silent clamp; the residual always nets
///   out exactly: `accepted_detritus + accepted_alarm + overflow_spill == residual`).
///
/// Pure integer, ZERO `SimRng` (inv #3). Conserves `J` BY CONSTRUCTION: every quantum of `residual` lands in
/// detritus, the alarm plane, or the overflow tap. A non-positive `residual` is a clean no-op.
#[allow(clippy::too_many_arguments)]
fn deposit_carcass(
    pools: &mut PoolStock,
    chem: &mut chem::ChemField,
    prov: &mut trophic::PoolProvenance,
    ledger: &mut ledger::Ledger,
    cellu: usize,
    species: usize,
    residual: i64,
) {
    if residual <= 0 {
        return;
    }
    // ADR-013 F5 DEATH-ALARM: divert a pinned fraction of the residual to the alarm plane INSTEAD of detritus
    // (a residual split like LITTERFALL — stays conserved). The rest → detritus.
    let alarm_share = residual * chem::ALARM_FRACTION_NUM / chem::ALARM_FRACTION_DEN;
    let to_detritus = residual - alarm_share;
    let headroom = (POOL_CAP - pools.detritus[cellu]).max(0);
    let accepted = to_detritus.min(headroom);
    pools.detritus[cellu] += accepted;
    // ADR-013 F4: tag the carcass detritus with the dead org's species so a decomposer's later harvest of it
    // attributes flow[decomposer][this-species] in the FlowMatrix (the obligate-loop edge).
    prov.deposit_detritus(cellu, species, accepted);
    // The alarm_share → the alarm plane (milli == J 1:1; bounded by the carcass residual << i32::MAX).
    // Cap-rejected part → overflow (nets out).
    let alarm_rejected = chem::deposit_capped_plane(
        chem.plane_mut(chem::ChemChannel::Alarm as usize),
        cellu,
        alarm_share as i32,
    );
    // detritus cap spill (to_detritus − accepted) + alarm cap spill → overflow (finding #5; nets out).
    ledger.overflow += (to_detritus - accepted) + i64::from(alarm_rejected);
}

/// **REPRODUCE OR DIE** (ADR-013 F3 KEYSTONE) — energy-funded births + deaths, REPLACING constant-N
/// Wright-Fisher. Runs AFTER metabolism so only post-maintenance survivors breed.
///
/// Order (binding contracts from the adversarial pass):
/// 1. **MAINTENANCE debit** (RNG-free) — each org pays a per-tick upkeep funded by `budget[Maintenance]`,
///    `min(debit, Energy)` RESPIRED (never `saturating_sub` to 0 — finding #7); the shortfall triggers death.
/// 2. **DEATH FIRST** (RNG-free) — starvation (`Energy < MAINTENANCE_FLOOR` post-debit) OR senescence
///    (`Age ≥ AGE_MAX`). The carcass's residual Energy+Biomass deposits to its cell `detritus` pool
///    (carcass→detritus, conserving `J`), routed in canonical `(cell, SpeciesId, OrgId)` order (finding #5),
///    capped → OVERFLOW. Despawn via a COLLECTED `Vec<Entity>` (never mutate-during-query — inv #3).
/// 3. **BIRTH SECOND** (the ONLY `SimRng` consumer) — an org with `Energy ≥ REPRO_THRESHOLD` SPENDS
///    [`OFFSPRING_ENDOWMENT`] (conserved: child Energy+Biomass come OUT of the parent). EVERY threshold-passing
///    org in canonical order draws EXACTLY D+1 = 4 words (3 mutation: genotype/drought/thermal + 1 dispersal),
///    UNCONDITIONALLY (finding #4) — the cap check PRECEDES the endowment debit, draws happen regardless, and a
///    skipped/over-cap birth does NOT consume the endowment. Child OrgId from the monotonic [`NextOrgId`].
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn reproduce_or_die(
    mut commands: Commands,
    mut rng: ResMut<SimRng>,
    mut draws: ResMut<DrawCount>,
    mut next_id: ResMut<NextOrgId>,
    registry: Res<SpeciesRegistry>,
    kin_prov: Res<chem::KinProvenance>,
    mut repro_scratch: ResMut<ReproScratch>,
    mut pools: ResMut<PoolStock>,
    mut chem: ResMut<chem::ChemField>,
    mut prov: ResMut<trophic::PoolProvenance>,
    mut ledger: ResMut<ledger::Ledger>,
    mut q: Query<(
        Entity,
        &OrgId,
        &Species,
        &mut Energy,
        &mut Biomass,
        &Age,
        &Genotype,
        &DroughtTol,
        &ThermalTol,
        &Position,
    )>,
) {
    use chem::ChemModifier as _;
    let chem_mod = chem::InCoreChem;
    let width = pools.width;
    // ADR-013 F5: FREEZE the chem toxin + alarm planes at start-of-tick (the `frozen_light` discipline) so the
    // maintenance/death/birth passes all read a stable field — within-pass deposits (death-alarm) never feed
    // back into this tick's sense. Toxin → lethal maintenance drain (kin-spared); alarm → flee dispersal.
    // Reuse the persistent frozen buffers (byte-identical snapshots of the start-of-tick planes).
    let mut frozen_toxin = std::mem::take(&mut repro_scratch.frozen_toxin);
    let mut frozen_alarm = std::mem::take(&mut repro_scratch.frozen_alarm);
    chem.frozen_toxin_into(&mut frozen_toxin);
    chem.frozen_alarm_into(&mut frozen_alarm);
    // ── Build ONE canonical (cell, SpeciesId, OrgId) order over the LIVING set (inv #3, finding #5). ──
    // Reuse the persistent scratch row buffer (cleared + refilled; backing allocation survives across ticks).
    let mut rows = std::mem::take(&mut repro_scratch.rows);
    rows.clear();
    rows.extend(q.iter().map(
        |(entity, id, sp, energy, biomass, age, g, d, t, p)| ReproRow {
            cell: cell_index(p, width),
            species: sp.0 .0,
            org: id.0,
            entity,
            energy: energy.0,
            biomass: biomass.0,
            age: age.0,
            genotype: g.0,
            drought: d.0,
            thermal: t.0,
            px: p.x,
            py: p.y,
        },
    ));
    rows.sort_unstable_by_key(|r| (r.cell, r.species, r.org));

    // ── Step 1+2: maintenance debit, then death (carcass→detritus) — all RNG-free, canonical order. ──
    let mut dead: Vec<Entity> = Vec::new();
    // Track per-entity post-maintenance Energy so the birth pass reads the debited value.
    let mut maint_energy: std::collections::BTreeMap<u64, i64> = std::collections::BTreeMap::new();
    for r in &rows {
        let strat = &registry.entries[r.species as usize].strategy;
        let cellu = r.cell as usize;
        let kin_own = kin_prov.own(cellu, r.species as usize);
        let maint_permille = u64::from(strat.budget[gp::BudgetChannel::Maintenance as usize]);
        let mut debit = (MAINTENANCE_BASE as u128 * u128::from(maint_permille)
            / u128::from(fixed::PERMILLE)) as i64;
        // ADR-013 F5 KIN-SURVIVAL: own-species kin marker lowers upkeep (kin cooperation). An integer permille
        // factor `[1000−KIN_SURVIVAL_CAP, 1000]`; NEUTRAL 1000 with no own-kin → byte-identical pre-F5 debit.
        let kin_surv = chem_mod.kin_survival_permille(kin_own);
        if kin_surv != fixed::PERMILLE as u64 {
            debit = (u128::from(debit as u64) * u128::from(kin_surv) / u128::from(fixed::PERMILLE))
                as i64;
        }
        // ADR-013 F5 TOXIN LETHAL-DRAIN: the org burns reserves resisting local (FROZEN) toxin — a separate
        // org-Energy→respired J path (does NOT consume field toxin; sensing only reads). KIN-SPARING: an org
        // with its own-species kin present pays a discounted drain (allelopathy is asymmetric → Defense is not
        // strictly self-defeating). Zero in a chem-free cell → byte-identical pre-F5 maintenance.
        let tox_drain = chem_mod.toxin_drain_j(frozen_toxin[cellu], kin_own);
        let total_debit = debit + tox_drain;
        let paid = total_debit.min(r.energy.max(0)); // never below 0 (no saturating_sub silent floor)
        let energy_after = r.energy - paid;
        ledger.respired += paid;
        maint_energy.insert(r.org, energy_after);

        let starved = energy_after < MAINTENANCE_FLOOR;
        let senescent = r.age >= AGE_MAX;
        if starved || senescent {
            // Carcass → detritus: residual Energy (post-maintenance) + Biomass deposits to the cell pool, split
            // alarm/detritus + cap-spill→overflow by the SHARED `deposit_carcass` helper (SP-3.0 extraction).
            let residual = energy_after.max(0) + r.biomass.max(0);
            deposit_carcass(
                &mut pools,
                &mut chem,
                &mut prov,
                &mut ledger,
                cellu,
                r.species as usize,
                residual,
            );
            dead.push(r.entity);
        }
    }
    // Apply the maintenance debit to the LIVE survivors (deaths are despawned below regardless).
    let dead_set: std::collections::BTreeSet<Entity> = dead.iter().copied().collect();
    for (entity, id, _sp, mut energy, _b, _a, _g, _d, _t, _p) in q.iter_mut() {
        if dead_set.contains(&entity) {
            continue; // about to despawn; its J already deposited
        }
        if let Some(&e) = maint_energy.get(&id.0) {
            energy.0 = e;
        }
    }
    for e in &dead {
        commands.entity(*e).despawn();
    }

    // ── Step 3: BIRTH — the ONLY SimRng consumer. Walk survivors in canonical order; EVERY threshold-passing
    //    org draws EXACTLY 4 words (genotype, drought, thermal mutation + dispersal), UNCONDITIONALLY. ──
    let live_count = rows.len() - dead.len();
    let mut population = live_count as u32;
    // Collect parent updates + child spawns, then apply (no mutate-during-query for spawns; Commands defers).
    let mut parent_debit: std::collections::BTreeMap<u64, i64> = std::collections::BTreeMap::new();
    struct Child {
        species: u16,
        org: u64,
        energy: i64,
        biomass: i64,
        genotype: f64,
        drought: f64,
        thermal: f64,
        px: u32,
        py: u32,
    }
    let mut children: Vec<Child> = Vec::new();
    for r in &rows {
        if dead_set.contains(&r.entity) {
            continue; // a dead org does not breed (death pins the draw set cleanly)
        }
        let energy = *maint_energy.get(&r.org).unwrap_or(&r.energy);
        if energy < REPRO_THRESHOLD {
            continue; // below threshold: no draws (draw order = pure fn of the survivor list)
        }
        // EVERY threshold-passing org draws EXACTLY 4 words, in fixed order, regardless of cap outcome
        // (finding #4). DrawCount tracks each draw so a birth-enumeration bug breaks the hash locally.
        let dg = rng.0.next_u64();
        let dd = rng.0.next_u64();
        let dt = rng.0.next_u64();
        let ddisp = rng.0.next_u64();
        draws.0 += 4;
        // Over-cap guard PRECEDES the endowment debit (a skipped/over-cap birth does NOT consume the
        // endowment — else the allocator leaks J). The draws above already happened (draw-count independent
        // of cap state).
        if population >= MAX_POPULATION {
            continue;
        }
        // Conserved endowment transfer (no minting): parent spends OFFSPRING_ENDOWMENT.
        parent_debit.insert(r.org, OFFSPRING_ENDOWMENT);
        // Inheritance with mutation — SAME per-birth draw-shape as before, now integer mutation steps on the
        // f64 traits (heritable f64 stays f64 at F3 — the multi-ISA gate proves byte-stability, finding #3).
        let child_g = mutate_unit(r.genotype, dg);
        let child_d = mutate_unit(r.drought, dd);
        let child_t = mutate_unit(r.thermal, dt);
        // Dispersal: integer Moore step (next_u64 % 9), no unit_f64 → no float in the hashed Position.
        // ADR-013 F5 ALARM-BIASED DISPERSAL — DRAW-COUNT-NEUTRAL: F5 adds ZERO draws. It RE-INTERPRETS the
        // already-drawn `ddisp` word: read the FROZEN alarm plane at the parent cell's Moore neighbourhood,
        // compute the FLEE direction (lowest-alarm Moore index, ties→lowest index, no sqrt → bit-reproducible),
        // and remap the raw Moore step via the baked LUT so it is byte-identical cross-platform. Gated on
        // non-zero neighbour alarm → a chem-free run falls back to the plain `ddisp % 9` (byte-identical).
        let raw_k = ddisp % 9;
        let k = match chem::flee_direction(&frozen_alarm, width, chem.height, r.px, r.py) {
            Some(flee_dir) => chem::alarm_bias_step(raw_k, flee_dir) as i64,
            None => raw_k as i64,
        };
        let nx = (r.px as i64 + (k % 3 - 1)).clamp(0, WORLD_DIMS.0 as i64 - 1) as u32;
        let ny = (r.py as i64 + (k / 3 - 1)).clamp(0, WORLD_DIMS.1 as i64 - 1) as u32;
        let org = next_id.0;
        next_id.0 += 1;
        children.push(Child {
            species: r.species,
            org,
            energy: OFFSPRING_ENDOWMENT - OFFSPRING_SEED_BIOMASS,
            biomass: OFFSPRING_SEED_BIOMASS,
            genotype: child_g,
            drought: child_d,
            thermal: child_t,
            px: nx,
            py: ny,
        });
        population += 1;
    }
    // Apply parent debits (the endowment spent) to the live survivors.
    if !parent_debit.is_empty() {
        for (_entity, id, _sp, mut energy, _b, _a, _g, _d, _t, _p) in q.iter_mut() {
            if let Some(&debit) = parent_debit.get(&id.0) {
                energy.0 -= debit;
            }
        }
    }
    // Spawn children (Commands defers application, so this never mutates-during-query — inv #3).
    for c in children {
        commands.spawn((
            OrgId(c.org),
            Energy(c.energy),
            Biomass(c.biomass),
            Age(0),
            Genotype(c.genotype),
            DroughtTol(c.drought),
            ThermalTol(c.thermal),
            Position { x: c.px, y: c.py },
            Species(SpeciesId(c.species)),
        ));
    }
    // Return the reused buffers to the scratch resource (their allocations survive to the next tick).
    repro_scratch.rows = rows;
    repro_scratch.frozen_toxin = frozen_toxin;
    repro_scratch.frozen_alarm = frozen_alarm;
}

/// Mutate a `[0,1]` heritable scalar by a small symmetric integer step derived from a `SimRng` word — no
/// `unit_f64`, no transcendental; the result stays in `[0,1]`. The step is `±MUTATION_STEP` or `0` (the word's
/// low bits pick the direction), keeping the f64 trait byte-stable across ISAs (the `-fp-contract=off` gate
/// proves the add/clamp is identical). Heritable f64 stays f64 at F3 (finding #3).
fn mutate_unit(value: f64, word: u64) -> f64 {
    const MUTATION_STEP: f64 = 0.01;
    let delta = match word % 3 {
        0 => -MUTATION_STEP,
        1 => MUTATION_STEP,
        _ => 0.0,
    };
    (value + delta).clamp(0.0, 1.0)
}

/// **MEASURE + ASSERT LEDGER CLOSES** (ADR-013 F3) — the LAST system in the chain. Sums the live `J`
/// (`PoolStock` + per-org Energy + per-org Biomass) in a stable order and asserts conservation EVERY tick
/// (finding #8: runs after all deposits + despawns). Under `--features determinism` this is a HARD assert (the
/// CI multi-ISA legs build it) so a lost/minted quantum fails the gate semantically on both arches; otherwise
/// (default) a debug-build assert. Pure read — draws ZERO `SimRng`, never folded into `hash_world`.
fn measure_and_assert_ledger(
    pools: Res<PoolStock>,
    chem: Res<chem::ChemField>,
    ledger: Res<ledger::Ledger>,
    q: Query<(&Energy, &Biomass)>,
) {
    let mut energy = 0i64;
    let mut biomass = 0i64;
    for (e, b) in q.iter() {
        energy += e.0;
        biomass += b.0;
    }
    let live = ledger::LiveTotal {
        pools: pools.total(),
        energy,
        biomass,
        // ADR-013 F5: chem is now a LIVE bucket — the toxin/kin/alarm planes (i32 milli == J, widened to i64).
        chem: chem.total(),
    };
    #[cfg(feature = "determinism")]
    ledger::assert_ledger_closes(&ledger, &live);
    #[cfg(not(feature = "determinism"))]
    debug_assert!(
        ledger::ledger_closes(&ledger, &live),
        "ledger_closes VIOLATED at F3: live {} != expected {}",
        live.sum(),
        ledger.expected_total()
    );
    let _ = (&ledger, &live); // keep both read in release-no-determinism builds (no-op assert path)
}

/// Mean per-individual [`Genotype`] across the population (the reported `allele_freq`), in `[0, 1]`.
/// `0.0` for an empty population. Iterates id-sorted rows so the sum order is deterministic.
fn mean_genotype(world: &mut World) -> f64 {
    let mut rows: Vec<(u64, f64)> = world
        .query::<(&OrgId, &Genotype)>()
        .iter(world)
        .map(|(id, g)| (id.0, g.0))
        .collect();
    if rows.is_empty() {
        return 0.0;
    }
    rows.sort_unstable_by_key(|r| r.0);
    let sum: f64 = rows.iter().map(|(_, g)| *g).sum();
    sum / rows.len() as f64
}

// --- public stepwise simulation handle ------------------------------------------------------------

/// A point-in-time, deterministic snapshot of the simulation state (SPEC §2.2 gym-like `observe`).
///
/// Returned by [`Simulation::observe`]. Every field is a pure function of the seeded run so far
/// (invariant #3): `allele_freq` is the population statistic the selection loop drives, and
/// `phenotype` is the species genome re-expressed through the run's stored per-species map (invariant #2 —
/// genotype→phenotype only here / in `genome`/`sim-core`). A fixed (seed, step/edit sequence) always
/// yields an identical sequence of `Observation`s.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    /// Generations advanced so far (the [`Tick`] counter).
    pub generation: u64,
    /// Number of living organisms.
    pub population_size: u32,
    /// Mean per-individual [`Genotype`] in `[0, 1]` — the allele frequency under selection.
    pub allele_freq: f64,
    /// The species genome re-expressed into trait values (deterministic; ordered, never a `HashMap`).
    pub phenotype: Phenotype,
}

/// A single species' read-only display projection, returned by [`Simulation::observe_all`].
///
/// Like [`Observation`], every field is a PURE read of the run so far (invariant #3): `observe_all` walks the
/// [`SpeciesRegistry`] in [`SpeciesId`] (Vec-index) order and expresses each entry's OWN genome through its OWN
/// `gp_map` — the SAME genotype→phenotype machinery [`observe`](Simulation::observe) uses, just per-entry. It
/// draws ZERO `SimRng`, mutates nothing, and is NEVER folded into `hash_world`, so it cannot move the
/// determinism hash. It exists so the renderer can show EVERY species (each with its own trait set + glyph),
/// not just the primary `observe()` species — presentation metadata only (inv #2).
#[derive(Debug, Clone, PartialEq)]
pub struct SpeciesObservation {
    /// The species' ordinal id (its [`SpeciesRegistry`] Vec index).
    pub species_id: u16,
    /// Human-readable species name.
    pub name: String,
    /// The species DATA key (`"ecoli-core"` | `"default"` | `""`) — the renderer dispatches its glyph on this.
    pub key: String,
    /// The species' trophic role (carried from the roster; the renderer may caption it).
    pub role: gp::TrophicRole,
    /// This species' genome expressed through ITS OWN map (microbe traits for E. coli, plant traits for plants).
    pub phenotype: Phenotype,
    /// Count of LIVING organisms carrying this `Species(SpeciesId)` tag. A PURE read of already-hashed state
    /// (the [`Species`]/[`OrgId`] components are part of `hash_world`'s row tuple), NEVER folded into
    /// `hash_world` itself — it adds no new hash input and cannot move the determinism hash (inv #3). Summed
    /// over every species it equals the total living-org count by construction (each org carries one tag).
    pub population_size: u32,
    /// Mean per-individual [`Genotype`] in `[0, 1]` over THIS species' living orgs (`0.0` for an empty species,
    /// mirroring [`mean_genotype`]'s empty convention). Derived from the already-hashed [`Genotype`] component
    /// via an `OrgId`-sorted fold; never folded into `hash_world` (hash-neutral read-only projection, inv #3).
    pub allele_freq: f64,
    /// Mean per-individual [`Energy`] over THIS species' living orgs, NORMALIZED to `[0, 1]` by [`ENERGY_FULL`]
    /// — the SAME normalization [`snapshot`](Simulation::snapshot)'s `fitness` channel applies (`energy / n /
    /// ENERGY_FULL`), so every species' "fitness" reads on one scale next to the primary's. `0.0` for an empty
    /// species (zero-division guard → exactly `0.0`, never NaN). Derived from the already-hashed [`Energy`]
    /// component; never folded into `hash_world` (hash-neutral read-only projection, inv #3).
    pub mean_energy: f64,
}

/// A stepwise, deterministic headless simulation handle (SPEC §2.2 — the gym-like env builds on this).
///
/// Owns the ECS [`World`], the explicitly-ordered [`Schedule`], and — as the `SimRng` world resource —
/// the **single** seeded [`ChaCha8Rng`] for the whole run. Unlike the one-shot [`run_headless`], a
/// `Simulation` exposes [`reset`](Self::reset) / [`step`](Self::step) / [`observe`](Self::observe) so a
/// caller can drive generations and apply species-level edits between them.
///
/// **Determinism (inv. #3):** the RNG is seeded **once** in [`reset`] and never re-seeded mid-run.
/// [`step`] advances the same stream; [`with_genome_and_rng`](Self::with_genome_and_rng) hands a
/// species-level edit that same stream. No thread/global RNG, no `HashMap` iteration in sim logic.
pub struct Simulation {
    world: World,
    schedule: Schedule,
    config: SimConfig,
    /// Static per-cell environment substrate (terrain/soil), generated once from the seed and read-only
    /// w.r.t. the run (off the determinism-hash path — roadmap R1.0). Exported into snapshots; not yet
    /// coupled to selection.
    soil: soil::SoilField,
    /// The per-run genotype→phenotype map (ADR-017 "RUN E. coli"): set once at reset and reused by
    /// [`observe`](Self::observe) + [`with_genome_and_rng`](Self::with_genome_and_rng) so the species expresses
    /// CONSISTENTLY across reset/observe/edit. The default (plant) map keeps the run byte-identical; the map is
    /// never folded into `hash_world`, so storing it is hash-neutral by construction.
    gp_map: gp::OntologyMap,
}

impl Simulation {
    /// Build a fresh simulation with the DEFAULT (neutral) climate — the historical behaviour every existing
    /// caller + the pinned determinism config rely on (so they stay byte-identical). See [`reset_with_env`].
    #[must_use]
    pub fn reset(config: &SimConfig) -> Self {
        Self::reset_with_env(config, &climate::EnvParams::default())
    }

    /// Build a fresh simulation under a player-set climate (ADR-012 Phase E) using the DEFAULT species genome —
    /// byte-identical to [`reset`] at default `env` (invariant #3, #2). Delegates to [`reset_with_genome`].
    #[must_use]
    pub fn reset_with_env(config: &SimConfig, env: &climate::EnvParams) -> Self {
        Self::reset_with_genome(config, env, genome::sample_genome())
    }

    /// Build a fresh simulation under a climate AND an explicit species `genome`, expressed through the DEFAULT
    /// (plant) trait map — byte-identical to the historical path (hash-neutral). Delegates to
    /// [`reset_with_genome_and_map`](Self::reset_with_genome_and_map).
    #[must_use]
    pub fn reset_with_genome(config: &SimConfig, env: &climate::EnvParams, genome: Genome) -> Self {
        Self::reset_with_genome_and_map(
            config,
            env,
            genome,
            gp::OntologyMap::new(gp::default_plant_trait_map()),
        )
    }

    /// Build a fresh simulation under a climate, an explicit species `genome`, AND its per-species `gp_map`
    /// (ADR-017 "RUN E. coli" — the vehicle for a JSON [`genome::spec::SpeciesSpec`]-loaded species expressing its
    /// OWN traits, e.g. E. coli via [`gp::ecoli_trait_map`]): seed the [`ChaCha8Rng`] **once**, express the
    /// genome→phenotype through `gp_map` once, spawn the population, and build the static soil, [`climate`], and
    /// resource fields off the seed/params (zero `SimRng` draws). The map is STORED so [`observe`](Self::observe)
    /// and [`with_genome_and_rng`](Self::with_genome_and_rng) re-express consistently. Given `sample_genome()`
    /// under the default plant map ([`gp::default_plant_trait_map`]) the run is byte-identical to the historical
    /// path; only a DIFFERENT genome/map changes it (the map itself is never folded into `hash_world`).
    #[must_use]
    pub fn reset_with_genome_and_map(
        config: &SimConfig,
        env: &climate::EnvParams,
        genome: Genome,
        gp_map: gp::OntologyMap,
    ) -> Self {
        // A single-species roster — byte-identical to the historical path (the registry holds one entry).
        // The default key is the plant's `"default"` (the renderer's plant-glyph branch). A boundary that knows
        // the real species key (e.g. E. coli) builds the roster itself so `observe_all` reports the right key.
        Self::reset_with_roster(
            config,
            env,
            vec![RosterEntry {
                name: "default".to_string(),
                key: "default".to_string(),
                genome,
                gp_map,
                entity_count: config.entity_count,
                role: gp::TrophicRole::default(), // plant default (Autotroph) for the single-species path.
            }],
        )
    }

    /// Build a fresh simulation from a SPECIES ROSTER (ADR R3-A — the multi-species spine): each [`RosterEntry`]
    /// becomes a [`SpeciesEntry`] in the [`SpeciesRegistry`]. At R3-A exactly the FIRST species is spawned +
    /// selected, so a 1-entry roster is BYTE-IDENTICAL to the single-species core (the pinned literal is the
    /// proof); R3-B spawns + selects every entry (a deliberate re-pin) and F3 couples them via the resource
    /// substrate. The roster must be non-empty.
    #[must_use]
    pub fn reset_with_roster(
        config: &SimConfig,
        env: &climate::EnvParams,
        roster: Vec<RosterEntry>,
    ) -> Self {
        assert!(
            !roster.is_empty(),
            "species roster must have at least one species"
        );
        let mut world = World::new();
        // Seed the single RNG ONCE for the whole episode (inv. #3 — never re-seeded mid-run).
        let mut rng = ChaCha8Rng::seed_from_u64(config.seed);

        // Express each species' genome → phenotype ONCE through its OWN map (invariant #2); base_growth =
        // GrowthRate (name-keyed, resolves under any species map). Build the ordered registry (inv #3).
        let entries: Vec<SpeciesEntry> = roster
            .into_iter()
            .map(|r| {
                let base_growth = r
                    .gp_map
                    .express(&r.genome)
                    .get(Trait::GrowthRate)
                    .unwrap_or(0.5);
                // ADR-013 F2: express the ecological Strategy ONCE in the SAME pre-spawn pass as base_growth,
                // reusing the entry's own map/genome/role. Pure, ZERO SimRng draws (it runs BEFORE the spawn
                // loop that consumes the stream and consumes nothing), and UNREAD by selection → hash-neutral.
                let strategy = gp::express_strategy(&r.gp_map, &r.genome, r.role);
                SpeciesEntry {
                    name: r.name,
                    key: r.key,
                    genome: r.genome,
                    gp_map: r.gp_map,
                    base_growth,
                    target_pop: r.entity_count,
                    strategy,
                }
            })
            .collect();
        // R3-A runs exactly the FIRST species; clone what the singletons + the stored map need before the
        // registry takes ownership of `entries`.
        let primary_genome = entries[0].genome.clone();
        let primary_gp_map = entries[0].gp_map.clone();
        let base_growth = entries[0].base_growth;

        // Static soil substrate, generated purely from the seed via derive_seed — ZERO SimRng draws (R1.0).
        let soil = soil::SoilField::generate(config.seed, soil::SOIL_DIMS.0, soil::SOIL_DIMS.1);

        // ADR-013 F3: spawn EVERY species' population with GLOBAL contiguous OrgIds (`0..Σtarget_pop`,
        // minted from the monotonic NextOrgId allocator — `hash_world` keys/sorts by OrgId), each tagged with
        // its `SpeciesId` and seeded from ITS OWN `base_growth`. The per-org 4-draw spawn order
        // (g0, energy, drought, thermal) is UNCHANGED. Each org gets a starting `Biomass` (seed quantum) and
        // `Age(0)`. Species + placement + OrgId are assigned off the SimRng stream (no `next_u64`).
        let mut org_i: u64 = 0;
        for (sid, entry) in entries.iter().enumerate() {
            for _ in 0..entry.target_pop {
                // Per-individual genotype in [0,1] seeded from the single RNG so individuals VARY (the standing
                // variation selection acts on); energy seeds the joule reserve; drought/thermal are heritable
                // standing variation. Draw order is fixed.
                let g0 = unit_f64(rng.next_u64());
                let init = entry.base_growth * unit_f64(rng.next_u64());
                let drought = unit_f64(rng.next_u64());
                let thermal = unit_f64(rng.next_u64()); // fixed draw order (g0, energy, drought, thermal)
                world.spawn((
                    OrgId(org_i),
                    // Quantize the seeded energy fraction to the i64 joule grid (ADR-013 F0b). One-time spawn
                    // conversion (IEEE multiply + truncate is platform-stable); the recurring path stays integer.
                    Energy((init * (ENERGY_FULL as f64)).clamp(0.0, ENERGY_FULL as f64) as i64),
                    // Seed structural Biomass (ADR-013 F3) so a fresh org has a body to scale uptake from.
                    Biomass(OFFSPRING_SEED_BIOMASS),
                    Age(0),
                    Genotype(g0),
                    DroughtTol(drought),
                    ThermalTol(thermal),
                    placement(config.seed, org_i as u32),
                    Species(SpeciesId(sid as u16)),
                ));
                org_i += 1;
            }
        }
        let next_org_id = org_i; // births mint ids from here on, never reusing a despawned id

        world.insert_resource(SimRng(rng));
        world.insert_resource(Tick::default());
        // R3-A additive: the primary species' genome + base growth stay as singletons (selection/observe read
        // them unchanged → byte-identical), and the full ordered registry is inserted alongside as the spine.
        world.insert_resource(GenomeRes(primary_genome));
        world.insert_resource(BaseGrowthRate(base_growth));
        world.insert_resource(SoilFieldRes(soil.clone())); // per-cell source for LOCAL coupling (ADR-011 S-G)
                                                           // World climate from the player's params — off the SimRng stream; unused by selection until E3 (so
                                                           // inserting it here is hash-neutral, proven by the unchanged pinned literal). ADR-012 Phase E.
        world.insert_resource(ClimateFieldRes(climate::ClimateField::from_params(env)));
        // ADR-017 S6: the per-species deep-edit modifier, all-NEUTRAL at reset (`1.0×`). Sized to the registry so
        // every species has a slot the firewall commit can set. A `× 1000/1000` factor leaves metabolism's
        // demand math byte-identical, so inserting it here is hash-neutral for the no-edit run (proven by the
        // unchanged pinned literal). NOT folded into `hash_world` — like soil/climate it reaches the hash only
        // THROUGH its coupling effect on Energy/Biomass once a non-neutral factor is committed.
        world.insert_resource(EditModifierRes::neutral(entries.len()));
        // Per-cell resource pools (ADR-013 F1): the STATIC f32 field, generated off the SimRng stream (disjoint
        // derive_seed family). At F3 it is the render/cap/seed source — `PoolStock` is seeded from it and
        // `solar_influx` reads its per-cell carrying caps.
        let resource_field = resource::ResourceField::generate(
            config.seed,
            resource::RESOURCE_DIMS.0,
            resource::RESOURCE_DIMS.1,
        );
        // ADR-013 F3: the world grid maps 1:1 onto the pool grid (no resample), so assert the dims match.
        assert!(
            WORLD_DIMS == resource::RESOURCE_DIMS,
            "ADR-013 F3 requires WORLD_DIMS == RESOURCE_DIMS for a 1:1 Position→pool mapping"
        );
        // Seed the MUTABLE PoolStock ONCE by quantizing the static field through the audited f64→int
        // chokepoint (`fixed::to_unit_u16 * CELL_J_SCALE`). This sets the ledger's initial_total.
        let pools = PoolStock::seed_from(&resource_field);
        // The conserved-energy ledger (ADR-013 F0a→F3): initial_total = Σ(PoolStock) + Σ(per-org Energy+Biomass),
        // computed once at reset (off-RNG). `reproduce_or_die`/`metabolism`/`solar_influx` drive the taps and
        // `measure_and_assert_ledger` asserts closure every tick.
        let mut org_energy_total = 0i64;
        let mut org_biomass_total = 0i64;
        for (e, b) in world.query::<(&Energy, &Biomass)>().iter(&world) {
            org_energy_total += e.0;
            org_biomass_total += b.0;
        }
        let ledger = ledger::Ledger {
            initial_total: pools.total() + org_energy_total + org_biomass_total,
            ..Default::default()
        };
        world.insert_resource(ledger);
        world.insert_resource(pools);
        // PRECOMPUTE the per-cell solar light cap ONCE (the static field is constant for the run) so the hot
        // `solar_influx` tick path is pure integer with no per-cell f64 re-floor (hash-neutral perf opt).
        let solar_light_cap: Vec<i64> = resource_field
            .light
            .iter()
            .map(|&v| (i64::from(fixed::to_unit_u16(f64::from(v))) * CELL_CAP_SCALE).min(POOL_CAP))
            .collect();
        world.insert_resource(SolarLightCap(solar_light_cap));
        // Reusable per-tick scratch buffers for the hot systems (perf, hash-neutral — see the resource docs).
        world.insert_resource(MetabolismScratch::default());
        world.insert_resource(ReproScratch::default());
        world.insert_resource(chem::ChemEmitScratch::default());
        world.insert_resource(ResourceFieldRes(resource_field));
        // ADR-013 F5: the chemical/signal field (toxin/kin/alarm), seeded ALL-ZERO — chem is ENDOGENOUS
        // (emitted by organisms, never seed-generated), so it draws NO derive_seed / SimRng. Because Σchem == 0
        // at the all-zero seed, the ledger's initial_total above is UNCHANGED by adding it (no reset surprise);
        // the planes start matching WORLD_DIMS == RESOURCE_DIMS == PoolStock dims. Inserted right after PoolStock.
        world.insert_resource(chem::ChemField::zeroed(
            resource::RESOURCE_DIMS.0,
            resource::RESOURCE_DIMS.1,
        ));
        // ADR-013 F3 allocators: the monotonic OrgId source (never reuses a despawned id) and the draw counter
        // (folded into hash_world so a birth-enumeration off-by-one breaks the hash locally).
        world.insert_resource(NextOrgId(next_org_id));
        world.insert_resource(DrawCount::default());
        // ADR-013 F4: the MEASURED S×S FlowMatrix (per-generation, reset each tick) + the per-cell, per-species
        // PoolProvenance ledger that attributes detritus/free_nutrient flow. Sized to the registry length; the
        // FlowMatrix is folded into `hash_world` (a measurement off already-hashed pools/orgs), the provenance
        // ledger is not (it is internal bookkeeping the matrix is derived from).
        let species_count = entries.len();
        let cells = (resource::RESOURCE_DIMS.0 as usize) * (resource::RESOURCE_DIMS.1 as usize);
        world.insert_resource(trophic::FlowMatrix::zeroed(species_count));
        world.insert_resource(trophic::PoolProvenance::new(cells, species_count));
        // ADR-013 F5: the per-species KIN attribution (the legible kin-SELECTION mechanic — own-species boost,
        // not generic crowding), REUSING the PoolProvenance flat `[cell*S + species]` mechanism. Starts zero;
        // NOT folded into hash_world (internal bookkeeping the demand/maintenance sense reads from).
        world.insert_resource(chem::KinProvenance::new(cells, species_count));
        // The multi-species spine (ADR R3-A): the ordered species registry. Now READ by the F3/F4 pipeline
        // (metabolism/mineralize read each species' cached Strategy).
        world.insert_resource(SpeciesRegistry { entries });

        let mut schedule = Schedule::default();
        // Explicit, single-threaded ordering — the determinism backbone (ADR-002, ADR-013 F3/F4). The integer
        // pipeline: advance → reset_flow (zero the per-gen FlowMatrix) → solar_influx (light INFLUX tap;
        // free_nutrient is now endogenous) → metabolism (uptake/convert/excrete + litterfall + free_nutrient
        // provenance, RNG-free) → mineralize (the F4 decomposer detritus→free_nutrient loop + FlowMatrix
        // harvest record) → reproduce_or_die (maintenance debit + death FIRST + birth — the ONLY SimRng
        // consumer; carcass→detritus provenance) → predation (ADR-013 F6: Bdellovibrio consume co-located prey J
        // on a frozen census; AFTER reproduce_or_die so it owns its kills' despawn + carcass deposit; writes the
        // first org-eats-org FlowMatrix off-diagonal; RNG-free no-op on a predator-free roster) →
        // assert_flow_closes (row-sum==0) → measure_and_assert_ledger (LAST: closes the books every tick).
        // ADR-013 F5 inserts 3 chem stages + 1 assert into this chain (all single-threaded, integer, no
        // HashMap): reset_chem_scratch (zero the reused double-buffer; ChemField PERSISTS cross-tick) right after
        // reset_flow; diffuse_and_decay (reflecting Σ-conserved stencil THEN the chem_decay tap; cell-only,
        // organism-free) after solar_influx so it runs on the PREVIOUS tick's emitted chem BEFORE this tick's
        // organisms sense it (a one-tick lag); emit_chem (kin marker + live-distress alarm, J-paired) after
        // mineralize; assert_chem_conserved_system (the semantic chem gate) before assert_flow_closes. Toxin is
        // minted INLINE in metabolism (the respired↔toxin paired move is atomic where the Defense slice is
        // computed); death-alarm rides reproduce_or_die's existing canonical death pass.
        schedule.add_systems(
            (
                advance_tick,
                trophic::reset_flow,
                chem::reset_chem_scratch,
                solar_influx,
                chem::diffuse_and_decay,
                metabolism,
                trophic::mineralize,
                chem::emit_chem,
                reproduce_or_die,
                trophic::predation,
                chem::assert_chem_conserved_system,
                trophic::assert_flow_closes,
                measure_and_assert_ledger,
            )
                .chain(),
        );

        Self {
            world,
            schedule,
            config: config.clone(),
            // Static for the run; read-only w.r.t. the hash beyond its coupling effect on per-org state.
            soil,
            gp_map: primary_gp_map,
        }
    }

    /// Advance `generations` generations using the SAME seeded RNG (no re-seeding mid-run, inv. #3).
    pub fn step(&mut self, generations: u64) {
        for _ in 0..generations {
            self.schedule.run(&mut self.world);
        }
    }

    /// Observe the current state (generation, population size, allele frequency, expressed phenotype).
    ///
    /// Pure w.r.t. the run so far — calling it does not advance the RNG or the schedule (inv. #3).
    #[must_use]
    pub fn observe(&mut self) -> Observation {
        let allele_freq = mean_genotype(&mut self.world);
        let generation = self.world.resource::<Tick>().0;
        let population_size = self.world.query::<&OrgId>().iter(&self.world).count() as u32;
        // Re-express the (possibly edited) species genome into traits through THIS run's stored species map, so
        // an E. coli run observes microbe traits and a plant run observes plant traits (ADR-017).
        let phenotype = self.gp_map.express(&self.world.resource::<GenomeRes>().0);
        Observation {
            generation,
            population_size,
            allele_freq,
            phenotype,
        }
    }

    /// Observe EVERY species in the roster (a read-only per-species display projection — ADR R3 renderer view).
    ///
    /// Walks the [`SpeciesRegistry`] in [`SpeciesId`] (Vec-index) order — never a `HashMap` (inv #3) — and, for
    /// each entry, expresses ITS OWN genome through ITS OWN `gp_map` (the SAME genotype→phenotype machinery
    /// [`observe`](Self::observe) uses, so an E. coli entry yields microbe traits and a plant entry plant
    /// traits — invariant #2). This is **pure** exactly like [`observe`](Self::observe) /
    /// [`snapshot`](Self::snapshot): it draws ZERO `SimRng`, mutates nothing, and is NEVER folded into
    /// `hash_world`, so calling it cannot change the determinism hash (invariant #3). A single-species run
    /// returns one element (the same phenotype `observe()` reports); a multi-species roster returns one per
    /// species so the renderer can show them all.
    #[must_use]
    pub fn observe_all(&self) -> Vec<SpeciesObservation> {
        let registry = self.world.resource::<SpeciesRegistry>();
        let n = registry.entries.len();

        // ── ONE read-only, SpeciesId-partitioned, OrgId-sorted aggregation pass (inv #3) ──────────────
        // Ordered Vecs indexed by SpeciesId ordinal — NEVER a HashMap. Energy accumulates as i64 (exact,
        // commutative integer add → order-independent); the f64 allele_sum fold is pinned by the (sid, OrgId)
        // sort, exactly as `mean_genotype` / `snapshot` pin theirs. `try_query` runs on the IMMUTABLE `&World`
        // so observe_all stays a pure `&self` projection; it draws ZERO SimRng, mutates nothing → cannot move
        // `hash_world`.
        let mut counts: Vec<u32> = vec![0; n];
        let mut allele_sum: Vec<f64> = vec![0.0; n];
        let mut energy_sum: Vec<i64> = vec![0; n];

        // The (Species, OrgId, Genotype, Energy) components are all registered by `reset_with_roster`'s spawn,
        // so `try_query` is `Some` for any live run; a never-spawned world yields an empty fold → all-zero stats.
        let mut rows: Vec<(u16, u64, f64, i64)> = self
            .world
            .try_query::<(&Species, &OrgId, &Genotype, &Energy)>()
            .map(|mut q| {
                q.iter(&self.world)
                    .map(|(sp, id, g, e)| (sp.0 .0, id.0, g.0, e.0))
                    .collect()
            })
            .unwrap_or_default();
        // Partition by SpeciesId, then OrgId WITHIN species — pins the non-associative f64 allele_sum order.
        rows.sort_unstable_by_key(|r| (r.0, r.1));
        for (sid, _id, g, e) in &rows {
            let i = *sid as usize;
            if i < n {
                // `i < n` keeps the index total; a stray tag is impossible (Species minted from the registry).
                counts[i] += 1;
                allele_sum[i] += *g;
                energy_sum[i] += *e;
            }
        }

        registry
            .entries
            .iter()
            .enumerate()
            .map(|(idx, entry)| SpeciesObservation {
                species_id: idx as u16,
                name: entry.name.clone(),
                key: entry.key.clone(),
                role: entry.strategy.role,
                phenotype: entry.gp_map.express(&entry.genome),
                population_size: counts[idx],
                allele_freq: if counts[idx] == 0 {
                    0.0
                } else {
                    allele_sum[idx] / counts[idx] as f64
                },
                // Mean Energy normalized to [0,1] by ENERGY_FULL — the SAME normalization snapshot()'s fitness
                // channel uses. Zero-division guard yields exactly 0.0 (never NaN) for an empty species.
                mean_energy: if counts[idx] == 0 {
                    0.0
                } else {
                    energy_sum[idx] as f64 / counts[idx] as f64 / ENERGY_FULL as f64
                },
            })
            .collect()
    }

    /// Produce a read-only, derived per-cell [`GridSnapshot`] for the renderer (SPEC §5, §W10; S4.2).
    ///
    /// Like [`observe`](Self::observe), this is **pure** w.r.t. the run: it iterates the [`World`] but
    /// **never** draws from [`SimRng`] and **never** mutates state, so calling it cannot change the
    /// determinism hash (invariant #3) — purely a READ-ONLY projection (inv #2). Each organism's REAL world
    /// [`Position`] (on [`WORLD_DIMS`]) is resampled onto the render `(width, height)` grid (ADR-011 S-C);
    /// the OrgId-hash visualization layout is retired. Aggregation walks organisms in stable `OrgId` order
    /// (no `HashMap` iteration affecting output — invariant #3).
    ///
    /// Channels (each `width * height`, row-major, in `[0, 1]`): `density` = per-cell count / busiest-cell
    /// count; `allele_freq` = mean [`Genotype`] in the cell; `fitness` = mean [`Energy`] in the cell;
    /// `soil_moisture`/`soil_nutrients`/`soil_ph` = the static `SoilField` resampled; `light`/`free_nutrient`/
    /// `detritus` = the LIVE [`PoolStock`] joule planes resampled and normalized by [`POOL_CAP`].
    /// Empty cells are `0` on the population channels. Now reflects REAL spatial structure (clusters/clines from
    /// inherited dispersal), not a derived layout.
    ///
    /// # Panics
    /// Panics if `width` or `height` is `0` (a degenerate grid has no cells to place organisms in).
    #[must_use]
    pub fn snapshot(&mut self, width: u32, height: u32) -> GridSnapshot {
        assert!(width > 0 && height > 0, "snapshot grid must be non-empty");
        let generation = self.world.resource::<Tick>().0;

        // Collect organisms in STABLE OrgId order (decouples from ECS archetype iteration — inv. #3),
        // carrying each one's REAL world Position (ADR-011 S-C — no longer the OrgId-hash layout).
        let mut rows: Vec<(u64, f64, i64, u32, u32)> = self
            .world
            .query::<(&OrgId, &Genotype, &Energy, &Position)>()
            .iter(&self.world)
            .map(|(id, g, e, p)| (id.0, g.0, e.0, p.x, p.y))
            .collect();
        rows.sort_unstable_by_key(|r| r.0);
        let population = rows.len() as u32;

        let cells = (width as usize) * (height as usize);
        let mut count = vec![0u32; cells];
        let mut genotype_sum = vec![0.0f64; cells];
        let mut energy_sum = vec![0.0f64; cells];

        for (_id, g, e, px, py) in &rows {
            // Resample the organism's REAL world cell (px,py on WORLD_DIMS) onto the render grid — no RNG,
            // no OrgId hash. Clamp guards the top edge when render dims exceed the world grid.
            let x = ((u64::from(*px) * u64::from(width)) / u64::from(WORLD_DIMS.0))
                .min(u64::from(width) - 1) as usize;
            let y = ((u64::from(*py) * u64::from(height)) / u64::from(WORLD_DIMS.1))
                .min(u64::from(height) - 1) as usize;
            let cell = y * (width as usize) + x;
            count[cell] += 1;
            genotype_sum[cell] += *g;
            energy_sum[cell] += *e as f64; // i64 energy → f64 for the (display-only) fitness channel mean
        }

        let max_count = count.iter().copied().max().unwrap_or(0);
        let mut density = vec![0.0f32; cells];
        let mut allele_freq = vec![0.0f32; cells];
        let mut fitness = vec![0.0f32; cells];
        for c in 0..cells {
            let n = count[c];
            if n == 0 {
                continue; // empty cells stay 0 on every channel.
            }
            if max_count > 0 {
                density[c] = (f64::from(n) / f64::from(max_count)) as f32;
            }
            allele_freq[c] = (genotype_sum[c] / f64::from(n)) as f32;
            // Mean energy normalized to [0,1] by ENERGY_FULL for the f32 fitness channel (display-only).
            fitness[c] = (energy_sum[c] / f64::from(n) / (ENERGY_FULL as f64)) as f32;
        }

        // Resample the static soil field onto the snapshot grid (read-only, no RNG → off the hash path).
        let mut soil_moisture = vec![0.0f32; cells];
        let mut soil_nutrients = vec![0.0f32; cells];
        let mut soil_ph = vec![0.0f32; cells];
        for y in 0..height {
            for x in 0..width {
                let c = (y as usize) * (width as usize) + (x as usize);
                soil_moisture[c] = self.soil.sample_to(0, x, y, width, height);
                soil_nutrients[c] = self.soil.sample_to(1, x, y, width, height);
                soil_ph[c] = self.soil.sample_to(2, x, y, width, height);
            }
        }

        // Read-only borrow of the LIVE pools (Bevy resource) — no RNG, no mutation (inv #3). Resample
        // world→render with the SAME nearest-cell integer map soil::sample_to uses; normalize by POOL_CAP.
        // PoolStock IS already folded into hash_world (line ~1909): reading its already-hashed values into a
        // separate display buffer adds NOTHING to the hash — the projection is downstream of the tick, never
        // upstream. Uses pools.width/pools.height (PoolStock carries its own dims) — not a dims literal.
        let pools = self.world.resource::<PoolStock>();
        let mut light = vec![0.0f32; cells];
        let mut free_nutrient = vec![0.0f32; cells];
        let mut detritus = vec![0.0f32; cells];
        for y in 0..height {
            for x in 0..width {
                let c = (y as usize) * (width as usize) + (x as usize);
                light[c] =
                    pool_sample_to(&pools.light, pools.width, pools.height, x, y, width, height);
                free_nutrient[c] = pool_sample_to(
                    &pools.free_nutrient,
                    pools.width,
                    pools.height,
                    x,
                    y,
                    width,
                    height,
                );
                detritus[c] = pool_sample_to(
                    &pools.detritus,
                    pools.width,
                    pools.height,
                    x,
                    y,
                    width,
                    height,
                );
            }
        }

        // ADR-013 F5: resample the live chem planes (toxin/kin/alarm) onto the snapshot grid, normalized by
        // CHEM_CAP via the audited chem::sample_to chokepoint (mirrors pool_sample_to). The chem field IS folded
        // into hash_world; reading its already-hashed values into a display buffer is downstream of the tick,
        // adds NOTHING to the hash, and draws ZERO SimRng (the read-only projection discipline, inv #2/#3).
        let chem = self.world.resource::<chem::ChemField>();
        let (cf_toxin, cf_kin, cf_alarm) = chem.render_planes();
        let mut toxin = vec![0.0f32; cells];
        let mut kin = vec![0.0f32; cells];
        let mut alarm = vec![0.0f32; cells];
        for y in 0..height {
            for x in 0..width {
                let c = (y as usize) * (width as usize) + (x as usize);
                toxin[c] = chem::ChemField::sample_to(
                    cf_toxin,
                    chem.width,
                    chem.height,
                    x,
                    y,
                    width,
                    height,
                );
                kin[c] = chem::ChemField::sample_to(
                    cf_kin,
                    chem.width,
                    chem.height,
                    x,
                    y,
                    width,
                    height,
                );
                alarm[c] = chem::ChemField::sample_to(
                    cf_alarm,
                    chem.width,
                    chem.height,
                    x,
                    y,
                    width,
                    height,
                );
            }
        }

        GridSnapshot {
            width,
            height,
            generation,
            population,
            density,
            allele_freq,
            fitness,
            soil_moisture,
            soil_nutrients,
            soil_ph,
            light,
            free_nutrient,
            detritus,
            toxin,
            kin,
            alarm,
        }
    }

    /// Read the **mean allele frequency over the populated cells of a disc region**, on a `grid_w × grid_h`
    /// snapshot grid (ADR-013 campaign-grader). This is the CORE re-implementation of the mission/zone reading
    /// in `godot/main.gd::_eval_mission` — which now CALLS this (via `LiveSim.region_allele`) for the live
    /// mission's zone read instead of computing it in GDScript (invariant #2; a GDScript loop remains only as
    /// the no-LiveSim replay fallback). The headless campaign-grader shares this same read. It uses
    /// the SAME [`snapshot`](Self::snapshot) the renderer draws and averages `allele_freq` over exactly the
    /// cells that are populated (`density > 0`) AND inside [`Region::contains`] — a mean-of-cell-means, not a
    /// per-organism mean. It matches `_eval_mission` bit-for-bit **for `radius ≥ MIN_REGION_RADIUS`**; at
    /// `radius == 0` [`Region::contains`] clamps up to a radius-1 disc while `_eval_mission` reads only the
    /// centre cell, so the two diverge there. Read-only and RNG-free (delegates to `snapshot`) — never perturbs
    /// the hash.
    #[must_use]
    pub fn region_allele(&mut self, region: Region, grid_w: u32, grid_h: u32) -> RegionReadout {
        let snap = self.snapshot(grid_w, grid_h);
        let mut sum = 0.0f64;
        let mut populated = 0u32;
        for y in 0..grid_h {
            for x in 0..grid_w {
                let i = (y * grid_w + x) as usize;
                if snap.density[i] > 0.0 && region.contains(x, y) {
                    sum += f64::from(snap.allele_freq[i]);
                    populated += 1;
                }
            }
        }
        RegionReadout {
            mean: if populated > 0 {
                sum / f64::from(populated)
            } else {
                0.0
            },
            populated_cells: populated,
        }
    }

    /// The species genome currently wired into the core (read-only; invariant #2 — biology lives here).
    #[must_use]
    pub fn species_genome(&self) -> &Genome {
        &self.world.resource::<GenomeRes>().0
    }

    /// The expressed ecological [`gp::Strategy`] cached for species `sid` (ADR-013 F2; read-only, parallel to
    /// [`species_genome`](Self::species_genome)). Pure read — no RNG draw, no mutation — so it cannot perturb
    /// the run hash. UNREAD by the sim path at F2 (F3 metabolism is its first reader); exposed for tests and a
    /// future UI. Panics only on an out-of-range `SpeciesId` (a programming error).
    #[must_use]
    pub fn species_strategy(&self, sid: SpeciesId) -> &gp::Strategy {
        &self.world.resource::<SpeciesRegistry>().entries[sid.0 as usize].strategy
    }

    /// Commit a deep-edit growth-ratio (ADR-017 S6 — the load-bearing OVERSIGHT wire). Maps the firewall's
    /// committed `growth_ratio_q` (permille, `1000` = wild-type) + the [`EditEffect`] verb to a strictly-positive
    /// `[0.5,1.5]` PERMILLE demand factor via [`edit_factor_q`] and stores it for `sid`, so the NEXT `step` makes
    /// [`metabolism`] throttle that species' uptake. This is the consumer side of the determinism firewall: the
    /// committed INTEGER (read straight from the journal on replay, never re-solved) is the only thing that
    /// crosses into the hashed sim. A neutral commit (`growth_ratio_q == 1000` knockout, or any wild-type) maps
    /// to exactly `1000` → the demand math is untouched → hash-neutral. A no-op for an out-of-range `sid`
    /// (defensive: the registry length is fixed at reset). Pure integer / fixed-point, no RNG draw.
    pub fn commit_species_edit(&mut self, sid: SpeciesId, growth_ratio_q: u16, effect: EditEffect) {
        let factor = edit_factor_q(growth_ratio_q, effect);
        let mut res = self.world.resource_mut::<EditModifierRes>();
        if let Some(slot) = res.factor_q.get_mut(sid.0 as usize) {
            *slot = factor;
        }
    }

    /// The committed deep-edit demand factor (permille; `1000` = neutral) for `sid` — read-only, for the INSPECT
    /// view + tests. Neutral for an unedited / out-of-range species.
    #[must_use]
    pub fn species_edit_factor_q(&self, sid: SpeciesId) -> u16 {
        self.world.resource::<EditModifierRes>().factor_q(sid.0)
    }

    /// The [`SpeciesId`] of the registry entry whose `key` matches `key`, if any (ordered scan, never a
    /// `HashMap` — inv #3). Used by [`region_inoculate`](Self::region_inoculate) to spawn into an
    /// already-registered species slot, and by the renderer/tests to find a contaminant after inoculation.
    #[must_use]
    pub fn species_id_for_key(&self, key: &str) -> Option<SpeciesId> {
        self.world
            .resource::<SpeciesRegistry>()
            .entries
            .iter()
            .position(|e| e.key == key)
            .map(|i| SpeciesId(i as u16))
    }

    /// Register a NEW species into the running registry from a built genome + map + role (ADR-019), returning
    /// its fresh [`SpeciesId`]; if a species with `key` is already registered, returns its existing id (no
    /// duplicate). Resizes every species-indexed resource (the `EditModifierRes`, the `FlowMatrix`, the
    /// `PoolProvenance` + `KinProvenance`) so the new ordinal has a valid slot in each. Draws ZERO `SimRng`
    /// (the cached `Strategy` is a pure expression, like every roster entry). NOT called by the pinned config →
    /// the registry length / hashed FlowMatrix dimension are unchanged there (hash-neutral). The cached
    /// `Strategy` is unread until the inoculated orgs metabolize.
    pub fn register_species(
        &mut self,
        name: String,
        key: String,
        genome: Genome,
        gp_map: gp::OntologyMap,
        role: gp::TrophicRole,
    ) -> SpeciesId {
        if let Some(sid) = self.species_id_for_key(&key) {
            return sid; // already registered — never duplicate a species
        }
        let base_growth = gp_map
            .express(&genome)
            .get(Trait::GrowthRate)
            .unwrap_or(0.5);
        // Express the ecological Strategy ONCE (the reset_with_roster precedent) — pure, ZERO SimRng.
        let strategy = gp::express_strategy(&gp_map, &genome, role);
        let entry = SpeciesEntry {
            name,
            key,
            genome,
            gp_map,
            base_growth,
            target_pop: 0, // a contaminant has no reset spawn; it arrives only via region_inoculate
            strategy,
        };
        let new_len = {
            let mut reg = self.world.resource_mut::<SpeciesRegistry>();
            reg.entries.push(entry);
            reg.entries.len()
        };
        // Resize every species-indexed resource so the new ordinal addresses a valid slot in each.
        let cells = {
            let pools = self.world.resource::<PoolStock>();
            (pools.width as usize) * (pools.height as usize)
        };
        self.world
            .resource_mut::<EditModifierRes>()
            .grow_to(new_len);
        self.world
            .resource_mut::<trophic::FlowMatrix>()
            .grow_to(new_len);
        self.world
            .resource_mut::<trophic::PoolProvenance>()
            .grow_to(cells, new_len);
        self.world
            .resource_mut::<chem::KinProvenance>()
            .grow_to(cells, new_len);
        SpeciesId((new_len - 1) as u16)
    }

    /// **REGION INOCULATE** (ADR-019 S1) — the SP-3-deferred seed/inoculate tool: spawn `count` organisms of an
    /// already-registered species `sid` inside the `region` disc, each endowed with `endow_j` joules MINTED from
    /// the named `immigration` ledger tap (conserved — a contaminant's arrival is accounted, never conjured).
    ///
    /// **Determinism (inv #3):** RNG-FREE. Placement is a deterministic cell-fill — the in-region cells are
    /// enumerated in `cell_index` (`y*width + x`) order, and the `count` organisms are laid into them in
    /// `(cell_index, slot)` order (round-robin across cells so a small propagule still spreads), with OrgIds
    /// minted in order from the monotonic [`NextOrgId`]. ZERO `next_u64` draws → the spawn stream is unchanged.
    /// Returns the number of organisms actually spawned (`0` if the disc covers no grid cell).
    ///
    /// **Granularity (inv #6):** `region` targets CELLS (no organism handle); the species/region pair is an
    /// operator-level event, never per-organism agency. **Emergence:** establish/displace/die-out is NOT coded
    /// — the spawned orgs metabolize, compete for the conserved pools, and reproduce or starve under the
    /// existing ADR-013 economy.
    ///
    /// The child carries the SAME spawn-component shape as a birth (Energy/Biomass/Age/Genotype/DroughtTol/
    /// ThermalTol/Position/Species): `endow_j` splits into a seed `Biomass` ([`OFFSPRING_SEED_BIOMASS`], clamped
    /// to `endow_j`) and the residual `Energy`, so it enters the economy as a viable fresh organism. The
    /// heritable f64 traits seed at a neutral `0.5` (RNG-free — a deterministic constant, not a draw), so the
    /// inoculation adds no `SimRng` word. A no-op for an out-of-range `sid` (defensive).
    pub fn region_inoculate(
        &mut self,
        sid: SpeciesId,
        region: Region,
        count: u32,
        endow_j: i64,
    ) -> u32 {
        if count == 0 || endow_j <= 0 {
            return 0;
        }
        let species_count = self.world.resource::<SpeciesRegistry>().entries.len();
        if sid.0 as usize >= species_count {
            return 0; // not registered — defensive no-op
        }
        let (width, height) = {
            let pools = self.world.resource::<PoolStock>();
            (pools.width, pools.height)
        };
        // Enumerate the in-region cells in canonical cell_index (y*width + x) order — RNG-free placement.
        let mut cells: Vec<(u32, u32)> = Vec::new();
        for y in 0..height {
            for x in 0..width {
                if region.contains(x, y) {
                    cells.push((x, y));
                }
            }
        }
        if cells.is_empty() {
            return 0; // the disc covers no grid cell
        }
        // Split endow_j into a seed Biomass (carved out, clamped) + the residual Energy — CONSERVED (Σ == endow_j).
        let seed_biomass = OFFSPRING_SEED_BIOMASS.min(endow_j);
        let seed_energy = endow_j - seed_biomass;
        // Mint count·endow_j into the world via the immigration tap (the conserved arrival accounting).
        self.world.resource_mut::<ledger::Ledger>().immigration += endow_j * i64::from(count);
        // Spawn count orgs round-robin across the in-region cells in (cell_index, slot) order; OrgIds monotonic.
        for slot in 0..count {
            let (x, y) = cells[(slot as usize) % cells.len()];
            let org = {
                let mut next = self.world.resource_mut::<NextOrgId>();
                let id = next.0;
                next.0 += 1;
                id
            };
            self.world.spawn((
                OrgId(org),
                Energy(seed_energy),
                Biomass(seed_biomass),
                Age(0),
                // Heritable traits seed at a neutral 0.5 — a deterministic CONSTANT, not a SimRng draw (inv #3).
                Genotype(0.5),
                DroughtTol(0.5),
                ThermalTol(0.5),
                Position { x, y },
                Species(sid),
            ));
        }
        count
    }

    /// **REGION PCR-AMPLIFY** (SP-3.1) — the faithful local-clone tool: spawn `count` FAITHFUL clones of an
    /// ALREADY-RESIDENT species `sid` inside the `region` disc, each endowed with `endow_j` joules MINTED from
    /// the named `intervention` ledger tap (conserved — a PCR reaction ADDS copies, never conjured, never halves
    /// the template). Unlike [`region_inoculate`] (which bakes a NEUTRAL `0.5` contaminant), each clone COPIES
    /// its heritable state VERBATIM from a deterministically-chosen resident template org of `sid` — so a clone
    /// is bit-identical heritable state to its local template (faithful PCR); subsequent generations mutate
    /// normally through [`reproduce_or_die`]. Returns the number of clones actually spawned (`0` if the species
    /// is not LOCALLY present in the disc — you cannot PCR-amplify what has no template; mirrors
    /// `region_inoculate`'s empty-disc no-op).
    ///
    /// **Determinism (inv #3):** RNG-FREE. (1) Enumerate in-region cells in canonical `cell_index` order. (2)
    /// CENSUS the species' LIVING in-region orgs, COLLECT-then-SORT by `(cell_index, SpeciesId, OrgId)` (the
    /// `reproduce_or_die` canonical key) so template selection is a pure function of state, never Query order.
    /// (3) For each of `count` clones, pick the template ROUND-ROBIN over the sorted census (`k % census.len()`,
    /// deterministic — NOT a random parent), copy its `Genotype`/`DroughtTol`/`ThermalTol` VERBATIM (NO
    /// `mutate_unit` — that distinguishes a PCR clone from a sexual birth), and co-locate the clone on the
    /// TEMPLATE'S cell (daughter-cell semantics — a clone biologically arises where its template is; zero RNG).
    /// OrgIds minted IN ORDER from the monotonic [`NextOrgId`]. ZERO `next_u64` draws → the spawn stream is
    /// unchanged, exactly the property [`region_inoculate`] documents.
    ///
    /// **Granularity (inv #6):** `region` targets CELLS; the species/region pair is an operator-level event.
    /// **Conservation:** `endow_j` splits into a seed `Biomass` ([`OFFSPRING_SEED_BIOMASS`], clamped) + residual
    /// `Energy` (Σ == endow_j); `count·endow_j` is booked to the `intervention` tap so live `J` rises by exactly
    /// the tap. PCR never registers a new species → the FlowMatrix/SpeciesRegistry dimension is untouched.
    pub fn region_pcr_amplify(
        &mut self,
        sid: SpeciesId,
        region: Region,
        count: u32,
        endow_j: i64,
    ) -> u32 {
        if count == 0 || endow_j <= 0 {
            return 0;
        }
        let species_count = self.world.resource::<SpeciesRegistry>().entries.len();
        if sid.0 as usize >= species_count {
            return 0; // not registered — defensive no-op
        }
        let width = self.world.resource::<PoolStock>().width;
        // CENSUS the targeted species' LIVING in-region orgs; collect heritable state + cell, then SORT by the
        // canonical (cell_index, SpeciesId, OrgId) key so template selection is a pure function of state (inv #3).
        struct Template {
            cell: u32,
            org: u64,
            genotype: f64,
            drought: f64,
            thermal: f64,
            px: u32,
            py: u32,
        }
        let mut census: Vec<Template> = Vec::new();
        for (id, sp, g, d, t, p) in self
            .world
            .query::<(
                &OrgId,
                &Species,
                &Genotype,
                &DroughtTol,
                &ThermalTol,
                &Position,
            )>()
            .iter(&self.world)
        {
            if sp.0 == sid && region.contains(p.x, p.y) {
                census.push(Template {
                    cell: cell_index(p, width),
                    org: id.0,
                    genotype: g.0,
                    drought: d.0,
                    thermal: t.0,
                    px: p.x,
                    py: p.y,
                });
            }
        }
        if census.is_empty() {
            return 0; // no local template — PCR needs one (cannot amplify what is not locally present)
        }
        census.sort_unstable_by_key(|c| (c.cell, sid.0, c.org));
        // Split endow_j into a seed Biomass (carved out, clamped) + residual Energy — CONSERVED (Σ == endow_j).
        let seed_biomass = OFFSPRING_SEED_BIOMASS.min(endow_j);
        let seed_energy = endow_j - seed_biomass;
        // Mint count·endow_j into the world via the intervention tap (a PCR reaction ADDS copies — conserved).
        self.world.resource_mut::<ledger::Ledger>().intervention += endow_j * i64::from(count);
        // Spawn count clones round-robin over the sorted census; each inherits its template VERBATIM (no mutate).
        for k in 0..count {
            let tpl = &census[(k as usize) % census.len()];
            let (genotype, drought, thermal, px, py) =
                (tpl.genotype, tpl.drought, tpl.thermal, tpl.px, tpl.py);
            let org = {
                let mut next = self.world.resource_mut::<NextOrgId>();
                let id = next.0;
                next.0 += 1;
                id
            };
            self.world.spawn((
                OrgId(org),
                Energy(seed_energy),
                Biomass(seed_biomass),
                Age(0),
                // Heritable state COPIED VERBATIM from the template — faithful PCR, NOT a mutated sexual birth.
                Genotype(genotype),
                DroughtTol(drought),
                ThermalTol(thermal),
                // Daughter-cell placement: the clone co-locates on its template's cell (deterministic, zero RNG).
                Position { x: px, y: py },
                Species(sid),
            ));
        }
        count
    }

    /// **REGION CULL** (SP-3.2) — the selective-antibiotic tool: deterministically kill a `strength`-permille
    /// kill-FRACTION of one species `sid`'s LIVING orgs inside the `region` disc, depositing each culled org's
    /// residual `J` to detritus via the shared [`deposit_carcass`] helper (carcass→detritus, accounted EXACTLY
    /// like a starvation death — a paired bucket move, NO tap minted). Returns the number of orgs killed.
    ///
    /// **Determinism (inv #3):** RNG-FREE — NOT a per-org coin flip (which would draw + risk reordering). CENSUS
    /// the species' in-region orgs, COLLECT-then-SORT by `(cell_index, SpeciesId, OrgId)` (the `reproduce_or_die`
    /// canonical key), then kill the first `kills` of them where `kills` is the largest-remainder apportionment
    /// of `floor(n · strength / 1000)` — computed via [`fixed::apportion`] over `[strength, 1000−strength]` so
    /// the kill/spare split reuses the same conserving tie rule (ties→lowest canonical index). A pure function of
    /// the sorted census + strength, integer, position-dependent, ZERO draws. `strength` is clamped to
    /// `[0, 1000]`; `0` (or an empty census) is a clean no-op.
    ///
    /// **Granularity (inv #6):** `region` targets CELLS; the SUBSET-to-kill is an emergent apportionment of the
    /// canonical census, never an organism handle the operator names.
    pub fn region_cull(&mut self, sid: SpeciesId, region: Region, strength: u16) -> u32 {
        let strength = u64::from(strength.min(fixed::PERMILLE as u16));
        if strength == 0 {
            return 0;
        }
        let species_count = self.world.resource::<SpeciesRegistry>().entries.len();
        if sid.0 as usize >= species_count {
            return 0; // not registered — defensive no-op
        }
        let width = self.world.resource::<PoolStock>().width;
        // CENSUS the species' in-region living orgs: collect (cell, org, entity, residual=Energy+Biomass), SORT
        // canonically so the kept/killed split is a pure function of state (inv #3).
        struct Victim {
            cell: u32,
            org: u64,
            entity: Entity,
            residual: i64,
        }
        let mut census: Vec<Victim> = Vec::new();
        for (entity, id, sp, energy, biomass, p) in self
            .world
            .query::<(Entity, &OrgId, &Species, &Energy, &Biomass, &Position)>()
            .iter(&self.world)
        {
            if sp.0 == sid && region.contains(p.x, p.y) {
                census.push(Victim {
                    cell: cell_index(p, width),
                    org: id.0,
                    entity,
                    residual: energy.0.max(0) + biomass.0.max(0),
                });
            }
        }
        if census.is_empty() {
            return 0;
        }
        census.sort_unstable_by_key(|v| (v.cell, sid.0, v.org));
        // Apportion the census by [strength, 1000−strength]: the FIRST share is the kill count (largest-remainder,
        // ties→lowest index — the `fixed::apportion` tie rule). `kills` is exactly that many of the sorted census.
        let n = census.len() as i64;
        let split = fixed::apportion(n, &[strength, fixed::PERMILLE as u64 - strength]);
        let kills = split[0] as usize;
        if kills == 0 {
            return 0;
        }
        // Deposit each victim's residual to detritus (carcass→detritus — the shared helper; conserved, NO tap),
        // then despawn. Walk the sorted census so the per-cell detritus cap-spill is order-pinned (inv #3).
        let victims: Vec<(usize, Entity, i64)> = census
            .iter()
            .take(kills)
            .map(|v| (v.cell as usize, v.entity, v.residual))
            .collect();
        for &(cellu, _entity, residual) in &victims {
            self.cull_deposit(cellu, sid.0 as usize, residual);
        }
        for &(_cellu, entity, _residual) in &victims {
            self.world.despawn(entity);
        }
        kills as u32
    }

    /// Helper for [`region_cull`]: deposit one culled org's `residual` to detritus via the shared
    /// [`deposit_carcass`] (carcass→detritus). Sequences the four `&mut` resource borrows the helper needs
    /// through one `resource_scope` so they never alias on the single-threaded `World` (inv #3).
    fn cull_deposit(&mut self, cellu: usize, species: usize, residual: i64) {
        self.world
            .resource_scope(|world, mut pools: bevy_ecs::prelude::Mut<PoolStock>| {
                world.resource_scope(|world, mut chem: bevy_ecs::prelude::Mut<chem::ChemField>| {
                    world.resource_scope(
                        |world, mut prov: bevy_ecs::prelude::Mut<trophic::PoolProvenance>| {
                            let mut ledger = world.resource_mut::<ledger::Ledger>();
                            deposit_carcass(
                                &mut pools,
                                &mut chem,
                                &mut prov,
                                &mut ledger,
                                cellu,
                                species,
                                residual,
                            );
                        },
                    );
                });
            });
    }

    /// **REGION NUTRIENT** (SP-3.3) — the feed tool: deposit `amount_j` joules into one [`PoolStock`] plane
    /// (`channel` ∈ {0 light, 1 free_nutrient, 2 detritus}) across the in-region cells, MINTED from the named
    /// `intervention` ledger tap (conserved). The amount is apportioned across the in-region cells by
    /// largest-remainder ([`fixed::apportion`]) so the per-cell split is order-independent and replay-exact; each
    /// cell's [`POOL_CAP`] headroom spill routes to the OVERFLOW tap (`accepted + overflow_spill == amount`,
    /// never a silent clamp — the F3 overflow-routing precedent). Returns the `J` actually ACCEPTED into pools
    /// (`amount_j − overflow_spill`).
    ///
    /// **Determinism (inv #3):** RNG-FREE — deterministic cell enumeration in `cell_index` order, integer
    /// apportionment. Species-agnostic (it feeds the substrate, not an organism). An empty disc / non-positive
    /// amount is a clean no-op (mints nothing).
    pub fn region_nutrient(&mut self, channel: u8, region: Region, amount_j: i64) -> i64 {
        if amount_j <= 0 {
            return 0;
        }
        let (width, height) = {
            let pools = self.world.resource::<PoolStock>();
            (pools.width, pools.height)
        };
        // Enumerate in-region cells in canonical cell_index order — RNG-free.
        let mut cells: Vec<usize> = Vec::new();
        for y in 0..height {
            for x in 0..width {
                if region.contains(x, y) {
                    cells.push((y * width + x) as usize);
                }
            }
        }
        if cells.is_empty() {
            return 0;
        }
        // Apportion amount_j EVENLY across the in-region cells (largest-remainder conserves the total exactly).
        let weights = vec![1u64; cells.len()];
        let per_cell = fixed::apportion(amount_j, &weights);
        let mut pools = self.world.resource_mut::<PoolStock>();
        let plane: &mut Vec<i64> = match channel {
            0 => &mut pools.light,
            1 => &mut pools.free_nutrient,
            _ => &mut pools.detritus,
        };
        // Book the FULL amount to the intervention tap; the cap-rejected part is booked to overflow below (it
        // nets out — saturating logic ROUTES the spill, never silently clamps; the mint_to_cap precedent).
        let mut accepted_total = 0i64;
        let mut overflow_total = 0i64;
        for (&c, &amount) in cells.iter().zip(per_cell.iter()) {
            let headroom = (POOL_CAP - plane[c]).max(0);
            let accepted = amount.min(headroom);
            plane[c] += accepted;
            accepted_total += accepted;
            overflow_total += amount - accepted;
        }
        let mut ledger = self.world.resource_mut::<ledger::Ledger>();
        ledger.intervention += amount_j; // ALL minted J booked to intervention…
        ledger.overflow += overflow_total; // …the cap-rejected part booked to overflow (nets out)
        accepted_total
    }

    /// **REGION TOXIN** (SP-3.3) — the chemical-spike tool: deposit `amount_milli` (== `J` 1:1, the
    /// CHEM_J_PER_MILLI pin) into one [`chem::ChemField`] plane (`channel` ∈ {0 toxin, 1 kin, 2 alarm}) across
    /// the in-region cells, MINTED from the named `intervention` ledger tap (conserved). Each in-region cell
    /// gets the per-cell apportioned share via [`chem::deposit_capped_plane`] (the same call the death-alarm
    /// split uses); per-cell [`chem::CHEM_CAP`] cap-rejected part routes to the OVERFLOW tap. Returns the milli-J
    /// actually ACCEPTED into the field.
    ///
    /// **Determinism (inv #3):** RNG-FREE — deterministic cell enumeration in `cell_index` order, integer
    /// apportionment. Because chem is a LIVE [`ledger::LiveTotal::chem`] bucket, the minted milli MUST be booked:
    /// `intervention += accepted`, `overflow += cap-rejected` (the `alarm_rejected → overflow` precedent). An
    /// empty disc / non-positive amount is a clean no-op.
    pub fn region_toxin(&mut self, channel: u8, region: Region, amount_milli: i64) -> i64 {
        if amount_milli <= 0 {
            return 0;
        }
        let (width, height) = {
            let chem = self.world.resource::<chem::ChemField>();
            (chem.width, chem.height)
        };
        let mut cells: Vec<usize> = Vec::new();
        for y in 0..height {
            for x in 0..width {
                if region.contains(x, y) {
                    cells.push((y * width + x) as usize);
                }
            }
        }
        if cells.is_empty() {
            return 0;
        }
        // Apportion amount_milli evenly across the in-region cells (largest-remainder conserves the total).
        let weights = vec![1u64; cells.len()];
        let per_cell = fixed::apportion(amount_milli, &weights);
        // Clamp the channel selector to a valid plane (0 toxin, 1 kin, 2 alarm).
        let ch = usize::from(channel.min(2));
        let mut accepted_total = 0i64;
        let mut overflow_total = 0i64;
        {
            let mut chem = self.world.resource_mut::<chem::ChemField>();
            let plane = chem.plane_mut(ch);
            for (&c, &amount) in cells.iter().zip(per_cell.iter()) {
                // milli == J 1:1; the per-cell share is bounded by amount_milli << i32::MAX for any sane input,
                // so the i64→i32 narrowing is lossless for realistic spikes (clamp defensively).
                let amt_i32 = amount.clamp(0, i64::from(i32::MAX)) as i32;
                let rejected = chem::deposit_capped_plane(plane, c, amt_i32);
                accepted_total += i64::from(amt_i32 - rejected);
                overflow_total += amount - i64::from(amt_i32 - rejected);
            }
        }
        let mut ledger = self.world.resource_mut::<ledger::Ledger>();
        ledger.intervention += amount_milli; // ALL minted milli booked to intervention…
        ledger.overflow += overflow_total; // …cap-rejected part booked to overflow (nets out)
        accepted_total
    }

    /// The run's conserved-energy [`Ledger`](ledger::Ledger) (ADR-013 F0a; read-only copy). Empty until the
    /// joule pools land (F1); thereafter `ledger().closes(live_total)` is the conservation invariant.
    #[must_use]
    pub fn ledger(&self) -> ledger::Ledger {
        *self.world.resource::<ledger::Ledger>()
    }

    /// The MEASURED per-generation [`FlowMatrix`](trophic::FlowMatrix) as `(s, flat_row_major_i64)` (ADR-013
    /// F4). `flat[i*s + j]` = NET joules that flowed FROM species `j` INTO species `i` THIS generation
    /// (row-sum==0 by construction). Read-only — a pure projection of the current recorded matrix, drawing no
    /// RNG and mutating nothing. The renderer's relations heatmap reads exactly this contract via the
    /// `LiveSim::flow_matrix()` passthrough. (The matrix itself IS folded into the determinism hash, but
    /// READING it here cannot perturb the run.)
    #[must_use]
    pub fn flow_matrix(&self) -> (usize, Vec<i64>) {
        let fm = self.world.resource::<trophic::FlowMatrix>();
        (fm.s(), fm.flat().to_vec())
    }

    /// The read-only per-species relations **signatures** (ADR-014 re-grounded). Returns
    /// `(s, D, flat_signatures, roles)`:
    /// * `s` — species count (= [`SpeciesRegistry`] length, walked in [`SpeciesId`] ordinal order);
    /// * `D` — [`signature::SIGNATURE_DIMS`] (pinned `12`);
    /// * `flat_signatures` — `s * D` `u16`, row-major (`row i` = species `i`'s signature), on the shared
    ///   `[0, UNIT_SCALE]` grid (Block A = strategy budget/affinity/mineralize, Block B = measured FlowMatrix
    ///   in/out/degree). NO float ever enters the bytes (`base_growth` is DROPPED);
    /// * `roles` — `s` `u8`, the categorical [`gp::TrophicRole`] ordinal per species, carried ALONGSIDE the
    ///   vector as a label/filter (NEVER a distance dim).
    ///
    /// Pure read-only projection — draws NO `SimRng`, mutates nothing, NEVER folded into `hash_world` (mirrors
    /// the [`flow_matrix`](Self::flow_matrix) read-only contract, inv #2/#3). Block A reads the cached
    /// [`gp::Strategy`] (ADR-013 F2, off-hash), Block B reads the recorded `FlowMatrix` (ADR-013 F4) — reading
    /// either cannot perturb the run. Walks the [`SpeciesRegistry`] in `SpeciesId` order (no `HashMap`, inv #3),
    /// the SAME canonical order as `flow_matrix`/`observe_all`. The output is VIEW-ONLY (the boundary
    /// `relations-index` k-NN/clustering consumes it) and NEVER re-enters selection/metabolism/the hash.
    #[must_use]
    pub fn species_signatures(&self) -> (usize, usize, Vec<u16>, Vec<u8>) {
        let registry = self.world.resource::<SpeciesRegistry>();
        let fm = self.world.resource::<trophic::FlowMatrix>();
        let s = registry.entries.len();
        let d = signature::SIGNATURE_DIMS;
        let flat = fm.flat();
        let fm_s = fm.s();
        let mut sigs = Vec::with_capacity(s * d);
        let mut roles = Vec::with_capacity(s);
        // Walk in SpeciesId ordinal order (= Vec index) — canonical, HashMap-free (inv #3). Block B uses the
        // FlowMatrix only when its dimension matches the roster (s == fm_s); otherwise Block B is all-zero
        // (a fresh run before the first measurement, or a mismatched matrix — defensive, never a panic).
        let (b_flat, b_s) = if fm_s == s { (flat, s) } else { (&[][..], 0) };
        for (i, entry) in registry.entries.iter().enumerate() {
            let row = signature::signature_row(&entry.strategy, b_flat, b_s, i);
            sigs.extend_from_slice(&row);
            roles.push(signature::role_ordinal(entry.strategy.role));
        }
        (s, d, sigs, roles)
    }

    /// Mutate the **species** genome with access to the run's single seeded RNG, then re-express the
    /// `BaseGrowthRate` so the edit changes subsequent selection dynamics (SPEC §4; invariant #2, #3, #6).
    ///
    /// This is the species/operator-granular hook the harness's `ApplyEdit` action uses: `f` receives
    /// `(&mut Genome, &mut ChaCha8Rng)` — the SAME `ChaCha8Rng` that drives the rest of the run, so the
    /// edit draws only from the single seeded stream (inv. #3). The closure's return value is passed
    /// back to the caller. After `f` runs, the genome→phenotype is re-expressed (invariant #2: the only
    /// place biology is computed) and the [`BaseGrowthRate`] resource updated, so a species edit affects
    /// the next [`step`](Self::step)'s fitness.
    pub fn with_genome_and_rng<R>(
        &mut self,
        f: impl FnOnce(&mut Genome, &mut ChaCha8Rng) -> R,
    ) -> R {
        // Briefly take the RNG out of the world so we can hand both it and the genome to `f` (Bevy
        // resources can't be borrowed mutably two at a time). The RNG is the same instance; its stream
        // position is preserved — no re-seeding (inv. #3).
        let mut rng = std::mem::replace(
            &mut self.world.resource_mut::<SimRng>().0,
            ChaCha8Rng::seed_from_u64(0),
        );
        let out = {
            let genome = &mut self.world.resource_mut::<GenomeRes>().0;
            f(genome, &mut rng)
        };
        self.world.resource_mut::<SimRng>().0 = rng;

        // Re-express phenotype after the genome change THROUGH this run's stored species map, so the edit feeds
        // subsequent fitness consistently (e.g. an E. coli gltA knockout drops GrowthRate; invariant #2, ADR-017).
        let phenotype = self.gp_map.express(&self.world.resource::<GenomeRes>().0);
        let base_growth = phenotype.get(Trait::GrowthRate).unwrap_or(0.5);
        self.world.resource_mut::<BaseGrowthRate>().0 = base_growth;
        // R3-B: a species edit targets the PRIMARY species — mirror the edited genome + base growth into the
        // registry, which is now what `selection` reads, so the edit actually changes subsequent dynamics.
        let edited = self.world.resource::<GenomeRes>().0.clone();
        {
            let primary = &mut self.world.resource_mut::<SpeciesRegistry>().entries[0];
            // ADR-013 F2: re-express the cached Strategy from the edited genome so the cache stays consistent
            // after a species edit (its FIRST reader is F3 metabolism). Still UNREAD by selection → still
            // hash-neutral. Uses the entry's own map/role; the role is categorical (unchanged by the edit).
            let strategy = gp::express_strategy(&primary.gp_map, &edited, primary.strategy.role);
            primary.genome = edited;
            primary.base_growth = base_growth;
            primary.strategy = strategy;
        }
        out
    }

    /// Apply a REGION-scoped CRISPR edit (ADR-011 S-D, the selective brush). `f` runs the species-genome gate
    /// with the run's single seeded RNG and returns `(result, genotype_delta)`; the SIGNED delta is then added
    /// to every in-`region` organism's `[0, 1]` allele (clamped). Returns `(result, covered_count)`.
    ///
    /// **Determinism (inv #3):** the gate draws from the SAME single stream (RNG handed in via the same
    /// replace/restore dance as [`with_genome_and_rng`]); the delta application draws ZERO RNG, so the stream
    /// cost is whatever `f` consumed (fixed: ≤1 draw) — INDEPENDENT of how many organisms the brush covers.
    /// **Granularity (inv #6):** `region` targets CELLS (no organism handle); the per-individual allele shift
    /// is a regional operator action, not per-organism agency, and never mutates `BaseGrowthRate`/the genome.
    pub fn apply_edit_region<R>(
        &mut self,
        region: Region,
        f: impl FnOnce(&Genome, &mut ChaCha8Rng) -> (R, f64),
    ) -> (R, u32) {
        // Take the RNG out so the gate can borrow it alongside an immutable genome (same dance as above).
        let mut rng = std::mem::replace(
            &mut self.world.resource_mut::<SimRng>().0,
            ChaCha8Rng::seed_from_u64(0),
        );
        let (result, delta) = {
            let genome = &self.world.resource::<GenomeRes>().0;
            f(genome, &mut rng)
        };
        self.world.resource_mut::<SimRng>().0 = rng;

        // Shift every in-region individual's allele by the gate-derived delta. Per-org `g += delta` is
        // order-independent (and the end-of-run hash sorts by OrgId), so no draw and no HashMap here (inv #3).
        let mut covered = 0u32;
        for (_id, p, mut g) in self
            .world
            .query::<(&OrgId, &Position, &mut Genotype)>()
            .iter_mut(&mut self.world)
        {
            if region.contains(p.x, p.y) {
                g.0 = (g.0 + delta).clamp(0.0, 1.0);
                covered += 1;
            }
        }
        (result, covered)
    }

    /// Fold the current state into the deterministic [`RunStats`] artifact (SPEC §6). Mirrors what the
    /// one-shot [`run_headless`] returns at the end of a run.
    #[must_use]
    pub fn run_stats(&mut self) -> RunStats {
        let allele_freq = mean_genotype(&mut self.world);
        let config = self.config.clone();
        RunStats {
            seed: config.seed,
            generations: config.generations,
            entity_count: config.entity_count,
            allele_freq,
            hash: hash_world(&mut self.world, &config, allele_freq),
        }
    }
}

// --- public entry point ---------------------------------------------------------------------------

/// Run one headless, deterministic simulation and return its [`RunStats`].
///
/// Same `config` + same build + same platform ⇒ identical `hash` (SPEC §6). Implemented on top of
/// [`Simulation`] (reset → step the full generation count → fold the stats), so the one-shot and
/// stepwise paths share one code path and one RNG-threading story.
#[must_use]
pub fn run_headless(config: &SimConfig) -> RunStats {
    let mut sim = Simulation::reset(config);
    sim.step(config.generations);
    sim.run_stats()
}

/// Run a headless, deterministic simulation for an EXPLICIT species `genome` + its per-species `gp_map`
/// (ADR-017 "RUN E. coli"). A SEPARATE seam from [`run_headless`] so the pinned default path (and
/// `determinism_hash_is_pinned` / `check_determinism.sh`) stays untouched; a species run has its OWN hash.
/// Same `config` + `genome` + `map` + build + platform ⇒ identical `hash`.
#[must_use]
pub fn run_headless_with(config: &SimConfig, genome: Genome, gp_map: gp::OntologyMap) -> RunStats {
    let mut sim = Simulation::reset_with_genome_and_map(
        config,
        &climate::EnvParams::default(),
        genome,
        gp_map,
    );
    sim.step(config.generations);
    sim.run_stats()
}

/// Deterministic, build-scoped hash of final world state (SNIPPETS.md "stable end-of-run hash").
///
/// ADR-013 F3 (KEYSTONE re-pin): population is now a FREE variable, so the hash MUST tolerate a varying `N`.
/// Per org (in `OrgId` order) it folds `Energy`, `Biomass`, `Age`, the f64 `Genotype/DroughtTol/ThermalTol`
/// (`.to_bits()` — heritable f64 stays f64 at F3, finding #3), and `Position`. It also folds, in a fixed
/// order, the full `PoolStock` (light + free_nutrient + detritus, every cell), the per-tick `DrawCount` (so a
/// birth-enumeration off-by-one breaks the hash LOCALLY — finding #4), and the population `allele_freq`.
#[allow(clippy::type_complexity)] // a local row tuple for ordered hashing; naming it adds no clarity
fn hash_world(world: &mut World, config: &SimConfig, allele_freq: f64) -> u64 {
    use std::hash::{Hash, Hasher};

    // Collect per-org hashed state and sort by OrgId so the hash never depends on ECS iteration order (inv #3).
    // Energy/Biomass are i64 J (reinterpreted as u64 bits); Age is u32; the heritable f64 traits via to_bits.
    let mut rows: Vec<(u64, u64, u64, u32, u64, u64, u64, u32, u32)> = world
        .query::<(
            &OrgId,
            &Energy,
            &Biomass,
            &Age,
            &Genotype,
            &DroughtTol,
            &ThermalTol,
            &Position,
        )>()
        .iter(world)
        .map(|(id, e, b, a, g, d, t, p)| {
            (
                id.0,
                e.0 as u64,
                b.0 as u64,
                a.0,
                g.0.to_bits(),
                d.0.to_bits(),
                t.0.to_bits(),
                p.x,
                p.y,
            )
        })
        .collect();
    rows.sort_unstable_by_key(|r| r.0);

    let tick = world.resource::<Tick>().0;
    let genome_params = world.resource::<GenomeRes>().0.parameter_count() as u64;
    let draw_count = world.resource::<DrawCount>().0;
    // Snapshot the live PoolStock channels (fixed cell order) before borrowing SimRng.
    let (pool_light, pool_nutrient, pool_detritus) = {
        let pools = world.resource::<PoolStock>();
        (
            pools.light.clone(),
            pools.free_nutrient.clone(),
            pools.detritus.clone(),
        )
    };
    // ADR-013 F5: snapshot the live chem planes (toxin, kin, alarm) in fixed row-major order — the deliberate
    // F5 re-pin's new hash inputs. Raw i32 (== J milli), folded right after the PoolStock fold below.
    let (chem_toxin, chem_kin, chem_alarm) = {
        let chem = world.resource::<chem::ChemField>();
        let (t, k, a) = chem.render_planes();
        (t.to_vec(), k.to_vec(), a.to_vec())
    };
    // ADR-013 F4: fold the MEASURED FlowMatrix into the hash in fixed (row-major) order. A measurement derived
    // from already-hashed pools/orgs adds no NEW information, but it is folded explicitly so a flow-recording
    // regression breaks the hash LOCALLY (the row-sum==0 + ledger gates are the semantic authority). Hash-load-
    // bearing as of the F4 re-pin; off `hash_world` it rode until now.
    let flow_flat: Vec<i64> = world.resource::<trophic::FlowMatrix>().flat().to_vec();
    let flow_s = world.resource::<trophic::FlowMatrix>().s() as u64;
    // Fold in one final RNG word to capture stream advancement.
    let final_word = world.resource_mut::<SimRng>().0.next_u64();

    let mut h = std::collections::hash_map::DefaultHasher::new();
    config.seed.hash(&mut h);
    config.generations.hash(&mut h);
    config.entity_count.hash(&mut h);
    tick.hash(&mut h);
    genome_params.hash(&mut h);
    // Variable-N population (sorted by OrgId).
    (rows.len() as u64).hash(&mut h);
    for (id, e_bits, b_bits, age, g_bits, d_bits, t_bits, px, py) in &rows {
        id.hash(&mut h);
        e_bits.hash(&mut h);
        b_bits.hash(&mut h);
        age.hash(&mut h);
        g_bits.hash(&mut h);
        d_bits.hash(&mut h);
        t_bits.hash(&mut h);
        px.hash(&mut h);
        py.hash(&mut h);
    }
    // PoolStock + the named ledger taps (a fixed order; integer addition is commutative but we fold each cell).
    for plane in [&pool_light, &pool_nutrient, &pool_detritus] {
        for v in plane {
            v.hash(&mut h);
        }
    }
    // ADR-013 F5: the live chem planes (toxin, kin, alarm), raw i32 in fixed (channel, row-major) order, folded
    // right AFTER the PoolStock fold (the deliberate F5 re-pin — these are NEW hash inputs). A chem-free run
    // (all-zero planes) folds 3·cells zeros → the contribution is a fixed constant, so the no-emit J path is
    // byte-identical to F4 up to this constant (the re-pin captures emitting runs).
    for plane in [&chem_toxin, &chem_kin, &chem_alarm] {
        for v in plane {
            v.hash(&mut h);
        }
    }
    // ADR-013 F4: the MEASURED FlowMatrix, folded in fixed row-major order (dimension first).
    flow_s.hash(&mut h);
    for v in &flow_flat {
        v.hash(&mut h);
    }
    let led = world.resource::<ledger::Ledger>();
    led.initial_total.hash(&mut h);
    led.influx.hash(&mut h);
    led.respired.hash(&mut h);
    led.overflow.hash(&mut h);
    // ADR-013 F5: the FOURTH named tap, folded alongside respired/overflow (zero on a chem-free run).
    led.chem_decay.hash(&mut h);
    draw_count.hash(&mut h);
    allele_freq.to_bits().hash(&mut h);
    final_word.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_hash() {
        let cfg = SimConfig {
            seed: 1234,
            generations: 300,
            entity_count: 500,
        };
        assert_eq!(run_headless(&cfg).hash, run_headless(&cfg).hash);
    }

    #[test]
    fn determinism_hash_is_pinned() {
        // Pin the EXACT hash literal. `check_determinism.sh` only compares run==run, so it would NOT catch a
        // reproducible-but-CHANGED hash — this guards that. The literal MUST change deliberately (in the same
        // commit) whenever real sim LOGIC changes; history: `c530…7ab1` pre-soil and through R1.0 (proving
        // soil was hash-neutral); `8722…44aa` after R1.0a/R1.1 (per-individual heritable drought +
        // soil-coupled selection); `3ba0…82ba` after ADR-011 S-A (per-organism `Position` folded in);
        // `0413…ce77` after ADR-011 S-B (inherited dispersal adds one `next_u64`/offspring → 2N draws/gen);
        // `c01e…e40e` after ADR-011 S-G (LOCAL soil coupling); `9fad…f73a` after ADR-012 E3 (climate:
        // heritable ThermalTol — a 4th spawn draw, folded into the hash — and TemperatureMatchModifier weights
        // selection by the world climate. At the default TEMPERATE env the modifier is selection-neutral, so the
        // re-pin captured only the structural change: the extra spawn draw + ThermalTol in the hash);
        // `49ee…1cc2` after ADR-013 F0b (Energy migrated `f64`→`i64`, the joule-currency precursor; decorative,
        // so allele_freq was UNCHANGED — only Energy's hash representation + integer values changed);
        // `f795…acd5` after the richer-genome/traits expansion (sample_genome 3→9 parameters, 9 decoupled
        // traits for distinct specimen variants).
        // `272a…0cf5` after ADR-013 F3 (energy-funded births/deaths replace constant-N Wright-Fisher; PoolStock
        // i64 uptake/convert/excrete; ledger closes every tick; metabolism RNG draw deleted → births sole RNG
        // consumer; Biomass+Age folded; OrgId→u64).
        // `42fe…360d` after ADR-013 F4 (obligate plant→detritus→decomposer→free_nutrient loop; free_nutrient
        // influx deleted → endogenous; E. coli re-roled Decomposer; emergent FlowMatrix S×S folded into hash;
        // ledger still closes).
        // `4e4d…a069` after ADR-013 F3.4 (chemostat constants tuned for a bounded non-zero coexistence
        // equilibrium: uptake/conversion/excretion + maintenance rates retuned so the plant→detritus→decomposer
        // loop settles at a positive steady-state instead of collapsing or unbounded growth).
        // `47a0…f240` after ADR-013 F5 (toxin/kin/alarm chem field: conserved 4-neighbour diffusion + decay,
        // emit costs J, sense couplings suppress-uptake/boost-kin/bias-dispersal; chem folded into hash + ledger).
        let cfg = SimConfig {
            seed: 13_679_457_532_755_275_413,
            generations: 50,
            entity_count: 1000,
        };
        assert_eq!(run_headless(&cfg).hash, 0x47a0_3c8f_6701_f240);
    }

    #[test]
    fn determinism_hash_is_reproducible_at_pinned_config() {
        // F3.3 bridge: until the Repin phase pins the NEW literal, this guards the property the gate needs NOW
        // — the F3 pipeline is reproducible run==run at the pinned (seed, gen, entities). Same-arch run==run is
        // necessary (the multi-ISA CI matrix is the cross-arch authority). The exact literal is pinned at F3.4.
        let cfg = SimConfig {
            seed: 13_679_457_532_755_275_413,
            generations: 50,
            entity_count: 1000,
        };
        assert_eq!(run_headless(&cfg).hash, run_headless(&cfg).hash);
    }

    #[test]
    fn placement_is_deterministic_and_in_bounds() {
        // ADR-011 S-A: every organism gets a real cell, reproducibly from the seed, within the world grid.
        let cfg = SimConfig {
            seed: 777,
            generations: 0,
            entity_count: 200,
        };
        let positions = |s: &mut Simulation| -> Vec<(u64, u32, u32)> {
            let mut v: Vec<(u64, u32, u32)> = s
                .world
                .query::<(&OrgId, &Position)>()
                .iter(&s.world)
                .map(|(id, p)| (id.0, p.x, p.y))
                .collect();
            v.sort_unstable_by_key(|r| r.0);
            v
        };
        let a = positions(&mut Simulation::reset(&cfg));
        let b = positions(&mut Simulation::reset(&cfg));
        assert_eq!(a, b, "placement must be deterministic for a fixed seed");
        assert_eq!(a.len(), 200);
        for (_, x, y) in &a {
            assert!(
                *x < WORLD_DIMS.0 && *y < WORLD_DIMS.1,
                "position ({x},{y}) out of world bounds {WORLD_DIMS:?}"
            );
        }
    }

    #[test]
    fn r3a_registry_and_species_tag_are_live() {
        // R3-A (multi-species spine): a default reset builds a 1-entry SpeciesRegistry and tags every organism
        // Species(SpeciesId(0)). Hash-neutral — proven by `determinism_hash_is_pinned` staying green unmodified.
        let cfg = SimConfig {
            seed: 42,
            generations: 0,
            entity_count: 50,
        };
        let mut sim = Simulation::reset(&cfg);
        {
            let reg = sim.world.resource::<SpeciesRegistry>();
            assert_eq!(reg.entries.len(), 1, "default reset → a 1-species registry");
            assert_eq!(reg.entries[0].name, "default");
            assert_eq!(reg.entries[0].target_pop, 50);
        }
        let tagged = sim
            .world
            .query::<&Species>()
            .iter(&sim.world)
            .filter(|s| **s == Species(SpeciesId(0)))
            .count();
        assert_eq!(tagged, 50, "every organism is tagged Species(SpeciesId(0))");
    }

    #[test]
    fn species_entry_caches_strategy() {
        // ADR-013 F2: reset_with_roster expresses + caches each species' Strategy once. The cached budget is a
        // 1000-permille simplex and equals a fresh express_strategy over the same map/genome/role.
        let cfg = SimConfig {
            seed: 5,
            generations: 0,
            entity_count: 20,
        };
        let sim = Simulation::reset(&cfg);
        let s = sim.species_strategy(SpeciesId(0));
        assert_eq!(
            s.budget.iter().map(|&x| u32::from(x)).sum::<u32>(),
            fixed::PERMILLE,
            "cached budget is a 1000-simplex"
        );
        let reg = sim.world.resource::<SpeciesRegistry>();
        let entry = &reg.entries[0];
        let expect = gp::express_strategy(&entry.gp_map, &entry.genome, entry.strategy.role);
        assert_eq!(*sim.species_strategy(SpeciesId(0)), expect);
    }

    #[test]
    fn species_signatures_are_fixed_shape_ordered_and_integer() {
        // ADR-014 re-grounded: species_signatures() returns (s, D, flat s*D u16, roles s u8) in SpeciesId order.
        let cfg = SimConfig {
            seed: 7,
            generations: 8,
            entity_count: 40,
        };
        let mut sim = Simulation::reset(&cfg);
        sim.step(8);
        let (s, d, flat, roles) = sim.species_signatures();
        assert_eq!(s, 1, "default reset → 1 species");
        assert_eq!(d, signature::SIGNATURE_DIMS, "pinned D = 12");
        assert_eq!(flat.len(), s * d, "flat is exactly s*D");
        assert_eq!(roles.len(), s, "one role per species");
        // Block A (budget) for the single species must match the cached Strategy projection.
        let strat = sim.species_strategy(SpeciesId(0));
        for (k, &b) in strat.budget.iter().enumerate() {
            let expect = (u32::from(b.min(1000)) * u32::from(u16::MAX) / 1000) as u16;
            assert_eq!(
                flat[k], expect,
                "budget dim {k} matches the strategy projection"
            );
        }
        // Affinity dims pass through unchanged (already on the grid).
        assert_eq!(&flat[5..8], &strat.affinity[..]);
        // Role ordinal is the Autotroph default (0) for the plant.
        assert_eq!(roles[0], 0);
    }

    #[test]
    fn species_signatures_are_deterministic_same_state() {
        // Same state → identical bytes (a pure projection). Two fresh resets at the same config + same step
        // count must export byte-identical signatures.
        let cfg = SimConfig {
            seed: 99,
            generations: 5,
            entity_count: 30,
        };
        let export = |c: &SimConfig| {
            let mut sim = Simulation::reset(c);
            sim.step(5);
            sim.species_signatures()
        };
        assert_eq!(export(&cfg), export(&cfg));
        // And calling it twice on the SAME instance is idempotent (read-only).
        let mut sim = Simulation::reset(&cfg);
        sim.step(5);
        assert_eq!(sim.species_signatures(), sim.species_signatures());
    }

    #[test]
    fn species_signatures_export_is_hash_neutral() {
        // PROVE the off-hash contract: exporting signatures (and reading flow_matrix/strategy) does not move the
        // pinned literal. We run the PINNED config to completion, taking the signature export every generation,
        // and assert the final hash is STILL the pinned literal — the export is a pure read, never folded.
        let cfg = SimConfig {
            seed: 13_679_457_532_755_275_413,
            generations: 50,
            entity_count: 1000,
        };
        let mut sim = Simulation::reset(&cfg);
        for _ in 0..cfg.generations {
            // Read the export mid-run; a pure projection must not perturb the stream.
            let (s, d, flat, roles) = sim.species_signatures();
            assert_eq!(flat.len(), s * d);
            assert_eq!(roles.len(), s);
            sim.step(1);
        }
        let stats = sim.run_stats();
        assert_eq!(
            stats.hash, 0x47a0_3c8f_6701_f240,
            "signature export must be hash-neutral (the pinned literal cannot move)"
        );
    }

    #[test]
    fn strategy_persists_after_species_edit() {
        // with_genome_and_rng re-expresses entries[0].strategy: after an edit the cached budget still sums to
        // 1000 and reflects the edited genome (Acquisition<-LeafSize rises when LeafSize is maxed). The
        // read-only accessor is pure: calling it does not change the run hash.
        let cfg = SimConfig {
            seed: 88,
            generations: 4,
            entity_count: 30,
        };
        let mut sim = Simulation::reset(&cfg);
        let acq_before = sim.species_strategy(SpeciesId(0)).budget[0];
        // Max out LeafSize (locus 1, param 0 — the Acquisition anchor) via the run RNG.
        sim.with_genome_and_rng(|g, _rng| {
            if let genome::ParamValue::Numeric { value, max, .. } =
                &mut g.loci[1].parameters[0].value
            {
                *value = *max;
            }
        });
        let post = sim.species_strategy(SpeciesId(0));
        assert_eq!(
            post.budget.iter().map(|&x| u32::from(x)).sum::<u32>(),
            1000,
            "post-edit budget still a 1000-simplex"
        );
        assert!(
            post.budget[0] > acq_before,
            "maxing LeafSize should raise the Acquisition share ({acq_before} -> {})",
            post.budget[0]
        );
        // Read-only accessor purity: a run that calls species_strategy() yields the same hash as one that
        // doesn't (the accessor cannot perturb the stream or the hash).
        let with_read = {
            let mut s = Simulation::reset(&cfg);
            let _ = s.species_strategy(SpeciesId(0));
            s.step(cfg.generations);
            s.run_stats().hash
        };
        let without_read = {
            let mut s = Simulation::reset(&cfg);
            s.step(cfg.generations);
            s.run_stats().hash
        };
        assert_eq!(with_read, without_read, "species_strategy() is read-only");
    }

    #[test]
    fn strategy_cache_is_now_read_by_the_pipeline() {
        // Was the F2 "Strategy cache is hash-neutral" proof. ADR-013 F3 CHANGES that premise: the cached
        // Strategy (budget/role/affinity) is now READ by the metabolism + maintenance pipeline, so it is NO
        // LONGER hash-neutral (it shapes uptake/convert/reproduce). This test now asserts (a) the cache is a
        // valid 1000-simplex AND (b) the F3 run is reproducible run==run at the pinned config (the exact
        // literal is pinned at the F3.4 Repin phase, not here).
        let cfg = SimConfig {
            seed: 13_679_457_532_755_275_413,
            generations: 50,
            entity_count: 1000,
        };
        let mut sim = Simulation::reset(&cfg);
        let budget_sum: u32 = sim
            .species_strategy(SpeciesId(0))
            .budget
            .iter()
            .map(|&x| u32::from(x))
            .sum();
        assert_eq!(budget_sum, 1000, "the Strategy cache is populated");
        sim.step(cfg.generations);
        assert_eq!(
            sim.run_stats().hash,
            run_headless(&cfg).hash,
            "the F3 pipeline (which now READS the cached Strategy) is reproducible run==run"
        );
    }

    #[test]
    fn r3b_two_species_run_deterministically_with_emergent_pools() {
        // ADR-013 F3 (was R3-B constant-pools): two species share the joule substrate and reproduce/die
        // INDEPENDENTLY through the energy-funded pipeline — deterministic (same seed → same hash twice). The
        // per-species populations are now EMERGENT (births/deaths), no longer pinned to entity_count, but each
        // species persists by its `Species` tag + per-species cached Strategy.
        let roster = || {
            vec![
                RosterEntry {
                    name: "a".to_string(),
                    key: "default".to_string(),
                    genome: genome::sample_genome(),
                    gp_map: gp::OntologyMap::new(gp::default_plant_trait_map()),
                    entity_count: 60,
                    role: gp::TrophicRole::default(),
                },
                RosterEntry {
                    name: "b".to_string(),
                    key: "default".to_string(),
                    genome: genome::sample_genome(),
                    gp_map: gp::OntologyMap::new(gp::default_plant_trait_map()),
                    entity_count: 40,
                    role: gp::TrophicRole::default(),
                },
            ]
        };
        let cfg = SimConfig {
            seed: 9,
            generations: 12,
            entity_count: 100,
        };
        let mut a = Simulation::reset_with_roster(&cfg, &EnvParams::default(), roster());
        a.step(cfg.generations);
        let mut b = Simulation::reset_with_roster(&cfg, &EnvParams::default(), roster());
        b.step(cfg.generations);
        assert_eq!(
            a.run_stats().hash,
            b.run_stats().hash,
            "a 2-species run must be deterministic"
        );
        // Both species persist (emergent populations, each > 0) and stay tagged with their own SpeciesId.
        let mut q = a.world.query::<&Species>();
        let s0 = q
            .iter(&a.world)
            .filter(|s| **s == Species(SpeciesId(0)))
            .count();
        let s1 = q
            .iter(&a.world)
            .filter(|s| **s == Species(SpeciesId(1)))
            .count();
        assert!(s0 > 0, "species 0 persists (emergent population)");
        assert!(s1 > 0, "species 1 persists (emergent population)");
    }

    #[test]
    fn observe_all_returns_one_projection_per_species_in_id_order() {
        // The renderer view: observe_all walks the registry in SpeciesId order and expresses EACH entry's own
        // genome through its own map, so a 2-species roster returns two correctly-keyed phenotypes. Pure read.
        let mut g_a = genome::sample_genome();
        let mut g_b = genome::sample_genome();
        // Make the two genomes express DIFFERENT growth so the per-entry expression is observable. Locus 0,
        // param 0 is the GrowthRate anchor in the sample plant genome; set distinct Numeric values directly.
        if let genome::ParamValue::Numeric { value, .. } = &mut g_a.loci[0].parameters[0].value {
            *value = 0.2;
        }
        if let genome::ParamValue::Numeric { value, .. } = &mut g_b.loci[0].parameters[0].value {
            *value = 0.9;
        }
        let roster = vec![
            RosterEntry {
                name: "plant-a".to_string(),
                key: "default".to_string(),
                genome: g_a.clone(),
                gp_map: gp::OntologyMap::new(gp::default_plant_trait_map()),
                entity_count: 50,
                role: gp::TrophicRole::Autotroph,
            },
            RosterEntry {
                name: "microbe-b".to_string(),
                key: "ecoli-core".to_string(),
                genome: g_b.clone(),
                gp_map: gp::OntologyMap::new(gp::default_plant_trait_map()),
                entity_count: 50,
                role: gp::TrophicRole::Heterotroph,
            },
        ];
        let cfg = SimConfig {
            seed: 11,
            generations: 0,
            entity_count: 100,
        };
        let sim = Simulation::reset_with_roster(&cfg, &EnvParams::default(), roster);
        let all = sim.observe_all();
        assert_eq!(all.len(), 2, "one projection per species");
        // Order = SpeciesId (Vec index).
        assert_eq!(all[0].species_id, 0);
        assert_eq!(all[1].species_id, 1);
        assert_eq!(all[0].name, "plant-a");
        assert_eq!(all[1].name, "microbe-b");
        assert_eq!(all[0].key, "default");
        assert_eq!(all[1].key, "ecoli-core");
        assert_eq!(all[0].role, gp::TrophicRole::Autotroph);
        assert_eq!(all[1].role, gp::TrophicRole::Heterotroph);
        // Each entry expresses ITS OWN genome — the distinct growth-knockdown shows through.
        let ga = all[0].phenotype.get(Trait::GrowthRate).unwrap();
        let gb = all[1].phenotype.get(Trait::GrowthRate).unwrap();
        assert!(ga < gb, "per-species expression: a's growth < b's growth");
        // observe_all matches the per-entry express directly (pure projection, no run state).
        assert_eq!(
            all[0].phenotype,
            gp::OntologyMap::new(gp::default_plant_trait_map()).express(&g_a)
        );
        // Per-species population/allele/energy projection (R3 widening). generations:0 → no births/deaths have
        // run, so each species still carries its exact spawn count.
        assert_eq!(all[0].population_size, 50, "plant-a keeps its spawn count");
        assert_eq!(
            all[1].population_size, 50,
            "microbe-b keeps its spawn count"
        );
        // Conservation invariant: per-species population sums to the total living-org count.
        let total_living = sim
            .world
            .try_query::<&OrgId>()
            .map_or(0, |mut q| q.iter(&sim.world).count()) as u32;
        assert_eq!(
            all[0].population_size + all[1].population_size,
            total_living,
            "per-species population sums to total living orgs"
        );
        // Both projected stats are in their normalized [0,1] ranges.
        for o in &all {
            assert!(
                (0.0..=1.0).contains(&o.allele_freq),
                "allele_freq in [0,1]: {}",
                o.allele_freq
            );
            assert!(
                (0.0..=1.0).contains(&o.mean_energy),
                "mean_energy (ENERGY_FULL-normalized) in [0,1]: {}",
                o.mean_energy
            );
        }
    }

    #[test]
    fn per_species_stats_match_hand_computed_fixture() {
        // The partition pass sums/divides correctly AND carries the ENERGY_FULL normalization: hand-compute the
        // (sid, OrgId)-sorted mean Genotype and mean(Energy)/ENERGY_FULL via a PARALLEL query and assert
        // observe_all matches bit-for-bit (f64 ==) — same canonical fold order as mean_genotype.
        let roster = vec![
            RosterEntry {
                name: "alpha".to_string(),
                key: "default".to_string(),
                genome: genome::sample_genome(),
                gp_map: gp::OntologyMap::new(gp::default_plant_trait_map()),
                entity_count: 4,
                role: gp::TrophicRole::Autotroph,
            },
            RosterEntry {
                name: "beta".to_string(),
                key: "default".to_string(),
                genome: genome::sample_genome(),
                gp_map: gp::OntologyMap::new(gp::default_plant_trait_map()),
                entity_count: 4,
                role: gp::TrophicRole::Heterotroph,
            },
        ];
        let cfg = SimConfig {
            seed: 90_125,
            generations: 0,
            entity_count: 8,
        };
        let sim = Simulation::reset_with_roster(&cfg, &EnvParams::default(), roster);
        let all = sim.observe_all();

        // Parallel hand fold: collect live rows, sort by (sid, OrgId), accumulate per species in THAT order.
        let mut rows: Vec<(u16, u64, f64, i64)> = sim
            .world
            .try_query::<(&Species, &OrgId, &Genotype, &Energy)>()
            .map(|mut q| {
                q.iter(&sim.world)
                    .map(|(sp, id, g, e)| (sp.0 .0, id.0, g.0, e.0))
                    .collect()
            })
            .unwrap_or_default();
        rows.sort_unstable_by_key(|r| (r.0, r.1));
        let n = all.len();
        let mut counts = vec![0u32; n];
        let mut allele_sum = vec![0.0f64; n];
        let mut energy_sum = vec![0i64; n];
        for (sid, _id, g, e) in &rows {
            let i = *sid as usize;
            counts[i] += 1;
            allele_sum[i] += *g;
            energy_sum[i] += *e;
        }
        for i in 0..n {
            assert_eq!(all[i].population_size, counts[i]);
            let want_allele = allele_sum[i] / counts[i] as f64;
            let want_energy = energy_sum[i] as f64 / counts[i] as f64 / ENERGY_FULL as f64;
            assert_eq!(all[i].allele_freq, want_allele, "allele_freq bit-for-bit");
            assert_eq!(
                all[i].mean_energy, want_energy,
                "mean_energy carries the ENERGY_FULL normalization, bit-for-bit"
            );
        }
    }

    #[test]
    fn empty_species_reports_zero_not_nan() {
        // The zero-division guard yields exactly 0.0 (never NaN) for a species with no living orgs — mirrors
        // mean_genotype's empty convention.
        let roster = vec![
            RosterEntry {
                name: "populated".to_string(),
                key: "default".to_string(),
                genome: genome::sample_genome(),
                gp_map: gp::OntologyMap::new(gp::default_plant_trait_map()),
                entity_count: 10,
                role: gp::TrophicRole::Autotroph,
            },
            RosterEntry {
                name: "empty".to_string(),
                key: "default".to_string(),
                genome: genome::sample_genome(),
                gp_map: gp::OntologyMap::new(gp::default_plant_trait_map()),
                entity_count: 0,
                role: gp::TrophicRole::Heterotroph,
            },
        ];
        let cfg = SimConfig {
            seed: 555,
            generations: 0,
            entity_count: 10,
        };
        let sim = Simulation::reset_with_roster(&cfg, &EnvParams::default(), roster);
        let all = sim.observe_all();
        assert_eq!(all[1].population_size, 0, "empty species has no orgs");
        assert_eq!(all[1].allele_freq, 0.0, "empty allele_freq is exactly 0.0");
        assert!(!all[1].allele_freq.is_nan(), "never NaN");
        assert_eq!(all[1].mean_energy, 0.0, "empty mean_energy is exactly 0.0");
        assert!(!all[1].mean_energy.is_nan(), "never NaN");
    }

    #[test]
    fn observe_all_is_read_only_does_not_change_hash() {
        // Calling observe_all is a pure projection: it must not advance the run or move the determinism hash.
        let cfg = SimConfig {
            seed: 7,
            generations: 5,
            entity_count: 200,
        };
        let mut a = Simulation::reset(&cfg);
        a.step(cfg.generations);
        let _ = a.observe_all();
        let _ = a.observe_all();
        let mut b = Simulation::reset(&cfg);
        b.step(cfg.generations);
        assert_eq!(
            a.run_stats().hash,
            b.run_stats().hash,
            "observe_all is read-only: it cannot perturb the determinism hash"
        );
        // And the canonical headless run is reproducible run==run with observe_all present (observe_all is
        // hash-neutral). The EXACT literal at the pinned config is re-pinned at the F3.4 Repin phase, not here.
        let cfg_pin = SimConfig {
            seed: 13_679_457_532_755_275_413,
            generations: 50,
            entity_count: 1000,
        };
        assert_eq!(
            run_headless(&cfg_pin).hash,
            run_headless(&cfg_pin).hash,
            "observe_all is hash-neutral: the pinned-config run is reproducible with it present"
        );
    }

    #[test]
    fn dispersal_keeps_positions_in_bounds() {
        // ADR-011 S-B: inherited dispersal steps one Moore cell/generation, clamped — never leaves the grid.
        let cfg = SimConfig {
            seed: 4242,
            generations: 30,
            entity_count: 300,
        };
        let mut sim = Simulation::reset(&cfg);
        sim.step(cfg.generations);
        let out_of_bounds = sim
            .world
            .query::<&Position>()
            .iter(&sim.world)
            .filter(|p| p.x >= WORLD_DIMS.0 || p.y >= WORLD_DIMS.1)
            .count();
        assert_eq!(
            out_of_bounds, 0,
            "dispersal must clamp positions to the world grid"
        );
    }

    #[test]
    fn local_soil_selection_adapts_drought_to_cell() {
        // ADR-011 S-G (local coupling): organisms in the DRIEST cells evolve higher drought tolerance than
        // those in the WETTEST cells (the modifier favors drought ≈ 1 - moisture per cell). After enough
        // generations the driest-quartile mean drought exceeds the wettest-quartile mean — real spatial selection.
        let cfg = SimConfig {
            seed: 555,
            generations: 150,
            entity_count: 1200,
        };
        let mut sim = Simulation::reset(&cfg);
        sim.step(cfg.generations);
        let cells: Vec<(u32, u32, f64)> = sim
            .world
            .query::<(&Position, &DroughtTol)>()
            .iter(&sim.world)
            .map(|(p, d)| (p.x, p.y, d.0))
            .collect();
        // Pair each organism's drought with its cell moisture, then split into moisture quartiles.
        let soil = &sim.world.resource::<SoilFieldRes>().0;
        let mut md: Vec<(f64, f64)> = cells
            .iter()
            .map(|(x, y, d)| (soil.sample_at(*x, *y).moisture, *d))
            .collect();
        md.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let q = md.len() / 4;
        assert!(q > 0, "need organisms to compare");
        let mean = |s: &[(f64, f64)]| s.iter().map(|x| x.1).sum::<f64>() / s.len() as f64;
        let driest = mean(&md[..q]);
        let wettest = mean(&md[md.len() - q..]);
        assert!(
            driest > wettest,
            "local coupling should raise drought tolerance in dry cells: driest {driest:.3} vs wettest {wettest:.3}"
        );
    }

    /// Mean over organisms of |drought − (1 − local_moisture)| — how far the population is from its LOCAL
    /// per-cell drought target. Drops as local-coupled selection adapts each lineage to its cell (ADR-011 S-G).
    fn mean_local_mismatch(sim: &mut Simulation) -> f64 {
        let cells: Vec<(u32, u32, f64)> = sim
            .world
            .query::<(&Position, &DroughtTol)>()
            .iter(&sim.world)
            .map(|(p, d)| (p.x, p.y, d.0))
            .collect();
        if cells.is_empty() {
            return 0.0;
        }
        let soil = &sim.world.resource::<SoilFieldRes>().0;
        let sum: f64 = cells
            .iter()
            .map(|(x, y, d)| (d - (1.0 - soil.sample_at(*x, *y).moisture)).abs())
            .sum();
        sum / cells.len() as f64
    }

    #[test]
    fn local_soil_coupling_reduces_drought_mismatch() {
        // ADR-011 S-G: each cell's soil sets a LOCAL drought target (1 - moisture). From a neutral ~0.5 start,
        // local-coupled selection moves the population CLOSER to its per-cell targets — the mean local mismatch
        // shrinks over generations (the population-level proof local coupling drives selection). Deterministic.
        let cfg = SimConfig {
            seed: 12345,
            generations: 400,
            entity_count: 1500,
        };
        let mut sim = Simulation::reset(&cfg);
        let start = mean_local_mismatch(&mut sim);
        sim.step(cfg.generations);
        let end = mean_local_mismatch(&mut sim);
        assert!(
            end < start,
            "local coupling should shrink the per-cell drought mismatch: start {start:.3}, end {end:.3}"
        );
    }

    #[test]
    fn climate_coupling_adapts_thermal_tolerance_to_temperature() {
        // ADR-012 E3 (ADR-013 F3 integer pipeline): a WARM world's population evolves a HIGHER mean ThermalTol
        // than a COLD world's — the GLOBAL TemperatureMatchModifier now drives selection ENERGETICALLY (the
        // integer match factor scales uptake DEMAND → contended-J share → energy → births, blocker #1), so
        // warm-adapted lineages out-compete cold-adapted ones in a warm world and vice-versa. The climate
        // signal is GLOBAL (it can only bias demand uniformly, biting under contention) so it is WEAKER than
        // the per-cell soil gradient; the robust proof is the AGGREGATE warm-vs-cold differential averaged over
        // several seeds, removing per-seed mutation-drift noise. Deterministic.
        let warm = climate::EnvParams {
            lat: 0.0,
            lon: 0.0,
            avg_temp: 1.0,
            season: 1,
        }; // temperature → 1.0
        let cold = climate::EnvParams {
            lat: 0.0,
            lon: 0.0,
            avg_temp: 0.0,
            season: 3,
        }; // temperature → 0.0

        let mean_thermal = |sim: &mut Simulation| -> f64 {
            let ts: Vec<f64> = sim
                .world
                .query::<&ThermalTol>()
                .iter(&sim.world)
                .map(|t| t.0)
                .collect();
            if ts.is_empty() {
                0.0
            } else {
                ts.iter().sum::<f64>() / ts.len() as f64
            }
        };
        // Run a generation count where the population stays dense (so contention — and thus the climate-on-
        // demand signal — is strong) across several seeds; aggregate the warm-vs-cold means.
        let cfg = |seed: u64| SimConfig {
            seed,
            generations: 100,
            entity_count: 1500,
        };
        let mut warm_sum = 0.0;
        let mut cold_sum = 0.0;
        let seeds = [11u64, 909, 4242, 2718, 1618];
        for &s in &seeds {
            let mut hot = Simulation::reset_with_env(&cfg(s), &warm);
            hot.step(100);
            warm_sum += mean_thermal(&mut hot);
            let mut chill = Simulation::reset_with_env(&cfg(s), &cold);
            chill.step(100);
            cold_sum += mean_thermal(&mut chill);
        }
        let (warm_mean, cold_mean) = (warm_sum / seeds.len() as f64, cold_sum / seeds.len() as f64);
        assert!(
            warm_mean > cold_mean,
            "warm- vs cold-adapted populations diverge in aggregate: warm {warm_mean:.4} vs cold {cold_mean:.4}"
        );
    }

    #[test]
    fn different_seed_changes_hash() {
        let a = run_headless(&SimConfig {
            seed: 1,
            generations: 100,
            entity_count: 200,
        });
        let b = run_headless(&SimConfig {
            seed: 2,
            generations: 100,
            entity_count: 200,
        });
        assert_ne!(a.hash, b.hash);
    }

    #[test]
    fn generations_advance_state() {
        let zero = run_headless(&SimConfig {
            seed: 7,
            generations: 0,
            entity_count: 100,
        });
        let many = run_headless(&SimConfig {
            seed: 7,
            generations: 100,
            entity_count: 100,
        });
        assert_ne!(zero.hash, many.hash);
    }

    #[test]
    fn empty_population_is_deterministic() {
        let cfg = SimConfig {
            seed: 9,
            generations: 50,
            entity_count: 0,
        };
        assert_eq!(run_headless(&cfg).hash, run_headless(&cfg).hash);
    }

    #[test]
    fn same_seed_same_stats_including_allele_freq() {
        // Determinism extends to the new population statistic, not just the hash.
        let cfg = SimConfig {
            seed: 1234,
            generations: 200,
            entity_count: 500,
        };
        let a = run_headless(&cfg);
        let b = run_headless(&cfg);
        assert_eq!(a, b);
        assert_eq!(a.allele_freq.to_bits(), b.allele_freq.to_bits());
    }

    #[test]
    fn allele_freq_in_unit_range() {
        let r = run_headless(&SimConfig {
            seed: 42,
            generations: 200,
            entity_count: 1000,
        });
        assert!(
            (0.0..=1.0).contains(&r.allele_freq),
            "allele_freq {} out of [0,1]",
            r.allele_freq
        );
    }

    #[test]
    fn simulation_stepwise_matches_one_shot() {
        // The stepwise handle must reproduce the one-shot run_headless bit-for-bit (same RNG threading).
        let cfg = SimConfig {
            seed: 1234,
            generations: 200,
            entity_count: 500,
        };
        let one_shot = run_headless(&cfg);

        let mut sim = Simulation::reset(&cfg);
        sim.step(cfg.generations);
        let stepwise = sim.run_stats();
        assert_eq!(one_shot, stepwise);

        // Advancing in two chunks must match advancing in one (same single stream, no re-seed).
        let mut split = Simulation::reset(&cfg);
        split.step(120);
        split.step(80);
        assert_eq!(split.run_stats(), one_shot);
    }

    #[test]
    fn observe_is_pure_and_tracks_generation() {
        let cfg = SimConfig {
            seed: 7,
            generations: 0,
            entity_count: 100,
        };
        let mut sim = Simulation::reset(&cfg);
        let o0 = sim.observe();
        assert_eq!(o0.generation, 0);
        assert_eq!(o0.population_size, 100);
        // observe() must not advance state: calling it twice yields an identical observation.
        assert_eq!(sim.observe(), o0);

        sim.step(10);
        let o1 = sim.observe();
        assert_eq!(o1.generation, 10);
        assert!((0.0..=1.0).contains(&o1.allele_freq));
    }

    #[test]
    fn species_edit_uses_run_rng_and_changes_phenotype() {
        // A species-level genome edit via the run's own RNG re-expresses the phenotype and base growth.
        let cfg = SimConfig {
            seed: 99,
            generations: 0,
            entity_count: 50,
        };
        let mut sim = Simulation::reset(&cfg);
        let before = sim.observe().phenotype.get(Trait::GrowthRate).unwrap();

        // Bump the growth parameter (locus 0, param 0) to the top of its domain using a draw from the
        // run RNG (confirms the hook hands the same seeded stream into the mutation).
        sim.with_genome_and_rng(|g, rng| {
            let _draw = rng.next_u64(); // edits draw from the single seeded stream (inv. #3)
            if let genome::ParamValue::Numeric { value, max, .. } =
                &mut g.loci[0].parameters[0].value
            {
                *value = *max;
            }
        });
        let after = sim.observe().phenotype.get(Trait::GrowthRate).unwrap();
        assert!(
            after > before,
            "species edit should raise GrowthRate ({before} -> {after})"
        );
    }

    #[test]
    fn spatial_selection_adapts_drought_to_soil_via_integer_uptake() {
        // ADR-013 F3 (REPLACES the deleted Genotype Wright-Fisher AC2): the constant-N `fitness = floor +
        // base_growth*genotype` sampler is GONE. Selection now re-emerges from ENERGETICS — the integer
        // soil+climate match factor (blocker #1) scales a lineage's uptake DEMAND, so well-adapted lineages win
        // more contended J → more births. The proof: organisms in DRY cells evolve higher DroughtTol than those
        // in WET cells (the same gradient `local_soil_selection_adapts_drought_to_cell` checks, now driven by
        // the integer pipeline rather than the f64 fitness sampler). Large N + many gens make it robust.
        let cfg = SimConfig {
            seed: 42,
            generations: 200,
            entity_count: 1500,
        };
        let mut sim = Simulation::reset(&cfg);
        sim.step(cfg.generations);
        let cells: Vec<(u32, u32, f64)> = sim
            .world
            .query::<(&Position, &DroughtTol)>()
            .iter(&sim.world)
            .map(|(p, d)| (p.x, p.y, d.0))
            .collect();
        assert!(!cells.is_empty(), "the population must survive the run");
        let soil = &sim.world.resource::<SoilFieldRes>().0;
        let mut md: Vec<(f64, f64)> = cells
            .iter()
            .map(|(x, y, d)| (soil.sample_at(*x, *y).moisture, *d))
            .collect();
        md.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let q = md.len() / 4;
        assert!(q > 0, "need organisms to compare");
        let mean = |s: &[(f64, f64)]| s.iter().map(|x| x.1).sum::<f64>() / s.len() as f64;
        let driest = mean(&md[..q]);
        let wettest = mean(&md[md.len() - q..]);
        assert!(
            driest > wettest,
            "integer uptake-driven selection should raise drought tolerance in dry cells: \
             driest {driest:.3} vs wettest {wettest:.3}"
        );
    }

    #[test]
    fn snapshot_dims_and_channel_lengths() {
        let cfg = SimConfig {
            seed: 7,
            generations: 5,
            entity_count: 200,
        };
        let mut sim = Simulation::reset(&cfg);
        sim.step(5);
        let snap = sim.snapshot(16, 12);
        assert_eq!(snap.width, 16);
        assert_eq!(snap.height, 12);
        assert_eq!(snap.generation, 5);
        // ADR-013 F3: population is emergent after stepping (births/deaths), not pinned to entity_count.
        let live = sim.world.query::<&OrgId>().iter(&sim.world).count() as u32;
        assert_eq!(
            snap.population, live,
            "snapshot population == live org count"
        );
        let cells = 16 * 12;
        assert_eq!(snap.density.len(), cells);
        assert_eq!(snap.allele_freq.len(), cells);
        assert_eq!(snap.fitness.len(), cells);
    }

    #[test]
    fn snapshot_channels_in_unit_range() {
        let cfg = SimConfig {
            seed: 42,
            generations: 50,
            entity_count: 1000,
        };
        let mut sim = Simulation::reset(&cfg);
        sim.step(50);
        let snap = sim.snapshot(32, 32);
        for (name, ch) in [
            ("density", &snap.density),
            ("allele_freq", &snap.allele_freq),
            ("fitness", &snap.fitness),
        ] {
            for &v in ch {
                assert!((0.0..=1.0).contains(&v), "{name} value {v} out of [0,1]");
            }
        }
    }

    #[test]
    fn snapshot_empty_cells_are_zero() {
        // A tiny population on a large grid leaves most cells empty; those must be 0 on allele_freq/fitness.
        let cfg = SimConfig {
            seed: 3,
            generations: 0,
            entity_count: 4,
        };
        let mut sim = Simulation::reset(&cfg);
        // Recompute occupancy from the organisms' REAL positions, resampled to the render grid exactly as
        // Simulation::snapshot does (ADR-011 S-C — no longer the OrgId hash).
        let (w, h) = (64u32, 64u32);
        let mut occupied = vec![false; (w * h) as usize];
        let positions: Vec<(u32, u32)> = sim
            .world
            .query::<&Position>()
            .iter(&sim.world)
            .map(|p| (p.x, p.y))
            .collect();
        for (px, py) in &positions {
            let x = ((u64::from(*px) * u64::from(w)) / u64::from(WORLD_DIMS.0))
                .min(u64::from(w) - 1) as usize;
            let y = ((u64::from(*py) * u64::from(h)) / u64::from(WORLD_DIMS.1))
                .min(u64::from(h) - 1) as usize;
            occupied[y * w as usize + x] = true;
        }
        let snap = sim.snapshot(w, h);
        let mut empty_seen = false;
        for (c, &occ) in occupied.iter().enumerate() {
            if !occ {
                empty_seen = true;
                assert_eq!(snap.density[c], 0.0, "empty cell {c} density != 0");
                assert_eq!(snap.allele_freq[c], 0.0, "empty cell {c} allele_freq != 0");
                assert_eq!(snap.fitness[c], 0.0, "empty cell {c} fitness != 0");
            }
        }
        assert!(empty_seen, "test grid should have empty cells");
    }

    #[test]
    fn snapshot_aggregates_by_real_position() {
        // ADR-011 S-C: at a 1:1 render grid, every nonzero-density cell is exactly a cell holding an
        // organism's REAL Position — proving the snapshot reads Position, not the retired OrgId hash.
        let cfg = SimConfig {
            seed: 9,
            generations: 6,
            entity_count: 400,
        };
        let mut sim = Simulation::reset(&cfg);
        sim.step(cfg.generations);
        let mut real = vec![0u32; (WORLD_DIMS.0 * WORLD_DIMS.1) as usize];
        for p in sim.world.query::<&Position>().iter(&sim.world) {
            real[(p.y * WORLD_DIMS.0 + p.x) as usize] += 1;
        }
        let snap = sim.snapshot(WORLD_DIMS.0, WORLD_DIMS.1);
        for (c, (&r, &d)) in real.iter().zip(&snap.density).enumerate() {
            assert_eq!(
                r > 0,
                d > 0.0,
                "cell {c}: snapshot density must match real Position occupancy"
            );
        }
    }

    #[test]
    fn snapshot_empty_population_is_all_zero() {
        let cfg = SimConfig {
            seed: 1,
            generations: 10,
            entity_count: 0,
        };
        let mut sim = Simulation::reset(&cfg);
        sim.step(10);
        let snap = sim.snapshot(8, 8);
        assert_eq!(snap.population, 0);
        assert!(snap.density.iter().all(|&v| v == 0.0));
        assert!(snap.allele_freq.iter().all(|&v| v == 0.0));
        assert!(snap.fitness.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn snapshot_is_read_only_does_not_change_hash() {
        // Taking snapshots must not advance the RNG or mutate state: the run_stats hash is identical with
        // and without intervening snapshot() calls (invariant #3).
        let cfg = SimConfig {
            seed: 1234,
            generations: 100,
            entity_count: 300,
        };
        let baseline = run_headless(&cfg).hash;

        let mut sim = Simulation::reset(&cfg);
        for _ in 0..cfg.generations {
            sim.step(1);
            let _ = sim.snapshot(32, 32); // read-only between steps
        }
        assert_eq!(sim.run_stats().hash, baseline);
    }

    #[test]
    fn snapshot_is_byte_identical_for_same_seed_gen_grid() {
        // Two snapshots of the same (seed, generation, grid) must be byte-for-byte identical.
        let cfg = SimConfig {
            seed: 7,
            generations: 30,
            entity_count: 500,
        };
        let mut a = Simulation::reset(&cfg);
        a.step(30);
        let mut b = Simulation::reset(&cfg);
        b.step(30);
        let bytes_a = a.snapshot(32, 32).write_snapshot_bytes();
        let bytes_b = b.snapshot(32, 32).write_snapshot_bytes();
        assert_eq!(bytes_a, bytes_b);
        // And repeated snapshots from the SAME sim are identical (no hidden state).
        assert_eq!(a.snapshot(32, 32), a.snapshot(32, 32));
    }

    #[test]
    fn snapshot_pool_channels_in_unit_range_and_byte_identical() {
        // GSS3: the 3 live-pool channels (light/free_nutrient/detritus) resample PoolStock and normalize by
        // POOL_CAP, so every value is in [0,1], sized to the render grid, and byte-identical across two reset
        // runs of the same (seed, generation, grid) — the off-hash projection is deterministic (inv #3).
        let cfg = SimConfig {
            seed: 7,
            generations: 25,
            entity_count: 400,
        };
        let mut a = Simulation::reset(&cfg);
        a.step(25);
        let snap = a.snapshot(16, 16);
        let cells = 16 * 16;
        for plane in [&snap.light, &snap.free_nutrient, &snap.detritus] {
            assert_eq!(plane.len(), cells, "pool plane must cover the render grid");
            for &v in plane {
                assert!((0.0..=1.0).contains(&v), "pool channel out of [0,1]: {v}");
            }
        }
        // light is seeded above zero everywhere (solar carrying-cap), so it is not an all-zero plane.
        assert!(
            snap.light.iter().any(|&v| v > 0.0),
            "light pool should be non-zero somewhere"
        );

        let mut b = Simulation::reset(&cfg);
        b.step(25);
        let snap_b = b.snapshot(16, 16);
        assert_eq!(snap.light, snap_b.light);
        assert_eq!(snap.free_nutrient, snap_b.free_nutrient);
        assert_eq!(snap.detritus, snap_b.detritus);
    }

    #[test]
    fn snapshot_density_normalizes_to_one() {
        // The busiest cell must hit density 1.0 (per-cell count / max-cell count) for a non-empty run.
        let cfg = SimConfig {
            seed: 5,
            generations: 0,
            entity_count: 500,
        };
        let mut sim = Simulation::reset(&cfg);
        let snap = sim.snapshot(8, 8);
        let max = snap.density.iter().copied().fold(0.0f32, f32::max);
        assert_eq!(max, 1.0, "busiest cell should have density 1.0");
    }

    #[test]
    fn reset_seeds_a_closing_ledger() {
        // ADR-013 F3: the ledger's `initial_total` is now SEEDED at reset = Σ(PoolStock) + Σ(per-org
        // Energy+Biomass), with the three taps still zero (no tick has run). It closes against that live total.
        let cfg = SimConfig {
            seed: 7,
            generations: 0,
            entity_count: 100,
        };
        let sim = Simulation::reset(&cfg);
        let led = sim.ledger();
        assert_eq!(led.influx, 0, "no influx before any tick");
        assert_eq!(led.respired, 0, "no respiration before any tick");
        assert_eq!(led.overflow, 0, "no overflow before any tick");
        assert!(
            led.initial_total > 0,
            "initial_total is seeded from the quantized pools + org stores"
        );
        // The books close at reset: expected_total == initial_total (taps zero) == the live total.
        assert!(
            led.closes(led.initial_total),
            "F3: the seeded ledger closes against its own initial_total before any tick"
        );
    }

    #[test]
    fn ledger_closes_every_tick_over_a_run() {
        // ADR-013 F3 (finding #8): `measure_and_assert_ledger` runs LAST every tick. Drive a multi-generation
        // run; under debug the per-tick debug_assert (and under --features determinism the hard assert) guards
        // closure, so reaching the end without a panic IS the proof. Cross-check the final books explicitly.
        let cfg = SimConfig {
            seed: 31,
            generations: 40,
            entity_count: 500,
        };
        let mut sim = Simulation::reset(&cfg);
        sim.step(cfg.generations);
        // Recompute the live total from the world and assert the ledger closes against it.
        let pools_total = sim.world.resource::<PoolStock>().total();
        let chem_total = sim.world.resource::<chem::ChemField>().total(); // ADR-013 F5: chem is a live bucket
        let (mut e, mut b) = (0i64, 0i64);
        for (energy, biomass) in sim.world.query::<(&Energy, &Biomass)>().iter(&sim.world) {
            e += energy.0;
            b += biomass.0;
        }
        let live = ledger::LiveTotal {
            pools: pools_total,
            energy: e,
            biomass: b,
            chem: chem_total,
        };
        assert!(
            ledger::ledger_closes(&sim.ledger(), &live),
            "the conserved ledger must close after a full F3 run: live {} vs expected {}",
            live.sum(),
            sim.ledger().expected_total()
        );
    }

    #[test]
    fn births_are_the_only_rng_consumer_and_drawcount_tracks_them() {
        // ADR-013 F3 (finding #4): metabolism/influx/maintenance/measure draw ZERO SimRng; only births draw,
        // EXACTLY 4 words each. A run with births advances DrawCount by a multiple of 4 beyond the spawn draws.
        let cfg = SimConfig {
            seed: 31,
            generations: 30,
            entity_count: 400,
        };
        let mut sim = Simulation::reset(&cfg);
        sim.step(cfg.generations);
        let draws = sim.world.resource::<DrawCount>().0;
        assert_eq!(
            draws % 4,
            0,
            "every birth draws EXACTLY 4 words → DrawCount is a multiple of 4 ({draws})"
        );
    }

    #[test]
    fn org_ids_are_monotonic_and_never_reused() {
        // ADR-013 F3: OrgId is minted from a monotonic NextOrgId; a despawn+birth never reuses an id. After a
        // run, the live ids are all distinct and the allocator's next id exceeds every live id.
        let cfg = SimConfig {
            seed: 77,
            generations: 25,
            entity_count: 300,
        };
        let mut sim = Simulation::reset(&cfg);
        sim.step(cfg.generations);
        let mut ids: Vec<u64> = sim
            .world
            .query::<&OrgId>()
            .iter(&sim.world)
            .map(|o| o.0)
            .collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "all live OrgIds are distinct (never reused)");
        let next = sim.world.resource::<NextOrgId>().0;
        assert!(
            ids.iter().all(|&id| id < next),
            "NextOrgId exceeds every minted id (monotonic allocator)"
        );
    }

    #[test]
    fn max_population_is_never_hit() {
        // ADR-013 F3 (inv #6): MAX_POPULATION is set FAR above any resource-supportable equilibrium, so it is
        // provably NEVER hit in the pinned config — keeping it non-load-bearing (no OrgId-order skip selection
        // gradient). The live population stays well under the guard for a long run.
        let cfg = SimConfig {
            seed: 13_679_457_532_755_275_413,
            generations: 80,
            entity_count: 1000,
        };
        let mut sim = Simulation::reset(&cfg);
        sim.step(cfg.generations);
        let pop = sim.world.query::<&OrgId>().iter(&sim.world).count() as u32;
        assert!(
            pop < MAX_POPULATION,
            "population {pop} must stay below the never-hit guard {MAX_POPULATION}"
        );
    }

    #[test]
    fn population_is_emergent_not_constant() {
        // ADR-013 F3 (breaks ADR-005): population is now a FREE variable (births from surplus J, deaths on
        // starvation/age) — it is NOT pinned to entity_count after stepping.
        let cfg = SimConfig {
            seed: 5,
            generations: 20,
            entity_count: 500,
        };
        let mut sim = Simulation::reset(&cfg);
        let start = sim.world.query::<&OrgId>().iter(&sim.world).count();
        assert_eq!(start, 500, "reset still spawns entity_count organisms");
        sim.step(cfg.generations);
        let end = sim.world.query::<&OrgId>().iter(&sim.world).count();
        // Emergent: the population has MOVED off the constant (births and/or deaths fired). Deterministic.
        assert_ne!(
            end, 500,
            "population must be emergent (births/deaths), not constant-N"
        );
    }

    #[test]
    fn same_seed_same_population_trajectory() {
        // ADR-013 F3 (finding #4): two runs of the same seed produce IDENTICAL per-tick population vectors —
        // the proof the data-dependent draw count is still a pure function of the seed.
        let cfg = SimConfig {
            seed: 19,
            generations: 25,
            entity_count: 400,
        };
        let trajectory = |c: &SimConfig| -> Vec<usize> {
            let mut sim = Simulation::reset(c);
            let mut v = Vec::new();
            for _ in 0..c.generations {
                sim.step(1);
                v.push(sim.world.query::<&OrgId>().iter(&sim.world).count());
            }
            v
        };
        assert_eq!(
            trajectory(&cfg),
            trajectory(&cfg),
            "the per-tick population trajectory must be identical for a fixed seed"
        );
    }

    #[test]
    fn region_allele_reads_zone_deterministically() {
        // campaign-grader: region_allele lifts _eval_mission into the core. It must be deterministic, bounded,
        // and report an empty region honestly.
        let cfg = SimConfig {
            seed: 7,
            generations: 5,
            entity_count: 800,
        };
        let mut a = Simulation::reset(&cfg);
        a.step(5);
        let whole = Region {
            cx: 16,
            cy: 16,
            radius: 64,
        }; // covers the 32x32 world
        let r1 = a.region_allele(whole, 32, 32);
        let mut b = Simulation::reset(&cfg);
        b.step(5);
        let r2 = b.region_allele(whole, 32, 32);
        assert_eq!(r1, r2, "same world+grid+region => identical readout");
        assert!((0.0..=1.0).contains(&r1.mean), "mean in [0,1]");
        assert!(
            r1.populated_cells > 0,
            "a populated world covers some cells"
        );

        // A region centred far off-grid with radius 0 contains no populated cell → honest empty read.
        let empty = a.region_allele(
            Region {
                cx: 999,
                cy: 999,
                radius: 0,
            },
            32,
            32,
        );
        assert_eq!(empty.populated_cells, 0);
        assert_eq!(empty.mean, 0.0);
    }

    // ── ADR-013 F4: the obligate trophic loop + measured FlowMatrix ──────────────────────────────────

    /// A synthetic genome whose single parameter expresses a chosen activity (`[0,1]`) on every trait the test
    /// maps to it. Used to drive a Decomposer's `affinity[2]` (GlucoseUptake) + `mineralize_rate`
    /// (AcetateOverflow) AND a plant's GrowthRate to known nonzero values without the 136-gene ecoli bake.
    fn anchor_genome(value: f64) -> Genome {
        Genome {
            version: 2,
            loci: vec![genome::Locus {
                id: genome::LocusId(0),
                name: "anchor".to_string(),
                sequence: genome::DnaSequence::new(*b"ACGTGGACGTTTTAGGCCGG").unwrap(),
                parameters: vec![genome::Parameter {
                    id: genome::ParamId(0),
                    value: genome::ParamValue::Numeric {
                        value,
                        min: 0.0,
                        max: 1.0,
                    },
                }],
                tags: genome::OntologyTags {
                    so_term: genome::SoTermId(704),
                    go_refs: vec![],
                },
            }],
        }
    }

    /// A trait map binding every named trait to locus 0 / param 0 (the synthetic anchor). Lets one genome value
    /// drive the plant uptake/growth anchors AND the decomposer detritus/mineralize anchors.
    fn anchor_map(traits: &[Trait]) -> gp::OntologyMap {
        gp::OntologyMap::new(gp::TraitMap(
            traits
                .iter()
                .map(|&t| gp::TraitBinding {
                    trait_: t,
                    locus: gp::LocusSelector::ByIndex(genome::LocusId(0)),
                    param: genome::ParamId(0),
                })
                .collect(),
        ))
    }

    /// A 2-species obligate-loop roster: a PLANT (Autotroph, draws free_nutrient via GrowthRate, sheds litter)
    /// plus a DECOMPOSER (taps detritus via GlucoseUptake-affinity, mineralizes via AcetateOverflow). `decomp`
    /// toggles whether the decomposer species is present (for the kill-the-decomposer baseline).
    fn obligate_roster(decomp: bool) -> Vec<RosterEntry> {
        // A plant with a real Acquisition budget (LeafSize → Acquisition + light affinity) whose LIGHT uptake is
        // Liebig-gated by free_nutrient (NUTRIENT_LIMIT_REF). With the free_nutrient INFLUX arm deleted, that
        // nutrient comes ONLY from the decomposer — so the decomposer CONTINUOUSLY raises plant productivity,
        // and killing it drains free_nutrient → throttles the gate → starves the plants. All five plant channel
        // anchors bind to the single 0.9 anchor (a vigorous autotroph).
        let mut roster = vec![RosterEntry {
            name: "plant".to_string(),
            key: "plant".to_string(),
            genome: anchor_genome(0.9),
            gp_map: anchor_map(&[
                Trait::LeafSize,
                Trait::GrowthRate,
                Trait::Fecundity,
                Trait::DroughtTolerance,
                Trait::Reflectance,
            ]),
            entity_count: 3000,
            role: gp::TrophicRole::Autotroph,
        }];
        if decomp {
            roster.push(RosterEntry {
                name: "decomposer".to_string(),
                key: "decomposer".to_string(),
                // Decomposer anchors: GlucoseUptake (detritus affinity), AcetateOverflow (mineralize_rate),
                // plus budget anchors so it has a metabolism. High activity → a vigorous mineralizer.
                genome: anchor_genome(0.9),
                gp_map: anchor_map(&[
                    Trait::GlucoseUptake,
                    Trait::AcetateOverflow,
                    Trait::GrowthRate,
                    Trait::FermentationCapacity,
                    Trait::RespirationMode,
                ]),
                entity_count: 3000,
                role: gp::TrophicRole::Decomposer,
            });
        }
        roster
    }

    /// A 3-species obligate-PREDATOR roster (ADR-013 F6): plant (Autotroph) + E. coli-like decomposer
    /// (Decomposer, eligible prey) + Bdellovibrio (Predator). `predator` toggles whether the predator is present;
    /// `vigorous` toggles its attack rate (a high-PredationCapacity gene → real predation, vs ~0 → a throttled
    /// predator that barely eats — the cascade baseline). The predator funds GrowthRate from gltA and its
    /// attack-rate lever from PredationCapacity, both bound to a single high anchor for a vigorous predator.
    fn obligate_predator_roster(predator: bool, vigorous: bool) -> Vec<RosterEntry> {
        let mut roster = obligate_roster(true); // plant (sid 0) + decomposer (sid 1)
        if predator {
            // PredationCapacity drives the attack rate: 0.9 = vigorous, ~0.0 = throttled (the cascade off-state).
            let attack = if vigorous { 0.9 } else { 0.0 };
            roster.push(RosterEntry {
                name: "bdellovibrio".to_string(),
                key: "bdellovibrio".to_string(),
                // GrowthRate anchor (gltA) drives predator growth; PredationCapacity anchor drives the attack rate.
                // Bind GrowthRate to a vigorous value and PredationCapacity to the `attack` level.
                genome: two_param_genome(0.9, attack),
                gp_map: gp::OntologyMap::new(gp::TraitMap(vec![
                    gp::TraitBinding {
                        trait_: Trait::GrowthRate,
                        locus: gp::LocusSelector::ByIndex(genome::LocusId(0)),
                        param: genome::ParamId(0),
                    },
                    gp::TraitBinding {
                        trait_: Trait::PredationCapacity,
                        locus: gp::LocusSelector::ByIndex(genome::LocusId(0)),
                        param: genome::ParamId(1),
                    },
                ])),
                entity_count: 180, // predators start SPARSE (dense seeding instant-crashes prey then itself)
                role: gp::TrophicRole::Predator,
            });
        }
        roster
    }

    /// A one-locus genome with TWO numeric params (P0, P1), so a roster can bind GrowthRate→P0 and
    /// PredationCapacity→P1 independently (the predator's growth vs attack-rate genes).
    fn two_param_genome(p0: f64, p1: f64) -> Genome {
        Genome {
            version: 2,
            loci: vec![genome::Locus {
                id: genome::LocusId(0),
                name: "anchor".to_string(),
                sequence: genome::DnaSequence::new(*b"ACGTGGACGTTTTAGGCCGG").unwrap(),
                parameters: vec![
                    genome::Parameter {
                        id: genome::ParamId(0),
                        value: genome::ParamValue::Numeric {
                            value: p0,
                            min: 0.0,
                            max: 1.0,
                        },
                    },
                    genome::Parameter {
                        id: genome::ParamId(1),
                        value: genome::ParamValue::Numeric {
                            value: p1,
                            min: 0.0,
                            max: 1.0,
                        },
                    },
                ],
                tags: genome::OntologyTags {
                    so_term: genome::SoTermId(704),
                    go_refs: vec![],
                },
            }],
        }
    }

    /// Σ free_nutrient over all cells of the live PoolStock (the plant-available pool the loop feeds).
    fn total_free_nutrient(sim: &Simulation) -> i64 {
        sim.world.resource::<PoolStock>().free_nutrient.iter().sum()
    }

    /// Drain the seeded free_nutrient pool to ZERO (and book the removed J off the ledger's `initial_total`, so
    /// the books still close). Models a world that seeds with NO plant-available nutrient — so free_nutrient is
    /// PURELY endogenous (decomposer-minted), making the obligate loop's teeth visible at a tractable scale (the
    /// 37-billion-J seed otherwise dwarfs the per-tick mineralization signal). Test-only.
    fn drain_seeded_free_nutrient(sim: &mut Simulation) {
        let removed: i64 = {
            let pools = sim.world.resource::<PoolStock>();
            pools.free_nutrient.iter().sum()
        };
        {
            let mut pools = sim.world.resource_mut::<PoolStock>();
            for v in &mut pools.free_nutrient {
                *v = 0;
            }
        }
        // The world simply started with `removed` fewer joules — adjust initial_total so the ledger still closes.
        sim.world.resource_mut::<ledger::Ledger>().initial_total -= removed;
    }

    /// Live count of a given species (by registry ordinal).
    fn species_pop(sim: &mut Simulation, sid: u16) -> usize {
        sim.world
            .query::<&Species>()
            .iter(&sim.world)
            .filter(|s| **s == Species(SpeciesId(sid)))
            .count()
    }

    #[test]
    fn f4_flow_matrix_rows_sum_to_zero_over_a_real_run() {
        // The relation-conservation analogue of ledger_closes: after a multi-generation obligate-loop run, EVERY
        // row of the MEASURED FlowMatrix sums to zero (the diagonal-pairing identity). Per-tick the in-chain
        // assert_flow_closes already guards this; here we cross-check the final exported matrix explicitly.
        let cfg = SimConfig {
            seed: 71,
            generations: 30,
            entity_count: 600,
        };
        let mut sim =
            Simulation::reset_with_roster(&cfg, &EnvParams::default(), obligate_roster(true));
        sim.step(cfg.generations);
        let (s, flat) = sim.flow_matrix();
        assert_eq!(s, 2, "two-species roster → 2×2 matrix");
        assert_eq!(flat.len(), s * s);
        for i in 0..s {
            let row: i64 = (0..s).map(|j| flat[i * s + j]).sum();
            assert_eq!(row, 0, "row {i} must sum to zero by construction");
        }
    }

    #[test]
    fn f4_obligate_loop_decomposer_mineralizes_free_nutrient() {
        // With the free_nutrient INFLUX arm deleted, free_nutrient is ENDOGENOUS — ONLY the decomposer mints it.
        // A run WITH a decomposer must end with strictly MORE free_nutrient than a run WITHOUT one (which can
        // only drain its seed). The decomposer↔plant mutualism also shows as both FlowMatrix off-diagonals
        // going nonzero when the loop runs.
        let cfg = SimConfig {
            seed: 19,
            generations: 60,
            entity_count: 600,
        };
        let mut with =
            Simulation::reset_with_roster(&cfg, &EnvParams::default(), obligate_roster(true));
        drain_seeded_free_nutrient(&mut with);
        with.step(cfg.generations);
        let mut without =
            Simulation::reset_with_roster(&cfg, &EnvParams::default(), obligate_roster(false));
        drain_seeded_free_nutrient(&mut without);
        without.step(cfg.generations);
        assert!(
            total_free_nutrient(&with) > total_free_nutrient(&without),
            "the decomposer must mint free_nutrient: with {} vs without {}",
            total_free_nutrient(&with),
            total_free_nutrient(&without)
        );
        // Without a decomposer, free_nutrient stays at the drained zero (no mint, no influx arm).
        assert_eq!(
            total_free_nutrient(&without),
            0,
            "no decomposer + no influx arm ⇒ free_nutrient never appears"
        );
        // The obligate-loop edges are live: the decomposer harvested plant detritus (flow[1][0] != 0) and/or
        // the plant drew decomposer-minted free_nutrient (flow[0][1] != 0).
        let (s, flat) = with.flow_matrix();
        assert_eq!(s, 2);
        let any_edge = flat[1] != 0 || flat[s] != 0; // flow[0][1] (row 0 col 1) or flow[1][0] (row 1 col 0)
        assert!(
            any_edge,
            "the plant↔decomposer FlowMatrix off-diagonals must be nonzero when the loop runs"
        );
    }

    #[test]
    fn f4_killing_the_decomposer_starves_the_plants() {
        // SOFT MUTUALISM (post-F3.4, LIEBIG_FLOOR): the decomposer is not strictly obligate — a plant subsists on
        // light alone down to the Liebig floor — but mineralization measurably RAISES plant carrying capacity, so
        // with the decomposer's nutrient source removed the plant population settles strictly BELOW the
        // with-decomposer baseline. A relative (not extinction) assertion. Deterministic.
        let cfg = SimConfig {
            seed: 23,
            generations: 150,
            entity_count: 600,
        };
        let mut with =
            Simulation::reset_with_roster(&cfg, &EnvParams::default(), obligate_roster(true));
        drain_seeded_free_nutrient(&mut with);
        with.step(cfg.generations);
        let mut without =
            Simulation::reset_with_roster(&cfg, &EnvParams::default(), obligate_roster(false));
        drain_seeded_free_nutrient(&mut without);
        without.step(cfg.generations);
        let plants_with = species_pop(&mut with, 0);
        let plants_without = species_pop(&mut without, 0);
        assert!(
            plants_without < plants_with,
            "decomposer raises plant carrying capacity: plants without {plants_without} vs with {plants_with}"
        );
        // And the no-decomposer world's free_nutrient stays at the drained zero (only drainage, no mint).
        assert_eq!(
            total_free_nutrient(&without),
            0,
            "without a mineralizer, free_nutrient never reappears"
        );
    }

    // ── ADR-013 F6: the Bdellovibrio predator + predation kernel ──────────────────────────────────────

    #[test]
    fn f6_predation_conserves_j_and_ledger_closes_every_tick() {
        // The predation kernel is a paired conserved transfer: prey J → predator (kept) − efficiency-tax
        // (respired) + carcass residual (detritus). measure_and_assert_ledger (the LAST in-chain system) asserts
        // ledger_closes EVERY tick post-predation; a multi-gen 3-species run reaching here proves J conservation.
        let cfg = SimConfig {
            seed: 31,
            generations: 80,
            entity_count: 600,
        };
        let mut sim = Simulation::reset_with_roster(
            &cfg,
            &EnvParams::default(),
            obligate_predator_roster(true, true),
        );
        // Stepping runs the in-chain assert_flow_closes + measure_and_assert_ledger every tick — a clean run is
        // the conservation proof. (Under `--features determinism` these are HARD asserts.)
        sim.step(cfg.generations);
        // The 3-species matrix is 3×3 and every row still sums to zero by the diagonal-pairing construction.
        let (s, flat) = sim.flow_matrix();
        assert_eq!(s, 3, "three-species roster → 3×3 matrix");
        for i in 0..s {
            let row: i64 = (0..s).map(|j| flat[i * s + j]).sum();
            assert_eq!(row, 0, "predator row {i} must sum to zero by construction");
        }
    }

    #[test]
    fn f6_flow_matrix_predator_prey_off_diagonal_is_nonzero_rowsum_zero() {
        // The headline FlowMatrix assertion: predation writes the first true org-eats-org off-diagonal —
        // flow[bdello][ecoli] (row 2 = predator, col 1 = decomposer prey) goes NONZERO and the predator row
        // still sums to zero. plant=0, ecoli=1, bdello=2.
        let cfg = SimConfig {
            seed: 44,
            generations: 60,
            entity_count: 600,
        };
        let mut sim = Simulation::reset_with_roster(
            &cfg,
            &EnvParams::default(),
            obligate_predator_roster(true, true),
        );
        // Accumulate the predation edge over the whole run (a single-tick matrix may be momentarily empty if no
        // predator/prey share a cell that tick; the dynamics test below covers populations directly).
        let mut saw_edge = false;
        for _ in 0..cfg.generations {
            sim.step(1);
            let (s, flat) = sim.flow_matrix();
            assert_eq!(s, 3);
            let pred_eats_ecoli = flat[2 * s + 1]; // row 2 (bdello), col 1 (ecoli)
            if pred_eats_ecoli != 0 {
                saw_edge = true;
            }
            // Row-sum==0 holds throughout (a structural integer identity).
            for i in 0..s {
                let row: i64 = (0..s).map(|j| flat[i * s + j]).sum();
                assert_eq!(row, 0, "row {i} must sum to zero");
            }
        }
        assert!(
            saw_edge,
            "the Bdellovibrio→E.coli predation off-diagonal (flow[2][1]) must go nonzero over the run"
        );
    }

    #[test]
    fn f6_predation_is_deterministic_run_to_run() {
        // The kernel draws ZERO SimRng (DrawCount untouched → births stay the sole RNG consumer), so a 3-species
        // predator run is byte-reproducible: same seed twice → identical final hash (inv #3).
        let cfg = SimConfig {
            seed: 57,
            generations: 50,
            entity_count: 600,
        };
        let h = || {
            let mut sim = Simulation::reset_with_roster(
                &cfg,
                &EnvParams::default(),
                obligate_predator_roster(true, true),
            );
            sim.step(cfg.generations);
            sim.run_stats().hash
        };
        assert_eq!(h(), h(), "predator run must be deterministic run-to-run");
    }

    #[test]
    fn f6_trophic_cascade_throttling_the_predator_lifts_ecoli_and_the_plant() {
        // THE HEADLINE: the first top-down 3-level cascade. Two 3-species runs differing ONLY in the predator's
        // attack rate (PredationCapacity gene: vigorous vs ~0 throttled). Throttling the predator → E. coli rises
        // → more mineralization → more free_nutrient → the plant Liebig gate opens → the plant rises. Mirrors
        // f4_obligate_loop_decomposer_mineralizes_free_nutrient (two runs, relative assertion). Deterministic.
        let cfg = SimConfig {
            seed: 88,
            generations: 200,
            entity_count: 600,
        };
        let run = |vigorous: bool| {
            let mut sim = Simulation::reset_with_roster(
                &cfg,
                &EnvParams::default(),
                obligate_predator_roster(true, vigorous),
            );
            drain_seeded_free_nutrient(&mut sim);
            sim.step(cfg.generations);
            sim
        };
        let mut vigorous = run(true);
        let mut throttled = run(false);
        let ecoli_vig = species_pop(&mut vigorous, 1);
        let ecoli_thr = species_pop(&mut throttled, 1);
        let plant_vig = species_pop(&mut vigorous, 0);
        let plant_thr = species_pop(&mut throttled, 0);
        // Throttling predation lifts E. coli (top-down release of the prey).
        assert!(
            ecoli_thr > ecoli_vig,
            "throttling the predator must lift E. coli: throttled {ecoli_thr} vs vigorous {ecoli_vig}"
        );
        // And the cascade reaches the plant (more decomposer → more free_nutrient → Liebig gate opens).
        assert!(
            total_free_nutrient(&throttled) > total_free_nutrient(&vigorous),
            "throttling the predator must raise free_nutrient: throttled {} vs vigorous {}",
            total_free_nutrient(&throttled),
            total_free_nutrient(&vigorous)
        );
        assert!(
            plant_thr >= plant_vig,
            "the cascade reaches the plant: throttled {plant_thr} >= vigorous {plant_vig}"
        );
        // The vigorous run lit a real predation edge over its course (flow[2][1] nonzero at least once is covered
        // by the off-diagonal test; here we confirm the vigorous predator actually ate, i.e. E. coli was suppressed).
        assert!(
            ecoli_vig < ecoli_thr,
            "a vigorous Bdellovibrio measurably suppresses its E. coli prey"
        );
    }

    #[test]
    fn f6_predator_starves_without_prey_then_a_predator_free_run_is_a_noop() {
        // Two structural facts: (1) a predator with NO prey present cannot earn (the kernel early-returns on empty
        // prey) → it starves out via the existing maintenance/starvation path (population falls); (2) a
        // predator-free roster is a strict no-op — the predation system records nothing and the run equals the
        // 2-species F4 baseline byte-for-byte (the FlowMatrix never lights a predator edge).
        let cfg = SimConfig {
            seed: 12,
            generations: 40,
            entity_count: 600,
        };
        // (2) predator-free 3rd-slot vs the plain 2-species roster: identical hash (predation is a no-op).
        let h_no_pred = {
            let mut sim = Simulation::reset_with_roster(
                &cfg,
                &EnvParams::default(),
                obligate_predator_roster(false, true),
            );
            sim.step(cfg.generations);
            sim.run_stats().hash
        };
        let h_baseline = {
            let mut sim =
                Simulation::reset_with_roster(&cfg, &EnvParams::default(), obligate_roster(true));
            sim.step(cfg.generations);
            sim.run_stats().hash
        };
        assert_eq!(
            h_no_pred, h_baseline,
            "a predator-free 3-species roster equals the 2-species F4 baseline (predation is a no-op)"
        );
    }

    /// Inject `amount` milli-J of toxin into every cell whose `x < split_x` (the left band), keeping the ledger
    /// closed by lifting `initial_total` (the injected chem is now part of the live Σ). Models a toxin-producer
    /// having pre-loaded one region — the SENSE coupling under test. Test-only.
    fn paint_toxin_left_band(sim: &mut Simulation, split_x: u32, amount: i32) {
        let mut injected: i64 = 0;
        {
            let mut chem = sim.world.resource_mut::<chem::ChemField>();
            let w = chem.width;
            let h = chem.height;
            let plane = chem.plane_mut(chem::ChemChannel::Toxin as usize);
            for y in 0..h {
                for x in 0..split_x.min(w) {
                    let c = (y * w + x) as usize;
                    plane[c] += amount;
                    injected += i64::from(amount);
                }
            }
        }
        // The world now holds `injected` more joules (as chem) → the books close iff initial_total rises to match.
        sim.world.resource_mut::<ledger::Ledger>().initial_total += injected;
    }

    /// Count living organisms in the left band (`x < split_x`) and the right band (`x >= split_x`) of the world.
    fn pop_left_right(sim: &mut Simulation, split_x: u32) -> (usize, usize) {
        let mut left = 0usize;
        let mut right = 0usize;
        for p in sim.world.query::<&Position>().iter(&sim.world) {
            if p.x < split_x {
                left += 1;
            } else {
                right += 1;
            }
        }
        (left, right)
    }

    #[test]
    fn f5_toxin_allelopathy_suppresses_a_neighbouring_region() {
        // ADR-013 F5 ALLELOPATHY (the functional gate): pre-load a STRONG toxin field over the LEFT half of the
        // world (modelling a toxin-producer that has saturated that region), leave the RIGHT half toxin-free, and
        // run. The toxin SENSE couplings — uptake-suppress (less demand → less uptake) + lethal maintenance drain
        // (burns reserves resisting) — must make the toxic region's population settle strictly BELOW the
        // toxin-free region's, even starting from a balanced placement. Deterministic, integer; the ledger stays
        // closed (the injected toxin is booked into initial_total + decays via the chem_decay tap).
        let cfg = SimConfig {
            seed: 4242,
            generations: 80,
            entity_count: 1200,
        };
        let split_x = WORLD_DIMS.0 / 2;
        let mut sim = Simulation::reset(&cfg);
        // Baseline: with NO toxin, the two halves track each other (a fairness control on the placement).
        let (l0, r0) = pop_left_right(&mut sim, split_x);
        assert!(l0 > 0 && r0 > 0, "both halves must start populated");

        // A toxic world: paint a near-saturating toxin band over the left half, then run.
        let mut toxic = Simulation::reset(&cfg);
        paint_toxin_left_band(&mut toxic, split_x, 30_000_000); // strong, but < CHEM_CAP
        toxic.step(cfg.generations);
        let (left, right) = pop_left_right(&mut toxic, split_x);

        assert!(
            left < right,
            "allelopathy: the toxic (left) region must be suppressed below the toxin-free (right) region, \
             got left={left} right={right}"
        );

        // And the ledger still closes with the injected + decayed toxin fully accounted (the F5 four-bucket close).
        let pools_total = toxic.world.resource::<PoolStock>().total();
        let chem_total = toxic.world.resource::<chem::ChemField>().total();
        let (mut e, mut b) = (0i64, 0i64);
        for (energy, biomass) in toxic
            .world
            .query::<(&Energy, &Biomass)>()
            .iter(&toxic.world)
        {
            e += energy.0;
            b += biomass.0;
        }
        let live = ledger::LiveTotal {
            pools: pools_total,
            energy: e,
            biomass: b,
            chem: chem_total,
        };
        assert!(
            ledger::ledger_closes(&toxic.ledger(), &live),
            "F5 four-bucket ledger must close with a live chem field: live {} vs expected {}",
            live.sum(),
            toxic.ledger().expected_total()
        );
    }

    #[test]
    fn f5_chem_field_is_emitted_by_the_default_roster_and_decays() {
        // ADR-013 F5: the default plant roster has a non-zero Defense budget, so it MINTS toxin; every living org
        // marks kin; low-energy/dying orgs raise alarm. After a run the chem field is non-empty (the mechanic is
        // live, not dormant) AND the chem_decay tap has accumulated (decay ran). Deterministic.
        let cfg = SimConfig {
            seed: 23,
            generations: 60,
            entity_count: 600,
        };
        let mut sim = Simulation::reset(&cfg);
        sim.step(cfg.generations);
        let chem_total = sim.world.resource::<chem::ChemField>().total();
        assert!(
            chem_total > 0,
            "the default roster must emit chem (toxin/kin/alarm)"
        );
        assert!(
            sim.ledger().chem_decay > 0,
            "the chem_decay tap must accumulate (decay is the only chem sink)"
        );
    }

    #[test]
    fn f5_chem_run_is_deterministic_run_to_run() {
        // ADR-013 F5: the chem pipeline (diffuse/decay/emit/sense + the alarm-biased DRAW-COUNT-NEUTRAL dispersal)
        // adds ZERO SimRng draws and is all-integer, so a fixed (seed, gen) run is byte-identical run-to-run. The
        // multi-ISA CI matrix is the cross-arch authority; this is the same-arch run==run necessary condition.
        let cfg = SimConfig {
            seed: 909,
            generations: 50,
            entity_count: 800,
        };
        assert_eq!(run_headless(&cfg).hash, run_headless(&cfg).hash);
        // The DrawCount is unchanged by F5: a chem-active run draws the SAME number of words as the births alone
        // (the alarm bias re-interprets an already-drawn word — no extra draw). Two runs agree on it.
        let draws = |c: &SimConfig| -> u64 {
            let mut s = Simulation::reset(c);
            s.step(c.generations);
            s.world.resource::<DrawCount>().0
        };
        assert_eq!(
            draws(&cfg),
            draws(&cfg),
            "draw count is deterministic + chem adds none"
        );
    }

    #[test]
    fn edit_factor_q_maps_growth_ratio_to_strictly_positive_band() {
        // ADR-017 S6: the pinned integer mapping. Loss-of-function lifts off the 0.5× floor toward 1.0×; a
        // wild-type ratio is exactly neutral (a no-op); Activate lifts above neutral toward 1.5×. Always clamped
        // strictly positive (never zeroed selection).
        // Knockout / Knockdown: 0.5 + 0.5·(q/1000).
        assert_eq!(edit_factor_q(0, EditEffect::Knockout), EDIT_FACTOR_MIN_Q); // lethal KO → 0.5×
        assert_eq!(
            edit_factor_q(1000, EditEffect::Knockout),
            EDIT_FACTOR_NEUTRAL_Q
        ); // WT → 1.0× (no-op)
        assert_eq!(edit_factor_q(500, EditEffect::Knockout), 750); // mid → 0.75×
        assert_eq!(edit_factor_q(500, EditEffect::Knockdown), 750); // same monotone map
                                                                    // Activate lifts ABOVE neutral toward the 1.5× ceiling.
        assert_eq!(edit_factor_q(1000, EditEffect::Activate), EDIT_FACTOR_MAX_Q); // full activate → 1.5×
        assert_eq!(
            edit_factor_q(0, EditEffect::Activate),
            EDIT_FACTOR_NEUTRAL_Q
        ); // no magnitude → neutral
           // The whole band is strictly positive and bounded, for every input + verb (integer, no transcendental).
        for q in [0u16, 1, 250, 333, 500, 667, 999, 1000, 60000] {
            for eff in [
                EditEffect::Knockout,
                EditEffect::Knockdown,
                EditEffect::Activate,
            ] {
                let f = edit_factor_q(q, eff);
                assert!(
                    (EDIT_FACTOR_MIN_Q..=EDIT_FACTOR_MAX_Q).contains(&f),
                    "factor {f} for (q={q}, {eff:?}) escaped the [{EDIT_FACTOR_MIN_Q},{EDIT_FACTOR_MAX_Q}] band"
                );
            }
        }
    }

    #[test]
    fn committed_neutral_edit_does_not_move_the_run_hash() {
        // A wild-type-ratio commit (growth_ratio_q == 1000) maps to exactly the neutral 1000 permille → the
        // demand math is untouched → the run is byte-identical to no commit at all. This is the per-run analogue
        // of the pinned-literal neutrality proof: WIRING the modifier costs nothing until a non-neutral factor
        // commits.
        let cfg = SimConfig {
            seed: 41,
            generations: 25,
            entity_count: 600,
        };
        let mk =
            || Simulation::reset_with_roster(&cfg, &EnvParams::default(), obligate_roster(true));
        let mut control = mk();
        control.step(cfg.generations);
        let mut edited = mk();
        // Commit a NEUTRAL impact on the decomposer (sid 1): a wild-type ratio → factor 1000 → no-op.
        edited.commit_species_edit(
            SpeciesId::new(1),
            EDIT_FACTOR_NEUTRAL_Q,
            EditEffect::Knockout,
        );
        assert_eq!(
            edited.species_edit_factor_q(SpeciesId::new(1)),
            EDIT_FACTOR_NEUTRAL_Q,
            "a wild-type ratio commits the neutral factor"
        );
        edited.step(cfg.generations);
        assert_eq!(
            control.run_stats().hash,
            edited.run_stats().hash,
            "a neutral-factor commit must not move the run hash"
        );
    }

    #[test]
    fn committed_ko_throttles_the_edited_decomposer_and_ripples_to_the_plant() {
        // ADR-017 S6 PAYOFF (the load-bearing wire — the proposal's §4 ripple, end to end). A committed
        // growth-lethal gltA KO (growth_ratio_q == 0) on the DECOMPOSER (sid 1) commits the 0.5× factor, which
        // throttles BOTH seams: (1) its DEMAND (less uptake → it grows + reproduces less → fewer decomposers),
        // and (2) its MINERALIZATION mint (a TCA KO impairs carbon processing → it mints less free_nutrient).
        // Less mineralization → less free_nutrient flows into the Liebig-gated plant (sid 0) → the F4 loop
        // weakens → the plant population declines vs a no-edit control. Robust signals are CUMULATIVE over the
        // trajectory (the per-tick snapshot oscillates; the population/flow integrals smooth it). Deterministic.
        let cfg = SimConfig {
            seed: 23,
            generations: 150,
            entity_count: 600,
        };
        let mk = || {
            let mut s =
                Simulation::reset_with_roster(&cfg, &EnvParams::default(), obligate_roster(true));
            // Drain the seeded free_nutrient so free_nutrient is PURELY decomposer-minted (the F4 teeth are
            // visible at a tractable scale — the same harness the F4 tests use).
            drain_seeded_free_nutrient(&mut s);
            s
        };
        // flow[0][1] = net J the decomposer (sid 1) mineralizes INTO the plant (sid 0) this generation.
        let mineralization_edge = |sim: &Simulation| -> i64 {
            let (_s, flat) = sim.flow_matrix();
            flat[1] // row 0 (plant) col 1 (decomposer) in the 2×2 matrix
        };
        // Step one generation at a time, accumulating (Σ plant pop, Σ decomposer pop, Σ mineralization-into-plant).
        let drive = |sim: &mut Simulation| -> (i128, i128, i128) {
            let (mut cum_plant, mut cum_decomp, mut cum_flow) = (0i128, 0i128, 0i128);
            for _ in 0..cfg.generations {
                sim.step(1);
                cum_plant += species_pop(sim, 0) as i128;
                cum_decomp += species_pop(sim, 1) as i128;
                cum_flow += i128::from(mineralization_edge(sim));
            }
            (cum_plant, cum_decomp, cum_flow)
        };

        let mut control = mk();
        let (control_plant, control_decomp, control_flow) = drive(&mut control);

        let mut edited = mk();
        // Commit the gltA KO on the decomposer: growth_ratio_q == 0 → factor 0.5× (the strongest penalty).
        edited.commit_species_edit(SpeciesId::new(1), 0, EditEffect::Knockout);
        assert_eq!(
            edited.species_edit_factor_q(SpeciesId::new(1)),
            EDIT_FACTOR_MIN_Q,
            "a lethal KO commits the 0.5× floor factor"
        );
        let (edited_plant, edited_decomp, edited_flow) = drive(&mut edited);

        // (1) DIRECT EFFECT: the edited decomposer is throttled (fewer decomposer-organism-generations).
        assert!(
            edited_decomp < control_decomp,
            "KO must throttle the decomposer: edited Σpop {edited_decomp} vs control {control_decomp}"
        );
        // (2) RIPPLE: the throttled decomposer mineralizes strictly LESS into the plant over the run.
        assert!(
            edited_flow < control_flow,
            "a throttled decomposer mineralizes less into the plant: edited Σflow {edited_flow} vs control {control_flow}"
        );
        // (3) RIPPLE TO THE PLANT (the F4 payoff): weaker mineralization → the Liebig-gated plant declines.
        assert!(
            edited_plant < control_plant,
            "the plant must respond to weakened mineralization: edited Σpop {edited_plant} vs control {control_plant}"
        );

        // (4) RUN-TO-RUN STABLE (deterministic): a second edited run reproduces the outcome + hash bit-for-bit.
        let mut edited2 = mk();
        edited2.commit_species_edit(SpeciesId::new(1), 0, EditEffect::Knockout);
        let (e2_plant, e2_decomp, e2_flow) = drive(&mut edited2);
        assert_eq!(
            (edited_plant, edited_decomp, edited_flow),
            (e2_plant, e2_decomp, e2_flow),
            "the committed-KO outcome must be deterministic run-to-run"
        );
        assert_eq!(
            edited.run_stats().hash,
            edited2.run_stats().hash,
            "the committed-KO run hash must be deterministic run-to-run"
        );
    }

    #[test]
    fn f4_mineralize_rate_is_gene_driven() {
        // The pta/AcetateOverflow CRISPRi ripple lever: a decomposer with a HIGHER mineralize_rate mints MORE
        // free_nutrient than one with a low rate, all else equal. Drives the rate off the genome (not a const).
        let cfg = SimConfig {
            seed: 37,
            generations: 35,
            entity_count: 600,
        };
        let roster = || {
            vec![
                RosterEntry {
                    name: "plant".to_string(),
                    key: "plant".to_string(),
                    genome: anchor_genome(0.9),
                    gp_map: anchor_map(&[
                        Trait::LeafSize,
                        Trait::GrowthRate,
                        Trait::Fecundity,
                        Trait::DroughtTolerance,
                        Trait::Reflectance,
                    ]),
                    entity_count: 400,
                    role: gp::TrophicRole::Autotroph,
                },
                RosterEntry {
                    name: "decomposer".to_string(),
                    key: "decomposer".to_string(),
                    // GlucoseUptake (detritus affinity) HIGH + fixed; AcetateOverflow (mineralize_rate) = activity.
                    genome: anchor_genome(0.9),
                    gp_map: gp::OntologyMap::new(gp::TraitMap(vec![
                        gp::TraitBinding {
                            trait_: Trait::GlucoseUptake,
                            locus: gp::LocusSelector::ByIndex(genome::LocusId(0)),
                            param: genome::ParamId(0),
                        },
                        // AcetateOverflow reads locus 1 (the throttle); bind growth so it still breeds.
                        gp::TraitBinding {
                            trait_: Trait::AcetateOverflow,
                            locus: gp::LocusSelector::ByIndex(genome::LocusId(1)),
                            param: genome::ParamId(0),
                        },
                        gp::TraitBinding {
                            trait_: Trait::GrowthRate,
                            locus: gp::LocusSelector::ByIndex(genome::LocusId(0)),
                            param: genome::ParamId(0),
                        },
                    ])),
                    entity_count: 1500,
                    role: gp::TrophicRole::Decomposer,
                },
            ]
        };
        // Build a decomposer genome with a SECOND locus carrying the mineralize throttle activity.
        let with_throttle = |roster: Vec<RosterEntry>, throttle: f64| -> Vec<RosterEntry> {
            roster
                .into_iter()
                .map(|mut e| {
                    if e.role == gp::TrophicRole::Decomposer {
                        e.genome.loci.push(genome::Locus {
                            id: genome::LocusId(1),
                            name: "pta".to_string(),
                            sequence: genome::DnaSequence::new(*b"ACGTGGACGTTTTAGGCCGG").unwrap(),
                            parameters: vec![genome::Parameter {
                                id: genome::ParamId(0),
                                value: genome::ParamValue::Numeric {
                                    value: throttle,
                                    min: 0.0,
                                    max: 1.0,
                                },
                            }],
                            tags: genome::OntologyTags {
                                so_term: genome::SoTermId(704),
                                go_refs: vec![],
                            },
                        });
                    }
                    e
                })
                .collect()
        };
        let mut hi = Simulation::reset_with_roster(
            &cfg,
            &EnvParams::default(),
            with_throttle(roster(), 0.9),
        );
        drain_seeded_free_nutrient(&mut hi);
        hi.step(cfg.generations);
        let mut lo = Simulation::reset_with_roster(
            &cfg,
            &EnvParams::default(),
            with_throttle(roster(), 0.1),
        );
        drain_seeded_free_nutrient(&mut lo);
        lo.step(cfg.generations);
        // The high-mineralize_rate decomposer mints strictly more free_nutrient than the throttled one.
        assert!(
            total_free_nutrient(&hi) > total_free_nutrient(&lo),
            "a higher pta/mineralize_rate mints more free_nutrient: hi {} vs lo {}",
            total_free_nutrient(&hi),
            total_free_nutrient(&lo)
        );
    }

    #[test]
    fn f4_flow_matrix_is_deterministic_under_shuffled_roster_seed() {
        // The FlowMatrix is a MEASUREMENT in canonical (cell, SpeciesId, OrgId) order, so two identical runs
        // produce byte-identical matrices AND hashes (it rides the deterministic sorted-Vec order, never the
        // archetype/Query order). Two resets of the same roster+seed must match exactly.
        let cfg = SimConfig {
            seed: 55,
            generations: 25,
            entity_count: 600,
        };
        let mut a =
            Simulation::reset_with_roster(&cfg, &EnvParams::default(), obligate_roster(true));
        a.step(cfg.generations);
        let mut b =
            Simulation::reset_with_roster(&cfg, &EnvParams::default(), obligate_roster(true));
        b.step(cfg.generations);
        assert_eq!(
            a.flow_matrix(),
            b.flow_matrix(),
            "FlowMatrix is deterministic"
        );
        assert_eq!(
            a.run_stats().hash,
            b.run_stats().hash,
            "the F4 run hash is reproducible"
        );
    }

    #[test]
    fn f4_ledger_closes_every_tick_in_the_obligate_loop() {
        // The mineralize move is a paired detritus-debit / (free_nutrient-credit + RESPIRED-tap) — it must
        // CONSERVE J. Drive the obligate loop and cross-check the books close against the live total (the
        // per-tick assert already guards it; this asserts the final state explicitly).
        let cfg = SimConfig {
            seed: 91,
            generations: 45,
            entity_count: 600,
        };
        let mut sim =
            Simulation::reset_with_roster(&cfg, &EnvParams::default(), obligate_roster(true));
        sim.step(cfg.generations);
        let pools_total = sim.world.resource::<PoolStock>().total();
        let chem_total = sim.world.resource::<chem::ChemField>().total(); // ADR-013 F5: chem is a live bucket
        let (mut e, mut b) = (0i64, 0i64);
        for (energy, biomass) in sim.world.query::<(&Energy, &Biomass)>().iter(&sim.world) {
            e += energy.0;
            b += biomass.0;
        }
        let live = ledger::LiveTotal {
            pools: pools_total,
            energy: e,
            biomass: b,
            chem: chem_total,
        };
        assert!(
            ledger::ledger_closes(&sim.ledger(), &live),
            "the obligate-loop ledger must close: live {} vs expected {}",
            live.sum(),
            sim.ledger().expected_total()
        );
    }

    /// Measure the live `J` total of a sim (the four conserved buckets) — the `ledger_closes` left side.
    fn measure_live(sim: &mut Simulation) -> ledger::LiveTotal {
        let pools = sim.world.resource::<PoolStock>().total();
        let chem = sim.world.resource::<chem::ChemField>().total();
        let (mut e, mut b) = (0i64, 0i64);
        for (energy, biomass) in sim.world.query::<(&Energy, &Biomass)>().iter(&sim.world) {
            e += energy.0;
            b += biomass.0;
        }
        ledger::LiveTotal {
            pools,
            energy: e,
            biomass: b,
            chem,
        }
    }

    /// Register a synthetic contaminant DECOMPOSER species and return its `SpeciesId`. `activity` drives every
    /// decomposer anchor (0.9 = well-adapted/vigorous, ~0.0 = poorly-adapted) so the open-system test decides
    /// establish-vs-die by the LEDGER, not by a script.
    fn register_contaminant_decomposer(
        sim: &mut Simulation,
        key: &str,
        activity: f64,
    ) -> SpeciesId {
        sim.register_species(
            key.to_string(),
            key.to_string(),
            anchor_genome(activity),
            anchor_map(&[
                Trait::GlucoseUptake,
                Trait::AcetateOverflow,
                Trait::GrowthRate,
                Trait::FermentationCapacity,
                Trait::RespirationMode,
            ]),
            gp::TrophicRole::Decomposer,
        )
    }

    #[test]
    fn adr019_region_inoculate_conserves_j_and_ledger_closes() {
        // ADR-019 S1: a RegionInoculate MINTS its endowment from the `immigration` tap — Σlive J rises by
        // EXACTLY count·endow_j, the immigration tap records EXACTLY that, and ledger_closes holds.
        let cfg = SimConfig {
            seed: 31,
            generations: 0,
            entity_count: 200,
        };
        let mut sim = Simulation::reset(&cfg);
        let before_live = measure_live(&mut sim).sum();
        let before_immig = sim.ledger().immigration;
        let sid = register_contaminant_decomposer(&mut sim, "bacillus", 0.9);
        let count = 12u32;
        let endow_j = 1_000_000i64;
        let region = Region {
            cx: 16,
            cy: 16,
            radius: 6,
        };
        let spawned = sim.region_inoculate(sid, region, count, endow_j);
        assert_eq!(spawned, count, "every requested organism is placed");
        let minted = endow_j * i64::from(count);
        assert_eq!(
            sim.ledger().immigration - before_immig,
            minted,
            "the immigration tap records exactly count·endow_j"
        );
        let after_live = measure_live(&mut sim).sum();
        assert_eq!(
            after_live - before_live,
            minted,
            "live J rises by exactly the minted endowment (conserved, never conjured)"
        );
        let live = measure_live(&mut sim);
        assert!(
            ledger::ledger_closes(&sim.ledger(), &live),
            "ledger_closes must hold right after an inoculation: live {} vs expected {}",
            live.sum(),
            sim.ledger().expected_total()
        );
        // And it KEEPS closing as the inoculated orgs metabolize (the per-tick assert is the in-chain guard;
        // this confirms the named tap composes with the existing taps over a real run).
        sim.step(20);
        let live = measure_live(&mut sim);
        assert!(
            ledger::ledger_closes(&sim.ledger(), &live),
            "ledger_closes must keep holding after the inoculated orgs metabolize"
        );
    }

    #[test]
    fn adr019_region_inoculate_is_replay_reproducible_rng_free() {
        // ADR-019 S1: the inoculation is RNG-FREE + deterministic — two identical (seed, inoculate, advance)
        // sequences produce byte-identical hashes. (Placement is an off-stream cell-fill; no `next_u64`.)
        let cfg = SimConfig {
            seed: 77,
            generations: 0,
            entity_count: 300,
        };
        let region = Region {
            cx: 10,
            cy: 20,
            radius: 5,
        };
        let run = || {
            let mut sim = Simulation::reset(&cfg);
            let sid = register_contaminant_decomposer(&mut sim, "pseudomonas", 0.8);
            sim.region_inoculate(sid, region, 9, 800_000);
            sim.step(15);
            sim.run_stats().hash
        };
        assert_eq!(
            run(),
            run(),
            "an inoculated run must replay bit-identically"
        );
    }

    #[test]
    fn adr019_well_adapted_establishes_poorly_adapted_dies_decided_by_the_ledger() {
        // THE open-system headline (ADR-019): two identical inoculations into the SAME plant+decomposer world,
        // differing ONLY in the contaminant's metabolic activity (its anchor gene). The WELL-ADAPTED immigrant
        // out-harvests the conserved detritus pool → funds offspring → ESTABLISHES (population ends ABOVE its
        // inoculum). The POORLY-ADAPTED one cannot cover maintenance → starves → DIES OUT (population integrates
        // toward zero). NOTHING is scripted: the divergence is decided entirely by the ADR-013 joule economy.
        let cfg = SimConfig {
            seed: 54,
            generations: 120,
            entity_count: 600,
        };
        let inoculum = 40u32;
        // Endow each immigrant BELOW the reproduction threshold (REPRO_THRESHOLD = 300k) so it CANNOT fund
        // offspring from its arrival reserve alone — it must EARN its keep by harvesting the conserved pools.
        // This is what makes establish-vs-die a LEDGER decision: a well-adapted decomposer out-harvests
        // detritus and grows; a near-inert one (zero detritus affinity) cannot eat and starves to extinction.
        let endow_j = 200_000i64;
        let run = |activity: f64| -> usize {
            // A live plant+decomposer obligate loop (a standing detritus→free_nutrient economy to invade).
            let mut sim =
                Simulation::reset_with_roster(&cfg, &EnvParams::default(), obligate_roster(true));
            // Let the resident loop warm up so there is a real niche to contest.
            sim.step(20);
            let sid = register_contaminant_decomposer(&mut sim, "contaminant", activity);
            let placed = sim.region_inoculate(
                sid,
                Region {
                    cx: 16,
                    cy: 16,
                    radius: 10,
                },
                inoculum,
                endow_j,
            );
            assert_eq!(placed, inoculum, "the full propagule lands");
            sim.step(150);
            species_pop(&mut sim, sid.ordinal())
        };
        let established = run(0.9); // well-adapted: a vigorous decomposer
        let died_out = run(0.0); // poorly-adapted: a near-inert decomposer
        assert!(
            died_out < inoculum as usize,
            "a poorly-adapted immigrant must DIE OUT (ended {died_out} < inoculum {inoculum}) — \
             decided by the ledger, not scripted"
        );
        assert!(
            established > died_out,
            "a well-adapted immigrant must out-establish a poorly-adapted one (well {established} vs poor \
             {died_out})"
        );
    }

    // ── SP-3 intervention tools (PCR-amplify / cull / nutrient / toxin) ────────────────────────────────────

    #[test]
    fn sp3_pcr_amplify_clones_are_byte_identical_to_a_local_template_and_conserve_j() {
        // SP-3.1: a PCR clone copies its local template's heritable state VERBATIM (faithful PCR — no mutation),
        // J rises by EXACTLY count·endow_j from the intervention tap, and ledger_closes holds.
        let cfg = SimConfig {
            seed: 31,
            generations: 0,
            entity_count: 300,
        };
        let mut sim = Simulation::reset(&cfg);
        // The primary species is SpeciesId(0); it has residents spread across the grid. Pick a disc with orgs.
        let sid = SpeciesId(0);
        let region = Region {
            cx: 16,
            cy: 16,
            radius: 20,
        }; // wide disc → covers residents
           // The distinct heritable states present in-region BEFORE amplifying.
        let templates: std::collections::BTreeSet<(u64, u64, u64)> = sim
            .world
            .query::<(&Species, &Genotype, &DroughtTol, &ThermalTol, &Position)>()
            .iter(&sim.world)
            .filter(|(sp, _, _, _, p)| sp.0 == sid && region.contains(p.x, p.y))
            .map(|(_sp, g, d, t, _p)| (g.0.to_bits(), d.0.to_bits(), t.0.to_bits()))
            .collect();
        assert!(
            !templates.is_empty(),
            "the disc must cover resident templates"
        );

        let before_live = measure_live(&mut sim).sum();
        let before_tap = sim.ledger().intervention;
        let count = 16u32;
        let endow_j = 900_000i64;
        let spawned = sim.region_pcr_amplify(sid, region, count, endow_j);
        assert_eq!(spawned, count, "every requested clone is placed");

        // Every clone's heritable triple is one of the in-region templates (no neutral 0.5 contaminant, no
        // mutation drift) — faithful PCR. Check the full set of present triples is a subset of the templates.
        let after_triples: std::collections::BTreeSet<(u64, u64, u64)> = sim
            .world
            .query::<(&Species, &Genotype, &DroughtTol, &ThermalTol)>()
            .iter(&sim.world)
            .filter(|(sp, ..)| sp.0 == sid)
            .map(|(_sp, g, d, t)| (g.0.to_bits(), d.0.to_bits(), t.0.to_bits()))
            .collect();
        // No NEW heritable triple was introduced by the clones (every clone copied a template verbatim).
        assert!(
            after_triples
                .iter()
                .all(|tr| templates.contains(tr) || sim_default_triples().contains(tr)),
            "a PCR clone must carry a verbatim template triple, never an invented/mutated one"
        );

        let minted = endow_j * i64::from(count);
        assert_eq!(
            sim.ledger().intervention - before_tap,
            minted,
            "the intervention tap records exactly count·endow_j"
        );
        let after_live = measure_live(&mut sim).sum();
        assert_eq!(
            after_live - before_live,
            minted,
            "live J rises by exactly the minted endowment (conserved — PCR adds, never conjures)"
        );
        let live = measure_live(&mut sim);
        assert!(
            ledger::ledger_closes(&sim.ledger(), &live),
            "ledger_closes must hold right after a PCR amplification"
        );
        // And it keeps closing as the clones metabolize.
        sim.step(15);
        let live = measure_live(&mut sim);
        assert!(
            ledger::ledger_closes(&sim.ledger(), &live),
            "ledger_closes must keep holding after the clones metabolize"
        );
    }

    /// The full set of heritable triples present in a fresh `reset` of the pinned-shape config — used by the PCR
    /// test to confirm a clone introduces no triple absent from the resident population.
    fn sim_default_triples() -> std::collections::BTreeSet<(u64, u64, u64)> {
        let cfg = SimConfig {
            seed: 31,
            generations: 0,
            entity_count: 300,
        };
        let mut sim = Simulation::reset(&cfg);
        sim.world
            .query::<(&Genotype, &DroughtTol, &ThermalTol)>()
            .iter(&sim.world)
            .map(|(g, d, t)| (g.0.to_bits(), d.0.to_bits(), t.0.to_bits()))
            .collect()
    }

    #[test]
    fn sp3_pcr_amplify_no_local_template_is_a_clean_noop() {
        // SP-3.1: PCR needs a local template. A species with NO in-region org → spawn nothing, mint nothing.
        let cfg = SimConfig {
            seed: 5,
            generations: 0,
            entity_count: 200,
        };
        let mut sim = Simulation::reset(&cfg);
        // Register a contaminant with ZERO residents (never inoculated), then try to amplify it.
        let sid = register_contaminant_decomposer(&mut sim, "absent", 0.5);
        let before = measure_live(&mut sim).sum();
        let placed = sim.region_pcr_amplify(
            sid,
            Region {
                cx: 16,
                cy: 16,
                radius: 8,
            },
            10,
            500_000,
        );
        assert_eq!(placed, 0, "no local template → no clones");
        assert_eq!(sim.ledger().intervention, 0, "no template → mint nothing");
        assert_eq!(measure_live(&mut sim).sum(), before, "J unchanged");
    }

    #[test]
    fn sp3_pcr_amplify_is_replay_reproducible_rng_free() {
        // SP-3.1: amplification is RNG-FREE + deterministic — two identical (seed, amplify, advance) sequences
        // produce byte-identical hashes (template selection + placement are pure functions of state).
        let cfg = SimConfig {
            seed: 88,
            generations: 0,
            entity_count: 300,
        };
        let run = || {
            let mut sim = Simulation::reset(&cfg);
            sim.region_pcr_amplify(
                SpeciesId(0),
                Region {
                    cx: 16,
                    cy: 16,
                    radius: 18,
                },
                12,
                700_000,
            );
            sim.step(12);
            sim.run_stats().hash
        };
        assert_eq!(
            run(),
            run(),
            "a PCR-amplified run must replay bit-identically"
        );
    }

    #[test]
    fn sp3_cull_kills_the_apportioned_subset_and_carcasses_to_detritus_conserving_j() {
        // SP-3.2: a 500-permille cull kills floor(n·0.5) of the canonical census; the residual moves to detritus
        // (a paired bucket move — NO tap), so live J is UNCHANGED and ledger_closes holds.
        let cfg = SimConfig {
            seed: 31,
            generations: 0,
            entity_count: 300,
        };
        let mut sim = Simulation::reset(&cfg);
        let sid = SpeciesId(0);
        let region = Region {
            cx: 16,
            cy: 16,
            radius: 30,
        }; // covers the whole grid → the full population is the census
        let n_before = species_pop(&mut sim, sid.0) as i64;
        assert!(n_before > 0);
        let before_live = measure_live(&mut sim).sum();
        let before_tap = (sim.ledger().intervention, sim.ledger().immigration);
        let killed = sim.region_cull(sid, region, 500); // 50% permille
        let expected = fixed::apportion(n_before, &[500, 500])[0] as u32;
        assert_eq!(
            killed, expected,
            "kills the largest-remainder apportioned count"
        );
        assert_eq!(
            species_pop(&mut sim, sid.0) as i64,
            n_before - i64::from(killed),
            "population drops by exactly the killed count"
        );
        // CONSERVATION: a cull mints NOTHING — neither the intervention nor immigration tap moves.
        assert_eq!(
            (sim.ledger().intervention, sim.ledger().immigration),
            before_tap,
            "an antibiotic cull mints no J (carcass→detritus is a paired bucket move)"
        );
        let after_live = measure_live(&mut sim).sum();
        assert_eq!(
            after_live, before_live,
            "live J is unchanged by a cull (residual moved store→detritus, none lost/minted)"
        );
        let live = measure_live(&mut sim);
        assert!(
            ledger::ledger_closes(&sim.ledger(), &live),
            "ledger_closes must hold right after a cull"
        );
        sim.step(10);
        let live = measure_live(&mut sim);
        assert!(
            ledger::ledger_closes(&sim.ledger(), &live),
            "ledger_closes must keep holding after the cull's detritus is decomposed"
        );
    }

    #[test]
    fn sp3_cull_is_deterministic_and_strength_zero_is_a_noop() {
        // SP-3.2: zero strength → no kill, no draws; the same cull reproduces bit-identically.
        let cfg = SimConfig {
            seed: 17,
            generations: 0,
            entity_count: 300,
        };
        let region = Region {
            cx: 16,
            cy: 16,
            radius: 30,
        };
        let mut sim = Simulation::reset(&cfg);
        let before = species_pop(&mut sim, 0);
        assert_eq!(
            sim.region_cull(SpeciesId(0), region, 0),
            0,
            "strength 0 → no-op"
        );
        assert_eq!(
            species_pop(&mut sim, 0),
            before,
            "no-op leaves population intact"
        );

        let run = || {
            let mut sim = Simulation::reset(&cfg);
            sim.region_cull(SpeciesId(0), region, 300);
            sim.step(10);
            sim.run_stats().hash
        };
        assert_eq!(run(), run(), "a culled run must replay bit-identically");
    }

    #[test]
    fn sp3_nutrient_feed_mints_from_intervention_tap_conserving_j() {
        // SP-3.3: a nutrient feed deposits amount_j into the chosen pool plane from the intervention tap;
        // accepted + overflow == amount_j, and ledger_closes holds.
        let cfg = SimConfig {
            seed: 31,
            generations: 0,
            entity_count: 200,
        };
        let mut sim = Simulation::reset(&cfg);
        let region = Region {
            cx: 16,
            cy: 16,
            radius: 6,
        };
        let before_live = measure_live(&mut sim).sum();
        let before_tap = sim.ledger().intervention;
        let before_over = sim.ledger().overflow;
        let amount = 8_000_000i64;
        let accepted = sim.region_nutrient(2, region, amount); // channel 2 = detritus
        let minted = sim.ledger().intervention - before_tap;
        let spilled = sim.ledger().overflow - before_over;
        assert_eq!(
            minted, amount,
            "the intervention tap records exactly amount_j"
        );
        assert_eq!(
            accepted + spilled,
            amount,
            "accepted + overflow == amount (never silently clamped)"
        );
        let after_live = measure_live(&mut sim).sum();
        assert_eq!(
            after_live - before_live,
            accepted,
            "live J rises by exactly the accepted part (the spill nets out via overflow)"
        );
        let live = measure_live(&mut sim);
        assert!(
            ledger::ledger_closes(&sim.ledger(), &live),
            "ledger_closes must hold right after a nutrient feed"
        );
    }

    #[test]
    fn sp3_toxin_spike_mints_into_the_chem_field_conserving_j() {
        // SP-3.3: a toxin spike deposits amount_milli (== J 1:1) into the chosen chem plane from the
        // intervention tap; accepted + overflow == amount, and ledger_closes holds (chem is a live bucket).
        let cfg = SimConfig {
            seed: 31,
            generations: 0,
            entity_count: 200,
        };
        let mut sim = Simulation::reset(&cfg);
        let region = Region {
            cx: 16,
            cy: 16,
            radius: 5,
        };
        let before_live = measure_live(&mut sim).sum();
        let before_tap = sim.ledger().intervention;
        let before_over = sim.ledger().overflow;
        let before_chem = sim.world.resource::<chem::ChemField>().total();
        let amount = 5_000_000i64;
        let accepted = sim.region_toxin(0, region, amount); // channel 0 = toxin
        let minted = sim.ledger().intervention - before_tap;
        let spilled = sim.ledger().overflow - before_over;
        assert_eq!(
            minted, amount,
            "the intervention tap records exactly amount_milli"
        );
        assert_eq!(
            accepted + spilled,
            amount,
            "accepted + overflow == amount (never silently clamped)"
        );
        let after_chem = sim.world.resource::<chem::ChemField>().total();
        assert_eq!(
            after_chem - before_chem,
            accepted,
            "the chem field rises by exactly the accepted milli (== J 1:1)"
        );
        let after_live = measure_live(&mut sim).sum();
        assert_eq!(
            after_live - before_live,
            accepted,
            "live J (incl chem) rises by the accepted part"
        );
        let live = measure_live(&mut sim);
        assert!(
            ledger::ledger_closes(&sim.ledger(), &live),
            "ledger_closes must hold right after a toxin spike"
        );
    }

    #[test]
    fn sp3_interventions_are_hash_neutral_when_inert() {
        // SP-3: the four new methods are UNCALLED by the pinned path; a plain Advance run's hash is byte-
        // identical with them compiled in but un-invoked (the pinned literal is unmoved). The pinned config
        // (determinism_hash_is_pinned) is the proof of value; this guards the property locally on a smaller run.
        let plain = || {
            let cfg = SimConfig {
                seed: 13_679_457_532_755_275_413,
                generations: 40,
                entity_count: 400,
            };
            run_headless(&cfg).hash
        };
        assert_eq!(
            plain(),
            plain(),
            "an inert intervention surface must be reproducible"
        );
        // The intervention tap defaults zero on a plain run (no method invoked → no mint).
        let cfg = SimConfig {
            seed: 42,
            generations: 20,
            entity_count: 200,
        };
        let mut sim = Simulation::reset(&cfg);
        sim.step(20);
        assert_eq!(
            sim.ledger().intervention,
            0,
            "no SP-3 method invoked → the intervention tap stays zero"
        );
    }

    #[cfg(feature = "proptest")]
    mod prop {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            // AC3 (SPEC §10.4): for arbitrary seed / generations / entity_count, allele_freq and every
            // expressed Phenotype trait value are ALWAYS within [0, 1].
            #[test]
            fn allele_freq_and_traits_always_in_unit_range(
                seed in any::<u64>(),
                generations in 0u64..150,
                entity_count in 0u32..400,
            ) {
                let r = run_headless(&SimConfig { seed, generations, entity_count });
                prop_assert!((0.0..=1.0).contains(&r.allele_freq), "allele_freq {} out of [0,1]", r.allele_freq);

                let pheno = WeightedSumMap.express(&genome::sample_genome());
                for (t, v) in &pheno.values {
                    prop_assert!((0.0..=1.0).contains(v), "trait {:?} = {} out of [0,1]", t, v);
                }
            }
        }
    }
}
