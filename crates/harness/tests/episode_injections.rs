//! Integration test for the P2 episode-export path (R5-viz, ADR-010): `--record-episode <DIR>` combined
//! with `--snapshots`/`--grid`.
//!
//! Drives the built `harness` binary end-to-end and asserts the run dir gets the data a renderer timeline
//! needs to draw INJECTION MARKERS: per-cell `snap_<gen>.bin` snapshots aligned in generation with the
//! journaled `actions.ndjson`, plus a stamped `injections.json` recording the generation + Applied/Failed
//! state of each `ApplyEdit`. Crucially, it also re-runs `--replay` on the same dir and asserts the hash is
//! still bit-identical to the recorded one — proving the snapshot/injection export is read-only w.r.t. the
//! determinism contract (invariant #3).

use std::process::Command;

const HARNESS: &str = env!("CARGO_BIN_EXE_harness");

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gene_sim_p2_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Parse the recorded run dir + hash from the `--record-episode` stdout ("recorded <dir> (hash <hex>)").
fn parse_recorded(out: &str) -> (String, String) {
    let dir = out
        .split_once("recorded ")
        .and_then(|(_, r)| r.split_once(" (hash "))
        .map(|(d, _)| d.to_string())
        .expect("recorded dir in output");
    let hash = out
        .rsplit_once("(hash ")
        .and_then(|(_, h)| h.split_once(')'))
        .map(|(h, _)| h.trim().to_string())
        .expect("recorded hash in output");
    (dir, hash)
}

#[test]
fn record_episode_with_snapshots_writes_snaps_and_injections_and_replays() {
    let root = temp_dir("episode_injections");

    // Record the demo episode WITH snapshots + a small grid (the P2 path).
    let rec = Command::new(HARNESS)
        .args([
            "--record-episode",
            root.to_str().unwrap(),
            "--seed",
            "7",
            "--entities",
            "300",
            "--snapshots",
            ".", // any non-empty value enables the export; files land in the run dir, not here
            "--grid",
            "16x16",
        ])
        .output()
        .expect("run --record-episode --snapshots");
    assert!(rec.status.success(), "record failed: {rec:?}");
    let rec_out = String::from_utf8_lossy(&rec.stdout);
    let (dir, recorded_hash) = parse_recorded(&rec_out);
    let dir = std::path::Path::new(&dir);

    // The journaled replay files still exist (unchanged contract).
    assert!(dir.join("seed.json").exists(), "seed.json missing");
    assert!(
        dir.join("actions.ndjson").exists(),
        "actions.ndjson missing"
    );

    // Snapshots exist at the post-Advance generations: 0 (initial), 20, 40, 60.
    for gen in [0u64, 20, 40, 60] {
        let snap = dir.join(format!("snap_{gen}.bin"));
        assert!(snap.exists(), "expected {snap:?} to exist");
        assert!(
            std::fs::metadata(&snap).unwrap().len() > 0,
            "snapshot {snap:?} is empty"
        );
    }

    // injections.json exists and stamps the two demo edits at the cumulative Advance generations: the first
    // edit lands after Advance(20) → gen 20, the second after the next Advance(20) → gen 40.
    let injections = std::fs::read_to_string(dir.join("injections.json")).expect("injections.json");
    // Two entries (one per ApplyEdit), each with a generation + label + applied flag.
    let gens: Vec<&str> = injections
        .match_indices("\"generation\":")
        .map(|(_, m)| m)
        .collect();
    assert_eq!(gens.len(), 2, "expected 2 injection entries: {injections}");
    assert!(
        injections.contains("\"generation\": 20"),
        "first injection must be stamped at gen 20: {injections}"
    );
    assert!(
        injections.contains("\"generation\": 40"),
        "second injection must be stamped at gen 40: {injections}"
    );
    // Each entry carries a label and an explicit applied bool (never a silent no-op).
    assert!(
        injections.contains("\"label\""),
        "label missing: {injections}"
    );
    assert!(
        injections.contains("\"applied\": true") || injections.contains("\"applied\": false"),
        "applied flag missing: {injections}"
    );
    // The labels resolve the Cas variant + targeted species locus (display only — no biology here).
    assert!(
        injections.contains("SpCas9 → locus 0"),
        "first injection label wrong: {injections}"
    );
    assert!(
        injections.contains("AsCas12a → locus 1"),
        "second injection label wrong: {injections}"
    );

    // The snapshot/injection export is read-only w.r.t. the determinism hash: replaying the same dir still
    // reproduces the recorded hash bit-for-bit (invariant #3).
    let rep = Command::new(HARNESS)
        .args(["--replay", dir.to_str().unwrap()])
        .output()
        .expect("run --replay");
    assert!(rep.status.success(), "replay failed: {rep:?}");
    let replayed_hash = String::from_utf8_lossy(&rep.stdout).trim().to_string();
    assert_eq!(
        replayed_hash, recorded_hash,
        "replay must be BIT-IDENTICAL after the snapshot/injection export (inv #3)"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn record_episode_without_snapshots_writes_no_injections() {
    // Without --snapshots, --record-episode keeps its original behaviour: replay files only, no snaps and
    // no injections.json (the P2 export is strictly additive and opt-in).
    let root = temp_dir("no_injections");

    let rec = Command::new(HARNESS)
        .args([
            "--record-episode",
            root.to_str().unwrap(),
            "--seed",
            "7",
            "--entities",
            "300",
        ])
        .output()
        .expect("run --record-episode");
    assert!(rec.status.success(), "record failed: {rec:?}");
    let rec_out = String::from_utf8_lossy(&rec.stdout);
    let (dir, _hash) = parse_recorded(&rec_out);
    let dir = std::path::Path::new(&dir);

    assert!(dir.join("seed.json").exists());
    assert!(dir.join("actions.ndjson").exists());
    assert!(
        !dir.join("injections.json").exists(),
        "injections.json must NOT be written without --snapshots"
    );
    assert!(
        !dir.join("snap_0.bin").exists(),
        "snapshots must NOT be written without --snapshots"
    );

    let _ = std::fs::remove_dir_all(&root);
}
