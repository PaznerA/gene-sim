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

use serde::{Deserialize, Serialize};

use crate::{Action, Env, GeneSimEnv};

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

/// The minimal env configuration an episode replays against: the population spawned at `reset`.
///
/// This is exactly what [`GeneSimEnv::new`] needs to rebuild an identical env; the per-episode `seed`
/// is recorded separately (it is the thing being replayed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvConfig {
    /// Organisms spawned at each `reset` (the env's `entity_count`).
    pub entity_count: u32,
}

/// The `seed.json` schema (SPEC §5): everything needed to reproduce an episode except the ordered
/// actions, which live in `actions.ndjson`.
///
/// `generations` is the [`sim_core::SimConfig::generations`] the env hands the core; the env advances
/// time via [`Action::Advance`], so this is recorded metadata (the env uses `0`) — kept for parity with
/// the CLI's `seed.json` and to fully pin the config that produced `hash`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedJson {
    /// The master/episode seed handed to `reset` (invariant #3 — one seed drives the whole run).
    pub seed: u64,
    /// [`sim_core::SimConfig::generations`] metadata (the env advances via `Advance`; recorded as `0`).
    pub generations: u64,
    /// Organisms spawned at `reset` (`SimConfig::entity_count`).
    pub entity_count: u32,
    /// Number of actions in the companion `actions.ndjson` (sanity-checked on replay).
    pub action_count: usize,
    /// Pinned tool versions for the reference build (invariant #7).
    pub toolchain: Toolchain,
    /// The harness crate version that produced this log.
    pub harness_version: String,
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
    let seed_json = SeedJson {
        seed,
        generations: 0,
        entity_count: env_config.entity_count,
        action_count: actions.len(),
        toolchain: Toolchain::default(),
        harness_version: env!("CARGO_PKG_VERSION").to_string(),
    };
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
    let env_config = EnvConfig {
        entity_count: seed_json.entity_count,
    };
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

    let seed_json = SeedJson {
        seed,
        generations: 0,
        entity_count: env_config.entity_count,
        action_count: actions.len(),
        toolchain: Toolchain::default(),
        harness_version: env!("CARGO_PKG_VERSION").to_string(),
    };
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

    #[test]
    fn save_journal_round_trips_and_replays() {
        // The live save/load mechanic (ADR-011 S-G): save_journal writes a journal that read_journal parses
        // back identically, and replaying the saved dir reproduces the exact hash of running the actions —
        // proving a LOAD restores the session deterministically. Includes a region edit (the brush).
        let dir = temp_dir("saveload");
        let env = EnvConfig { entity_count: 200 };
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
    fn record_then_replay_is_bit_identical() {
        // THE acceptance criterion (SPEC §10.5/§6): record an episode of mixed Advance + ApplyEdit
        // actions, then replay it, and assert the replayed stats hash == the recorded stats hash.
        let env_config = EnvConfig { entity_count: 300 };
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
        let env_config = EnvConfig { entity_count: 100 };
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
        let env_config = EnvConfig { entity_count: 256 };
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
        let env_config = EnvConfig { entity_count: 50 };
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
        let env_config = EnvConfig { entity_count: 50 };
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
