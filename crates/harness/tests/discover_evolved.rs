//! D2b — the EVOLUTIONARY search runner integration tests (ADR-025): the `discover_evolved()`
//! generational loop grounded on the REAL headless core + the real `data/species` boundary.
//!
//! Load-bearing properties (the slice acceptance criteria):
//!  1. DETERMINISM (inv #3): same `search_seed` → byte-identical saved gem files + scores.
//!  2. GEM VALIDITY: every saved gem ROUND-TRIPS — `replay(gem.config) == gem.recorded_hash`.
//!  3. DIVERSITY (the whole point of D2b): on the WIDENED space the evolutionary loop keeps MORE distinct
//!     gems (novelty-deduped) than a same-budget pure-random D2a run.
//!  4. NO REGRESSION: the evolutionary best score is >= the random best (evolution does not regress).
//!  5. THE PINNED LITERAL `0x47a0_3c8f_6701_f240` is STILL produced by the normal pinned config — the search
//!     added NO sim-path change (the proposal/operator sampler is the meta-RNG, never `SimRng`).

use std::path::{Path, PathBuf};

use discovery::search::Gem;
use harness::capture::capture_trace;
use harness::discover::{discover, discover_evolved, env_config_for};
use harness::GeneSimEnv;
use sim_core::SimConfig;

/// The repo-root `data/species` dir (the byte-mover boundary; mirrors the discover/replay test helpers).
fn species_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/species"))
}

/// A unique temp output dir for a test run (no external tempfile crate; deterministic per-test name + pid).
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "gene_sim_discover_evo_it_{tag}_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Read every saved gem JSON from an output dir into `(file_name, Gem)` pairs, sorted by file name (the
/// score-then-seed rank ordering — `<padded score>-<hex seed>.json`).
fn read_saved_gems(dir: &Path) -> Vec<(String, Gem)> {
    let mut out: Vec<(String, Gem)> = Vec::new();
    for entry in std::fs::read_dir(dir).expect("read out dir") {
        let entry = entry.expect("dir entry");
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".json") {
            continue; // skip any stray staging dir / non-gem file
        }
        let text = std::fs::read_to_string(entry.path()).expect("read gem json");
        let gem: Gem = serde_json::from_str(&text).expect("parse gem json");
        out.push((name, gem));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The set of DISTINCT roster shapes (present-species keys) over a gem set — the diversity measure D2b targets.
fn distinct_shapes(gems: &[Gem]) -> usize {
    let mut shapes: Vec<Vec<String>> = Vec::new();
    for g in gems {
        let shape: Vec<String> = g
            .config
            .roster
            .iter()
            .filter(|(_, c)| *c > 0)
            .map(|(k, _)| k.clone())
            .collect();
        if !shapes.contains(&shape) {
            shapes.push(shape);
        }
    }
    shapes.len()
}

#[test]
fn discover_evolved_is_deterministic_same_search_seed() {
    // (1) DETERMINISM: two evolutionary runs of the same (search_seed, pop, gens, keep, gens-per-trial) into
    // distinct temp dirs produce byte-identical saved gem files (same names + same JSON bytes) and equal libs.
    let a_dir = temp_dir("det_a");
    let b_dir = temp_dir("det_b");

    let lib_a =
        discover_evolved(2024, 8, 3, 6, 60, &species_dir(), &a_dir, None).expect("evolve a");
    let lib_b =
        discover_evolved(2024, 8, 3, 6, 60, &species_dir(), &b_dir, None).expect("evolve b");

    assert_eq!(
        lib_a, lib_b,
        "same search_seed must yield an identical gem library"
    );

    let saved_a = read_saved_gems(&a_dir);
    let saved_b = read_saved_gems(&b_dir);
    assert!(!saved_a.is_empty(), "evolve must save at least one gem");
    assert_eq!(saved_a.len(), saved_b.len(), "same number of saved gems");
    for ((na, _), (nb, _)) in saved_a.iter().zip(&saved_b) {
        assert_eq!(na, nb, "gem file names must match across runs");
        let ta = std::fs::read_to_string(a_dir.join(na)).unwrap();
        let tb = std::fs::read_to_string(b_dir.join(nb)).unwrap();
        assert_eq!(ta, tb, "gem JSON bytes must be identical for {na}");
    }

    std::fs::remove_dir_all(&a_dir).ok();
    std::fs::remove_dir_all(&b_dir).ok();
}

#[test]
fn every_evolved_gem_round_trips() {
    // (2) GEM VALIDITY: every saved gem reproduces its recorded_hash — rebuild the (seed, EnvConfig, journal)
    // from the gem's config, re-run the capture, and compare. (discover_evolved already asserts the
    // record→replay round-trip before writing; this independently re-derives the hash to prove the on-disk
    // contract held — the UNCHANGED verify_and_write_library guarantee.)
    let dir = temp_dir("roundtrip");
    let lib = discover_evolved(99, 8, 3, 6, 60, &species_dir(), &dir, None).expect("evolve");
    let saved = read_saved_gems(&dir);
    assert!(!saved.is_empty(), "at least one gem saved");
    assert_eq!(
        saved.len(),
        lib.len(),
        "saved files match the returned library"
    );

    for (name, gem) in &saved {
        let (env_config, skipped) = env_config_for(&gem.config, &species_dir());
        assert!(
            skipped.is_empty(),
            "gem roster keys must resolve: {skipped:?}"
        );
        let env_config = env_config.expect("gem roster resolves");
        let mut env = GeneSimEnv::new(env_config.entity_count);
        env.set_environment(env_config.env);
        env.set_roster(env_config.roster.clone());
        for built in &env_config.consortium {
            env.register_contaminant(built.clone());
        }
        if let Some((level, config)) = &env_config.containment {
            env.set_containment(*level, config.clone());
        }
        let trace = capture_trace(&mut env, gem.config.master_seed, gem.gens.max(1), &[]);
        assert_eq!(
            trace.recorded_hash, gem.recorded_hash,
            "saved gem {name} must round-trip to its recorded_hash"
        );
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn evolutionary_keeps_more_distinct_gems_than_same_budget_random() {
    // (3) THE DIVERSITY WIN (the whole point of D2b): on the WIDENED space, the evolutionary loop keeps MORE
    // distinct community shapes (novelty-deduped) than a same-budget pure-random D2a run.
    //
    // Budget parity: evolve evaluates pop_size*(gens+1) configs. We match the random `trials` to that exact
    // budget so the comparison is fair — the diversity gain must come from the evolutionary structure
    // (mutate/crossover toward the kept elites + the explore fraction), not from a bigger evaluation budget.
    let pop = 8u64;
    let gens = 3u64;
    let keep = 8usize;
    let trace_gens = 60u32;
    let budget = pop * (gens + 1);

    let evo_dir = temp_dir("div_evo");
    let rnd_dir = temp_dir("div_rnd");

    let evo = discover_evolved(
        4242,
        pop,
        gens,
        keep,
        trace_gens,
        &species_dir(),
        &evo_dir,
        None,
    )
    .expect("evolve");
    let rnd = discover(
        4242,
        budget,
        keep,
        trace_gens,
        &species_dir(),
        &rnd_dir,
        None,
    )
    .expect("random");

    let evo_distinct = distinct_shapes(&evo.gems);
    let rnd_distinct = distinct_shapes(&rnd.gems);

    assert!(
        evo_distinct >= rnd_distinct,
        "evolutionary diversity must NOT regress vs same-budget random: evo {evo_distinct} distinct shapes, random {rnd_distinct}"
    );
    // The headline D2b assertion: the evolutionary loop keeps STRICTLY MORE distinct community shapes (the
    // novelty-deduped diversity win). If a future tuning makes the strict win flaky, this is the line to relax
    // to `>=` with a logged note — but on the widened space the evolutionary spread should beat pure random.
    assert!(
        evo_distinct > rnd_distinct,
        "evolutionary loop should keep MORE distinct gems than same-budget random: evo {evo_distinct} vs random {rnd_distinct}"
    );

    std::fs::remove_dir_all(&evo_dir).ok();
    std::fs::remove_dir_all(&rnd_dir).ok();
}

#[test]
fn evolutionary_best_score_does_not_regress_below_random() {
    // (4) NO REGRESSION: the evolutionary best score is >= the same-budget random best. Evolution carries the
    // elites forward (the GemLibrary keeps the top-K across generations), so it can never end below the best
    // a random sample of the SAME budget found — the elites of generation 0 ARE that random sample's gems.
    let pop = 8u64;
    let gens = 3u64;
    let keep = 8usize;
    let trace_gens = 60u32;
    let budget = pop * (gens + 1);

    let evo_dir = temp_dir("score_evo");
    let rnd_dir = temp_dir("score_rnd");

    let evo = discover_evolved(
        7,
        pop,
        gens,
        keep,
        trace_gens,
        &species_dir(),
        &evo_dir,
        None,
    )
    .expect("evolve");
    let rnd =
        discover(7, budget, keep, trace_gens, &species_dir(), &rnd_dir, None).expect("random");

    let evo_best = evo.gems.iter().map(|g| g.score).max().unwrap_or(0);
    let rnd_best = rnd.gems.iter().map(|g| g.score).max().unwrap_or(0);

    assert!(
        evo_best >= rnd_best,
        "evolutionary best ({evo_best}) must not regress below random best ({rnd_best})"
    );

    std::fs::remove_dir_all(&evo_dir).ok();
    std::fs::remove_dir_all(&rnd_dir).ok();
}

#[test]
fn evolve_gens_zero_reduces_to_random_d2a() {
    // The non-evolutionary base case: discover_evolved with generations=0 is a single random generation of
    // `pop_size` trials — i.e. byte-identical to discover(.., trials=pop_size, ..) for the same search_seed.
    let evo_dir = temp_dir("zero_evo");
    let rnd_dir = temp_dir("zero_rnd");

    let pop = 10u64;
    let evo =
        discover_evolved(555, pop, 0, 5, 60, &species_dir(), &evo_dir, None).expect("evolve g=0");
    let rnd = discover(555, pop, 5, 60, &species_dir(), &rnd_dir, None).expect("random");

    assert_eq!(
        evo, rnd,
        "discover_evolved(generations=0) must equal discover(trials=pop_size)"
    );

    std::fs::remove_dir_all(&evo_dir).ok();
    std::fs::remove_dir_all(&rnd_dir).ok();
}

#[test]
fn pinned_determinism_literal_is_unmoved_by_the_evolutionary_slice() {
    // (5) THE STOP-THE-LINE CHECK (inv #3): the normal pinned single-species config still produces
    // 0x47a0_3c8f_6701_f240. The evolutionary loop adds mutate/crossover proposals (all meta-RNG splitmix over
    // the search seed) but NO sim-path change — the sim runs are pure functions of their configs. Unmoved.
    let cfg = SimConfig {
        seed: 13_679_457_532_755_275_413,
        generations: 50,
        entity_count: 1000,
    };
    let stats = sim_core::run_headless(&cfg);
    assert_eq!(
        stats.hash, 0x47a0_3c8f_6701_f240,
        "the evolutionary search slice must leave the pinned determinism literal UNMOVED (inv #3)"
    );
    let mut sim = sim_core::Simulation::reset(&cfg);
    sim.step(50);
    assert_eq!(
        sim.run_stats().hash,
        0x47a0_3c8f_6701_f240,
        "the stepwise pinned config is also unmoved by the evolutionary search slice"
    );
}
