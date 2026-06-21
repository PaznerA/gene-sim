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

use crispr::{CasVariantId, EditOutcome, GuideSequence, RegionEditOutcome};
use genome::LocusId;
use godot::builtin::VarDictionary;
use godot::prelude::*;
use harness::{Action, EditAction, Env, GeneSimEnv, RegionSpec};
use sim_core::{EnvParams, Observation, Simulation};

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
    /// The climate the next `reset` builds the world under (ADR-012 Phase E). Set via `set_environment`.
    env_params: EnvParams,
    /// Master seed of the current session (for save/load).
    seed: u64,
    /// Ordered journal of the session's actions (Advance coalesced) — the SAVED PROGRESS. Replaying
    /// `reset(seed)` + this journal restores the exact session deterministically (inv #3).
    journal: Vec<Action>,
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for LiveSim {
    fn init(base: Base<RefCounted>) -> Self {
        Self {
            env: None,
            entity_count: DEFAULT_ENTITY_COUNT,
            env_params: EnvParams::default(),
            seed: 0,
            journal: Vec::new(),
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

    /// Set the CLIMATE the **next** `reset` builds the world under (ADR-012 Phase E): latitude / longitude in
    /// degrees, average temperature (normalized `[0,1]`), season (`0` Spring · `1` Summer · `2` Autumn · `3`
    /// Winter). The main menu calls this before `reset`. Stores params only — biology stays in the core (inv #2).
    #[func]
    fn set_environment(&mut self, lat: f64, lon: f64, avg_temp: f64, season: i64) {
        self.env_params = EnvParams {
            lat,
            lon,
            avg_temp: avg_temp.clamp(0.0, 1.0),
            season: season.clamp(0, 3),
        };
    }

    /// CORE-computed climate preview for the main menu (ADR-012 E4): the `{day_length, insolation, temperature}`
    /// the given params would produce (all `[0,1]`). The menu DISPLAYS these — it never computes climate itself
    /// (inv #2: biology stays in the core). Pure: builds a `ClimateField` off the params, touches no run state.
    #[func]
    fn preview_climate(&self, lat: f64, lon: f64, avg_temp: f64, season: i64) -> VarDictionary {
        let sample = sim_core::climate::ClimateField::from_params(&EnvParams {
            lat,
            lon,
            avg_temp: avg_temp.clamp(0.0, 1.0),
            season: season.clamp(0, 3),
        })
        .sample();
        let mut d = VarDictionary::new();
        d.set("day_length", sample.day_length);
        d.set("insolation", sample.insolation);
        d.set("temperature", sample.temperature);
        d
    }

    /// Start a fresh episode from `seed` and return the initial observation as a `Dictionary`.
    ///
    /// Builds a new [`harness::GeneSimEnv`] (which seeds the single `ChaCha8Rng` once — invariant #3)
    /// and returns `{generation, population, allele_freq}` (plus the expressed `phenotype` traits).
    /// `seed` is taken as the master seed verbatim.
    #[func]
    fn reset(&mut self, seed: i64) -> VarDictionary {
        let mut env = GeneSimEnv::new(self.entity_count);
        env.set_environment(self.env_params); // build the world under the player's climate (ADR-012)
        // `seed` is the master seed; reinterpret the i64 bits as u64 so the full 64-bit space is usable
        // from GDScript (which has no native u64) without changing the deterministic stream.
        let obs = env.reset(seed as u64);
        self.env = Some(env);
        self.seed = seed as u64; // a fresh session: remember the seed + start an empty journal (save/load)
        self.journal.clear();
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
                self.journal_advance(n);
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

    /// CORE-computed mission/zone read (invariant #2): the mean allele frequency over the populated cells of
    /// a disc `(cx, cy, radius)` on a `grid_w × grid_h` snapshot grid, as `{mean: float, populated: int}`.
    /// The renderer's mission evaluation (`_eval_mission`) calls this instead of looping over the snapshot in
    /// GDScript, so the zone biology read lives in the core. Delegates to
    /// [`sim_core::Simulation::region_allele`] via the env — read-only, RNG-free (cannot change the hash).
    /// Empty (`mean 0`, `populated 0`) before `reset`.
    #[func]
    fn region_allele(
        &mut self,
        cx: i64,
        cy: i64,
        radius: i64,
        grid_w: i64,
        grid_h: i64,
    ) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(env) = self.env.as_mut() else {
            godot_error!("LiveSim::region_allele called before reset()");
            d.set("mean", 0.0);
            d.set("populated", 0_i64);
            return d;
        };
        let region = sim_core::Region {
            cx: cx.max(0) as u32,
            cy: cy.max(0) as u32,
            radius: radius.max(0) as u32,
        };
        let r = env.region_allele(region, grid_w.max(1) as u32, grid_h.max(1) as u32);
        d.set("mean", r.mean);
        d.set("populated", i64::from(r.populated_cells));
        d
    }

    /// Apply a CRISPR edit to the **species** genome live (P4 / R6.1) and return its outcome.
    ///
    /// `cas` = Cas-variant id, `target` = species-genome locus id, `guide` = the ACGT guide string. Builds a
    /// species-granular [`harness::EditAction`] (invariant #6 — no organism handle) and steps it through the
    /// env's single seeded stream (invariant #3 — the edit draws only from that stream, exactly as the gym
    /// env does). Returns `{applied: bool, detail: String, generation: int}` — never a silent no-op (the core
    /// always yields an explicit Applied/Failed outcome). Authoritative PAM/score/gate logic stays in
    /// `crispr` (invariant #2): GDScript only assembles ids + a guide string and reads the verdict.
    #[func]
    fn apply_edit(&mut self, cas: i64, target: i64, guide: GString) -> VarDictionary {
        let Some(env) = self.env.as_mut() else {
            godot_error!("LiveSim::apply_edit called before reset()");
            return edit_dict(false, "not reset", 0);
        };
        let g = match GuideSequence::new(guide.to_string().into_bytes()) {
            Ok(g) => g,
            Err(pos) => {
                return edit_dict(
                    false,
                    &format!("invalid guide (bad base at {pos})"),
                    env_gen(env),
                );
            }
        };
        let edit = EditAction {
            cas: CasVariantId(cas.clamp(0, i64::from(u16::MAX)) as u16),
            target: LocusId(target.max(0) as u32),
            guide: g,
        };
        let action = Action::ApplyEdit(edit);
        env.step(action.clone());
        self.journal.push(action); // record for save/load (disjoint field borrow from `env`)
        let env = self.env.as_mut().expect("env present");
        let cur_gen = env_gen(env);
        match env.last_edit() {
            Some(EditOutcome::Applied {
                locus,
                param,
                on_efficiency,
                off_target_hits,
            }) => edit_dict(
                true,
                &format!(
                    "applied → locus {} param {} · on-eff {on_efficiency:.2} · off-target {off_target_hits}",
                    locus.0, param.0
                ),
                cur_gen,
            ),
            Some(EditOutcome::Failed { reason, .. }) => {
                edit_dict(false, &format!("failed: {reason:?}"), cur_gen)
            }
            None => edit_dict(false, "no outcome", cur_gen),
        }
    }

    /// Apply a REGION-scoped CRISPR edit — the selective brush (ADR-011 S-D). Same args as [`apply_edit`] plus
    /// a CELL disc `(cx, cy, radius)` on the world grid; the edit's gate-derived allele shift is applied to
    /// only the organisms inside that disc. Returns `{applied, detail, generation, covered}` (`covered` = how
    /// many organisms the brush touched). Cell-scoped, no organism handle (invariant #6); biology in the core
    /// (invariant #2) — GDScript only passes ids + a guide + a disc and reads the verdict.
    #[func]
    #[allow(clippy::too_many_arguments)]
    fn apply_edit_region(
        &mut self,
        cas: i64,
        target: i64,
        guide: GString,
        cx: i64,
        cy: i64,
        radius: i64,
    ) -> VarDictionary {
        let Some(env) = self.env.as_mut() else {
            godot_error!("LiveSim::apply_edit_region called before reset()");
            return region_dict(false, "not reset", 0, 0);
        };
        let g = match GuideSequence::new(guide.to_string().into_bytes()) {
            Ok(g) => g,
            Err(pos) => {
                return region_dict(
                    false,
                    &format!("invalid guide (bad base at {pos})"),
                    env_gen(env),
                    0,
                );
            }
        };
        let edit = EditAction {
            cas: CasVariantId(cas.clamp(0, i64::from(u16::MAX)) as u16),
            target: LocusId(target.max(0) as u32),
            guide: g,
        };
        let region = RegionSpec {
            cx: cx.max(0) as u32,
            cy: cy.max(0) as u32,
            radius: radius.max(0) as u32,
        };
        let action = Action::ApplyEditRegion(edit, region);
        env.step(action.clone());
        self.journal.push(action); // record for save/load
        let env = self.env.as_mut().expect("env present");
        let cur_gen = env_gen(env);
        match env.last_region_edit() {
            Some((
                RegionEditOutcome::Applied {
                    on_efficiency,
                    off_target_hits,
                    genotype_delta,
                },
                covered,
            )) => region_dict(
                true,
                &format!(
                    "region applied → {covered} organisms · on-eff {on_efficiency:.2} · Δallele {genotype_delta:+.2} · off-target {off_target_hits}"
                ),
                cur_gen,
                *covered,
            ),
            Some((RegionEditOutcome::Failed { reason }, _)) => {
                region_dict(false, &format!("failed: {reason:?}"), cur_gen, 0)
            }
            None => region_dict(false, "no outcome", cur_gen, 0),
        }
    }

    /// The Cas-variant table as `[{id, name}, ...]` so the intervention UI can offer real choices (ids +
    /// names only — no biology in GDScript; the table is data, SPEC §4). From `crispr::default_cas_variants`.
    #[func]
    fn cas_variants(&self) -> VarArray {
        let mut arr = VarArray::new();
        for v in crispr::default_cas_variants() {
            let mut d = VarDictionary::new();
            d.set("id", i64::from(v.id.0));
            d.set("name", v.name.as_str());
            arr.push(&d.to_variant());
        }
        arr
    }

    /// The species-genome loci as `[{id, name}, ...]` for the intervention UI's target picker (ids + names
    /// only). From `genome::sample_genome` (the species baseline).
    #[func]
    fn loci(&self) -> VarArray {
        let mut arr = VarArray::new();
        for l in genome::sample_genome().loci {
            let mut d = VarDictionary::new();
            d.set("id", i64::from(l.id.0));
            d.set("name", l.name.as_str());
            arr.push(&d.to_variant());
        }
        arr
    }

    /// Convenience: whether `reset` has been called (an episode is live).
    #[func]
    fn is_ready(&self) -> bool {
        self.env.is_some()
    }

    /// SAVE the live session's progress to `dir` (the journal: seed + the ordered action sequence). Restored
    /// by [`load_session`](Self::load_session). Writes `dir/{seed.json,actions.ndjson}` only — it does NOT fold
    /// a hash on the LIVE env (that would draw `next_u64` and desync the stream); the determinism proof is that
    /// `replay(dir)` reproduces the live run. Returns `false` before `reset` or on an I/O error.
    #[func]
    fn save_session(&mut self, dir: GString) -> bool {
        if self.env.is_none() {
            godot_error!("LiveSim::save_session called before reset()");
            return false;
        }
        let env_config = harness::replay::EnvConfig {
            entity_count: self.entity_count,
            env: self.env_params, // persist the climate so the saved session replays under it (ADR-012)
        };
        match harness::replay::save_journal(dir.to_string(), &env_config, self.seed, &self.journal)
        {
            Ok(()) => true,
            Err(e) => {
                godot_error!("LiveSim::save_session failed: {e}");
                false
            }
        }
    }

    /// LOAD a saved session from `dir`: read the journal and restore the exact state by building a FRESH env
    /// (never reusing the live one — keeps the single stream clean) and replaying `reset(seed)` + the recorded
    /// actions deterministically (inv #3). Returns `{ok, generation, population, allele_freq, phenotype,
    /// actions}` (`ok=false` + `detail` on a read error). The journal is restored so a later save re-extends it.
    #[func]
    fn load_session(&mut self, dir: GString) -> VarDictionary {
        let (seed_json, actions) = match harness::replay::read_journal(dir.to_string()) {
            Ok(v) => v,
            Err(e) => {
                godot_error!("LiveSim::load_session failed: {e}");
                let mut d = VarDictionary::new();
                d.set("ok", false);
                d.set("detail", e.to_string());
                return d;
            }
        };
        self.entity_count = seed_json.entity_count;
        self.env_params = seed_json.env_params(); // restore the saved climate (ADR-012)
        let mut env = GeneSimEnv::new(self.entity_count);
        env.set_environment(self.env_params);
        env.reset(seed_json.seed);
        for action in &actions {
            let _ = env.step(action.clone());
        }
        self.seed = seed_json.seed;
        self.journal = actions;
        let obs = env.observe();
        self.env = Some(env);
        let mut d = observation_to_dict(&obs);
        d.set("ok", true);
        d.set("actions", self.journal.len() as i64);
        d
    }
}

impl LiveSim {
    /// Append `n` generations to the journal, COALESCING consecutive Advances — `Advance(a)+Advance(b)` is
    /// bit-identical to `Advance(a+b)` on the single stream, so the saved file stays O(edits) not O(generations)
    /// and the replayed hash is unchanged. The live env still steps tick-by-tick (this only records).
    fn journal_advance(&mut self, n: u64) {
        if n == 0 {
            return;
        }
        if let Some(Action::Advance(last)) = self.journal.last_mut() {
            *last += n;
        } else {
            self.journal.push(Action::Advance(n));
        }
    }
}

/// The current generation of a live env (for stamping an edit's outcome).
fn env_gen(env: &mut GeneSimEnv) -> i64 {
    env.observe().generation as i64
}

/// Build the GDScript-facing edit-outcome `Dictionary` (display only — the authoritative outcome is the core's).
fn edit_dict(applied: bool, detail: &str, generation: i64) -> VarDictionary {
    let mut d = VarDictionary::new();
    d.set("applied", applied);
    d.set("detail", detail);
    d.set("generation", generation);
    d
}

/// Build the GDScript-facing region-edit `Dictionary` — `edit_dict` plus a `covered` organism count.
fn region_dict(applied: bool, detail: &str, generation: i64, covered: u32) -> VarDictionary {
    let mut d = edit_dict(applied, detail, generation);
    d.set("covered", i64::from(covered));
    d
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
