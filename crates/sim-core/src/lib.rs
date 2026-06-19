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
use rand_chacha::ChaCha8Rng;

pub mod det;
pub mod gp;

pub use det::derive_seed;
pub use gp::{GenotypePhenotypeMap, Phenotype, Trait, WeightedSumMap};

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
fn unit_f64(x: u64) -> f64 {
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

// --- public entry point ---------------------------------------------------------------------------

/// Run one headless, deterministic simulation and return its [`RunStats`].
///
/// Same `config` + same build + same platform ⇒ identical `hash` (SPEC §6).
#[must_use]
pub fn run_headless(config: &SimConfig) -> RunStats {
    let mut world = World::new();
    let mut rng = ChaCha8Rng::seed_from_u64(config.seed);

    // Express the genome → phenotype ONCE (invariant #2; genotype→phenotype only here / in `genome`).
    // The Wright-Fisher loop then selects over per-individual genotypes modulated by this base growth rate.
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

    for _ in 0..config.generations {
        schedule.run(&mut world);
    }

    let allele_freq = mean_genotype(&mut world);
    RunStats {
        seed: config.seed,
        generations: config.generations,
        entity_count: config.entity_count,
        allele_freq,
        hash: hash_world(&mut world, config, allele_freq),
    }
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
