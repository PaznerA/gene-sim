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

pub mod climate;
pub mod det;
pub mod fixed;
pub mod gp;
pub mod ledger;
pub mod resource;
pub mod snapshot;
pub mod soil;
pub mod trophic;

pub use climate::EnvParams;
pub use det::derive_seed;
pub use gp::{GenotypePhenotypeMap, Phenotype, Trait, WeightedSumMap};
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

/// The STATIC per-cell resource field (ADR-013 F1→F3): light / free_nutrient / detritus (`f32` `[0,1]`),
/// generated off the `SimRng` stream. At F3 it is the render/cap/seed SOURCE — [`PoolStock`] is seeded from it
/// at reset and [`solar_influx`] reads its per-cell carrying caps each tick — while the mutable joule pools
/// live in `PoolStock`. Read by the F3 pipeline (no longer hash-neutral via its coupling to the live pools).
#[derive(Resource)]
struct ResourceFieldRes(resource::ResourceField);

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

/// Joules per unit of the quantized static `ResourceField` `[0,1]→u16` seed (ADR-013 F3, finding #9). A cell's
/// initial pool `J` = `to_unit_u16(field_value) as i64 * CELL_J_SCALE`. The single audited f64→int chokepoint
/// is `fixed::to_unit_u16`; this scale lifts that `[0, 65535]` grid into the joule economy.
const CELL_J_SCALE: i64 = 1_000;

/// Per-cell hard ceiling on any single `PoolStock` channel (`light`/`free_nutrient`/`detritus`). Influx /
/// excretion past this routes the spill to [`ledger::Ledger::overflow`] (never silently clamped). Sized above
/// the max seed (`UNIT_SCALE * CELL_J_SCALE = 65_535_000`) with headroom for accumulation.
pub(crate) const POOL_CAP: i64 = 200_000_000;

/// Per-cell solar `J` minted into `PoolStock.light` each tick by [`solar_influx`] — but only up to the cell's
/// static `ResourceField.light` carrying-cap (so a bright cell refills, a dark cell stays poor). The ONLY
/// source of new `J` (the INFLUX tap). free_nutrient regen toward its target is also booked as INFLUX at F3 (a
/// documented open tap, closed endogenously by the F4 plant→detritus→decomposer→free_nutrient loop).
const SOLAR_PER_CELL: i64 = 40_000;

/// Uptake saturation: the Monod-like `uptake = (Vmax·S)/(K_half + S)` taps a channel hard at high stock and
/// gently at low stock. `VMAX` is the per-org-per-tick ceiling at infinite stock (scaled by demand); `K_HALF`
/// is the stock at which uptake is half-max. Pure integer (`u128` intermediate, floored to `i64`).
const UPTAKE_VMAX: i64 = 60_000;
/// Half-saturation stock for the Monod uptake curve (see [`UPTAKE_VMAX`]).
const UPTAKE_K_HALF: i64 = 20_000_000;

/// Per-org-per-tick MAINTENANCE upkeep debit funded by the `budget[Maintenance]` slice, subtracted from Energy
/// → RESPIRED (the only per-org sink that makes starvation possible). A flat base scaled by the maintenance
/// permille share so a maintenance-heavy strategy pays more upkeep (a real trade-off).
const MAINTENANCE_BASE: i64 = 12_000;

/// Starvation floor: after the maintenance debit, an org whose Energy is BELOW this dies (ADR-013 F3). Its
/// residual Energy+Biomass deposits to the cell detritus pool (carcass→detritus).
const MAINTENANCE_FLOOR: i64 = 1;

/// Senescence ceiling: an org at this [`Age`] dies of old age (HARD at F3; soft coupling deferred).
const AGE_MAX: u32 = 240;

/// Reproduction threshold: an org whose Energy is ≥ this AFTER maintenance may spend an [`OFFSPRING_ENDOWMENT`]
/// to produce one child this tick (ADR-013 F3). Set above the endowment so a birth never drives the parent
/// negative.
const REPRO_THRESHOLD: i64 = 600_000;

/// The conserved `J` a parent SPENDS per birth: `parent.Energy -= OFFSPRING_ENDOWMENT`, and the child receives
/// `Biomass = OFFSPRING_SEED_BIOMASS`, `Energy = OFFSPRING_ENDOWMENT − OFFSPRING_SEED_BIOMASS` — no minting, a
/// pure transfer out of the parent's reserve.
const OFFSPRING_ENDOWMENT: i64 = 400_000;

/// The child's initial structural [`Biomass`], carved OUT of the [`OFFSPRING_ENDOWMENT`] (the rest seeds the
/// child's Energy reserve). Conserved.
pub(crate) const OFFSPRING_SEED_BIOMASS: i64 = 100_000;

/// Per-org Energy cap (ADR-013 F3). Convert/uptake past this routes to [`ledger::Ledger::overflow`].
const ENERGY_CAP: i64 = 4_000_000;

/// Per-org Biomass cap (ADR-013 F3). Growth past this routes to [`ledger::Ledger::overflow`].
pub(crate) const BIOMASS_CAP: i64 = 4_000_000;

/// Trophic-efficiency NUMERATOR/DENOMINATOR (`EFF_NUM/EFF_DEN < 1`): the fraction of CONVERTED uptake that is
/// KEPT; the residual `granted − Σ(kept)` is RESPIRED (computed as a residual, never an independent divide that
/// double-floors a quantum — adversarial finding #7).
const EFF_NUM: i64 = 7;
/// Trophic-efficiency denominator (see [`EFF_NUM`]).
const EFF_DEN: i64 = 10;

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
/// Autotroph's LIGHT uptake is UN-throttled (gate = 1000 permille). Below it, light demand scales DOWN linearly
/// (nutrients limit photosynthesis), so when `free_nutrient` drains toward 0 — the fate of every cell once the
/// decomposer is gone — the plant's light uptake collapses and it starves. The obligate-loop teeth. Set high
/// (above the per-cell seed) so the gate is ALWAYS proportional to local nutrient → decomposer mineralization
/// CONTINUOUSLY raises nearby plant productivity (a legible, measurable coupling rather than a cliff).
const NUTRIENT_LIMIT_REF: i64 = 40_000;

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
    mut pools: ResMut<PoolStock>,
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
    use climate::ClimateModifier as _;
    use soil::EnvironmentModifier as _;
    let soil_mod = soil::LinearTraitMatchModifier;
    let clim_mod = climate::TemperatureMatchModifier;
    let clim_sample = climate_field.0.sample(); // GLOBAL climate coupling (ADR-012 E3).
    let width = pools.width;
    // ── Canonical order: (cell_index, SpeciesId, OrgId). Built ONCE over the LIVING set (inv #3). ──
    // BLOCKER #1 fix: the ADR-011/012 soil+climate match factor is re-expressed as an INTEGER permille that
    // scales DEMAND (pre-apportion) — NOT an f64 multiply on the granted J. The f64 factor is computed once
    // per org, normalized to a [0,1] match, and quantized via the single audited `fixed::to_unit_u16`
    // chokepoint; the single floored GRANTED value is then both the pool debit and the org credit (no f64 ever
    // touches hashed Energy/Biomass). This preserves spatial selection as a REAL integer energetic advantage.
    let mut items: Vec<MetabolismItem> = q
        .iter()
        .map(|(id, sp, _e, biomass, _a, d, t, p)| {
            let local_soil = soil_field.0.sample_at(p.x, p.y);
            // Both modifiers return a strictly-positive [0.5,1.5] band; product ∈ [0.25,2.25]. Map to a [0,1]
            // match by `(factor - 0.25)/2.0` (linear, monotone), quantize ONCE to the u16 grid → an integer
            // match permille. A better trait↔environment match ⇒ a higher permille ⇒ more demand.
            let factor = soil_mod.fitness_factor(local_soil, d.0)
                * clim_mod.fitness_factor(clim_sample, t.0);
            let match_unit = ((factor - 0.25) / 2.0).clamp(0.0, 1.0);
            let match_permille = (u64::from(fixed::to_unit_u16(match_unit))
                * u64::from(fixed::PERMILLE))
                / u64::from(fixed::UNIT_SCALE);
            MetabolismItem {
                cell: cell_index(p, width),
                species: sp.0 .0,
                org: id.0,
                // Bigger bodies demand more (size→uptake feedback); floor at seed biomass so a fresh org eats.
                body: biomass.0.max(OFFSPRING_SEED_BIOMASS),
                // Floor at a baseline so a poor match still eats a little (no zeroed weight — ADR-005 spirit).
                match_permille: match_permille.max(u64::from(fixed::PERMILLE) / 4),
            }
        })
        .collect();
    items.sort_unstable_by_key(|it| (it.cell, it.species, it.org));

    // ── Pass 1: per-org DEMAND against the FROZEN stock (clone the three channels start-of-tick). ──
    let frozen_light = pools.light.clone();
    let frozen_nutrient = pools.free_nutrient.clone();
    let frozen_detritus = pools.detritus.clone();

    // Per-channel demand vector, indexed parallel to `items` (canonical order → finding #6 apportion index).
    let n = items.len();
    let mut demand = vec![[0i64; resource::RESOURCE_CHANNELS]; n];
    for (i, it) in items.iter().enumerate() {
        let strat = &registry.entries[it.species as usize].strategy;
        // Acquisition permille scales demand; body size scales it further. demand_permille folds affinity.
        let acq = u64::from(strat.budget[gp::BudgetChannel::Acquisition as usize]);
        let body_factor = ((it.body as u128 * u128::from(fixed::PERMILLE)) / (BIOMASS_CAP as u128))
            .min(1000) as u64;
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
        let nutrient_limit = ((frozen_nutrient[cell].max(0) as u128 * u128::from(fixed::PERMILLE))
            / u128::from(NUTRIENT_LIMIT_REF as u64))
        .min(u128::from(fixed::PERMILLE)) as u64;
        for (c, stock, taps_channel) in taps {
            if !taps_channel {
                continue;
            }
            // demand_permille = acq · affinity[c] · body · match, all on permille grids → one combined
            // permille. The match factor (blocker #1) makes a well-adapted lineage demand — and thus win — more
            // of a contended pool, the integer spatial-selection gradient (ADR-011/012).
            let aff_permille =
                (u64::from(aff[c]) * u64::from(fixed::PERMILLE)) / u64::from(fixed::UNIT_SCALE);
            let p = u64::from(fixed::PERMILLE);
            let mut dp = acq * aff_permille / p * body_factor / p * it.match_permille / p;
            // Liebig gate on the Autotroph LIGHT channel only (c == 0).
            if c == 0 && role == gp::TrophicRole::Autotroph {
                dp = dp * nutrient_limit / p;
            }
            demand[i][c] = monod_demand(stock, dp.min(p));
        }
    }

    // ── Pass 2: per-cell APPORTION the actual available J across co-located demanders (canonical order). ──
    // Group item indices by (channel, cell) and apportion the live pool ONCE per (channel, cell).
    let mut granted = vec![[0i64; resource::RESOURCE_CHANNELS]; n];
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
            let weights: Vec<u64> = items[i..j]
                .iter()
                .enumerate()
                .map(|(k, _)| demand[i + k][c].max(0) as u64)
                .collect();
            let total_demand: i64 = weights.iter().map(|&w| w as i64).sum();
            if total_demand > 0 {
                let cellu = cell as usize;
                let available = pool_channel(&pools, c)[cellu].min(total_demand);
                let shares = fixed::apportion(available, &weights);
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
    for (id, sp, mut energy, mut biomass, mut age, _d, _t, p) in q.iter_mut() {
        age.0 = age.0.saturating_add(1);
        let granted_total = match by_org.get(&id.0) {
            Some(&g) if g > 0 => g,
            _ => continue,
        };
        // CONVERT: split granted J across the 5 budget channels (conserved by split_budget).
        let strat = &registry.entries[sp.0 .0 as usize].strategy;
        let split = fixed::split_budget(granted_total, &strat.budget);
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

        // ADR-013 F4 LITTERFALL: an AUTOTROPH sheds a fraction of its convert-respired inefficiency to detritus
        // (a living canopy rains litter even without death). A residual SPLIT of the already-floored respired
        // value (no double-floor): `litter` → detritus, `respired_convert − litter` → respired. Decomposers
        // shed nothing here (their loss is the mineralization respired tap).
        let litter = if strat.role == gp::TrophicRole::Autotroph {
            respired_convert * LITTERFALL_NUM / LITTERFALL_DEN
        } else {
            0
        };
        ledger.respired += respired_convert - litter;
        ledger.overflow += b_over + e_over;
        if litter > 0 {
            litterfall.insert(id.0, (cell_index(p, width), sp.0 .0, litter));
        }
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
fn credit_capped(value: i64, amount: i64, cap: i64) -> (i64, i64) {
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
    field: Res<ResourceFieldRes>,
    mut pools: ResMut<PoolStock>,
    mut ledger: ResMut<ledger::Ledger>,
) {
    let cells = (pools.width as usize) * (pools.height as usize);
    for c in 0..cells {
        // light: mint SOLAR_PER_CELL, capped by the static field's per-cell carrying capacity.
        let light_cap = (i64::from(fixed::to_unit_u16(f64::from(field.0.light[c]))) * CELL_J_SCALE)
            .min(POOL_CAP);
        mint_to_cap(&mut pools.light[c], SOLAR_PER_CELL, light_cap, &mut ledger);
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
    mut pools: ResMut<PoolStock>,
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
    let width = pools.width;
    // ── Build ONE canonical (cell, SpeciesId, OrgId) order over the LIVING set (inv #3, finding #5). ──
    let mut rows: Vec<ReproRow> = q
        .iter()
        .map(
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
        )
        .collect();
    rows.sort_unstable_by_key(|r| (r.cell, r.species, r.org));

    // ── Step 1+2: maintenance debit, then death (carcass→detritus) — all RNG-free, canonical order. ──
    let mut dead: Vec<Entity> = Vec::new();
    // Track per-entity post-maintenance Energy so the birth pass reads the debited value.
    let mut maint_energy: std::collections::BTreeMap<u64, i64> = std::collections::BTreeMap::new();
    for r in &rows {
        let strat = &registry.entries[r.species as usize].strategy;
        let maint_permille = u64::from(strat.budget[gp::BudgetChannel::Maintenance as usize]);
        let debit = (MAINTENANCE_BASE as u128 * u128::from(maint_permille)
            / u128::from(fixed::PERMILLE)) as i64;
        let paid = debit.min(r.energy.max(0)); // never below 0 (no saturating_sub silent floor)
        let energy_after = r.energy - paid;
        ledger.respired += paid;
        maint_energy.insert(r.org, energy_after);

        let starved = energy_after < MAINTENANCE_FLOOR;
        let senescent = r.age >= AGE_MAX;
        if starved || senescent {
            // Carcass → detritus: residual Energy (post-maintenance) + Biomass deposits to the cell pool.
            let residual = energy_after.max(0) + r.biomass.max(0);
            let cellu = r.cell as usize;
            let headroom = (POOL_CAP - pools.detritus[cellu]).max(0);
            let accepted = residual.min(headroom);
            pools.detritus[cellu] += accepted;
            // ADR-013 F4: tag the carcass detritus with the dead org's species so a decomposer's later harvest
            // of it attributes flow[decomposer][this-species] in the FlowMatrix (the obligate-loop edge).
            prov.deposit_detritus(cellu, r.species as usize, accepted);
            ledger.overflow += residual - accepted; // detritus cap spill → overflow (finding #5)
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
        let k = (ddisp % 9) as i64;
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
        chem: 0, // documented zero until F5
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
        world.insert_resource(ResourceFieldRes(resource_field));
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
        // The multi-species spine (ADR R3-A): the ordered species registry. Now READ by the F3/F4 pipeline
        // (metabolism/mineralize read each species' cached Strategy).
        world.insert_resource(SpeciesRegistry { entries });

        let mut schedule = Schedule::default();
        // Explicit, single-threaded ordering — the determinism backbone (ADR-002, ADR-013 F3/F4). The integer
        // pipeline: advance → reset_flow (zero the per-gen FlowMatrix) → solar_influx (light INFLUX tap;
        // free_nutrient is now endogenous) → metabolism (uptake/convert/excrete + litterfall + free_nutrient
        // provenance, RNG-free) → mineralize (the F4 decomposer detritus→free_nutrient loop + FlowMatrix
        // harvest record) → reproduce_or_die (maintenance debit + death FIRST + birth — the ONLY SimRng
        // consumer; carcass→detritus provenance) → assert_flow_closes (row-sum==0) → measure_and_assert_ledger
        // (LAST: closes the books every tick).
        schedule.add_systems(
            (
                advance_tick,
                trophic::reset_flow,
                solar_influx,
                metabolism,
                trophic::mineralize,
                reproduce_or_die,
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
    /// count; `allele_freq` = mean [`Genotype`] in the cell; `fitness` = mean [`Energy`] in the cell.
    /// Empty cells are `0` on every channel. Now reflects REAL spatial structure (clusters/clines from
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
        let cfg = SimConfig {
            seed: 13_679_457_532_755_275_413,
            generations: 50,
            entity_count: 1000,
        };
        assert_eq!(run_headless(&cfg).hash, 0x42fe_54f2_f6d8_360d);
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
        let (mut e, mut b) = (0i64, 0i64);
        for (energy, biomass) in sim.world.query::<(&Energy, &Biomass)>().iter(&sim.world) {
            e += energy.0;
            b += biomass.0;
        }
        let live = ledger::LiveTotal {
            pools: pools_total,
            energy: e,
            biomass: b,
            chem: 0,
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
        // OBLIGATE: kill the decomposer → free_nutrient drains to a dead minimum and the plant population ends
        // BELOW the with-decomposer baseline (its only nutrient source is gone). Deterministic.
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
            "no decomposer ⇒ plants starve: plants without {plants_without} vs with {plants_with}"
        );
        // And the no-decomposer world's free_nutrient stays at the drained zero (only drainage, no mint).
        assert_eq!(
            total_free_nutrient(&without),
            0,
            "without a mineralizer, free_nutrient never reappears"
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
        let (mut e, mut b) = (0i64, 0i64);
        for (energy, biomass) in sim.world.query::<(&Energy, &Biomass)>().iter(&sim.world) {
            e += energy.0;
            b += biomass.0;
        }
        let live = ledger::LiveTotal {
            pools: pools_total,
            energy: e,
            biomass: b,
            chem: 0,
        };
        assert!(
            ledger::ledger_closes(&sim.ledger(), &live),
            "the obligate-loop ledger must close: live {} vs expected {}",
            live.sum(),
            sim.ledger().expected_total()
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
