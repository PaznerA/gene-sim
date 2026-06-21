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
pub mod snapshot;
pub mod soil;

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

/// Stable per-organism id (0..entity_count), assigned at spawn. Gives a deterministic hash order
/// independent of ECS query/archetype iteration order.
#[derive(Component, Clone, Copy)]
struct OrgId(u32);

/// Full-scale energy quantum (ADR-013 F0b): one "unit" of organism energy is `ENERGY_FULL` integer joules.
/// Energy migrates from `f64` to the conserved `i64` currency the CHEMOSTAT-J economy will denominate
/// everything in (the first fixed-point type migration; later phases F1/F3 give it real metabolic meaning).
const ENERGY_FULL: i64 = 1_000_000;

/// Per-organism energy as an integer joule quantum in `[0, ENERGY_FULL]` (ADR-013 F0b — was `f64`). Still
/// decorative w.r.t. selection (a metabolism placeholder that relaxes toward a fresh draw, fed to nothing yet),
/// so migrating it to `i64` changes only its hash contribution + the snapshot fitness channel, never the
/// selection dynamics — a deliberate, isolated determinism RE-PIN. F1/F3 wire it into the joule ledger.
#[derive(Component, Clone, Copy)]
struct Energy(i64);

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
struct Position {
    x: u32,
    y: u32,
}

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

/// Empty-but-deterministic metabolism: each organism's integer energy relaxes 1% toward a fresh draw, in pure
/// fixed-point (ADR-013 F0b — no float in the recurring path). Exactly ONE `next_u64` per organism in stable
/// spawn/table order, so the RNG stream — and therefore every downstream draw (selection) — is byte-unchanged;
/// only the integer energy VALUES + their hash contribution differ from the old `f64` path.
fn metabolism(mut rng: ResMut<SimRng>, mut q: Query<&mut Energy>) {
    for mut energy in &mut q {
        let draw = (rng.0.next_u64() % (ENERGY_FULL as u64 + 1)) as i64; // [0, ENERGY_FULL], no float
        energy.0 = ((energy.0 * 99 + draw) / 100).clamp(0, ENERGY_FULL);
    }
}

/// An individual's deterministic, strictly-positive fitness. The genome's [`Trait::GrowthRate`] sets the
/// floor (`base`); the individual's [`Genotype`] modulates it, so higher genotypes are selectively favored.
/// `> 0` for every input (the `+` floor avoids zero-weight degeneracy in the cumulative sampler).
fn fitness(base: f64, genotype: f64) -> f64 {
    // Floor keeps every weight positive; the `genotype` term creates the directional gradient.
    0.05 + base * genotype
}

/// Constant-population **Wright-Fisher selection** (chained after [`metabolism`], explicit order — ADR-002).
///
/// Each generation we build the next generation's [`Genotype`]s by sampling `N` parents with probability
/// proportional to [`fitness`], drawing from the single [`SimRng`]. Population size is held constant, so the
/// loop cannot trivially go extinct. Fully deterministic: parents are read in stable [`OrgId`] order, the
/// cumulative-weight table and the draws are ordered, and there is no `HashMap` iteration (invariant #3).
fn selection(
    mut rng: ResMut<SimRng>,
    base: Res<BaseGrowthRate>,
    soil_field: Res<SoilFieldRes>,
    climate_field: Res<ClimateFieldRes>,
    mut q: Query<(
        &OrgId,
        &mut Genotype,
        &mut DroughtTol,
        &mut ThermalTol,
        &mut Position,
    )>,
) {
    use climate::ClimateModifier as _;
    use soil::EnvironmentModifier as _;
    let soil_mod = soil::LinearTraitMatchModifier;
    let clim_mod = climate::TemperatureMatchModifier;
    let clim_sample = climate_field.0.sample(); // GLOBAL climate coupling (ADR-012 E3); per-cell is a follow-up.

    // Snapshot parents (id, genotype, drought, thermal, x, y) in stable id order. Position rides along so
    // offspring INHERIT the sampled parent's cell (+ disperse) — ADR-011; thermal rides for the climate factor.
    let mut parents: Vec<(u32, f64, f64, f64, u32, u32)> = q
        .iter()
        .map(|(id, g, d, t, p)| (id.0, g.0, d.0, t.0, p.x, p.y))
        .collect();
    if parents.len() < 2 {
        return; // nothing to select between (also the empty-population fast path).
    }
    parents.sort_unstable_by_key(|p| p.0);

    // Weights = base fitness × the parent's OWN-cell SOIL factor (ADR-011 S-G) × the CLIMATE factor (ADR-012 E3:
    // warm-adapted individuals win in warm climates). Both factors strictly positive (ADR-005 no-extinction);
    // both RNG-free, no HashMap → deterministic.
    let mut cumulative: Vec<f64> = Vec::with_capacity(parents.len());
    let mut total = 0.0;
    for &(_, g, d, t, x, y) in &parents {
        let local_soil = soil_field.0.sample_at(x, y);
        let w = fitness(base.0, g)
            * soil_mod.fitness_factor(local_soil, d)
            * clim_mod.fitness_factor(clim_sample, t);
        total += w;
        cumulative.push(total);
    }

    // Draw N offspring, each INHERITING a fitness-proportional parent's (genotype, drought, thermal, position)
    // then dispersing one bounded step. EXACTLY two draws/offspring in a fixed order (select, then disperse).
    let n = parents.len();
    let mut offspring: Vec<(f64, f64, f64, u32, u32)> = Vec::with_capacity(n);
    for _ in 0..n {
        let target = unit_f64(rng.0.next_u64()) * total;
        let idx = cumulative.partition_point(|&c| c <= target).min(n - 1);
        let (_, pg, pd, pt, px, py) = parents[idx];
        let k = (unit_f64(rng.0.next_u64()) * 9.0) as i64; // 0..=8 → a 9-cell Moore step
        let nx = (px as i64 + (k % 3 - 1)).clamp(0, WORLD_DIMS.0 as i64 - 1) as u32;
        let ny = (py as i64 + (k / 3 - 1)).clamp(0, WORLD_DIMS.1 as i64 - 1) as u32;
        offspring.push((pg, pd, pt, nx, ny));
    }

    // Map each id to its inherited (genotype, drought, thermal, position), then write back (ordered BTreeMap).
    let by_id: std::collections::BTreeMap<u32, (f64, f64, f64, u32, u32)> =
        parents.iter().map(|p| p.0).zip(offspring).collect();
    for (id, mut g, mut d, mut t, mut p) in &mut q {
        if let Some(&(new_g, new_d, new_t, new_x, new_y)) = by_id.get(&id.0) {
            g.0 = new_g;
            d.0 = new_d;
            t.0 = new_t;
            p.x = new_x;
            p.y = new_y;
        }
    }
}

/// Map a u64 to a `[0, 1)` f64 using the top 53 bits (deterministic, no rand-API churn).
pub(crate) fn unit_f64(x: u64) -> f64 {
    (x >> 11) as f64 / (1u64 << 53) as f64
}

/// Mean per-individual [`Genotype`] across the population (the reported `allele_freq`), in `[0, 1]`.
/// `0.0` for an empty population. Iterates id-sorted rows so the sum order is deterministic.
fn mean_genotype(world: &mut World) -> f64 {
    let mut rows: Vec<(u32, f64)> = world
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
/// `phenotype` is the species genome re-expressed through the [`WeightedSumMap`] (invariant #2 —
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
}

impl Simulation {
    /// Build a fresh simulation with the DEFAULT (neutral) climate — the historical behaviour every existing
    /// caller + the pinned determinism config rely on (so they stay byte-identical). See [`reset_with_env`].
    #[must_use]
    pub fn reset(config: &SimConfig) -> Self {
        Self::reset_with_env(config, &climate::EnvParams::default())
    }

    /// Build a fresh simulation under a player-set climate (ADR-012 Phase E): seed the [`ChaCha8Rng`] **once**,
    /// express the genome→phenotype once, spawn the population, and build the static soil + [`climate`] fields
    /// off the seed/params (zero `SimRng` draws). The climate is inserted but only SHAPES selection once E3
    /// couples it; until then this is byte-identical to [`reset`] at default `env` (invariant #3, #2).
    #[must_use]
    pub fn reset_with_env(config: &SimConfig, env: &climate::EnvParams) -> Self {
        let mut world = World::new();
        // Seed the single RNG ONCE for the whole episode (inv. #3 — never re-seeded mid-run).
        let mut rng = ChaCha8Rng::seed_from_u64(config.seed);

        // Express the genome → phenotype ONCE (invariant #2; genotype→phenotype only here / in `genome`).
        // The Wright-Fisher loop then selects over per-individual genotypes modulated by base growth rate.
        let genome = genome::sample_genome();
        let phenotype = WeightedSumMap.express(&genome);
        let base_growth = phenotype.get(Trait::GrowthRate).unwrap_or(0.5);

        // Static soil substrate, generated purely from the seed via derive_seed — ZERO SimRng draws (R1.0).
        let soil = soil::SoilField::generate(config.seed, soil::SOIL_DIMS.0, soil::SOIL_DIMS.1);

        for i in 0..config.entity_count {
            // Per-individual genotype in [0,1] seeded from the single RNG so individuals VARY (the standing
            // variation selection acts on); energy keeps the Stage-0 metabolism behaviour; drought tolerance
            // is the new heritable standing variation soil-coupled selection acts on (R1.0a). Draw order is
            // fixed (genotype, energy, drought) so the stream is reproducible.
            let g0 = unit_f64(rng.next_u64());
            let init = base_growth * unit_f64(rng.next_u64());
            let drought = unit_f64(rng.next_u64());
            let thermal = unit_f64(rng.next_u64()); // ADR-012 E3: fixed draw order (g0, energy, drought, thermal)
                                                    // Initial cell from a DISJOINT off-SimRng stream (ADR-011): no next_u64 draw here.
            world.spawn((
                OrgId(i),
                // Quantize the seeded energy fraction to the i64 joule grid (ADR-013 F0b). One-time spawn
                // conversion (IEEE multiply + truncate is platform-stable); the recurring path stays integer.
                Energy((init * (ENERGY_FULL as f64)).clamp(0.0, ENERGY_FULL as f64) as i64),
                Genotype(g0),
                DroughtTol(drought),
                ThermalTol(thermal),
                placement(config.seed, i),
            ));
        }

        world.insert_resource(SimRng(rng));
        world.insert_resource(Tick::default());
        world.insert_resource(GenomeRes(genome));
        world.insert_resource(BaseGrowthRate(base_growth));
        world.insert_resource(SoilFieldRes(soil.clone())); // per-cell source for LOCAL coupling (ADR-011 S-G)
                                                           // World climate from the player's params — off the SimRng stream; unused by selection until E3 (so
                                                           // inserting it here is hash-neutral, proven by the unchanged pinned literal). ADR-012 Phase E.
        world.insert_resource(ClimateFieldRes(climate::ClimateField::from_params(env)));
        // The conserved-energy ledger (ADR-013 F0a). Empty at reset and read by no system yet, so inserting
        // it is hash-neutral (not folded into hash_world, draws nothing from SimRng) — proven by the unchanged
        // pinned literal. F0b/F1 seed `initial_total` and the metabolism phases drive the taps.
        world.insert_resource(ledger::Ledger::default());

        let mut schedule = Schedule::default();
        // Explicit, single-threaded ordering — the determinism backbone (ADR-002). Selection runs AFTER
        // metabolism each generation.
        schedule.add_systems((advance_tick, metabolism, selection).chain());

        Self {
            world,
            schedule,
            config: config.clone(),
            // Static for the run; read-only w.r.t. the hash beyond its coupling effect on per-org state.
            soil,
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
        // Re-express the (possibly edited) species genome into traits — the only genotype→phenotype site.
        let phenotype = WeightedSumMap.express(&self.world.resource::<GenomeRes>().0);
        Observation {
            generation,
            population_size,
            allele_freq,
            phenotype,
        }
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
        let mut rows: Vec<(u32, f64, i64, u32, u32)> = self
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

    /// The run's conserved-energy [`Ledger`](ledger::Ledger) (ADR-013 F0a; read-only copy). Empty until the
    /// joule pools land (F1); thereafter `ledger().closes(live_total)` is the conservation invariant.
    #[must_use]
    pub fn ledger(&self) -> ledger::Ledger {
        *self.world.resource::<ledger::Ledger>()
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

        // Re-express phenotype after the genome change so the edit feeds subsequent fitness (invariant #2).
        let phenotype = WeightedSumMap.express(&self.world.resource::<GenomeRes>().0);
        let base_growth = phenotype.get(Trait::GrowthRate).unwrap_or(0.5);
        self.world.resource_mut::<BaseGrowthRate>().0 = base_growth;
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

/// Deterministic, build-scoped hash of final world state (SNIPPETS.md "stable end-of-run hash").
/// Folds in each individual's `Genotype` and the population `allele_freq` alongside the Stage-0 fields.
fn hash_world(world: &mut World, config: &SimConfig, allele_freq: f64) -> u64 {
    use std::hash::{Hash, Hasher};

    // Collect (id, energy, genotype, drought) bits and sort by id so the hash never depends on iteration
    // order. Drought tolerance is per-individual heritable state (R1.0a) so it MUST enter the hash.
    // (ADR-011) Position is per-individual heritable spatial state, so it MUST enter the hash.
    // (ADR-012 E3) ThermalTol is per-individual heritable state, so it MUST enter the hash too.
    let mut rows: Vec<(u32, u64, u64, u64, u64, u32, u32)> = world
        .query::<(
            &OrgId,
            &Energy,
            &Genotype,
            &DroughtTol,
            &ThermalTol,
            &Position,
        )>()
        .iter(world)
        .map(|(id, e, g, d, t, p)| {
            (
                id.0,
                e.0 as u64, // ADR-013 F0b: integer energy reinterpreted (was f64 to_bits) — RE-PIN
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
    // Fold in one final RNG word to capture stream advancement.
    let final_word = world.resource_mut::<SimRng>().0.next_u64();

    let mut h = std::collections::hash_map::DefaultHasher::new();
    config.seed.hash(&mut h);
    config.generations.hash(&mut h);
    config.entity_count.hash(&mut h);
    tick.hash(&mut h);
    genome_params.hash(&mut h);
    for (id, e_bits, g_bits, d_bits, t_bits, px, py) in &rows {
        id.hash(&mut h);
        e_bits.hash(&mut h);
        g_bits.hash(&mut h);
        d_bits.hash(&mut h);
        t_bits.hash(&mut h);
        px.hash(&mut h);
        py.hash(&mut h);
    }
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
        // now `49ee…1cc2` after ADR-013 F0b (Energy migrated `f64`→`i64`, the joule-currency precursor). Energy
        // is decorative w.r.t. selection (drives no fitness), and metabolism still draws exactly one `next_u64`
        // per organism, so the RNG stream + allele_freq are UNCHANGED — the re-pin captures only Energy's changed
        // representation in `hash_world` (`as u64` vs `to_bits`) + its seeded/metabolized integer values.
        let cfg = SimConfig {
            seed: 13_679_457_532_755_275_413,
            generations: 50,
            entity_count: 1000,
        };
        assert_eq!(run_headless(&cfg).hash, 0x49ee_0f17_6852_1cc2);
    }

    #[test]
    fn placement_is_deterministic_and_in_bounds() {
        // ADR-011 S-A: every organism gets a real cell, reproducibly from the seed, within the world grid.
        let cfg = SimConfig {
            seed: 777,
            generations: 0,
            entity_count: 200,
        };
        let positions = |s: &mut Simulation| -> Vec<(u32, u32, u32)> {
            let mut v: Vec<(u32, u32, u32)> = s
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
        // ADR-012 E3: in a WARM climate the population's mean ThermalTol rises toward 1 (warm-adapted wins); in
        // a COLD climate it falls toward 0. From the same neutral ~0.5 standing variation, the
        // TemperatureMatchModifier shifts the distribution — the proof the climate shapes selection. Deterministic.
        let cfg = SimConfig {
            seed: 909,
            generations: 120,
            entity_count: 1200,
        };
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

        let mut hot = Simulation::reset_with_env(&cfg, &warm);
        let start = mean_thermal(&mut hot); // same spawn draws as cold ⇒ identical neutral start
        hot.step(cfg.generations);
        let hot_end = mean_thermal(&mut hot);

        let mut chill = Simulation::reset_with_env(&cfg, &cold);
        chill.step(cfg.generations);
        let cold_end = mean_thermal(&mut chill);

        assert!(
            hot_end > start,
            "warm climate raises thermal tolerance: {start:.3} → {hot_end:.3}"
        );
        assert!(
            cold_end < start,
            "cold climate lowers thermal tolerance: {start:.3} → {cold_end:.3}"
        );
        assert!(
            hot_end > cold_end,
            "warm- vs cold-adapted populations diverge: {hot_end:.3} vs {cold_end:.3}"
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
    fn selection_responds_to_a_trait() {
        // AC2: fitness rewards high Genotype (fitness = floor + base_growth * genotype, base_growth > 0),
        // so over enough generations directional selection pushes the mean Genotype well above the initial
        // ~0.5 of a uniform [0,1] standing variation. Large N + many generations make this robust.
        let r = run_headless(&SimConfig {
            seed: 42,
            generations: 300,
            entity_count: 2000,
        });
        // Initial mean of a uniform-[0,1] population is ~0.5; assert a clear upward shift.
        assert!(
            r.allele_freq > 0.7,
            "expected directional selection to raise mean genotype above 0.7, got {}",
            r.allele_freq
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
        assert_eq!(snap.population, 200);
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
    fn reset_inserts_a_closing_ledger() {
        // ADR-013 F0a: the conserved-energy ledger is present at reset, empty, and closes trivially — no
        // joule MOVES yet (Energy is now i64 per F0b but is not in the ledger; no pools). F1 will seed
        // `initial_total` and assert against
        // the real live total each tick.
        let cfg = SimConfig {
            seed: 7,
            generations: 0,
            entity_count: 100,
        };
        let sim = Simulation::reset(&cfg);
        let led = sim.ledger();
        assert_eq!(
            led,
            ledger::Ledger::default(),
            "F0a: the ledger starts empty"
        );
        assert!(
            led.closes(0),
            "F0a: nothing moves J yet, so the books close at 0"
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
