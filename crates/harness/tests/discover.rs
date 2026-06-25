//! D2a STAGE 2 — the SEARCH RUNNER integration tests (ADR-023): the `discover()` meta-loop grounded on the
//! REAL headless core + the real `data/species` boundary.
//!
//! Load-bearing properties (the slice acceptance criteria):
//!  1. DETERMINISM (inv #3): same `search_seed` → byte-identical saved gem files + scores.
//!  2. GEM VALIDITY: every saved gem ROUND-TRIPS — `replay(gem.config) == gem.recorded_hash`.
//!  3. NOVELTY DEDUP: no two kept gems are within `dedup_min` (the library invariant survives to disk).
//!  4. NON-DEGENERATE: the search finds at least one quality > 0 gem over the Primordial space.
//!  5. THE PINNED LITERAL `0x47a0_3c8f_6701_f240` is STILL produced by the normal pinned config — the search
//!     added NO sim-path change (the proposal is the meta-RNG, never `SimRng`).

use std::path::{Path, PathBuf};

use discovery::search::Gem;
use discovery::{novelty_l1, DefaultScorer, InterestingnessScorer};
use harness::capture::capture_trace;
use harness::discover::{discover, gem_file_name};
use harness::{Action, GeneSimEnv};
use sim_core::SimConfig;

/// The repo-root `data/species` dir (the byte-mover boundary; mirrors the species/replay test helpers).
fn species_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/species"))
}

/// A unique temp output dir for a test run (no external tempfile crate; deterministic per-test name + pid).
fn temp_dir(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("gene_sim_discover_it_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Read every saved gem JSON from a `discover()` output dir into `(file_name, Gem)` pairs, sorted by file name
/// (a stable, score-then-seed ordering — `gem_file_name` is `<padded score>-<hex seed>.json`).
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

#[test]
fn discover_is_deterministic_same_search_seed() {
    // (1) DETERMINISM: two discover() runs of the same (search_seed, trials, keep, gens) into distinct temp dirs
    // produce byte-identical saved gem files (same names + same JSON bytes) and identical scores.
    let a_dir = temp_dir("det_a");
    let b_dir = temp_dir("det_b");

    let lib_a = discover(2024, 12, 4, 60, &species_dir(), &a_dir, None).expect("discover a");
    let lib_b = discover(2024, 12, 4, 60, &species_dir(), &b_dir, None).expect("discover b");

    // The returned libraries match exactly (config + every integer signal).
    assert_eq!(
        lib_a, lib_b,
        "same search_seed must yield an identical gem library"
    );

    // The on-disk files match: same names, byte-identical contents.
    let saved_a = read_saved_gems(&a_dir);
    let saved_b = read_saved_gems(&b_dir);
    assert!(!saved_a.is_empty(), "discover must save at least one gem");
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
fn every_saved_gem_round_trips() {
    // (2) GEM VALIDITY: every saved gem reproduces its recorded_hash — rebuild the (seed, EnvConfig, journal)
    // and re-run the capture; the trace's recorded_hash must equal the stored gem.recorded_hash. (discover()
    // already asserts the record→replay round-trip before writing, so any saved gem is reproducible by
    // construction; this independently re-derives the hash from the config to prove the on-disk contract.)
    let dir = temp_dir("roundtrip");
    let lib = discover(99, 12, 4, 60, &species_dir(), &dir, None).expect("discover");
    let saved = read_saved_gems(&dir);
    assert!(!saved.is_empty(), "at least one gem saved");
    assert_eq!(
        saved.len(),
        lib.len(),
        "saved files match the returned library"
    );

    for (name, gem) in &saved {
        // Rebuild the env from the gem's config exactly as discover() does, re-capture, compare the hash.
        let (env_config, skipped) = harness::discover::env_config_for(&gem.config, &species_dir());
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
        // The file name encodes the score + seed (the rank-ordered on-disk key).
        assert_eq!(
            name,
            &gem_file_name(gem),
            "file name is a pure function of the gem"
        );
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn saved_gems_are_novelty_deduped() {
    // (3) NOVELTY DEDUP: no two kept gems are within dedup_min (SCALE) of each other in fingerprint L1 — the
    // GemLibrary dedup invariant survives onto disk. Also assert each gem is internally below the K cap.
    let dir = temp_dir("dedup");
    let lib = discover(31337, 16, 6, 60, &species_dir(), &dir, None).expect("discover");
    let saved = read_saved_gems(&dir);
    assert!(!saved.is_empty(), "at least one gem saved");
    assert!(saved.len() <= 6, "no more than keep=6 gems kept");

    let fps: Vec<[u16; discovery::FP_DIMS]> = saved.iter().map(|(_, g)| g.fingerprint).collect();
    for (i, fp) in fps.iter().enumerate() {
        // The nearest-neighbour distance to the OTHER kept fingerprints must be >= dedup_min (SCALE).
        let others: Vec<[u16; discovery::FP_DIMS]> = fps
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, f)| *f)
            .collect();
        if others.is_empty() {
            continue; // a single kept gem trivially satisfies dedup
        }
        let nn = novelty_l1(fp, &others);
        assert!(
            nn >= lib.dedup_min,
            "two kept gems are within dedup_min ({nn} < {})",
            lib.dedup_min
        );
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn discover_finds_a_non_degenerate_gem() {
    // (4) NON-DEGENERATE: over the Primordial space the search finds at least one gem with quality > 0 — i.e. a
    // multi-species run that is more than mere survival (the M6 gate / coexistence metrics fire). A degenerate
    // search (every config dead/monoculture) would keep only quality==0 gems.
    let dir = temp_dir("nondegen");
    let lib = discover(7, 24, 8, 120, &species_dir(), &dir, None).expect("discover");
    assert!(!lib.is_empty(), "the search must keep at least one gem");
    assert!(
        lib.gems.iter().any(|g| g.quality > 0),
        "the search must find at least one non-degenerate (quality > 0) gem; got {:?}",
        lib.gems.iter().map(|g| g.quality).collect::<Vec<_>>()
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn d0_scorer_orders_a_real_living_run_above_a_monoculture_via_discover_capture() {
    // Sanity bridge: the capture+score path discover() uses ranks a living multi-species run strictly above a
    // monoculture (the synthetic oracle's "mere survival is not interesting"). This is the property the search
    // exploits — a non-degenerate gem out-scores a flat one.
    let scorer = DefaultScorer::default();

    let mut living = GeneSimEnv::new(200);
    living.set_roster(vec![
        (
            harness::species::load_species_file(species_dir().join("default.json")).unwrap(),
            600,
        ),
        (
            harness::species::load_species_file(species_dir().join("ecoli.json")).unwrap(),
            400,
        ),
        (
            harness::species::load_species_file(species_dir().join("bdellovibrio.json")).unwrap(),
            120,
        ),
    ]);
    let live = scorer.score(&capture_trace(&mut living, 2024, 200, &[]));

    let mut mono = GeneSimEnv::new(1000);
    let monoc = scorer.score(&capture_trace(
        &mut mono,
        13_679_457_532_755_275_413,
        50,
        &[],
    ));

    assert!(
        live.quality > monoc.quality,
        "a living multi-species run ({}) must out-score a monoculture ({})",
        live.quality,
        monoc.quality
    );
}

#[test]
fn pinned_determinism_literal_is_unmoved_by_the_search_slice() {
    // (5) THE STOP-THE-LINE CHECK (inv #3): the normal pinned single-species config still produces
    // 0x47a0_3c8f_6701_f240. The search added a NEW module + CLI but NO sim-path change — the proposal sampler
    // is the meta-RNG (splitmix over the search seed), never SimRng. This run is byte-identical to before.
    let cfg = SimConfig {
        seed: 13_679_457_532_755_275_413,
        generations: 50,
        entity_count: 1000,
    };
    let stats = sim_core::run_headless(&cfg);
    assert_eq!(
        stats.hash, 0x47a0_3c8f_6701_f240,
        "the search slice must leave the pinned determinism literal UNMOVED (inv #3)"
    );
    // A stepwise run (the env capture path's RNG-driving shape) lands on the same anchor too.
    let mut sim = sim_core::Simulation::reset(&cfg);
    sim.step(50);
    assert_eq!(
        sim.run_stats().hash,
        0x47a0_3c8f_6701_f240,
        "the stepwise pinned config is also unmoved by the search slice"
    );
    let _ = Action::Advance(0); // keep the Action import load-bearing (the search action surface is unchanged)
}
