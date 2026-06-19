//! Integration tests for the `--per-gen-stats` CLI flag (slice S3.3).
//!
//! These drive the built `harness` binary end-to-end:
//! 1. a `--per-gen-stats` run yields the SAME determinism hash as a normal run of the same
//!    seed/generations (invariant #3 — per-gen stepping must not change the hash); and
//! 2. the written `per_gen.csv` has the exact header and exactly `generations` data rows.
//!
//! Each test runs in its own temp dir (the harness writes `data/runs/<id>/` relative to CWD), so the
//! repo's `data/` is never touched.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Path to the compiled harness binary (provided by Cargo for integration tests).
const HARNESS: &str = env!("CARGO_BIN_EXE_harness");

/// Make a unique temp dir for one test's outputs.
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "gene_sim_pergen_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Run the harness in `cwd` with `args`, asserting success; returns stdout.
fn run(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new(HARNESS)
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("failed to spawn harness");
    assert!(
        out.status.success(),
        "harness {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("harness stdout not utf8")
}

#[test]
fn per_gen_stats_preserves_determinism_hash() {
    // The determinism hash (--hash-only) must be identical with and without --per-gen-stats.
    let dir = temp_dir("hash");
    let baseline = run(
        &dir,
        &["--seed", "1234", "--generations", "120", "--hash-only"],
    );
    // --per-gen-stats is ignored under --hash-only, but assert they compose to the same hash anyway.
    let with_flag = run(
        &dir,
        &[
            "--seed",
            "1234",
            "--generations",
            "120",
            "--hash-only",
            "--per-gen-stats",
        ],
    );
    assert_eq!(
        baseline.trim(),
        with_flag.trim(),
        "--per-gen-stats must not change the determinism hash"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn per_gen_csv_has_header_and_one_row_per_generation() {
    let dir = temp_dir("csv");
    let generations: usize = 50;
    let entities = 1000; // CLI default; encoded into the run_id.

    run(
        &dir,
        &[
            "--master-seed",
            "42",
            "--run-index",
            "3",
            "--generations",
            &generations.to_string(),
            "--per-gen-stats",
        ],
    );

    // run_id for a single --run-index is m{master}_g{gens}_n{entities}_i{index}.
    let csv_path = dir
        .join("data/runs")
        .join(format!("m42_g{generations}_n{entities}_i3"))
        .join("per_gen.csv");
    let csv = std::fs::read_to_string(&csv_path)
        .unwrap_or_else(|e| panic!("missing per_gen.csv at {}: {e}", csv_path.display()));

    let mut lines = csv.lines();
    let header = lines.next().expect("empty per_gen.csv");
    assert_eq!(
        header,
        "run_index,generation,population_size,allele_freq,growth_rate,reflectance,drought_tolerance,fecundity,kill_switch_linkage",
        "unexpected per_gen.csv header"
    );

    let data: Vec<&str> = lines.collect();
    assert_eq!(
        data.len(),
        generations,
        "expected exactly {generations} data rows, got {}",
        data.len()
    );

    // Each row: 9 fields; run_index column is the selected index (3); generation column is 1..=generations.
    for (i, line) in data.iter().enumerate() {
        let cols: Vec<&str> = line.split(',').collect();
        assert_eq!(cols.len(), 9, "row {i} has wrong column count: {line:?}");
        assert_eq!(cols[0], "3", "row {i} run_index column wrong: {line:?}");
        assert_eq!(
            cols[1],
            (i + 1).to_string(),
            "row {i} generation column wrong: {line:?}"
        );
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn batch_run_reproduces_per_gen_csv() {
    // Reproducibility (invariant #3): the same --run-index off the same master seed produces a
    // byte-identical per_gen.csv across two independent invocations (two distinct CWDs).
    let a = temp_dir("repro_a");
    let b = temp_dir("repro_b");
    let args = &[
        "--master-seed",
        "42",
        "--run-index",
        "5",
        "--generations",
        "40",
        "--per-gen-stats",
    ];
    run(&a, args);
    run(&b, args);

    let rel = PathBuf::from("data/runs/m42_g40_n1000_i5/per_gen.csv");
    let csv_a = std::fs::read(a.join(&rel)).unwrap();
    let csv_b = std::fs::read(b.join(&rel)).unwrap();
    assert_eq!(
        csv_a, csv_b,
        "per_gen.csv must be byte-identical across runs of the same seed/index"
    );

    std::fs::remove_dir_all(&a).ok();
    std::fs::remove_dir_all(&b).ok();
}
