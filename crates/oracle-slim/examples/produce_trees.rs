//! Produce a `.trees` via the SLiM oracle — the bridge from the S2.2 driver to the S2.3 tskit analysis.
//!
//! Run: `cargo run -q -p oracle-slim --example produce_trees [seed]`
//! Prints the path to the produced tree sequence on stdout; analyze it with
//! `.venv/bin/python scripts/slim_analyze.py <path>`.
//!
//! Invariant #1: this only calls the `slim` CLI as a subprocess (via the dependency-free oracle-slim crate).

use std::path::PathBuf;

use oracle_slim::{run_model, SlimParams};

fn main() {
    // Seed is a CLI arg (caller would derive it via sim-core::derive_seed in the real pipeline).
    let seed: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(424_242);

    // Parameters chosen to yield a healthy number of segregating sites for the analysis.
    let params = SlimParams {
        population_size: 500,
        mutation_rate: 1e-7,
        recombination_rate: 1e-8,
        sequence_length: 1_000_000,
        generations: 200,
        seed,
    };

    let work_dir = PathBuf::from("data/runs/slim_demo");
    match run_model(&params, &work_dir) {
        Ok(run) => println!("{}", run.trees_path.display()),
        Err(e) => {
            eprintln!("slim run failed: {e}");
            std::process::exit(1);
        }
    }
}
