//! Integration test for the `--record-episode` / `--replay` CLI (roadmap R6/P1, ADR-010).
//!
//! Drives the built `harness` binary end-to-end: record a journaled reset+Advance+ApplyEdit episode, then
//! replay it, and assert the replayed stats hash is BIT-IDENTICAL to the recorded one. This is the live-sim
//! replay contract (SPEC §5/§6, invariant #3) the gdext `LiveSim` node will satisfy — proven HEADLESS, with
//! no Godot. The harness writes the episode under the caller-supplied dir, so the repo's `data/` is untouched.

use std::process::Command;

const HARNESS: &str = env!("CARGO_BIN_EXE_harness");

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gene_sim_p1_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn record_episode_then_replay_is_bit_identical_via_cli() {
    let root = temp_dir("replay_cli");

    // Record.
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

    // Parse "recorded <dir> (hash <hex>)".
    let dir = rec_out
        .split_once("recorded ")
        .and_then(|(_, r)| r.split_once(" (hash "))
        .map(|(d, _)| d.to_string())
        .expect("recorded dir in output");
    let recorded_hash = rec_out
        .rsplit_once("(hash ")
        .and_then(|(_, h)| h.split_once(')'))
        .map(|(h, _)| h.trim().to_string())
        .expect("recorded hash in output");

    // The two replay files exist.
    assert!(std::path::Path::new(&dir).join("seed.json").exists());
    assert!(std::path::Path::new(&dir).join("actions.ndjson").exists());

    // Replay.
    let rep = Command::new(HARNESS)
        .args(["--replay", &dir])
        .output()
        .expect("run --replay");
    assert!(rep.status.success(), "replay failed: {rep:?}");
    let replayed_hash = String::from_utf8_lossy(&rep.stdout).trim().to_string();

    assert_eq!(
        replayed_hash, recorded_hash,
        "CLI replay must be BIT-IDENTICAL to the recorded episode (inv #3; the LiveSim replay contract)"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn replay_of_missing_dir_fails_cleanly() {
    let rep = Command::new(HARNESS)
        .args(["--replay", "/nonexistent/gene_sim_p1_missing"])
        .output()
        .expect("run --replay");
    assert!(!rep.status.success(), "replay of a missing dir must fail");
}
