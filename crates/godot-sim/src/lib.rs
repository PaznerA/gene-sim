//! gene-sim live-sim GDExtension — the `LiveSim` Godot node (ADR-010, gameplay batch P1b).
//!
//! A **thin binding** (invariant #2): this crate embeds the headless [`harness::GeneSimEnv`] (which
//! wraps [`sim_core::Simulation`]) and exposes a minimal surface to GDScript — `reset` / `step` /
//! `observe` / `snapshot`. **No genotype→phenotype biology lives here**: every biological computation
//! stays in `sim-core` / `genome` / `crispr`; GDScript only *calls* these methods. Invariant #2 is
//! about biology *written in* GDScript — a Rust binding that GDScript calls is fine.
//!
//! ## Determinism (invariant #3)
//! This crate adds **no new RNG**. The single seeded `rand_chacha::ChaCha8Rng` is created once per
//! [`reset`](LiveSim::reset) inside the wrapped [`harness::GeneSimEnv`] and threaded through every
//! `step` — exactly as the headless env does. `snapshot` is read-only (it never draws from the RNG).
//! `LiveSim` does **not** re-implement the replay contract (`harness --record-episode`/`--replay`,
//! `harness::replay`); a LATER phase will journal `reset`+`Advance(n)` into that existing path.
//!
//! ## What is NOT here yet (later phases, per ADR-010 / the brief)
//! `apply_edit` and `save_session` are deferred. The cadence rule (a fixed integer N generations per
//! tick, never wall-clock — invariant #3) is honored by `step(n: i64)` taking an explicit integer.
//!
//! gdext is MPL-2.0; this is a cdylib (a separate link unit), so the GPL process-boundary (invariant
//! #1) is untouched. Pinned to `godot` 0.5.3 / api-4-6 (invariant #7; ADR-010).

use godot::builtin::VarDictionary;
use godot::prelude::*;
use harness::{Action, Env, GeneSimEnv};
use sim_core::{Observation, Simulation};

/// gdext entry point. Registers every `#[derive(GodotClass)]` in this crate (here: [`LiveSim`]).
struct GodotSimExtension;

#[gdextension]
unsafe impl ExtensionLibrary for GodotSimExtension {}

/// Default population spawned at `reset` (matches the headless harness defaults' order of magnitude).
const DEFAULT_ENTITY_COUNT: u32 = 1000;
/// Generations advanced per `step(0)` / used to clamp negative inputs to a sane, deterministic value.
const NO_NEGATIVE: i64 = 0;

/// `LiveSim` — the one Godot node the live-sim feature exposes (ADR-010).
///
/// A thin `RefCounted` wrapper over [`harness::GeneSimEnv`]. GDScript drives it with
/// `reset(seed)` → `step(n)` → `observe()` and reads `snapshot(w, h)` bytes (GSS2, parsed by the
/// existing `godot/snapshot.gd`). All biology runs in the embedded Rust core (invariant #2).
#[derive(GodotClass)]
#[class(base=RefCounted)]
struct LiveSim {
    /// The headless env (single seeded RNG inside). `None` until [`reset`](Self::reset) is called.
    env: Option<GeneSimEnv>,
    /// Population spawned at the next `reset`. Set via [`set_entity_count`](Self::set_entity_count).
    entity_count: u32,
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for LiveSim {
    fn init(base: Base<RefCounted>) -> Self {
        Self {
            env: None,
            entity_count: DEFAULT_ENTITY_COUNT,
            base,
        }
    }
}

#[godot_api]
impl LiveSim {
    /// Set the population spawned at the **next** `reset` (does not disturb a run in progress).
    ///
    /// Clamped to `>= 0`; `0` is a valid (empty-population) deterministic run. Call before `reset`.
    #[func]
    fn set_entity_count(&mut self, count: i64) {
        self.entity_count = count.max(NO_NEGATIVE) as u32;
    }

    /// The population that the next `reset` will spawn.
    #[func]
    fn entity_count(&self) -> i64 {
        i64::from(self.entity_count)
    }

    /// Start a fresh episode from `seed` and return the initial observation as a `Dictionary`.
    ///
    /// Builds a new [`harness::GeneSimEnv`] (which seeds the single `ChaCha8Rng` once — invariant #3)
    /// and returns `{generation, population, allele_freq}` (plus the expressed `phenotype` traits).
    /// `seed` is taken as the master seed verbatim.
    #[func]
    fn reset(&mut self, seed: i64) -> VarDictionary {
        let mut env = GeneSimEnv::new(self.entity_count);
        // `seed` is the master seed; reinterpret the i64 bits as u64 so the full 64-bit space is usable
        // from GDScript (which has no native u64) without changing the deterministic stream.
        let obs = env.reset(seed as u64);
        self.env = Some(env);
        observation_to_dict(&obs)
    }

    /// Advance the simulation by `n` generations on the single seeded stream (invariant #3).
    ///
    /// **Cadence rule (ADR-010, invariant #3):** time advances by a fixed integer count, NEVER by
    /// wall-clock/delta — so a journaled `Advance(n)` sum reproduces. Negative `n` is clamped to `0`.
    /// Panics (Godot error) if called before `reset`.
    #[func]
    fn step(&mut self, n: i64) {
        let n = n.max(NO_NEGATIVE) as u64;
        match self.env.as_mut() {
            Some(env) => {
                // GeneSimEnv::step applies one Action; Advance(n) advances exactly n generations.
                let _ = env.step(Action::Advance(n));
            }
            None => godot_error!("LiveSim::step called before reset()"),
        }
    }

    /// Observe the current state without advancing it (pure w.r.t. the run — invariant #3).
    ///
    /// Returns `{generation, population, allele_freq, phenotype: {trait_name: value, ...}}`.
    /// Panics (Godot error) if called before `reset`; returns an empty Dictionary in that case.
    #[func]
    fn observe(&mut self) -> VarDictionary {
        match self.env.as_mut() {
            Some(env) => observation_to_dict(&env.observe()),
            None => {
                godot_error!("LiveSim::observe called before reset()");
                VarDictionary::new()
            }
        }
    }

    /// Produce the read-only GSS2 snapshot bytes for a `w × h` grid (parsed by `godot/snapshot.gd`).
    ///
    /// Read-only: it never draws from the RNG or mutates state, so taking snapshots cannot change the
    /// determinism hash (invariant #3). The bytes are exactly
    /// [`sim_core::GridSnapshot::write_snapshot_bytes`]. Non-positive `w`/`h` yield an empty
    /// `PackedByteArray` (the core requires a non-empty grid). Empty before `reset`.
    #[func]
    fn snapshot(&mut self, w: i64, h: i64) -> PackedByteArray {
        if w <= 0 || h <= 0 {
            godot_error!("LiveSim::snapshot requires w > 0 and h > 0 (got {w}x{h})");
            return PackedByteArray::new();
        }
        let Some(env) = self.env.as_mut() else {
            godot_error!("LiveSim::snapshot called before reset()");
            return PackedByteArray::new();
        };
        let bytes = snapshot_bytes(env, w as u32, h as u32);
        PackedByteArray::from(bytes.as_slice())
    }

    /// Convenience: whether `reset` has been called (an episode is live).
    #[func]
    fn is_ready(&self) -> bool {
        self.env.is_some()
    }
}

/// Build the GSS2 snapshot bytes from the env's live `Simulation` (read-only — invariant #3).
///
/// [`harness::GeneSimEnv::snapshot`] delegates to [`sim_core::Simulation::snapshot`] (no RNG draw,
/// no mutation); [`sim_core::GridSnapshot::write_snapshot_bytes`] emits the exact GSS2 layout that
/// `godot/snapshot.gd` parses.
fn snapshot_bytes(env: &mut GeneSimEnv, w: u32, h: u32) -> Vec<u8> {
    env.snapshot(w, h).write_snapshot_bytes()
}

/// Convert a [`sim_core::Observation`] into a GDScript-facing `Dictionary`.
///
/// Keys: `generation` (int), `population` (int), `allele_freq` (float), and `phenotype` — a nested
/// Dictionary of `{trait_name: value}`. Pure data marshalling; no biology (invariant #2).
fn observation_to_dict(obs: &Observation) -> VarDictionary {
    let mut dict = VarDictionary::new();
    dict.set("generation", obs.generation as i64);
    dict.set("population", i64::from(obs.population_size));
    dict.set("allele_freq", obs.allele_freq);

    let mut pheno = VarDictionary::new();
    for (trait_, value) in &obs.phenotype.values {
        // Trait names come straight from the core's Debug repr — presentation only, no biology here.
        pheno.set(format!("{trait_:?}"), *value);
    }
    // Nest the phenotype dict as a Variant value (VarDictionary's V = Variant); `&Dictionary`
    // implements `AsArg<Variant>`, so pass it by reference.
    dict.set("phenotype", &pheno);
    dict
}

// A compile-time witness that `Simulation` is the type the env wraps (keeps the import meaningful and
// documents the binding boundary: we wrap the headless handle, we do not reimplement it).
#[allow(dead_code)]
fn _binds_simulation(_: &Simulation) {}
