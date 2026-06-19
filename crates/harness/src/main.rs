//! gene-sim headless harness — runs N seeded, deterministic sim instances and dumps stats (SPEC §2.2, §W6-W8).
//!
//! All randomness derives from one master seed (invariant #3): run `i` uses `derive_seed(master, i)`.
//! `--hash-only` prints just the combined run hash — the determinism artifact compared by
//! `tools/check_determinism.sh`. In normal mode it also writes `data/runs/<run_id>/{seed.json,stats.ndjson}`.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::process::ExitCode;

use sim_core::{derive_seed, run_headless, RunStats, SimConfig};

const USAGE: &str = "\
gene-sim harness — headless deterministic sim runner

USAGE:
    harness [OPTIONS]

OPTIONS:
    --seed <u64>          Master seed (alias for --master-seed). Default: 42
    --master-seed <u64>   Master seed; every run derives its seed from this (invariant #3)
    --runs <u32>          Number of runs (indices 0..runs). Default: 1
    --run-index <u32>     Run only this single index off the master seed (for batch sharding, §W7)
    --generations <u64>   Generations per run. Default: 200
    --entities <u32>      Organisms spawned per run. Default: 1000
    --hash-only           Print only the combined determinism hash (no files written)
    -h, --help            Show this help

Examples:
    harness --seed 42 --runs 1 --generations 200
    harness --seed 1234 --generations 300 --hash-only
    harness --master-seed 42 --run-index 3 --generations 500
";

struct Args {
    master: u64,
    runs: u32,
    run_index: Option<u32>,
    generations: u64,
    entities: u32,
    hash_only: bool,
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut seed: Option<u64> = None;
    let mut master_seed: Option<u64> = None;
    let mut runs: u32 = 1;
    let mut run_index: Option<u32> = None;
    let mut generations: u64 = 200;
    let mut entities: u32 = 1000;
    let mut hash_only = false;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut take = |name: &str| -> Result<String, String> {
            it.next().ok_or_else(|| format!("missing value for {name}"))
        };
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "--hash-only" => hash_only = true,
            "--seed" => seed = Some(parse_num(&take("--seed")?, "--seed")?),
            "--master-seed" => {
                master_seed = Some(parse_num(&take("--master-seed")?, "--master-seed")?)
            }
            "--runs" => runs = parse_num(&take("--runs")?, "--runs")?,
            "--run-index" => run_index = Some(parse_num(&take("--run-index")?, "--run-index")?),
            "--generations" => generations = parse_num(&take("--generations")?, "--generations")?,
            "--entities" => entities = parse_num(&take("--entities")?, "--entities")?,
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    let master = master_seed.or(seed).unwrap_or(42);
    Ok(Some(Args {
        master,
        runs,
        run_index,
        generations,
        entities,
        hash_only,
    }))
}

fn parse_num<T: std::str::FromStr>(s: &str, name: &str) -> Result<T, String> {
    s.parse::<T>()
        .map_err(|_| format!("invalid value for {name}: {s:?}"))
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(Some(a)) => a,
        Ok(None) => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("error: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    // Which run indices to execute.
    let indices: Vec<u32> = match args.run_index {
        Some(i) => vec![i],
        None => (0..args.runs).collect(),
    };

    let mut results: Vec<RunStats> = Vec::with_capacity(indices.len());
    for &i in &indices {
        let cfg = SimConfig {
            seed: derive_seed(args.master, u64::from(i)),
            generations: args.generations,
            entity_count: args.entities,
        };
        results.push(run_headless(&cfg));
    }

    let combined = combine_hashes(results.iter().map(|r| r.hash));

    if args.hash_only {
        // Single deterministic line — this is what tools/check_determinism.sh compares.
        println!("{combined:016x}");
        return ExitCode::SUCCESS;
    }

    println!(
        "gene-sim harness · master_seed={} · runs={} · generations={} · entities={}",
        args.master,
        indices.len(),
        args.generations,
        args.entities
    );
    for (i, r) in indices.iter().zip(&results) {
        println!(
            "  run {i:>4}  seed={:<20}  entities={:<7}  generations={:<6}  hash={:016x}",
            r.seed, r.entity_count, r.generations, r.hash
        );
    }
    println!("combined_hash={combined:016x}");

    match write_run_artifacts(&args, &indices, &results, combined) {
        Ok(dir) => println!("wrote {}", dir.display()),
        Err(e) => {
            eprintln!("warning: could not write run artifacts: {e}");
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}

fn combine_hashes(hashes: impl Iterator<Item = u64>) -> u64 {
    let mut h = DefaultHasher::new();
    for x in hashes {
        x.hash(&mut h);
    }
    h.finish()
}

/// Write `data/runs/<run_id>/{seed.json,stats.ndjson}` (human-readable; replay contract seed, SPEC §5).
/// `run_id` is deterministic (no wall-clock) so the path itself is reproducible.
fn write_run_artifacts(
    args: &Args,
    indices: &[u32],
    results: &[RunStats],
    combined: u64,
) -> std::io::Result<PathBuf> {
    let run_id = format!(
        "m{}_g{}_n{}_r{}",
        args.master,
        args.generations,
        args.entities,
        indices.len()
    );
    let dir = PathBuf::from("data/runs").join(&run_id);
    std::fs::create_dir_all(&dir)?;

    let derived: Vec<String> = results.iter().map(|r| r.seed.to_string()).collect();
    let seed_json = format!(
        concat!(
            "{{\n",
            "  \"master_seed\": {master},\n",
            "  \"runs\": {runs},\n",
            "  \"run_index\": {run_index},\n",
            "  \"generations\": {generations},\n",
            "  \"entity_count\": {entities},\n",
            "  \"derived_seeds\": [{derived}],\n",
            "  \"combined_hash\": \"{combined:016x}\",\n",
            "  \"toolchain\": {{ \"rust\": \"1.96.0\", \"bevy_ecs\": \"0.19\", \"rand_chacha\": \"0.10\" }},\n",
            "  \"harness_version\": \"{hv}\"\n",
            "}}\n"
        ),
        master = args.master,
        runs = indices.len(),
        run_index = args.run_index.map_or_else(|| "null".to_string(), |i| i.to_string()),
        generations = args.generations,
        entities = args.entities,
        derived = derived.join(", "),
        combined = combined,
        hv = env!("CARGO_PKG_VERSION"),
    );
    std::fs::write(dir.join("seed.json"), seed_json)?;

    let mut ndjson = String::new();
    for (i, r) in indices.iter().zip(results) {
        ndjson.push_str(&format!(
            "{{\"run_index\": {i}, \"seed\": {seed}, \"entity_count\": {ec}, \"generations\": {g}, \"hash\": \"{h:016x}\"}}\n",
            seed = r.seed,
            ec = r.entity_count,
            g = r.generations,
            h = r.hash,
        ));
    }
    std::fs::write(dir.join("stats.ndjson"), ndjson)?;

    Ok(dir)
}
