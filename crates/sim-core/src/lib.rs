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

pub mod det;
pub mod gp;
pub mod snapshot;
pub mod soil;

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

/// Stable per-organism id (0..entity_count), assigned at spawn. Gives a deterministic hash order
/// independent of ECS query/archetype iteration order.
#[derive(Component, Clone, Copy)]
struct OrgId(u32);

/// Placeholder organism energy advanced each generation (metabolism). Kept from Stage 0 so the prior
/// behaviour/hash structure is preserved alongside the new selection loop.
#[derive(Component, Clone, Copy)]
struct Energy(f64);

/// Per-individual heritable scalar in `[0, 1]` — the "allele" under selection. Seeded at spawn from
/// [`SimRng`] so individuals vary; resampled each generation by [`selection`]. Higher fitness ⇒ more copies.
#[derive(Component, Clone, Copy)]
struct Genotype(f64);

// --- systems (fixed order via .chain()) -----------------------------------------------------------

fn advance_tick(mut tick: ResMut<Tick>) {
    tick.0 += 1;
}

/// Empty-but-deterministic metabolism: each organism's energy relaxes toward a fresh RNG draw.
/// Draws happen in stable spawn/table order, so the RNG stream is reproducible.
fn metabolism(mut rng: ResMut<SimRng>, mut q: Query<&mut Energy>) {
    for mut energy in &mut q {
        let draw = unit_f64(rng.0.next_u64());
        energy.0 = (energy.0 * 0.99 + draw * 0.01).clamp(0.0, 1.0);
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
    mut q: Query<(&OrgId, &mut Genotype)>,
) {
    // Snapshot parents in stable id order (decouples sampling from ECS archetype order).
    let mut parents: Vec<(u32, f64)> = q.iter().map(|(id, g)| (id.0, g.0)).collect();
    if parents.len() < 2 {
        return; // nothing to select between (also the empty-population fast path).
    }
    parents.sort_unstable_by_key(|p| p.0);

    // Cumulative fitness weights over the id-sorted parents.
    let mut cumulative: Vec<f64> = Vec::with_capacity(parents.len());
    let mut total = 0.0;
    for &(_, g) in &parents {
        total += fitness(base.0, g);
        cumulative.push(total);
    }

    // Draw N offspring genotypes, each from a fitness-proportional parent (ordered binary search).
    let n = parents.len();
    let mut offspring: Vec<f64> = Vec::with_capacity(n);
    for _ in 0..n {
        let target = unit_f64(rng.0.next_u64()) * total;
        // First cumulative weight strictly greater than target; partition_point is deterministic.
        let idx = cumulative.partition_point(|&c| c <= target).min(n - 1);
        offspring.push(parents[idx].1);
    }

    // Map each id (in stable order) to its new genotype, then write back. `BTreeMap` is ordered (not a
    // `HashMap`), so the build is deterministic; the write-back order over the query is irrelevant since
    // each entity is keyed by its own stable id.
    let by_id: std::collections::BTreeMap<u32, f64> =
        parents.iter().map(|p| p.0).zip(offspring).collect();
    for (id, mut g) in &mut q {
        if let Some(&new_g) = by_id.get(&id.0) {
            g.0 = new_g;
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
    /// Build a fresh simulation: seed the [`ChaCha8Rng`] **once**, express the genome→phenotype once,
    /// and spawn the population — exactly as the one-shot [`run_headless`] does (invariant #3, #2).
    #[must_use]
    pub fn reset(config: &SimConfig) -> Self {
        let mut world = World::new();
        // Seed the single RNG ONCE for the whole episode (inv. #3 — never re-seeded mid-run).
        let mut rng = ChaCha8Rng::seed_from_u64(config.seed);

        // Express the genome → phenotype ONCE (invariant #2; genotype→phenotype only here / in `genome`).
        // The Wright-Fisher loop then selects over per-individual genotypes modulated by base growth rate.
        let genome = genome::sample_genome();
        let phenotype = WeightedSumMap.express(&genome);
        let base_growth = phenotype.get(Trait::GrowthRate).unwrap_or(0.5);

        for i in 0..config.entity_count {
            // Per-individual genotype in [0,1] seeded from the single RNG so individuals VARY (the standing
            // variation selection acts on); energy keeps the Stage-0 metabolism behaviour.
            let g0 = unit_f64(rng.next_u64());
            let init = base_growth * unit_f64(rng.next_u64());
            world.spawn((OrgId(i), Energy(init), Genotype(g0)));
        }

        world.insert_resource(SimRng(rng));
        world.insert_resource(Tick::default());
        world.insert_resource(GenomeRes(genome));
        world.insert_resource(BaseGrowthRate(base_growth));

        let mut schedule = Schedule::default();
        // Explicit, single-threaded ordering — the determinism backbone (ADR-002). Selection runs AFTER
        // metabolism each generation.
        schedule.add_systems((advance_tick, metabolism, selection).chain());

        Self {
            world,
            schedule,
            config: config.clone(),
            // Generated purely from the seed via derive_seed — draws ZERO from SimRng, so it cannot move
            // the determinism hash (invariant #3). Static for the run.
            soil: soil::SoilField::generate(config.seed, soil::SOIL_DIMS.0, soil::SOIL_DIMS.1),
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
    /// determinism hash (invariant #3). Each organism is placed into a cell by a deterministic function of
    /// its [`OrgId`] only (`x = derive_seed(id, 1) % width`, `y = derive_seed(id, 2) % height`), so the
    /// layout is reproducible and independent of the RNG stream. Aggregation walks organisms in stable
    /// `OrgId` order (no `HashMap` iteration affecting output — invariant #3).
    ///
    /// Channels (each `width * height`, row-major, in `[0, 1]`): `density` = per-cell count / busiest-cell
    /// count; `allele_freq` = mean [`Genotype`] in the cell; `fitness` = mean [`Energy`] in the cell.
    /// Empty cells are `0` on every channel.
    ///
    /// PoC note: this is a **derived spatial layout** for visualization — the core has no real spatial
    /// dynamics yet (future work). See [`crate::snapshot`] for the placement model and binary format.
    ///
    /// # Panics
    /// Panics if `width` or `height` is `0` (a degenerate grid has no cells to place organisms in).
    #[must_use]
    pub fn snapshot(&mut self, width: u32, height: u32) -> GridSnapshot {
        assert!(width > 0 && height > 0, "snapshot grid must be non-empty");
        let generation = self.world.resource::<Tick>().0;

        // Collect organisms in STABLE OrgId order (decouples from ECS archetype iteration — inv. #3).
        let mut rows: Vec<(u32, f64, f64)> = self
            .world
            .query::<(&OrgId, &Genotype, &Energy)>()
            .iter(&self.world)
            .map(|(id, g, e)| (id.0, g.0, e.0))
            .collect();
        rows.sort_unstable_by_key(|r| r.0);
        let population = rows.len() as u32;

        let cells = (width as usize) * (height as usize);
        let mut count = vec![0u32; cells];
        let mut genotype_sum = vec![0.0f64; cells];
        let mut energy_sum = vec![0.0f64; cells];

        for (id, g, e) in &rows {
            // Deterministic placement from OrgId ONLY (splitmix in det.rs) — independent of the RNG stream.
            let x = (derive_seed(u64::from(*id), 1) % u64::from(width)) as usize;
            let y = (derive_seed(u64::from(*id), 2) % u64::from(height)) as usize;
            let cell = y * (width as usize) + x;
            count[cell] += 1;
            genotype_sum[cell] += *g;
            energy_sum[cell] += *e;
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
            fitness[c] = (energy_sum[c] / f64::from(n)) as f32;
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

    /// The species genome currently wired into the core (read-only; invariant #2 — biology lives here).
    #[must_use]
    pub fn species_genome(&self) -> &Genome {
        &self.world.resource::<GenomeRes>().0
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

    // Collect (id, energy-bits, genotype-bits) and sort by id so the hash never depends on iteration order.
    let mut rows: Vec<(u32, u64, u64)> = world
        .query::<(&OrgId, &Energy, &Genotype)>()
        .iter(world)
        .map(|(id, e, g)| (id.0, e.0.to_bits(), g.0.to_bits()))
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
    for (id, e_bits, g_bits) in &rows {
        id.hash(&mut h);
        e_bits.hash(&mut h);
        g_bits.hash(&mut h);
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
    fn determinism_hash_is_pinned_and_soil_is_hash_neutral() {
        // Pin the EXACT hash literal captured BEFORE the soil layer existed (the harness's run-0 derived
        // seed for seed=42, gens=50, entities=1000). `check_determinism.sh` only compares run==run, so it
        // would NOT catch a reproducible-but-CHANGED hash — this guards that, AND because the literal was
        // measured pre-soil, matching it on the with-soil build PROVES soil is hash-neutral (it is generated
        // from `derive_seed` with zero `SimRng` draws and never folded into `hash_world`). If this literal
        // ever needs to change, it means real sim LOGIC changed (e.g. R1.1 soil→selection coupling) — update
        // it deliberately in that same commit.
        let cfg = SimConfig {
            seed: 13_679_457_532_755_275_413,
            generations: 50,
            entity_count: 1000,
        };
        assert_eq!(
            run_headless(&cfg).hash,
            0xc530_7d86_dba9_7ab1,
            "soil must be hash-neutral in R1.0 (this literal was measured pre-soil)"
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
        let snap = sim.snapshot(64, 64);
        // Recompute occupancy via the same OrgId-derived placement to find empty cells.
        let mut occupied = vec![false; 64 * 64];
        for id in 0..4u32 {
            let x = (derive_seed(u64::from(id), 1) % 64) as usize;
            let y = (derive_seed(u64::from(id), 2) % 64) as usize;
            occupied[y * 64 + x] = true;
        }
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
