//! Headless, deterministic Bevy ECS tick loop (SPEC §2, §6; ADR-002).
//!
//! Stage 0 is an *empty but fully deterministic* core: organisms are ECS entities, a fixed, explicitly
//! ordered schedule advances them each generation, and **all** randomness flows from a single seeded
//! [`rand_chacha::ChaCha8Rng`] threaded through the world as a resource. No renderer is attached
//! (invariant #4); no biology yet (that arrives in Stage 1) — but the parametric [`genome::Genome`] is
//! already wired into the core (invariant #2).
//!
//! Determinism rules honored here (invariant #3):
//! - one seeded `ChaCha8Rng`, no thread-local/global RNG;
//! - a single-threaded, explicitly `.chain()`-ordered schedule;
//! - no `HashMap` iteration in sim logic — entities carry a stable [`OrgId`] and the end-of-run hash is
//!   computed over an id-sorted vector.

#![forbid(unsafe_code)]

use bevy_ecs::prelude::*;
use genome::Genome;
use rand_chacha::rand_core::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

pub mod det;

pub use det::derive_seed;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunStats {
    pub seed: u64,
    pub generations: u64,
    pub entity_count: u32,
    /// Stable, build-scoped hash of the final world state.
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

/// Stable per-organism id (0..entity_count), assigned at spawn. Gives a deterministic hash order
/// independent of ECS query/archetype iteration order.
#[derive(Component, Clone, Copy)]
struct OrgId(u32);

/// Placeholder organism state advanced each generation. (Real phenotype/biology arrives in Stage 1.)
#[derive(Component, Clone, Copy)]
struct Energy(f64);

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

/// Map a u64 to a `[0, 1)` f64 using the top 53 bits (deterministic, no rand-API churn).
fn unit_f64(x: u64) -> f64 {
    (x >> 11) as f64 / (1u64 << 53) as f64
}

// --- public entry point ---------------------------------------------------------------------------

/// Run one headless, deterministic simulation and return its [`RunStats`].
///
/// Same `config` + same build + same platform ⇒ identical `hash` (SPEC §6).
#[must_use]
pub fn run_headless(config: &SimConfig) -> RunStats {
    let mut world = World::new();
    let mut rng = ChaCha8Rng::seed_from_u64(config.seed);

    // Seed each organism's initial energy from the genome (growth parameter) × an RNG draw —
    // ties the parametric genome into the core (invariant #2) deterministically.
    let genome = genome::sample_genome();
    let base = genome
        .loci
        .first()
        .and_then(|l| l.parameters.first())
        .map_or(0.5, |p| p.value.as_unit_scalar());

    for i in 0..config.entity_count {
        let init = base * unit_f64(rng.next_u64());
        world.spawn((OrgId(i), Energy(init)));
    }

    world.insert_resource(SimRng(rng));
    world.insert_resource(Tick::default());
    world.insert_resource(GenomeRes(genome));

    let mut schedule = Schedule::default();
    // Explicit, single-threaded ordering — the determinism backbone (ADR-002).
    schedule.add_systems((advance_tick, metabolism).chain());

    for _ in 0..config.generations {
        schedule.run(&mut world);
    }

    RunStats {
        seed: config.seed,
        generations: config.generations,
        entity_count: config.entity_count,
        hash: hash_world(&mut world, config),
    }
}

/// Deterministic, build-scoped hash of final world state (SNIPPETS.md "stable end-of-run hash").
fn hash_world(world: &mut World, config: &SimConfig) -> u64 {
    use std::hash::{Hash, Hasher};

    // Collect (id, energy-bits) and sort by id so the hash never depends on iteration order.
    let mut rows: Vec<(u32, u64)> = world
        .query::<(&OrgId, &Energy)>()
        .iter(world)
        .map(|(id, e)| (id.0, e.0.to_bits()))
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
    for (id, bits) in &rows {
        id.hash(&mut h);
        bits.hash(&mut h);
    }
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
}
