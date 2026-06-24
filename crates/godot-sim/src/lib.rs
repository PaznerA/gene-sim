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

/// Resolve a species file `name` to an existing path, trying the process working dir first (dev / `run.sh`,
/// which runs from the repo root) then the directory beside the executable (shipped builds stage
/// `data/species/` next to the binary). Returns `None` if neither exists. This is what makes "RUN E. coli"
/// work in a packaged build, where the cwd is not the repo root.
fn resolve_species_path(name: &str) -> Option<std::path::PathBuf> {
    let rel = format!("data/species/{name}.json");
    let cwd = std::path::PathBuf::from(&rel);
    if cwd.is_file() {
        return Some(cwd);
    }
    let beside = std::env::current_exe().ok()?.parent()?.join(&rel);
    beside.is_file().then_some(beside)
}

/// `LiveSim` — the one Godot node the live-sim feature exposes (ADR-010).
///
/// A thin `RefCounted` wrapper over [`harness::GeneSimEnv`]. GDScript drives it with
/// `reset(seed)` → `step(n)` → `observe()` and reads `snapshot(w, h)` bytes (GSS5, parsed by the
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
    /// The species the next `reset` runs (ADR-017 "RUN E. coli"). `None` = the default plant; `Some` runs a
    /// loaded JSON `SpeciesSpec` (e.g. E. coli) through its per-species trait map. Set via `set_species`.
    species: Option<genome::spec::BuiltSpecies>,
    /// The MULTI-SPECIES ROSTER the next `reset` spawns (SP-2, ADR-020). An ordered `Vec` of
    /// `(BuiltSpecies, starting_count)` pairs the composer assembles via [`set_roster`](Self::set_roster). When
    /// non-empty it takes PRECEDENCE over `species` at `reset` (forwarded to `GeneSimEnv::set_roster`, which
    /// spawns the whole roster through one `reset_with_roster` — the single RNG seeded once, inv #3). Empty by
    /// default → the pinned single-species-plant path is never perturbed (hash-neutral). Pure config: GDScript
    /// hands inert JSON + int counts, the core builds every genome→phenotype (inv #2).
    roster: Vec<(genome::spec::BuiltSpecies, u32)>,
    /// The `entity_count` before a species' niche overrode it, so clearing the species (`set_species("")`)
    /// restores the player's count instead of leaving the microbe's stale.
    entity_count_before_species: Option<u32>,
    /// The CONTAINMENT knob + consortium config the **next** `reset` builds its immigration schedule under
    /// (ADR-019 S2/S3). Stored on the BINDING (not just the env) so `reset` — which builds a FRESH `GeneSimEnv`
    /// — re-applies it before the harness expands the schedule. `None` = Sealed/OFF (the default → empty
    /// schedule → hash-neutral). This is pure config FORWARDING (no biology): the level + `ConsortiumConfig` are
    /// handed verbatim to `harness::GeneSimEnv::set_containment`, which expands the journaled schedule in the
    /// core off the off-stream IMMG family. Set via [`set_containment`](Self::set_containment).
    containment: Option<(sim_core::ContainmentLevel, sim_core::ConsortiumConfig)>,
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
            species: None,
            roster: Vec::new(),
            entity_count_before_species: None,
            containment: None,
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

    /// Select the SPECIES the next `reset` runs (ADR-017 "RUN E. coli"). `name` is a file stem under
    /// `data/species/` (e.g. `"ecoli"` → `data/species/ecoli.json`); an EMPTY name clears back to the default
    /// plant. Loads + validates the JSON `SpeciesSpec` in the core (inv #2 — biology stays in Rust); returns
    /// `true` on success (`false` + a Godot error on a missing/invalid file). Call before `reset`.
    #[func]
    fn set_species(&mut self, name: GString) -> bool {
        let name = name.to_string();
        if name.is_empty() {
            // Clear back to the default plant, restoring the player's pre-species population.
            if let Some(prev) = self.entity_count_before_species.take() {
                self.entity_count = prev;
            }
            self.species = None;
            return true;
        }
        let Some(path) = resolve_species_path(&name) else {
            godot_error!(
                "LiveSim::set_species({name}): data/species/{name}.json not found (looked in the working dir and beside the executable)"
            );
            return false;
        };
        match harness::species::load_species_file(&path) {
            Ok(built) => {
                if built.entity_count > 0 {
                    self.entity_count_before_species
                        .get_or_insert(self.entity_count);
                    self.entity_count = built.entity_count;
                }
                self.species = Some(built);
                true
            }
            Err(e) => {
                godot_error!("LiveSim::set_species({name}): {e}");
                false
            }
        }
    }

    /// Select the species the next `reset` runs from its `SpeciesSpec` JSON TEXT (`res://` boundary, ADR-017):
    /// GDScript reads the bytes via `FileAccess(res://data/species/<stem>.json)` and passes the string; the core
    /// does zero file I/O (inv #2/#4). An EMPTY string clears back to the default plant (restoring the player's
    /// pre-species `entity_count`). Returns `true` on success (`false` + a `godot_error!` on invalid/un-buildable
    /// JSON). Call before `reset`. This is the renderer's loader; [`set_species`](Self::set_species) is kept for
    /// the harness CLI / exe-staged path (cwd-relative file lookup), so there are two byte sources, one biology
    /// path (both funnel through `harness::species::build_species_from_str`).
    #[func]
    fn set_species_json(&mut self, json: GString) -> bool {
        let json = json.to_string();
        if json.is_empty() {
            // Clear back to the default plant, restoring the player's pre-species population.
            if let Some(prev) = self.entity_count_before_species.take() {
                self.entity_count = prev;
            }
            self.species = None;
            return true;
        }
        match harness::species::build_species_from_str(&json) {
            Ok(built) => {
                if built.entity_count > 0 {
                    self.entity_count_before_species
                        .get_or_insert(self.entity_count);
                    self.entity_count = built.entity_count;
                }
                self.species = Some(built);
                true
            }
            Err(e) => {
                godot_error!("LiveSim::set_species_json: {e}");
                false
            }
        }
    }

    /// Set the MULTI-SPECIES ROSTER the next `reset` spawns (SP-2, ADR-020 — the composer boundary). `jsons` and
    /// `counts` are zipped POSITIONALLY (by index): each `SpeciesSpec` JSON text is built + validated through the
    /// SAME core path as [`set_species_json`](Self::set_species_json) / `register_contaminant_json`
    /// (`harness::species::build_species_from_str` — biology stays in Rust, inv #2/#4) and paired with
    /// `count.max(0)`. The ROW ORDER is load-bearing: it becomes the SpeciesId spawn order (a reorder is a
    /// different-but-deterministic run, inv #3). On ANY build error: a `godot_error!` + return `false` WITHOUT
    /// mutating the stored roster (graceful — `main.gd` can fall back to the default plant). On success the new
    /// roster replaces the old and `true` is returned. An EMPTY `jsons` array CLEARS the roster (== `clear_roster`).
    /// Does NOT touch `entity_count` (each row carries its own count). Call before `reset`.
    #[func]
    fn set_roster(&mut self, jsons: PackedStringArray, counts: PackedInt32Array) -> bool {
        if jsons.is_empty() {
            self.roster.clear();
            return true;
        }
        // Build EVERY entry into a staging Vec first so a failure leaves the stored roster untouched (graceful).
        let mut built: Vec<(genome::spec::BuiltSpecies, u32)> = Vec::with_capacity(jsons.len());
        for i in 0..jsons.len() {
            let json = jsons.get(i).map(|s| s.to_string()).unwrap_or_default();
            // A missing count for a given index falls back to 0 (a zero-count row is a no-op at spawn, not an error).
            let count = counts.get(i).unwrap_or(0).max(0) as u32;
            match harness::species::build_species_from_str(&json) {
                Ok(b) => built.push((b, count)),
                Err(e) => {
                    godot_error!("LiveSim::set_roster: entry {i} failed to build: {e}");
                    return false;
                }
            }
        }
        self.roster = built;
        true
    }

    /// Clear the multi-species roster (SP-2) — the next `reset` falls back to the `set_species` / default-plant
    /// precedence. Leaves `entity_count` untouched. Call before `reset`.
    #[func]
    fn clear_roster(&mut self) {
        self.roster.clear();
    }

    /// The active species key (`"ecoli-core"` | `"default"` | `""`), a pure read of already-loaded data (no
    /// biology — inv #2). The renderer can route presentation on this CORE key as the authoritative tiebreak.
    #[func]
    fn species_key(&self) -> GString {
        GString::from(self.species.as_ref().map(|b| b.key.as_str()).unwrap_or(""))
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
        // SP-2 (ADR-020): a composed ROSTER takes PRECEDENCE (roster > species > default plant), mirroring the
        // harness arm. Forwarded by clone; the harness maps each (built, count) to a RosterEntry and seeds the
        // single RNG once over the full population (inv #3). Empty by default → skipped → pinned path untouched.
        if !self.roster.is_empty() {
            env.set_roster(self.roster.clone());
        }
        if let Some(built) = &self.species {
            env.set_species(built.clone()); // ADR-017: run the selected species (e.g. E. coli) instead of plant
        }
        // ADR-019 S2/S3: re-apply the stored containment knob + consortium config so THIS fresh env expands the
        // SAME deterministic immigration schedule (the harness expands it inside `env.reset`, off the off-stream
        // IMMG family — zero SimRng draws). Pure config forwarding, no biology (inv #2). Sealed/None (the default)
        // → no call → empty schedule → hash-neutral (the pinned literal is untouched).
        if let Some((level, config)) = &self.containment {
            env.set_containment(*level, config.clone());
        }
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

    /// Observe EVERY species in the roster (the renderer's specimen view shows them all — ADR R3).
    ///
    /// Returns an Array of Dictionaries, one per species in `species_id` order, each
    /// `{species_id, name, key, role, phenotype: {trait_name: value, ...}}`. A pure read-only display
    /// projection delegating to [`harness::GeneSimEnv::observe_all`] → [`sim_core::Simulation::observe_all`]:
    /// it draws no RNG, mutates nothing, and is never folded into the determinism hash (invariant #2/#3).
    /// No biology is computed here — only data marshalling. Empty before `reset`.
    #[func]
    fn observe_species(&self) -> VarArray {
        let mut arr = VarArray::new();
        match self.env.as_ref() {
            Some(env) => {
                for obs in env.observe_all() {
                    arr.push(&species_observation_to_dict(&obs).to_variant());
                }
            }
            None => godot_error!("LiveSim::observe_species called before reset()"),
        }
        arr
    }

    /// The MEASURED per-generation FlowMatrix as `{s: int, j: PackedInt64Array}` (ADR-013 F4 — the relations
    /// heatmap contract `godot/relations_heatmap.gd` reads). `j` is flat row-major: `j[i*s + j_]` = NET joules
    /// that flowed FROM species `j_` INTO species `i` this generation (row-sum==0 by construction). Delegates
    /// to [`harness::GeneSimEnv::flow_matrix`] → [`sim_core::Simulation::flow_matrix`] — a pure read-only
    /// projection (no RNG, no mutation, no biology computed here, inv #2/#3). Empty (`s:0`) before `reset`.
    #[func]
    fn flow_matrix(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        match self.env.as_ref() {
            Some(env) => {
                let (s, flat) = env.flow_matrix();
                d.set("s", s as i64);
                // Packed arrays pass by-ref into a Dictionary; marshal through a Variant (the `.to_variant()`
                // pattern used elsewhere in this file for VarArray pushes).
                d.set("j", &PackedInt64Array::from(flat.as_slice()).to_variant());
            }
            None => {
                godot_error!("LiveSim::flow_matrix called before reset()");
                d.set("s", 0_i64);
                d.set("j", &PackedInt64Array::new().to_variant());
            }
        }
        d
    }

    /// The VIEW-ONLY per-species relations overlay (ADR-014 re-grounded): nearest-species similarity + guild
    /// clustering over the off-hash `species_signatures()` export. Returns
    /// `{s: int, guild_of: PackedInt32Array, nearest: Dictionary}`:
    /// * `guild_of[i]` — the guild id (lowest-member `SpeciesId`) of species `i`, single-link clustered at the
    ///   pinned [`relations_index::GUILD_THRESHOLD`];
    /// * `nearest[focal] = PackedInt32Array[sid0, dist0, sid1, dist1, …]` — each focal species' top-k nearest
    ///   by EXACT integer-L1 distance, ordered `(distance asc, sid asc)`.
    ///
    /// The k-NN / clustering runs HERE in the std-only `relations-index` boundary crate (downstream of the
    /// deterministic core, which never calls it). No biology/index math in GDScript — GDScript only colours
    /// with the finished ordered integer arrays. Read-only (inv #2/#3): `species_signatures()` is a pure
    /// off-hash projection, so this never perturbs the run. Empty (`s:0`) before `reset` or for an empty roster.
    #[func]
    fn species_relations(&self) -> VarDictionary {
        use relations_index::{GUILD_THRESHOLD, GuildIndex, InRustIndex, NearestIndex};
        let mut d = VarDictionary::new();
        match self.env.as_ref() {
            Some(env) => {
                let (s, dims, sigs, roles) = env.species_signatures();
                let idx = InRustIndex::index(s, dims, &sigs, &roles);
                d.set("s", s as i64);
                // Guild id per species (widen u16 → i32 for Godot int marshaling).
                let guilds: Vec<i32> = idx
                    .guilds(GUILD_THRESHOLD)
                    .iter()
                    .map(|&g| i32::from(g))
                    .collect();
                d.set(
                    "guild_of",
                    &PackedInt32Array::from(guilds.as_slice()).to_variant(),
                );
                // Top-k nearest per focal species, flattened as [sid, dist, sid, dist, …].
                let k = 3usize;
                let mut nearest = VarDictionary::new();
                for focal in 0..s {
                    let mut flat: Vec<i32> = Vec::new();
                    for n in idx.nearest(focal, k) {
                        flat.push(n.sid as i32);
                        flat.push(n.distance.min(i32::MAX as u32) as i32);
                    }
                    nearest.set(
                        focal as i64,
                        &PackedInt32Array::from(flat.as_slice()).to_variant(),
                    );
                }
                d.set("nearest", &nearest.to_variant());
            }
            None => {
                godot_error!("LiveSim::species_relations called before reset()");
                d.set("s", 0_i64);
                d.set("guild_of", &PackedInt32Array::new().to_variant());
                d.set("nearest", &VarDictionary::new().to_variant());
            }
        }
        d
    }

    /// Produce the read-only GSS5 snapshot bytes for a `w × h` grid (parsed by `godot/snapshot.gd`).
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

    /// Apply a CRISPR edit to the **chosen species'** genome live (P4 / R6.1) and return its outcome.
    ///
    /// `cas` = Cas-variant id, `target` = species-genome locus id, `guide` = the ACGT guide string, `species` =
    /// the target species ORDINAL (Variant-Lab A — picked in the CRISPR panel, default `0` = the resident
    /// primary). Builds a species-granular [`harness::EditAction`] (invariant #6 — no organism handle) and steps
    /// it through the env's single seeded stream (invariant #3 — the edit draws only from that stream, exactly
    /// as the gym env does). The raw `species: i64` is CLAMPED to a `u16` ordinal and resolved to a `SpeciesId`
    /// at the env boundary — the SAME `species: u16 → SpeciesId` mapping the SP-3 `pcr_amplify` / `cull` tools
    /// use. Returns `{applied: bool, detail: String, generation: int}` — never a silent no-op (the core always
    /// yields an explicit Applied/Failed outcome). Authoritative PAM/score/gate logic stays in `crispr`
    /// (invariant #2): GDScript only assembles ids + a guide string + a species ordinal and reads the verdict.
    #[func]
    fn apply_edit(&mut self, cas: i64, target: i64, guide: GString, species: i64) -> VarDictionary {
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
            // Variant-Lab A: the CRISPR panel's target-species picker. Clamp to u16 like pcr_amplify/cull; the
            // env resolves it to a SpeciesId (default 0 = the resident primary when the picker has no selection).
            species: species.clamp(0, i64::from(u16::MAX)) as u16,
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
            species: 0, // the resident primary (region edits target the resident; per-species picker is a later UI slice)
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

    /// Register a CONTAMINANT species from its `SpeciesSpec` JSON TEXT (`res://` boundary, ADR-019 S1): the
    /// renderer reads the bytes via `FileAccess(res://data/species/<key>.json)` and passes the string; the core
    /// does zero file I/O (inv #2/#4). A subsequent [`inoculate`](Self::inoculate) (or a scheduled event) keyed
    /// on the built `key` resolves this genome. Returns `true` on success (`false` + a `godot_error!` on
    /// invalid/un-buildable JSON). Call before the inoculation that references it.
    #[func]
    fn register_contaminant_json(&mut self, json: GString) -> bool {
        let Some(env) = self.env.as_mut() else {
            godot_error!("LiveSim::register_contaminant_json called before reset()");
            return false;
        };
        match harness::species::build_species_from_str(&json.to_string()) {
            Ok(built) => {
                env.register_contaminant(built);
                true
            }
            Err(e) => {
                godot_error!("LiveSim::register_contaminant_json: {e}");
                false
            }
        }
    }

    /// Fire a CONTAMINATION / IMMIGRATION event (ADR-019 S1 — the SP-3-deferred seed/inoculate tool): spawn
    /// `count` organisms of the contaminant `species_key` (must be registered via
    /// [`register_contaminant_json`](Self::register_contaminant_json)) inside the disc `(cx, cy, radius)`, each
    /// endowed with `endow_j` joules MINTED from the `immigration` ledger tap (conserved). RNG-free, journaled
    /// for save/load (inv #3). Cell-scoped, no organism handle (inv #6); establish/displace/die emerges from
    /// the core economy — GDScript only issues the Action (inv #2). Returns the cumulative immigration-tap J.
    #[func]
    fn inoculate(
        &mut self,
        species_key: GString,
        cx: i64,
        cy: i64,
        radius: i64,
        count: i64,
        endow_j: i64,
    ) -> i64 {
        let Some(env) = self.env.as_mut() else {
            godot_error!("LiveSim::inoculate called before reset()");
            return 0;
        };
        let action = Action::RegionInoculate {
            species_key: species_key.to_string(),
            region: RegionSpec {
                cx: cx.max(0) as u32,
                cy: cy.max(0) as u32,
                radius: radius.max(0) as u32,
            },
            count: count.max(0) as u32,
            endow_j: endow_j.max(0),
        };
        env.step(action.clone());
        self.journal.push(action); // record for save/load (disjoint field borrow from `env`)
        self.env
            .as_ref()
            .map(|e| e.immigration_minted())
            .unwrap_or(0)
    }

    /// **SP-3 PCR-AMPLIFY** — spawn `count` FAITHFUL clones of an ALREADY-RESIDENT `species` ordinal inside the
    /// disc `(cx, cy, radius)`, each endowed with `endow_j` joules MINTED from the `intervention` ledger tap
    /// (conserved). Each clone copies its local template's heritable state VERBATIM (no mutation). RNG-free,
    /// journaled for save/load (inv #3). Cell-scoped, no organism handle (inv #6); biology in the core (inv #2)
    /// — GDScript only issues the Action + reads the verdict. Returns `{applied, detail, generation, covered}`
    /// (`covered` = the clones placed; `0` if the species has no in-region template).
    #[func]
    fn pcr_amplify(
        &mut self,
        species: i64,
        cx: i64,
        cy: i64,
        radius: i64,
        count: i64,
        endow_j: i64,
    ) -> VarDictionary {
        let Some(env) = self.env.as_mut() else {
            godot_error!("LiveSim::pcr_amplify called before reset()");
            return region_dict(false, "not reset", 0, 0);
        };
        let before = env.intervention_minted();
        let n = count.clamp(0, i64::from(u32::MAX)) as u32;
        let action = Action::RegionPcrAmplify {
            species: species.clamp(0, i64::from(u16::MAX)) as u16,
            region: RegionSpec {
                cx: cx.max(0) as u32,
                cy: cy.max(0) as u32,
                radius: radius.max(0) as u32,
            },
            count: n,
            endow_j: endow_j.max(0),
        };
        env.step(action.clone());
        self.journal.push(action); // record for save/load
        let env = self.env.as_mut().expect("env present");
        let minted = env.intervention_minted() - before;
        let cur_gen = env_gen(env);
        // A clone mints OFFSPRING from the tap iff a local template existed; minted==0 → no-op (no template).
        let covered = if endow_j > 0 {
            (minted / endow_j.max(1)) as u32
        } else {
            0
        };
        region_dict(
            covered > 0,
            &format!("PCR → {covered} clones · +{minted} J"),
            cur_gen,
            covered,
        )
    }

    /// **SP-3 ANTIBIOTIC CULL** — deterministically kill a `strength`-permille `[0,1000]` kill-fraction of the
    /// `species` ordinal's living orgs inside the disc `(cx, cy, radius)`; each culled org's residual J → detritus
    /// (carcass→detritus, conserved — no tap minted). RNG-free, journaled (inv #3). Cell-scoped (inv #6); biology
    /// in the core (inv #2). Returns `{applied, detail, generation, covered}` (`covered` = orgs killed).
    #[func]
    fn cull(
        &mut self,
        species: i64,
        cx: i64,
        cy: i64,
        radius: i64,
        strength: i64,
    ) -> VarDictionary {
        let Some(env) = self.env.as_mut() else {
            godot_error!("LiveSim::cull called before reset()");
            return region_dict(false, "not reset", 0, 0);
        };
        let before_pop = env.observe().population_size;
        let action = Action::RegionCull {
            species: species.clamp(0, i64::from(u16::MAX)) as u16,
            region: RegionSpec {
                cx: cx.max(0) as u32,
                cy: cy.max(0) as u32,
                radius: radius.max(0) as u32,
            },
            strength: strength.clamp(0, 1000) as u16,
        };
        env.step(action.clone());
        self.journal.push(action); // record for save/load
        let env = self.env.as_mut().expect("env present");
        let killed = before_pop.saturating_sub(env.observe().population_size);
        let cur_gen = env_gen(env);
        region_dict(
            killed > 0,
            &format!("Cull → {killed} killed → detritus"),
            cur_gen,
            killed,
        )
    }

    /// **SP-3 NUTRIENT FEED** — deposit `amount_j` joules into one pool plane (`channel`: `0` light · `1`
    /// free_nutrient · `2` detritus) across the disc `(cx, cy, radius)`, MINTED from the `intervention` ledger
    /// tap (conserved; `POOL_CAP` spill → overflow). Species-agnostic. RNG-free, journaled (inv #3). Cell-scoped
    /// (inv #6); biology in the core (inv #2). Returns `{applied, detail, generation, covered}` (`covered` = 0;
    /// `detail` reports the minted J).
    #[func]
    fn nutrient(
        &mut self,
        channel: i64,
        cx: i64,
        cy: i64,
        radius: i64,
        amount_j: i64,
    ) -> VarDictionary {
        let Some(env) = self.env.as_mut() else {
            godot_error!("LiveSim::nutrient called before reset()");
            return region_dict(false, "not reset", 0, 0);
        };
        let before = env.intervention_minted();
        let action = Action::RegionNutrient {
            channel: channel.clamp(0, 2) as u8,
            region: RegionSpec {
                cx: cx.max(0) as u32,
                cy: cy.max(0) as u32,
                radius: radius.max(0) as u32,
            },
            amount_j: amount_j.max(0),
        };
        env.step(action.clone());
        self.journal.push(action); // record for save/load
        let env = self.env.as_mut().expect("env present");
        let minted = env.intervention_minted() - before;
        let cur_gen = env_gen(env);
        region_dict(
            minted > 0,
            &format!("Nutrient ch{channel} → +{minted} J"),
            cur_gen,
            0,
        )
    }

    /// **SP-3 TOXIN SPIKE** — deposit `amount_milli` (== J 1:1) into one chem plane (`channel`: `0` toxin · `1`
    /// kin · `2` alarm) across the disc `(cx, cy, radius)`, MINTED from the `intervention` ledger tap (conserved;
    /// `CHEM_CAP` spill → overflow). RNG-free, journaled (inv #3). Cell-scoped (inv #6); biology in the core
    /// (inv #2). Returns `{applied, detail, generation, covered}` (`covered` = 0; `detail` reports the minted milli).
    #[func]
    fn toxin(
        &mut self,
        channel: i64,
        cx: i64,
        cy: i64,
        radius: i64,
        amount_milli: i64,
    ) -> VarDictionary {
        let Some(env) = self.env.as_mut() else {
            godot_error!("LiveSim::toxin called before reset()");
            return region_dict(false, "not reset", 0, 0);
        };
        let before = env.intervention_minted();
        let action = Action::RegionToxin {
            channel: channel.clamp(0, 2) as u8,
            region: RegionSpec {
                cx: cx.max(0) as u32,
                cy: cy.max(0) as u32,
                radius: radius.max(0) as u32,
            },
            amount_milli: amount_milli.max(0),
        };
        env.step(action.clone());
        self.journal.push(action); // record for save/load
        let env = self.env.as_mut().expect("env present");
        let minted = env.intervention_minted() - before;
        let cur_gen = env_gen(env);
        region_dict(
            minted > 0,
            &format!("Toxin ch{channel} → +{minted} milli"),
            cur_gen,
            0,
        )
    }

    /// Set the CONTAINMENT knob + consortium config the **next** `reset` builds its immigration schedule under
    /// (ADR-019 S2/S3). `level`: `0` Sealed (OFF, the default) · `1` Clean · `2` Lab · `3` Open. `species_keys`
    /// is the consortium menu (kebab keys the renderer also registers as contaminants); `radius`/`endow_j`/
    /// `horizon` are the pressure parameters. Hash-neutral while `level == 0` (empty schedule). Stores the config
    /// on the BINDING (so the next `reset` — which builds a fresh env — re-applies it) AND forwards it to the
    /// live env if one exists; the schedule expands deterministically at `reset` off the off-stream IMMG family
    /// (inv #3). Call it, then `reset()` to derive the schedule. Pure config storage — no biology (inv #2).
    #[func]
    fn set_containment(
        &mut self,
        level: i64,
        species_keys: PackedStringArray,
        radius: i64,
        endow_j: i64,
        horizon: i64,
    ) {
        let lvl = match level {
            1 => sim_core::ContainmentLevel::Clean,
            2 => sim_core::ContainmentLevel::Lab,
            3 => sim_core::ContainmentLevel::Open,
            _ => sim_core::ContainmentLevel::Sealed,
        };
        let keys: Vec<String> = species_keys
            .to_vec()
            .iter()
            .map(|s| s.to_string())
            .collect();
        let config = sim_core::ConsortiumConfig {
            species_keys: keys,
            radius: radius.max(0) as u32,
            endow_j: endow_j.max(0),
            horizon: horizon.max(0) as u32,
        };
        // Forward to a live env (no-op effect until the next reset re-expands the schedule)...
        if let Some(env) = self.env.as_mut() {
            env.set_containment(lvl, config.clone());
        }
        // ...and persist on the binding so `reset`'s fresh GeneSimEnv re-applies it before the schedule expands.
        self.containment = Some((lvl, config));
    }

    /// Drain every scheduled immigration event whose epoch has passed at the CURRENT generation (ADR-019 S2),
    /// firing each as a journaled `RegionInoculate` (the schedule is a SOURCE of journaled actions, so a
    /// scheduled arrival is byte-identical to a hand-fired one and save/load reproduces it). The GDScript live
    /// loop calls this once per advance tick. Returns how many events fired this call.
    #[func]
    fn fire_due_inoculations(&mut self) -> i64 {
        let Some(env) = self.env.as_mut() else {
            godot_error!("LiveSim::fire_due_inoculations called before reset()");
            return 0;
        };
        let current_gen = env.generation();
        let due = env.drain_due_inoculations(current_gen + 1);
        let n = due.len() as i64;
        for action in due {
            let env = self.env.as_mut().expect("env present");
            env.step(action.clone());
            self.journal.push(action); // record for save/load
        }
        n
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

    /// The ACTIVE species-genome loci as `[{id, name, so_term, go_refs}, ...]` for the intervention UI's target
    /// picker AND the SP-4 codex inspect ontology join (ids + names only — no biology in GDScript) — the SELECTED
    /// species when one is set (e.g. E. coli's 136 real genes), else the default plant baseline. The picker must
    /// be repopulated from this after `set_species`/`reset` so an edit targets the genome `apply_edit` actually
    /// resolves against (ADR-017).
    ///
    /// SP-4 (hash-neutral, PURELY ADDITIVE): `so_term` (the SO feature-type id) + `go_refs` (the ontology GO ids,
    /// in their stable genome order) are marshalled from the already-loaded `Genome` so the codex inspect can
    /// join each locus → `gene_for_go(go_refs[0])`. This is a READ-ONLY off-hash export exactly like
    /// `observe_species` / `flow_matrix`: the `{id, name}` fields and their order are UNCHANGED; it touches no
    /// selection / metabolism / RNG stream / `hash_world`.
    #[func]
    fn loci(&self) -> VarArray {
        let default = genome::sample_genome();
        let loci = match &self.species {
            Some(b) => &b.genome.loci,
            None => &default.loci,
        };
        let mut arr = VarArray::new();
        for l in loci {
            let mut d = VarDictionary::new();
            d.set("id", i64::from(l.id.0));
            d.set("name", l.name.as_str());
            // SP-4 ontology projection (additive): SO feature-type + GO refs in stable order.
            d.set("so_term", i64::from(l.tags.so_term.0));
            let mut go = VarArray::new();
            for g in &l.tags.go_refs {
                go.push(&i64::from(g.0).to_variant());
            }
            d.set("go_refs", &go.to_variant());
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
        let Some(env) = self.env.as_ref() else {
            godot_error!("LiveSim::save_session called before reset()");
            return false;
        };
        // ADR-019 R2: persist the FULL run composition, not just population + climate. Without the roster /
        // selected species / registered consortium / containment, a journaled RegionInoculate (or a multi-
        // species/non-default run) reloads against an EMPTY registry and diverges the hash. The roster /
        // species / containment ride on the binding; the consortium is read back from the live env.
        let env_config = harness::replay::EnvConfig {
            entity_count: self.entity_count,
            env: self.env_params, // persist the climate so the saved session replays under it (ADR-012)
            roster: self.roster.clone(),
            species: self.species.clone(),
            consortium: env.registered_consortium().to_vec(),
            containment: self.containment.clone(),
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
        // ADR-019 R2: rebuild the FULL run composition (roster / selected species / registered consortium /
        // containment) from the saved seed.json, not just population + climate. WITHOUT re-applying these, a
        // journaled RegionInoculate resolves against an empty registry and spawns nothing on replay (it DID
        // spawn live) → the reloaded run diverges from the live one. A pre-R2 save (no new fields) rebuilds the
        // historical single-species EnvConfig, so an old session.json still loads (serde-default).
        let env_config = match seed_json.env_config() {
            Ok(c) => c,
            Err(e) => {
                godot_error!("LiveSim::load_session: corrupt species in save: {e}");
                let mut d = VarDictionary::new();
                d.set("ok", false);
                d.set("detail", e.to_string());
                return d;
            }
        };
        self.entity_count = env_config.entity_count;
        self.env_params = env_config.env; // restore the saved climate (ADR-012)
        let mut env = GeneSimEnv::new(self.entity_count);
        env.set_environment(self.env_params);
        // Re-apply the composition BEFORE reset, exactly as the live session did (so the replay registry matches).
        if !env_config.roster.is_empty() {
            env.set_roster(env_config.roster.clone());
        }
        if let Some(species) = &env_config.species {
            env.set_species(species.clone());
        }
        for built in &env_config.consortium {
            env.register_contaminant(built.clone());
        }
        if let Some((level, cfg)) = &env_config.containment {
            env.set_containment(*level, cfg.clone());
        }
        env.reset(seed_json.seed);
        for action in &actions {
            let _ = env.step(action.clone());
        }
        // Restore the binding's composition so a LATER save re-extends the SAME session faithfully (the roster /
        // species / containment ride on the binding; the consortium is read back from the env at save time).
        self.roster = env_config.roster;
        self.species = env_config.species;
        self.containment = env_config.containment;
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

/// Build the GSS5 snapshot bytes from the env's live `Simulation` (read-only — invariant #3).
///
/// [`harness::GeneSimEnv::snapshot`] delegates to [`sim_core::Simulation::snapshot`] (no RNG draw,
/// no mutation); [`sim_core::GridSnapshot::write_snapshot_bytes`] emits the exact GSS5 layout that
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

/// Convert a [`sim_core::SpeciesObservation`] into a GDScript-facing `Dictionary` (the specimen view's
/// per-species row). Keys: `species_id` (int), `name` (string), `key` (string — the renderer's glyph
/// tiebreak), `role` (string), `population_size` (int), `allele_freq` (float), `mean_fitness` (float — the
/// Vitals panel reader key), `mean_energy` (float — field-named alias, == `mean_fitness`), and `phenotype`
/// (nested `{trait_name: value}`). Pure data marshalling; no biology (invariant #2).
fn species_observation_to_dict(obs: &sim_core::SpeciesObservation) -> VarDictionary {
    let mut dict = VarDictionary::new();
    dict.set("species_id", i64::from(obs.species_id));
    dict.set("name", obs.name.as_str());
    dict.set("key", obs.key.as_str());
    // The role's Debug repr is presentation only (e.g. "Autotroph"/"Heterotroph") — no biology here.
    let role = format!("{:?}", obs.role);
    dict.set("role", role.as_str());
    // Per-species vitals (R3 widening): pure reads carried verbatim from the core's read-only projection.
    dict.set("population_size", i64::from(obs.population_size));
    dict.set("allele_freq", obs.allele_freq);
    // LOAD-BEARING key — the EXACT key the Vitals "Fitness" row reads (main.gd `_species_stat`). mean_energy
    // is already ENERGY_FULL-normalized to [0,1] in-core, the SAME scale snapshot()'s fitness channel uses.
    dict.set("mean_fitness", obs.mean_energy);
    // Field-named alias (matches the struct field + the main.gd doc); equals mean_fitness by construction.
    dict.set("mean_energy", obs.mean_energy);

    let mut pheno = VarDictionary::new();
    for (trait_, value) in &obs.phenotype.values {
        pheno.set(format!("{trait_:?}"), *value);
    }
    dict.set("phenotype", &pheno);
    dict
}

// A compile-time witness that `Simulation` is the type the env wraps (keeps the import meaningful and
// documents the binding boundary: we wrap the headless handle, we do not reimplement it).
#[allow(dead_code)]
fn _binds_simulation(_: &Simulation) {}
