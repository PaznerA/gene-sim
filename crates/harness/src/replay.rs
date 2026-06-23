//! Replay logs — the determinism contract artifact (SPEC §5, §6; slice S3.2).
//!
//! An **episode** is a [`reset`](crate::Env::reset)`(seed)` followed by an ordered sequence of
//! [`Action`]s; its result is the final [`sim_core::RunStats::hash`]. This module records that episode
//! to disk as two human-readable, git-friendly files and replays them bit-identically:
//!
//! - [`seed.json`](SeedJson) — the master `seed`, the [`sim_core::SimConfig`] fields the env uses
//!   (`generations` / `entity_count`), the pinned tool versions (rust / bevy_ecs / rand_chacha — the
//!   same strings the CLI writes), the harness version, and the number of actions.
//! - `actions.ndjson` — one JSON [`Action`] per line, in order.
//!
//! Replaying `seed + actions` on the **same build** reproduces the recorded hash EXACTLY — this *is* the
//! determinism contract (SPEC §5). It holds **by construction** (invariant #3): [`record_episode`] and
//! [`replay`] drive the identical `(seed, action-sequence)` through the identical deterministic
//! [`GeneSimEnv`] (which threads a single seeded `ChaCha8Rng`), so no new randomness is introduced and
//! the hash cannot differ. The `run_id` is derived from the config (no wall-clock), mirroring the CLI,
//! so the output path is itself reproducible.

use std::io;
use std::path::{Path, PathBuf};

use genome::spec::{BuiltSpecies, SpeciesSpec};
use serde::{Deserialize, Serialize};

use crate::{Action, Env, GeneSimEnv};
use sim_core::{ConsortiumConfig, ContainmentLevel, EnvParams};

/// Pinned tool versions recorded into [`SeedJson`] (invariant #7). These mirror the strings the CLI
/// writes (`crates/harness/src/main.rs`) so a logged episode records the same reproducibility metadata.
const RUST_VERSION: &str = "1.96.0";
const BEVY_ECS_VERSION: &str = "0.19";
const RAND_CHACHA_VERSION: &str = "0.10";

/// Pinned tool versions for the reference build (invariant #7; SPEC §6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Toolchain {
    /// Rust toolchain version.
    pub rust: String,
    /// `bevy_ecs` version (the sim core's ECS).
    pub bevy_ecs: String,
    /// `rand_chacha` version (the single seeded RNG, invariant #3).
    pub rand_chacha: String,
}

impl Default for Toolchain {
    fn default() -> Self {
        Self {
            rust: RUST_VERSION.to_string(),
            bevy_ecs: BEVY_ECS_VERSION.to_string(),
            rand_chacha: RAND_CHACHA_VERSION.to_string(),
        }
    }
}

/// A persisted [`BuiltSpecies`] (ADR-019 R2): a fully-lossless [`SpeciesSpec`] JSON DTO. `SpeciesSpec` is the
/// on-disk genome contract, so persisting it (via [`SpeciesSpec::from_built`]) and rebuilding it (via
/// `SpeciesSpec::build`) reconstructs the IDENTICAL `BuiltSpecies` — keys, endowments, genome, and the niche
/// (`entity_count`/`trophic_role`/`host_key`) the roster/consortium boundary reads. This is why a saved roster
/// or registered contaminant survives the replay/load boundary intact (the R2 fix).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SpeciesJson(SpeciesSpec);

impl SpeciesJson {
    /// Capture a built species losslessly for persistence.
    #[must_use]
    pub fn from_built(built: &BuiltSpecies) -> Self {
        Self(SpeciesSpec::from_built(built))
    }

    /// Rebuild the [`BuiltSpecies`] from the persisted spec (the inverse of [`from_built`](Self::from_built)).
    ///
    /// # Errors
    /// An [`io::Error`] of kind [`io::ErrorKind::InvalidData`] if the persisted spec fails to build (a
    /// tampered/corrupt save — e.g. a non-ACGT base or out-of-domain parameter).
    pub fn build(&self) -> io::Result<BuiltSpecies> {
        self.0
            .build()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

/// A persisted multi-species roster row (SP-2, ADR-020): a built species + its starting count. Re-applied via
/// [`GeneSimEnv::set_roster`] on replay so a multi-species run reloads to the SAME hash (the R2 fix).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RosterRowJson {
    /// The species spawned for this roster row.
    pub species: SpeciesJson,
    /// The starting population for this row (the per-species count, not the env fallback `entity_count`).
    pub count: u32,
}

/// A persisted containment setting (ADR-019 S2/S3): the [`ContainmentLevel`] (as a stable integer ladder
/// `0`=Sealed/`1`=Clean/`2`=Lab/`3`=Open — `sim-core` carries no serde) + the [`ConsortiumConfig`] fields.
/// Re-applied via [`GeneSimEnv::set_containment`] on replay so a contaminated run rebuilds the SAME journaled
/// immigration schedule (the R2 fix).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContainmentJson {
    /// Containment ladder ordinal: `0` Sealed (OFF) · `1` Clean · `2` Lab · `3` Open.
    pub level: u8,
    /// The consortium contaminant keys, in fixed order (the schedule's species index keys into this).
    pub species_keys: Vec<String>,
    /// Brush radius every scheduled event uses.
    pub radius: u32,
    /// Per-organism starting endowment every scheduled immigrant receives.
    pub endow_j: i64,
    /// Run horizon in generations the events are spread across.
    pub horizon: u32,
}

impl ContainmentJson {
    /// Capture the env's containment setting for persistence.
    #[must_use]
    fn from_setting(level: ContainmentLevel, config: &ConsortiumConfig) -> Self {
        let level = match level {
            ContainmentLevel::Sealed => 0,
            ContainmentLevel::Clean => 1,
            ContainmentLevel::Lab => 2,
            ContainmentLevel::Open => 3,
        };
        Self {
            level,
            species_keys: config.species_keys.clone(),
            radius: config.radius,
            endow_j: config.endow_j,
            horizon: config.horizon,
        }
    }

    /// Rebuild the `(ContainmentLevel, ConsortiumConfig)` pair (the inverse of
    /// [`from_setting`](Self::from_setting)). An unknown ordinal falls back to `Sealed` (OFF) — defensive
    /// against a corrupt save, never a panic.
    #[must_use]
    fn to_setting(&self) -> (ContainmentLevel, ConsortiumConfig) {
        let level = match self.level {
            1 => ContainmentLevel::Clean,
            2 => ContainmentLevel::Lab,
            3 => ContainmentLevel::Open,
            _ => ContainmentLevel::Sealed,
        };
        let config = ConsortiumConfig {
            species_keys: self.species_keys.clone(),
            radius: self.radius,
            endow_j: self.endow_j,
            horizon: self.horizon,
        };
        (level, config)
    }
}

/// The minimal env configuration an episode replays against: the population spawned at `reset`, plus the
/// roster / selected species / registered consortium / containment that a non-default run needs re-applied
/// BEFORE replaying the journal (ADR-019 R2 — without these a journaled `RegionInoculate` resolves against an
/// empty registry and spawns nothing, diverging the hash).
///
/// This is exactly what [`GeneSimEnv`] needs to rebuild an identical env; the per-episode `seed` is recorded
/// separately (it is the thing being replayed). The single-species-plant pinned config leaves all the
/// non-default fields empty/`None`, so it is byte-identical to before (hash-neutral).
#[derive(Debug, Clone, PartialEq)]
pub struct EnvConfig {
    /// Organisms spawned at each `reset` (the env's fallback `entity_count`).
    pub entity_count: u32,
    /// The player-set climate the run was built under (ADR-012 Phase E). Default = the neutral world.
    pub env: EnvParams,
    /// The multi-species roster (SP-2, ADR-020), re-applied via [`GeneSimEnv::set_roster`]. Empty = none.
    pub roster: Vec<(BuiltSpecies, u32)>,
    /// The selected single species (ADR-017), re-applied via [`GeneSimEnv::set_species`]. `None` = default plant.
    pub species: Option<BuiltSpecies>,
    /// The registered contaminant consortium (ADR-019 S1), re-applied via
    /// [`GeneSimEnv::register_contaminant`] so a journaled `RegionInoculate` resolves its key. Empty = none.
    pub consortium: Vec<BuiltSpecies>,
    /// The containment knob + consortium config (ADR-019 S2/S3), re-applied via
    /// [`GeneSimEnv::set_containment`]. `None` = Sealed/OFF (the default → empty schedule → hash-neutral).
    pub containment: Option<(ContainmentLevel, ConsortiumConfig)>,
}

impl Default for EnvConfig {
    fn default() -> Self {
        Self {
            entity_count: 1000,
            env: EnvParams::default(),
            roster: Vec::new(),
            species: None,
            consortium: Vec::new(),
            containment: None,
        }
    }
}

impl EnvConfig {
    /// A bare config carrying only the population + climate (the historical fields). For the single-species
    /// replay paths (tests / CLI) that never compose a roster, select a species, or register a consortium.
    #[must_use]
    pub fn bare(entity_count: u32, env: EnvParams) -> Self {
        Self {
            entity_count,
            env,
            ..Self::default()
        }
    }
}

/// The `seed.json` schema (SPEC §5): everything needed to reproduce an episode except the ordered
/// actions, which live in `actions.ndjson`.
///
/// `generations` is the [`sim_core::SimConfig::generations`] the env hands the core; the env advances
/// time via [`Action::Advance`], so this is recorded metadata (the env uses `0`) — kept for parity with
/// the CLI's `seed.json` and to fully pin the config that produced `hash`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeedJson {
    /// The master/episode seed handed to `reset` (invariant #3 — one seed drives the whole run).
    pub seed: u64,
    /// [`sim_core::SimConfig::generations`] metadata (the env advances via `Advance`; recorded as `0`).
    pub generations: u64,
    /// Organisms spawned at `reset` (`SimConfig::entity_count`).
    pub entity_count: u32,
    /// Number of actions in the companion `actions.ndjson` (sanity-checked on replay).
    pub action_count: usize,
    /// Climate the run was built under (ADR-012 Phase E). `#[serde(default)]` so pre-Phase-E saves (no env)
    /// still load as the neutral world.
    #[serde(default = "default_lat")]
    pub lat: f64,
    #[serde(default = "default_lon")]
    pub lon: f64,
    #[serde(default = "default_temp")]
    pub avg_temp: f64,
    #[serde(default)]
    pub season: i64,
    /// The multi-species ROSTER the run spawned (SP-2, ADR-020). `#[serde(default)]` so a pre-R2 save (no
    /// roster field) loads as an empty roster → the historical single-species behavior. Re-applied on replay
    /// via [`GeneSimEnv::set_roster`] BEFORE the journal replays, so a multi-species run reloads to the same
    /// hash (the R2 fix).
    #[serde(default)]
    pub roster: Vec<RosterRowJson>,
    /// The selected single SPECIES the run ran (ADR-017). `#[serde(default)]` → `None` for a pre-R2 save (the
    /// default plant). Re-applied on replay via [`GeneSimEnv::set_species`].
    #[serde(default)]
    pub species: Option<SpeciesJson>,
    /// The registered contaminant CONSORTIUM (ADR-019 S1) — the keys + genomes a journaled `RegionInoculate`
    /// resolves against. `#[serde(default)]` → empty for a pre-R2 save. Re-applied on replay via
    /// [`GeneSimEnv::register_contaminant`] BEFORE the journal replays — WITHOUT this a journaled inoculate
    /// resolves nothing and spawns nothing, diverging the hash (the core of the R2 break).
    #[serde(default)]
    pub consortium: Vec<SpeciesJson>,
    /// The CONTAINMENT knob + consortium config (ADR-019 S2/S3) the immigration schedule expands under.
    /// `#[serde(default)]` → `None` (Sealed/OFF) for a pre-R2 save. Re-applied on replay via
    /// [`GeneSimEnv::set_containment`].
    #[serde(default)]
    pub containment: Option<ContainmentJson>,
    /// Pinned tool versions for the reference build (invariant #7).
    pub toolchain: Toolchain,
    /// The harness crate version that produced this log.
    pub harness_version: String,
}

fn default_lat() -> f64 {
    EnvParams::default().lat
}
fn default_lon() -> f64 {
    EnvParams::default().lon
}
fn default_temp() -> f64 {
    EnvParams::default().avg_temp
}

impl SeedJson {
    /// Reconstruct the [`EnvParams`] from the persisted climate fields.
    #[must_use]
    pub fn env_params(&self) -> EnvParams {
        EnvParams {
            lat: self.lat,
            lon: self.lon,
            avg_temp: self.avg_temp,
            season: self.season,
        }
    }

    /// Build the `seed.json` payload from a `(seed, EnvConfig, action_count)` (the single construction path
    /// shared by [`record_episode`] and [`save_journal`], so the two on-disk producers stay in sync). Persists
    /// the roster / selected species / consortium / containment (ADR-019 R2) by capturing each `BuiltSpecies`
    /// as a lossless [`SpeciesJson`].
    #[must_use]
    fn from_config(seed: u64, env_config: &EnvConfig, action_count: usize) -> Self {
        SeedJson {
            seed,
            generations: 0,
            entity_count: env_config.entity_count,
            action_count,
            lat: env_config.env.lat,
            lon: env_config.env.lon,
            avg_temp: env_config.env.avg_temp,
            season: env_config.env.season,
            roster: env_config
                .roster
                .iter()
                .map(|(b, n)| RosterRowJson {
                    species: SpeciesJson::from_built(b),
                    count: *n,
                })
                .collect(),
            species: env_config.species.as_ref().map(SpeciesJson::from_built),
            consortium: env_config
                .consortium
                .iter()
                .map(SpeciesJson::from_built)
                .collect(),
            containment: env_config
                .containment
                .as_ref()
                .map(|(level, config)| ContainmentJson::from_setting(*level, config)),
            toolchain: Toolchain::default(),
            harness_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Rebuild the [`EnvConfig`] from the persisted fields (the inverse of [`from_config`](Self::from_config)):
    /// climate + the roster / selected species / consortium / containment that [`run_episode`] re-applies
    /// BEFORE replaying the journal (ADR-019 R2). A pre-R2 save (all the new fields defaulted) yields the
    /// historical single-species `EnvConfig` (hash-neutral).
    ///
    /// # Errors
    /// An [`io::Error`] of kind [`io::ErrorKind::InvalidData`] if any persisted species spec fails to rebuild
    /// (a corrupt/tampered save).
    pub fn env_config(&self) -> io::Result<EnvConfig> {
        let mut roster = Vec::with_capacity(self.roster.len());
        for row in &self.roster {
            roster.push((row.species.build()?, row.count));
        }
        let species = match &self.species {
            Some(s) => Some(s.build()?),
            None => None,
        };
        let mut consortium = Vec::with_capacity(self.consortium.len());
        for c in &self.consortium {
            consortium.push(c.build()?);
        }
        Ok(EnvConfig {
            entity_count: self.entity_count,
            env: self.env_params(),
            roster,
            species,
            consortium,
            containment: self.containment.as_ref().map(ContainmentJson::to_setting),
        })
    }
}

/// The outcome of recording an episode: the directory written and the final stats hash (SPEC §6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpisodeResult {
    /// `data/runs/<run_id>/` (or the caller-supplied root joined with `<run_id>`).
    pub dir: PathBuf,
    /// The final [`sim_core::RunStats::hash`] — the bit-identical replay artifact.
    pub hash: u64,
}

/// Standard file names under a run directory (SPEC §5).
const SEED_FILE: &str = "seed.json";
const ACTIONS_FILE: &str = "actions.ndjson";

/// A deterministic `run_id` for an episode — no wall-clock, so the path itself is reproducible
/// (mirrors the CLI's scheme in `main.rs`). `e` = entity_count, `s` = seed, `a` = action count.
#[must_use]
fn run_id(env: &EnvConfig, seed: u64, action_count: usize) -> String {
    format!("ep_s{seed}_e{}_a{action_count}", env.entity_count)
}

/// Run an episode (`reset(seed)` then each action in order) through a fresh [`GeneSimEnv`], returning the
/// final [`sim_core::RunStats::hash`]. Shared by [`record_episode`] and [`replay`] so both drive the
/// identical deterministic code path (invariant #3 — the hash is bit-identical by construction).
fn run_episode(env_config: &EnvConfig, seed: u64, actions: &[Action]) -> u64 {
    let mut env = GeneSimEnv::new(env_config.entity_count);
    env.set_environment(env_config.env); // ADR-012: replay under the recorded climate
                                         // ADR-019 R2: re-apply the run's composition BEFORE reset, EXACTLY as the live session did, so a journaled
                                         // RegionInoculate resolves its key against the SAME registry it did live and a multi-species/non-default run
                                         // reloads to the identical hash. Empty/None for the pinned single-species-plant config → these are all
                                         // no-ops → the historical reset is byte-identical (hash-neutral).
    if !env_config.roster.is_empty() {
        env.set_roster(env_config.roster.clone());
    }
    if let Some(species) = &env_config.species {
        env.set_species(species.clone());
    }
    for built in &env_config.consortium {
        env.register_contaminant(built.clone());
    }
    if let Some((level, config)) = &env_config.containment {
        env.set_containment(*level, config.clone());
    }
    env.reset(seed);
    for action in actions {
        env.step(action.clone());
    }
    env.run_stats().hash
}

/// Record an episode to `<out_dir>/<run_id>/{seed.json,actions.ndjson}` and return its final hash.
///
/// Runs `reset(seed)` + each `action` in order via a fresh [`GeneSimEnv`], writes the two replay files,
/// and returns the [`EpisodeResult`] (directory + final stats hash). The companion [`replay`] reads
/// those files and reproduces `hash` bit-for-bit on the same build (SPEC §5/§6).
///
/// `out_dir` is the **root** for run directories (e.g. `data/runs`); a deterministic `run_id`
/// subdirectory is created under it. Write logs only under `data/runs/` (gitignored) or a temp dir.
///
/// # Errors
/// Returns any I/O error from creating the directory or writing the two files. (Serialization of the
/// well-typed [`SeedJson`] / [`Action`] values is infallible in practice and is surfaced as an
/// [`io::Error`] of kind [`io::ErrorKind::InvalidData`] if it ever fails.)
pub fn record_episode(
    env_config: &EnvConfig,
    seed: u64,
    actions: &[Action],
    out_dir: impl AsRef<Path>,
) -> io::Result<EpisodeResult> {
    let dir = out_dir
        .as_ref()
        .join(run_id(env_config, seed, actions.len()));
    std::fs::create_dir_all(&dir)?;

    // seed.json — pretty-printed for human readability / git friendliness (SPEC §5).
    let seed_json = SeedJson::from_config(seed, env_config, actions.len());
    let seed_str = serde_json::to_string_pretty(&seed_json).map_err(to_io)?;
    std::fs::write(dir.join(SEED_FILE), format!("{seed_str}\n"))?;

    // actions.ndjson — one JSON Action per line, in order (SPEC §5).
    let mut ndjson = String::new();
    for action in actions {
        ndjson.push_str(&serde_json::to_string(action).map_err(to_io)?);
        ndjson.push('\n');
    }
    std::fs::write(dir.join(ACTIONS_FILE), ndjson)?;

    // The recorded hash is the episode result (re-run the same path once to fold in run_stats()).
    let hash = run_episode(env_config, seed, actions);

    Ok(EpisodeResult { dir, hash })
}

/// Replay a recorded episode from `dir`, returning the final [`sim_core::RunStats::hash`].
///
/// Reads `<dir>/seed.json` + `<dir>/actions.ndjson`, rebuilds the env from the recorded config, and
/// re-runs `reset(seed)` + the same actions in order. On the same build this returns the exact hash
/// [`record_episode`] returned (SPEC §6 — bit-identical by construction, invariant #3).
///
/// # Errors
/// Returns an [`io::Error`] if a file is missing, malformed (invalid JSON — including a malformed guide,
/// whose validation is preserved by [`crispr::GuideSequence`]'s deserialize), or if `actions.ndjson`'s
/// line count disagrees with `seed.json`'s `action_count`.
pub fn replay(dir: impl AsRef<Path>) -> io::Result<u64> {
    let (seed_json, actions) = read_journal(dir)?;
    // ADR-019 R2: rebuild the FULL composition (roster / species / consortium / containment), not just the
    // population + climate, so a contaminated/multi-species/non-default run replays to the recorded hash.
    let env_config = seed_json.env_config()?;
    Ok(run_episode(&env_config, seed_json.seed, &actions))
}

/// Read + parse a recorded journal from `dir` into its [`SeedJson`] + ordered [`Action`]s (the read half of
/// [`replay`], without re-running it). Used by the live save/load mechanic (ADR-011 S-G follow-up): a renderer
/// LOAD restores the exact session by `reset(seed)` + replaying these actions deterministically (inv #3).
///
/// # Errors
/// Missing/malformed `seed.json` or `actions.ndjson` (incl. a malformed guide), or an `action_count` mismatch.
pub fn read_journal(dir: impl AsRef<Path>) -> io::Result<(SeedJson, Vec<Action>)> {
    let dir = dir.as_ref();
    let seed_json: SeedJson =
        serde_json::from_str(&std::fs::read_to_string(dir.join(SEED_FILE))?).map_err(to_io)?;

    let actions_str = std::fs::read_to_string(dir.join(ACTIONS_FILE))?;
    let mut actions: Vec<Action> = Vec::new();
    for (i, line) in actions_str.lines().enumerate() {
        if line.trim().is_empty() {
            continue; // tolerate a trailing newline / blank lines
        }
        let action: Action = serde_json::from_str(line).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("actions.ndjson line {}: {e}", i + 1),
            )
        })?;
        actions.push(action);
    }

    // Sanity: the log is internally consistent (count in seed.json matches the ndjson lines).
    if actions.len() != seed_json.action_count {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "action_count mismatch: seed.json says {}, actions.ndjson has {}",
                seed_json.action_count,
                actions.len()
            ),
        ));
    }
    Ok((seed_json, actions))
}

/// Write a journal (`seed.json` + `actions.ndjson`) directly into `dir` (no `run_id` subdir, unlike
/// [`record_episode`]) — for a SAVE SLOT at a player-chosen path. Same on-disk format as `record_episode`, so
/// [`read_journal`] / [`replay`] restore it. The journal IS the saved progress (seed + the ordered action
/// sequence the live session drove); replaying it reproduces the exact state (inv #3).
///
/// # Errors
/// Any I/O error creating `dir` or writing the two files.
pub fn save_journal(
    dir: impl AsRef<Path>,
    env_config: &EnvConfig,
    seed: u64,
    actions: &[Action],
) -> io::Result<()> {
    let dir = dir.as_ref();
    std::fs::create_dir_all(dir)?;

    let seed_json = SeedJson::from_config(seed, env_config, actions.len());
    std::fs::write(
        dir.join(SEED_FILE),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&seed_json).map_err(to_io)?
        ),
    )?;

    let mut ndjson = String::new();
    for action in actions {
        ndjson.push_str(&serde_json::to_string(action).map_err(to_io)?);
        ndjson.push('\n');
    }
    std::fs::write(dir.join(ACTIONS_FILE), ndjson)?;
    Ok(())
}

/// Map a `serde_json` error into an [`io::Error`] so the public API surfaces a single error type.
fn to_io(e: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EditAction;
    use crispr::{default_cas_variants, CasVariantId, GuideSequence};
    use genome::LocusId;

    /// Look up a seed Cas variant id by name (the seed table is a build invariant).
    fn cas_id(name: &str) -> CasVariantId {
        default_cas_variants()
            .into_iter()
            .find(|v| v.name == name)
            .unwrap_or_else(|| panic!("seed table missing {name}"))
            .id
    }

    /// A unique temp directory for a test (no external tempfile crate; deterministic per-test name).
    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gene_sim_replay_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// A non-trivial mix of Advance + ApplyEdit actions (mirrors the S3.1 determinism test).
    fn sample_actions() -> Vec<Action> {
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
    }

    /// Load a baked species spec from `data/species/<stem>.json` (the byte-mover boundary; mirrors the lib.rs
    /// test helper). Used to compose a real multi-species roster for the R2 file-boundary tests.
    fn load_stem(stem: &str) -> BuiltSpecies {
        crate::species::load_species_file(format!(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/species/{}.json"),
            stem
        ))
        .unwrap_or_else(|e| panic!("{stem}.json loads: {e}"))
    }

    /// A synthetic CONTAMINANT [`BuiltSpecies`] (key `"contaminant"`, decomposer role) off the wired sample
    /// genome — the same shape the lib.rs ADR-019 tests use. A journaled `RegionInoculate` keyed on its
    /// `"contaminant"` key resolves THIS built once it is in the env's consortium.
    fn contaminant_built() -> BuiltSpecies {
        use genome::spec::SpeciesSpec;
        let mut spec =
            SpeciesSpec::from_genome(&genome::sample_genome(), "contaminant", "Contaminant");
        spec.niche.trophic_role = Some("decomposer".to_string());
        spec.build().expect("contaminant builds")
    }

    /// A journaled inoculate of the synthetic contaminant into a disc (mirrors `inoculate_action` in lib.rs).
    fn inoculate_action() -> Action {
        Action::RegionInoculate {
            species_key: "contaminant".to_string(),
            region: RegionSpec {
                cx: 16,
                cy: 16,
                radius: 6,
            },
            count: 10,
            endow_j: 800_000,
        }
    }

    use crate::RegionSpec;

    #[test]
    fn record_then_replay_contaminated_multi_species_is_bit_identical() {
        // ADR-019 R2 (the BLOCKER fix): a CONTAMINATED MULTI-SPECIES episode recorded to DISK and replayed
        // FROM DISK must reproduce the recorded hash bit-for-bit. This crosses the real file boundary (unlike the
        // lib.rs in-process test that manually re-registers in its run closure) — it would FAIL before the fix,
        // because `replay()` rebuilt a bare env with an EMPTY roster + EMPTY consortium, so the journaled
        // RegionInoculate resolved nothing and spawned nothing on replay (it DID spawn live), diverging the hash.
        let env_config = EnvConfig {
            entity_count: 200,
            roster: vec![(load_stem("default"), 400), (load_stem("ecoli"), 200)],
            consortium: vec![contaminant_built()],
            ..Default::default()
        };
        let actions = vec![
            Action::Advance(5),
            inoculate_action(), // resolves ONLY because the consortium is re-applied before the journal replays
            Action::Advance(10),
        ];
        let root = temp_dir("contaminated_multi");

        let recorded = record_episode(&env_config, 2024, &actions, &root).expect("record");
        let replayed = replay(&recorded.dir).expect("replay from disk");

        // The inoculation actually fired live (the contaminant is non-trivially present in the recorded run).
        let live_immig = {
            let mut env = GeneSimEnv::new(env_config.entity_count);
            for built in &env_config.consortium {
                env.register_contaminant(built.clone());
            }
            env.set_roster(env_config.roster.clone());
            env.reset(2024);
            env.step(actions[0].clone());
            env.step(actions[1].clone());
            env.immigration_minted()
        };
        assert!(
            live_immig > 0,
            "the contaminant must actually inoculate (mint immigration J) in the recorded run"
        );
        assert_eq!(
            replayed, recorded.hash,
            "a contaminated multi-species episode must replay from DISK to the recorded hash (ADR-019 R2)"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn save_then_reload_without_manual_reregister_matches_live() {
        // ADR-019 R2 (the save→reload variant): SAVE a contaminated session to disk, then RELOAD it by reading the
        // journal and replaying WITHOUT manually re-registering the consortium/roster — exactly what the godot-sim
        // `load_session` does. The reloaded hash must equal the LIVE run's hash. Pre-fix this diverged: the reload
        // path never re-applied the consortium, so the journaled inoculate was a clean no-op on reload.
        let env_config = EnvConfig {
            entity_count: 256,
            roster: vec![(load_stem("default"), 300), (load_stem("bdellovibrio"), 80)],
            consortium: vec![contaminant_built()],
            ..Default::default()
        };
        let actions = vec![Action::Advance(3), inoculate_action(), Action::Advance(7)];
        let dir = temp_dir("saveload_contaminated");

        // The LIVE reference hash (compose → reset → step the journal in process).
        let live = run_episode(&env_config, 99, &actions);

        // SAVE the journal (persisting the composition), then RELOAD by rebuilding the EnvConfig from seed.json
        // alone — NO caller re-registers anything (the save→reload-without-manual-reregister path).
        save_journal(&dir, &env_config, 99, &actions).expect("save");
        let (seed_json, read_back) = read_journal(&dir).expect("read journal");
        let reloaded_config = seed_json
            .env_config()
            .expect("rebuild EnvConfig from seed.json");
        let reloaded = run_episode(&reloaded_config, seed_json.seed, &read_back);

        assert_eq!(
            reloaded, live,
            "a reloaded contaminated session must match the live run WITHOUT manual re-registration (ADR-019 R2)"
        );
        // The rebuilt config carries the roster + consortium back faithfully (keys + endowments intact).
        assert_eq!(reloaded_config.roster.len(), 2, "roster restored");
        assert_eq!(reloaded_config.consortium.len(), 1, "consortium restored");
        assert_eq!(reloaded_config.consortium[0].key, "contaminant");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn pre_r2_seed_json_without_new_fields_still_loads() {
        // ADR-019 R2 back-compat: an OLD seed.json (written before the roster/species/consortium/containment
        // fields existed) must still deserialize via serde-default into the historical single-species EnvConfig,
        // so an existing save keeps loading and the pinned single-species path is byte-identical.
        let dir = temp_dir("pre_r2_compat");
        // A minimal pre-R2 seed.json: only the original fields, none of the R2 additions.
        let old_seed_json = r#"{
            "seed": 7,
            "generations": 0,
            "entity_count": 200,
            "action_count": 1,
            "toolchain": { "rust": "1.96.0", "bevy_ecs": "0.19", "rand_chacha": "0.10" },
            "harness_version": "0.1.0"
        }"#;
        std::fs::write(dir.join(SEED_FILE), old_seed_json).unwrap();
        std::fs::write(dir.join(ACTIONS_FILE), "{\"Advance\":5}\n").unwrap();

        let (sj, actions) = read_journal(&dir).expect("a pre-R2 seed.json must still parse");
        assert_eq!(sj.seed, 7);
        assert!(sj.roster.is_empty(), "no roster → serde-default empty");
        assert!(sj.species.is_none(), "no species → serde-default None");
        assert!(
            sj.consortium.is_empty(),
            "no consortium → serde-default empty"
        );
        assert!(
            sj.containment.is_none(),
            "no containment → serde-default None"
        );

        let cfg = sj.env_config().expect("rebuild config");
        assert_eq!(cfg.entity_count, 200);
        // It replays as a plain default-plant run (the historical behavior), equal to the direct path.
        let replayed = replay(&dir).expect("replay a pre-R2 save");
        assert_eq!(replayed, run_episode(&cfg, 7, &actions));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_journal_round_trips_and_replays() {
        // The live save/load mechanic (ADR-011 S-G): save_journal writes a journal that read_journal parses
        // back identically, and replaying the saved dir reproduces the exact hash of running the actions —
        // proving a LOAD restores the session deterministically. Includes a region edit (the brush).
        let dir = temp_dir("saveload");
        let env = EnvConfig {
            entity_count: 200,
            ..Default::default()
        };
        let mut actions = sample_actions();
        actions.push(Action::ApplyEditRegion(
            EditAction {
                cas: cas_id("SpCas9"),
                target: LocusId(0),
                guide: GuideSequence::new(*b"ACGTGGACGTTTTAGGCCGG").unwrap(),
            },
            crate::RegionSpec {
                cx: 16,
                cy: 16,
                radius: 6,
            },
        ));
        save_journal(&dir, &env, 7, &actions).unwrap();
        let (sj, read_back) = read_journal(&dir).unwrap();
        assert_eq!(sj.seed, 7);
        assert_eq!(
            read_back, actions,
            "round-trip must preserve the action sequence"
        );
        assert_eq!(
            replay(&dir).unwrap(),
            run_episode(&env, 7, &actions),
            "a loaded journal must replay to the same hash as the direct run"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_journal_round_trips_and_replays_sp3_interventions() {
        // SP-3: a journal carrying the four intervention Actions round-trips through save/read and replays to the
        // same hash as a direct run (additive serde; all RNG-free → replay-exact). The journal IS the source of
        // truth for the timeline markers (a deterministic projection of these ordered lines).
        let dir = temp_dir("sp3_saveload");
        let env = EnvConfig {
            entity_count: 300,
            ..Default::default()
        };
        let region = crate::RegionSpec {
            cx: 16,
            cy: 16,
            radius: 20,
        };
        let actions = vec![
            Action::Advance(5),
            Action::RegionPcrAmplify {
                species: 0,
                region,
                count: 8,
                endow_j: 700_000,
            },
            Action::Advance(3),
            Action::RegionCull {
                species: 0,
                region,
                strength: 300,
            },
            Action::RegionNutrient {
                channel: 2,
                region,
                amount_j: 4_000_000,
            },
            Action::RegionToxin {
                channel: 0,
                region,
                amount_milli: 2_000_000,
            },
            Action::Advance(4),
        ];
        save_journal(&dir, &env, 11, &actions).unwrap();
        let (sj, read_back) = read_journal(&dir).unwrap();
        assert_eq!(sj.seed, 11);
        assert_eq!(
            read_back, actions,
            "round-trip must preserve the SP-3 action sequence"
        );
        assert_eq!(
            replay(&dir).unwrap(),
            run_episode(&env, 11, &actions),
            "a loaded SP-3 journal must replay to the same hash as the direct run"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn record_then_replay_is_bit_identical() {
        // THE acceptance criterion (SPEC §10.5/§6): record an episode of mixed Advance + ApplyEdit
        // actions, then replay it, and assert the replayed stats hash == the recorded stats hash.
        let env_config = EnvConfig {
            entity_count: 300,
            ..Default::default()
        };
        let seed = 2024;
        let actions = sample_actions();
        let root = temp_dir("bit_identical");

        let recorded = record_episode(&env_config, seed, &actions, &root).expect("record");
        let replayed = replay(&recorded.dir).expect("replay");

        assert_eq!(
            replayed, recorded.hash,
            "replayed stats hash must be BIT-IDENTICAL to the recorded hash (inv. #3; SPEC §6)"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn record_writes_human_readable_seed_json_and_ndjson() {
        let env_config = EnvConfig {
            entity_count: 100,
            ..Default::default()
        };
        let actions = sample_actions();
        let root = temp_dir("artifacts");

        let recorded = record_episode(&env_config, 7, &actions, &root).expect("record");

        // seed.json round-trips into the typed schema with the pinned versions + action count.
        let seed_str = std::fs::read_to_string(recorded.dir.join(SEED_FILE)).unwrap();
        let seed_json: SeedJson = serde_json::from_str(&seed_str).unwrap();
        assert_eq!(seed_json.seed, 7);
        assert_eq!(seed_json.entity_count, 100);
        assert_eq!(seed_json.action_count, actions.len());
        assert_eq!(seed_json.toolchain, Toolchain::default());

        // actions.ndjson has exactly one JSON Action per line, in order.
        let ndjson = std::fs::read_to_string(recorded.dir.join(ACTIONS_FILE)).unwrap();
        let lines: Vec<&str> = ndjson.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(lines.len(), actions.len());
        for (line, action) in lines.iter().zip(&actions) {
            let parsed: Action = serde_json::from_str(line).unwrap();
            assert_eq!(&parsed, action);
        }

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn replay_is_repeatable_and_matches_a_direct_run() {
        // Replaying twice yields the same hash, and it equals a direct in-memory episode run.
        let env_config = EnvConfig {
            entity_count: 256,
            ..Default::default()
        };
        let seed = 99;
        let actions = sample_actions();
        let root = temp_dir("repeatable");

        let recorded = record_episode(&env_config, seed, &actions, &root).expect("record");
        let a = replay(&recorded.dir).expect("replay a");
        let b = replay(&recorded.dir).expect("replay b");
        let direct = run_episode(&env_config, seed, &actions);

        assert_eq!(a, b, "replay must be repeatable");
        assert_eq!(a, recorded.hash);
        assert_eq!(a, direct, "replay must match a direct in-memory run");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn malformed_guide_in_actions_ndjson_fails_to_deserialize() {
        // AC: a malformed guide in actions.ndjson fails to replay (GuideSequence validation preserved).
        let env_config = EnvConfig {
            entity_count: 50,
            ..Default::default()
        };
        let actions = vec![Action::Advance(5)];
        let root = temp_dir("malformed_guide");

        let recorded = record_episode(&env_config, 1, &actions, &root).expect("record");
        // Overwrite actions.ndjson with an ApplyEdit carrying a non-ACGT guide ("ACGX...").
        let bad = "{\"ApplyEdit\":{\"cas\":0,\"target\":0,\"guide\":\"ACGXACGTACGT\"}}\n";
        std::fs::write(recorded.dir.join(ACTIONS_FILE), bad).unwrap();
        // Keep seed.json's action_count consistent so we exercise the GUIDE validation, not the count.
        let seed_json = SeedJson {
            seed: 1,
            generations: 0,
            entity_count: 50,
            action_count: 1,
            lat: 0.0,
            lon: 0.0,
            avg_temp: 0.5,
            season: 0,
            roster: Vec::new(),
            species: None,
            consortium: Vec::new(),
            containment: None,
            toolchain: Toolchain::default(),
            harness_version: env!("CARGO_PKG_VERSION").to_string(),
        };
        std::fs::write(
            recorded.dir.join(SEED_FILE),
            serde_json::to_string_pretty(&seed_json).unwrap(),
        )
        .unwrap();

        let err = replay(&recorded.dir).expect_err("malformed guide must fail to deserialize");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(
            err.to_string().contains("invalid guide sequence"),
            "unexpected error: {err}"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn action_count_mismatch_is_rejected() {
        let env_config = EnvConfig {
            entity_count: 50,
            ..Default::default()
        };
        let actions = sample_actions();
        let root = temp_dir("count_mismatch");

        let recorded = record_episode(&env_config, 3, &actions, &root).expect("record");
        // Drop a line from actions.ndjson without updating seed.json's action_count.
        let ndjson = std::fs::read_to_string(recorded.dir.join(ACTIONS_FILE)).unwrap();
        let trimmed: String = ndjson.lines().skip(1).map(|l| format!("{l}\n")).collect();
        std::fs::write(recorded.dir.join(ACTIONS_FILE), trimmed).unwrap();

        let err = replay(&recorded.dir).expect_err("count mismatch must be rejected");
        assert!(
            err.to_string().contains("action_count mismatch"),
            "got: {err}"
        );

        std::fs::remove_dir_all(&root).ok();
    }
}
