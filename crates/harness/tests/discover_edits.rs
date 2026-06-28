//! Variant Lab D (STAGE 2) — the mid-run CRISPR EDIT axis threaded through the search RUNNER. These integration
//! tests ground the `edit_budget` opt-in on the REAL headless core + the real `data/species` boundary.
//!
//! Load-bearing properties (the slice acceptance criteria):
//!  (a) HASH-NEUTRAL OFF: `edit_budget == 0` is byte-identical to the historical no-edit search — same saved
//!      gems + scores, and every kept gem carries an EMPTY edit schedule.
//!  (b) EDITED GEMS REPRODUCE: with `edit_budget > 0` the search keeps gems whose config carries mid-run edits,
//!      and EVERY saved gem already passed the `verify_and_write_library` round-trip (record → replay ==
//!      recorded_hash) BEFORE it was written — so an edited gem on disk is reproducible by construction.
//!  (c) DETERMINISM (inv #3): the same `(search_seed, edit_budget)` yields byte-identical saved gems.

use std::path::{Path, PathBuf};

use discovery::search::{Gem, SearchSpace};
use harness::discover::{discover, discover_in_space};

/// The repo-root `data/species` dir (the byte-mover boundary; mirrors the discover/replay test helpers).
fn species_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/species"))
}

/// A unique temp output dir for a test run (no external tempfile crate; deterministic per-test name + pid).
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "gene_sim_discover_edits_{tag}_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Read every saved gem JSON from an output dir into `(file_name, Gem)` pairs, sorted by file name.
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
fn edit_budget_zero_is_byte_identical_to_the_no_edit_search() {
    // (a) HASH-NEUTRAL OFF: discover_in_space with an explicit edit_budget-0 space equals the historical
    // discover() — same library + byte-identical saved gem files — and every kept gem carries NO edits.
    let plain_dir = temp_dir("zero_plain");
    let space_dir = temp_dir("zero_space");
    let zero = SearchSpace::default(); // edit_budget == 0
    assert_eq!(
        zero.edit_budget, 0,
        "the default space has the edit axis OFF"
    );

    let plain = discover(2024, 12, 4, 60, &species_dir(), &plain_dir, None).expect("discover");
    let via_space =
        discover_in_space(&zero, 2024, 12, 4, 60, &species_dir(), &space_dir, None).expect("space");

    assert_eq!(
        plain, via_space,
        "edit_budget 0 must be byte-identical to the historical no-edit search"
    );
    let saved_plain = read_saved_gems(&plain_dir);
    let saved_space = read_saved_gems(&space_dir);
    assert!(
        !saved_plain.is_empty(),
        "discover must save at least one gem"
    );
    assert_eq!(saved_plain.len(), saved_space.len(), "same gem count");
    for ((np, gp), (ns, _)) in saved_plain.iter().zip(&saved_space) {
        assert_eq!(np, ns, "gem file names must match");
        let tp = std::fs::read_to_string(plain_dir.join(np)).unwrap();
        let ts = std::fs::read_to_string(space_dir.join(ns)).unwrap();
        assert_eq!(tp, ts, "gem JSON bytes must be identical for {np}");
        assert!(
            gp.config.edits.is_empty(),
            "an edit_budget-0 gem must carry NO mid-run edits"
        );
    }

    std::fs::remove_dir_all(&plain_dir).ok();
    std::fs::remove_dir_all(&space_dir).ok();
}

#[test]
fn edit_budget_on_keeps_edited_gems_that_reproduce() {
    // (b) EDITED GEMS REPRODUCE: with the axis ON the search keeps at least one gem carrying mid-run edits.
    // Every saved gem already passed verify_and_write_library's round-trip (record → replay == recorded_hash)
    // BEFORE it was written — so a kept edited gem is reproducible by construction (the on-disk contract).
    let space = SearchSpace {
        edit_budget: 3,
        ..SearchSpace::default()
    };
    let dir = temp_dir("edits_on");
    let lib =
        discover_in_space(&space, 4242, 32, 8, 60, &species_dir(), &dir, None).expect("discover");

    let saved = read_saved_gems(&dir);
    assert!(!saved.is_empty(), "the search must keep at least one gem");
    assert_eq!(
        saved.len(),
        lib.len(),
        "saved files match the returned library"
    );

    // The whole point of the axis: at least one kept (and therefore round-trip-verified) gem carries edits.
    let edited = saved
        .iter()
        .filter(|(_, g)| !g.config.edits.is_empty())
        .count();
    assert!(
        edited > 0,
        "edit_budget > 0 must keep at least one gem carrying mid-run edits (got {edited}/{})",
        saved.len()
    );
    // Each edited gem's schedule is well-formed (every gene names a valid 20-base ACGT guide + a roster index).
    for (name, gem) in saved.iter().filter(|(_, g)| !g.config.edits.is_empty()) {
        for e in &gem.config.edits {
            assert_eq!(e.guide.len(), 20, "guide length (gem {name})");
            assert!(
                e.guide
                    .bytes()
                    .all(|b| matches!(b, b'A' | b'C' | b'G' | b'T')),
                "guide is ACGT (gem {name})"
            );
            assert!(
                (e.species_index as usize) < gem.config.roster.len(),
                "species_index in range (gem {name})"
            );
        }
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn same_search_seed_and_edit_budget_is_deterministic() {
    // (c) DETERMINISM (inv #3): the same (search_seed, edit_budget) yields byte-identical saved gems.
    let space = SearchSpace {
        edit_budget: 3,
        ..SearchSpace::default()
    };
    let a_dir = temp_dir("det_a");
    let b_dir = temp_dir("det_b");

    let lib_a =
        discover_in_space(&space, 7, 24, 6, 60, &species_dir(), &a_dir, None).expect("discover a");
    let lib_b =
        discover_in_space(&space, 7, 24, 6, 60, &species_dir(), &b_dir, None).expect("discover b");

    assert_eq!(lib_a, lib_b, "same (seed, edit_budget) → identical library");

    let saved_a = read_saved_gems(&a_dir);
    let saved_b = read_saved_gems(&b_dir);
    assert!(!saved_a.is_empty(), "at least one gem saved");
    assert_eq!(saved_a.len(), saved_b.len(), "same number of saved gems");
    for ((na, _), (nb, _)) in saved_a.iter().zip(&saved_b) {
        assert_eq!(na, nb, "gem file names must match across runs");
        let ta = std::fs::read_to_string(a_dir.join(na)).unwrap();
        let tb = std::fs::read_to_string(b_dir.join(nb)).unwrap();
        assert_eq!(ta, tb, "gem JSON bytes must be identical for {na}");
    }
    // The axis is genuinely on (at least one edited gem), so this is not a trivial empty-schedule run.
    assert!(
        saved_a.iter().any(|(_, g)| !g.config.edits.is_empty()),
        "the deterministic run must actually exercise the edit axis"
    );

    std::fs::remove_dir_all(&a_dir).ok();
    std::fs::remove_dir_all(&b_dir).ok();
}
