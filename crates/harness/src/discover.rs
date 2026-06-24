//! D2a/D2b — the SEARCH RUNNER: the engine that turns the [`discovery::search`] data model into saved,
//! replay-verified [`Gem`](discovery::search::Gem)s (the emergent-run discovery harness: D2a random = ADR-024,
//! D2b evolutionary = ADR-025; the D0 scorer + D1 trace it builds on = ADR-023).
//!
//! ## What it does (the search loop)
//! [`discover`] is the meta-loop. For each `trial`:
//!   1. PROPOSE a [`SearchConfig`] from the meta-RNG ([`discovery::search::propose`] — std-only splitmix64, NOT
//!      the sim RNG; the pinned literal `0x47a0_3c8f_6701_f240` is untouched because the sim runs are unchanged
//!      pure functions of their config — inv #3).
//!   2. BUILD a [`GeneSimEnv`] from the config — `set_roster` (each `(key, count)` resolved through the SAME
//!      `data/species/<key>.json` boundary the menu/CLI uses, [`species::load_species_file`]), `set_environment`
//!      (temp/season), `set_containment` (the airborne immigration knob).
//!   3. CAPTURE an off-hash [`PerGenTrace`](discovery::trace::PerGenTrace) via [`capture::capture_trace`] and
//!      SCORE it ([`DefaultScorer`] → [`discovery::final_score`] vs the kept-gem fingerprints).
//!   4. CONSIDER it for the [`GemLibrary`] (deduped top-K by novelty-adjusted final score).
//!
//! After the search, for every KEPT gem it REBUILDS the `(seed, EnvConfig, journal)` and
//! `record_episode → assert replay() == recorded_hash` BEFORE writing the gem JSON; a gem that fails the
//! round-trip is DROPPED (logged), never written (the gem reproducibility contract, discovery-scorer-spec).
//!
//! ## Determinism (inv #3)
//! The SIM runs are pure functions of their `SearchConfig` (one master seed → all sub-seeds). The PROPOSAL
//! sampler is the META-RNG ([`propose`](discovery::search::propose)), a std-only splitmix over `(search_seed,
//! trial, field)` — it never touches `SimRng`. So a fixed `(search_seed, trials, keep, gens, species_dir)`
//! produces a byte-identical set of saved gems, and the search adds NO sim-path change → the pinned literal is
//! unmoved.

use std::io;
use std::path::Path;

use discovery::search::{
    caption, propose, propose_evolved, Gem, GemLibrary, SearchConfig, SearchSpace,
};
use discovery::{final_score, DefaultScorer};
use genome::spec::BuiltSpecies;
use sim_core::{ConsortiumConfig, ContainmentLevel, EnvParams};

use crate::capture::capture_trace;
use crate::replay::{record_episode, replay, EnvConfig};
use crate::species::load_species_file;
use crate::Action;

/// The pinned-build fingerprint stored on every gem (inv #7). Anchored to the determinism literal so a re-pin
/// (which moves the literal) self-invalidates stored scores — a gem carrying an OLD `build_id` must be
/// recomputed by replay before its score is trusted (discovery-scorer-spec gem-validity contract).
pub const BUILD_ID: &str = "ecology-d0@47a03c8f6701f240";

/// The fallback population spawned at `reset` for the env's non-roster bookkeeping. The roster's per-species
/// counts drive the actual spawn (a search config always proposes a non-empty roster), so this only feeds the
/// metadata `entity_count` folded into the run hash — fixed so a config's hash is a pure function of the config.
const DISCOVER_ENTITY_COUNT: u32 = 1000;

/// Build the replay [`EnvConfig`] for a proposed [`SearchConfig`]: resolve the roster keys through the
/// `data/species/<key>.json` boundary, map the temp/season knobs to [`EnvParams`], and map the containment
/// ordinal to a `(ContainmentLevel, ConsortiumConfig)` pair (Sealed → `None`, hash-neutral).
///
/// A roster entry with a zero count, or a key whose species file fails to load, is SKIPPED — so a config that
/// references an absent species degrades to the species it CAN resolve (never a panic). Returns `None` (in the
/// first tuple slot) if the resolved roster is empty (nothing to run); the second slot is the `(key, error)`
/// skip list. Public so the gem-replay boundary (renderer/CLI loading a saved gem) can rebuild the SAME env a
/// gem was scored under from its `SearchConfig` alone — the gem reproducibility contract.
#[must_use]
pub fn env_config_for(
    cfg: &SearchConfig,
    species_dir: &Path,
) -> (Option<EnvConfig>, Vec<(String, String)>) {
    let mut roster: Vec<(BuiltSpecies, u32)> = Vec::with_capacity(cfg.roster.len());
    let mut skipped: Vec<(String, String)> = Vec::new();
    // The consortium (loaded contaminant resolver) the containment schedule's keys resolve against. We load the
    // Mode-A airborne keys so a non-Sealed level can actually inoculate; an unresolved key is a logged skip.
    let mut consortium: Vec<BuiltSpecies> = Vec::new();

    for (key, count) in &cfg.roster {
        if *count == 0 {
            continue; // a zero-count axis contributes no organisms (the proposal allows count_lo == 0 in theory)
        }
        match load_species_file(species_dir.join(format!("{key}.json"))) {
            Ok(built) => roster.push((built, *count)),
            Err(e) => skipped.push((key.clone(), e.to_string())),
        }
    }
    if roster.is_empty() {
        return (None, skipped);
    }

    // Map the containment ordinal → (level, config). Sealed (0) → None (the hash-neutral default: empty
    // schedule, no events). A dirtier level arms the Mode-A airborne consortium so immigration actually fires.
    let containment = if cfg.containment_level == 0 {
        None
    } else {
        let level = match cfg.containment_level {
            1 => ContainmentLevel::Clean,
            2 => ContainmentLevel::Lab,
            _ => ContainmentLevel::Open,
        };
        let consortium_config = ConsortiumConfig::default_mode_a();
        // Pre-load the consortium keys so a scheduled RegionInoculate resolves a genome on replay (ADR-019 R2).
        for key in &consortium_config.species_keys {
            if consortium.iter().any(|b| &b.key == key) {
                continue;
            }
            match load_species_file(species_dir.join(format!("{key}.json"))) {
                Ok(built) => consortium.push(built),
                Err(e) => skipped.push((key.clone(), e.to_string())),
            }
        }
        Some((level, consortium_config))
    };

    let env = EnvParams {
        lat: 0.0,
        lon: 0.0,
        // temp_q is q16 permille (0..=1000 ↔ 0.0..=1.0); avg_temp is the normalized [0,1] climate knob.
        avg_temp: f64::from(cfg.temp_q) / 1000.0,
        season: i64::from(cfg.season),
    };

    let env_config = EnvConfig {
        entity_count: DISCOVER_ENTITY_COUNT,
        env,
        roster,
        species: None,
        consortium,
        containment,
    };
    (Some(env_config), skipped)
}

/// Capture + score one [`SearchConfig`] into a [`Gem`] (the per-trial scoring step). Runs the off-hash
/// [`capture_trace`] over `gens` generations of the freshly-built env (NO journaled actions — the search probes
/// the INITIAL-CONFIG space; mid-run edits are a later axis), scores it vs the already-kept fingerprints, and
/// packages the full integer signal set + the reproducible config into a gem.
fn score_config(
    cfg: &SearchConfig,
    env_config: &EnvConfig,
    gens: u32,
    saved_fps: &[[u16; discovery::FP_DIMS]],
) -> Gem {
    // Build the env from the config (roster + climate + containment), exactly as record_episode/replay rebuild it.
    let mut env = crate::GeneSimEnv::new(env_config.entity_count);
    env.set_environment(env_config.env);
    env.set_roster(env_config.roster.clone());
    for built in &env_config.consortium {
        env.register_contaminant(built.clone());
    }
    if let Some((level, config)) = &env_config.containment {
        env.set_containment(*level, config.clone());
    }

    // Off-hash capture of the pure-config run, then the D0 score + the save-time novelty multiplier vs the kept set.
    let trace = capture_trace(&mut env, cfg.master_seed, gens, &[]);
    let scorer = DefaultScorer::default();
    let scored = final_score(&scorer, &trace, saved_fps);
    let sv = scored.score;

    Gem {
        config: cfg.clone(),
        score: scored.final_score,
        quality: sv.quality,
        novelty: scored.novelty_bp.min(u64::from(u16::MAX)) as u16,
        breakdown: sv.breakdown,
        fingerprint: sv.fingerprint,
        recorded_hash: trace.recorded_hash,
        build_id: BUILD_ID.to_string(),
        caption: caption(&sv, cfg),
        gens: trace.g,
    }
}

/// The deterministic gem file name: `<final_score>-<master_seed>.json` (zero-padded score so a lexical listing
/// of `data/runs/gems/` is also a rank ordering). No wall-clock — the path is reproducible (mirrors the replay
/// run-id discipline).
#[must_use]
pub fn gem_file_name(gem: &Gem) -> String {
    format!("{:020}-{:016x}.json", gem.score, gem.config.master_seed)
}

/// Run the emergent-run RANDOM SEARCH and write the verified top-`keep` gems to `out_dir` (ADR-024 D2a).
///
/// For `trial` in `0..trials`: PROPOSE a [`SearchConfig`] from the [`SearchSpace::default`] Primordial anchor
/// via the meta-RNG, BUILD a [`GeneSimEnv`] from it (roster via the `data/species/<key>.json` boundary, climate,
/// containment), CAPTURE an off-hash trace over `gens` generations, SCORE it, and CONSIDER it for the deduped
/// top-K [`GemLibrary`]. After the search, for every KEPT gem rebuild the `(seed, EnvConfig, journal)` and
/// `record_episode → assert replay() == recorded_hash` BEFORE writing `<out_dir>/<final_score>-<seed>.json`; a
/// gem that fails the round-trip is DROPPED (logged to stderr), never written.
///
/// Returns the [`GemLibrary`] of gems that PASSED the round-trip and were written (so a dropped gem is absent
/// from the returned library too). Deterministic: same `(search_seed, trials, keep, gens, species_dir)` →
/// identical saved files + scores (the proposal is the only RNG and it is the meta-RNG; the sim runs are pure
/// functions of their configs — the pinned literal is untouched).
///
/// `species_dir` is the `data/species` root the roster keys resolve against (the filesystem boundary; the core
/// stays filesystem-free, inv #2). `out_dir` is created if absent; existing files with a colliding name are
/// overwritten (the name is a pure function of the gem).
///
/// # Errors
/// Returns an [`io::Error`] only for a failure to create `out_dir` or write a gem file. A per-config species
/// resolution failure or a per-gem round-trip failure is handled internally (skip / drop + log), never an error.
pub fn discover(
    search_seed: u64,
    trials: u64,
    keep: usize,
    gens: u32,
    species_dir: &Path,
    out_dir: &Path,
) -> io::Result<GemLibrary> {
    let space = SearchSpace::default();
    let mut lib = GemLibrary::new(keep);

    // --- SEARCH: propose → build → capture → score → consider, in trial order (deterministic) ---
    for trial in 0..trials {
        let cfg = propose(search_seed, trial, &space);
        capture_and_consider(&cfg, species_dir, gens, &mut lib, "trial", trial);
    }

    verify_and_write_library(&lib, keep, species_dir, out_dir)
}

/// Build, capture, score, and CONSIDER one [`SearchConfig`] into `lib` — the shared per-config step both the
/// random ([`discover`]) and evolutionary ([`discover_evolved`]) loops use. Resolves the roster through the
/// `data/species` boundary (a skip/empty roster is LOGGED + dropped, never a panic), scores against `lib`'s
/// CURRENTLY-kept fingerprints (the save-time novelty multiplier), and folds the gem in. `phase`/`step` only
/// flavour the log line. Returns `true` iff a gem was produced (the config resolved to a non-empty roster).
fn capture_and_consider(
    cfg: &SearchConfig,
    species_dir: &Path,
    gens: u32,
    lib: &mut GemLibrary,
    phase: &str,
    step: u64,
) -> bool {
    let (env_config, skipped) = env_config_for(cfg, species_dir);
    for (key, err) in &skipped {
        eprintln!("discover: {phase} {step}: skipped species {key:?} ({err})");
    }
    let Some(env_config) = env_config else {
        eprintln!("discover: {phase} {step}: empty resolved roster — skipped");
        return false;
    };
    let gem = score_config(cfg, &env_config, gens, &lib.fingerprints());
    lib.consider(gem);
    true
}

/// VERIFY + WRITE the kept gems: each must round-trip (`record_episode → replay == recorded_hash`) before it is
/// written, so the on-disk library only ever holds reproducible gems; a gem that fails is DROPPED (logged),
/// never written. Returns the [`GemLibrary`] of the gems that PASSED and were written. Shared by [`discover`]
/// and [`discover_evolved`] so the round-trip contract has ONE implementation.
fn verify_and_write_library(
    lib: &GemLibrary,
    keep: usize,
    species_dir: &Path,
    out_dir: &Path,
) -> io::Result<GemLibrary> {
    std::fs::create_dir_all(out_dir)?;
    let mut verified = GemLibrary::new(keep);
    for gem in &lib.gems {
        let (env_config, _skipped) = env_config_for(&gem.config, species_dir);
        let Some(env_config) = env_config else {
            eprintln!(
                "discover: dropping gem (seed {:016x}): roster no longer resolves",
                gem.config.master_seed
            );
            continue;
        };
        // The gem's journal is a SINGLE Advance over the generations the capture actually ran (the search probes
        // the INITIAL-CONFIG space — no mid-run edits this slice). `capture_trace` drives `Advance(1)*g` which is
        // byte-identical to one `Advance(g)` (one seeded stream, no re-seed — proven in tests/trace_capture.rs),
        // and the capture EARLY-STOPS at `gem.gens` (== `trace.g`), so this reproduces the captured run exactly.
        let journal: Vec<Action> = vec![Action::Advance(u64::from(gem.gens))];

        // Round-trip the gem through the on-disk record/replay contract into a TEMP subdir, then compare.
        let stage = out_dir.join(format!(".verify-{:016x}", gem.config.master_seed));
        let _ = std::fs::remove_dir_all(&stage);
        let recorded = match record_episode(&env_config, gem.config.master_seed, &journal, &stage) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "discover: dropping gem (seed {:016x}): record failed ({e})",
                    gem.config.master_seed
                );
                let _ = std::fs::remove_dir_all(&stage);
                continue;
            }
        };
        let replayed = match replay(&recorded.dir) {
            Ok(h) => h,
            Err(e) => {
                eprintln!(
                    "discover: dropping gem (seed {:016x}): replay failed ({e})",
                    gem.config.master_seed
                );
                let _ = std::fs::remove_dir_all(&stage);
                continue;
            }
        };
        let _ = std::fs::remove_dir_all(&stage);

        if replayed != recorded.hash || recorded.hash != gem.recorded_hash {
            eprintln!(
                "discover: dropping gem (seed {:016x}): round-trip mismatch (recorded {:016x}, replay {:016x}, gem {:016x})",
                gem.config.master_seed, recorded.hash, replayed, gem.recorded_hash
            );
            continue;
        }

        // The gem reproduces — write it (pretty JSON, git-friendly), keyed by <final_score>-<seed>.
        let path = out_dir.join(gem_file_name(gem));
        let json = serde_json::to_string_pretty(gem)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        std::fs::write(&path, format!("{json}\n"))?;
        verified.consider(gem.clone());
    }

    Ok(verified)
}

/// The fraction of each generation's population that is FRESH RANDOM exploration (vs evolved from the kept
/// parents), in basis points — `2_500` ≈ 1/4 random, 3/4 evolved. Keeps the search from collapsing onto the
/// current pool (a deterministic, seeded explore/exploit split — NOT an RNG coin; the index threshold below).
const EVOLVE_EXPLORE_BP: u64 = 2_500;
/// The basis-point denominator for [`EVOLVE_EXPLORE_BP`] (== `discovery::fixed::SCALE`, kept local so this
/// crate stays free of a fixed-point import for one constant). `10_000` bp = 100%.
const BP_SCALE: u64 = 10_000;

/// Run the EVOLUTIONARY emergent-run SEARCH (D2b STAGE 2) and write the verified top-`keep` gems to `out_dir`.
///
/// GENERATION 0 proposes `pop_size` RANDOM configs (the D2a [`propose`]), builds/captures/scores each, and
/// folds them into a running [`GemLibrary`] (the kept gems are the PARENTS). For each subsequent generation
/// (`1..generations`) it proposes `pop_size` NEW configs: a leading EXPLORE fraction ([`EVOLVE_EXPLORE_BP`])
/// is fresh [`propose`] (so the search never collapses onto the pool), the rest are
/// [`propose_evolved`](discovery::search::propose_evolved) of the CURRENT kept gems' configs (mutate/crossover
/// of the parents). Every individual is built/captured/scored and folded in; the library carries the elites
/// forward (elitist — a strong parent survives until beaten). After all generations the kept gems are
/// round-trip-verified and written (the UNCHANGED [`verify_and_write_library`] contract — a gem that fails the
/// `record_episode → replay == recorded_hash` check is DROPPED, never written).
///
/// `generations == 0` reduces to a single random generation of `pop_size` trials — i.e. exactly the D2a
/// [`discover`] behaviour with `trials == pop_size` (the non-evolutionary base case).
///
/// ## Determinism (inv #3)
/// Every proposal/operator draw is the META-RNG (splitmix over `(search_seed, step, field)` — see
/// [`discovery::search`]); the per-generation `step` is a fixed function of `(generation, individual)`
/// (`generation * pop_size + i`), so a fixed `(search_seed, pop_size, generations, keep, gens, species_dir)`
/// produces a byte-identical set of saved gems. The sim runs are pure functions of their configs — the search
/// adds NO sim-path change, so the pinned literal `0x47a0_3c8f_6701_f240` is untouched.
///
/// # Errors
/// Returns an [`io::Error`] only for a failure to create `out_dir` or write a gem file (mirrors [`discover`]).
pub fn discover_evolved(
    search_seed: u64,
    pop_size: u64,
    generations: u64,
    keep: usize,
    gens: u32,
    species_dir: &Path,
    out_dir: &Path,
) -> io::Result<GemLibrary> {
    let space = SearchSpace::default();
    let mut lib = GemLibrary::new(keep);

    // The explore cut: the leading `explore` individuals of every post-0 generation are fresh random proposals,
    // the rest are evolved from the parents. At least 1 explorer per generation (a degenerate pop_size still
    // injects fresh blood); never more than the whole population.
    let explore = ((pop_size * EVOLVE_EXPLORE_BP / BP_SCALE).max(1)).min(pop_size);

    // The total number of individuals (== meta-RNG steps) is `pop_size * (generations + 1)`: one random
    // generation 0 plus `generations` evolved generations. `step` is monotonic across the whole run so no two
    // individuals share a proposal stream coordinate.
    let total_generations = generations + 1;
    for generation in 0..total_generations {
        // The PARENTS for this generation are the CURRENTLY-kept gems' configs (the elites). Snapshotted before
        // proposing this generation's children so the pool is stable within the generation (deterministic).
        let parents: Vec<SearchConfig> = lib.gems.iter().map(|g| g.config.clone()).collect();

        for i in 0..pop_size {
            // A globally-monotonic meta-RNG step so every individual draws an independent proposal stream.
            let step = generation * pop_size + i;
            let cfg = if generation == 0 || i < explore {
                // GENERATION 0 (cold start) + the per-generation EXPLORE fraction → fresh random proposal.
                propose(search_seed, step, &space)
            } else {
                // The EXPLOIT fraction → mutate/crossover of the current elite pool (empty pool → cold propose,
                // handled inside propose_evolved). Drawn off the evolve stream salt (disjoint from propose).
                propose_evolved(&parents, search_seed, step, &space)
            };
            capture_and_consider(&cfg, species_dir, gens, &mut lib, "gen", generation);
        }
    }

    verify_and_write_library(&lib, keep, species_dir, out_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The repo-root `data/species` dir (the byte-mover boundary; mirrors the species/replay test helpers).
    fn species_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/species"))
    }

    #[test]
    fn env_config_maps_roster_climate_containment() {
        // A proposed config resolves its roster keys through the data dir and maps temp/season/containment.
        let cfg = SearchConfig {
            master_seed: 7,
            roster: vec![("default".to_string(), 400), ("ecoli".to_string(), 200)],
            containment_level: 2, // Lab → a (level, config) pair, Mode-A consortium pre-loaded
            temp_q: 600,
            season: 1,
        };
        let (env_config, skipped) = env_config_for(&cfg, &species_dir());
        let env_config = env_config.expect("roster resolves");
        assert!(skipped.is_empty(), "all keys resolve: {skipped:?}");
        assert_eq!(env_config.roster.len(), 2);
        assert!(
            (env_config.env.avg_temp - 0.6).abs() < 1e-9,
            "temp_q 600 → 0.6"
        );
        assert_eq!(env_config.env.season, 1);
        assert!(
            matches!(env_config.containment, Some((ContainmentLevel::Lab, _))),
            "containment ordinal 2 → Lab"
        );
        assert!(
            !env_config.consortium.is_empty(),
            "a non-Sealed level pre-loads the Mode-A consortium so immigration resolves"
        );
        // Sealed (0) → None (hash-neutral).
        let sealed = SearchConfig {
            containment_level: 0,
            ..cfg
        };
        let (sealed_cfg, _) = env_config_for(&sealed, &species_dir());
        assert!(
            sealed_cfg.unwrap().containment.is_none(),
            "Sealed → no containment"
        );
    }
}
