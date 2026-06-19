//! gene-sim headless harness — runs N seeded, deterministic sim instances and dumps stats (SPEC §2.2, §W6-W8).
//!
//! All randomness derives from one master seed (invariant #3): run `i` uses `derive_seed(master, i)`.
//! `--hash-only` prints just the combined run hash — the determinism artifact compared by
//! `tools/check_determinism.sh`. In normal mode it also writes `data/runs/<run_id>/{seed.json,stats.ndjson}`.

use std::collections::hash_map::DefaultHasher;
use std::fmt::Write as _;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::process::ExitCode;

use sim_core::{derive_seed, run_headless, RunStats, SimConfig, Simulation, Trait};

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
    --per-gen-stats       Also write per-generation columnar stats to data/runs/<id>/per_gen.csv
    --hash-only           Print only the combined determinism hash (no files written)
    -h, --help            Show this help

Examples:
    harness --seed 42 --runs 1 --generations 200
    harness --seed 1234 --generations 300 --hash-only
    harness --master-seed 42 --run-index 3 --generations 500 --per-gen-stats
";

struct Args {
    master: u64,
    runs: u32,
    run_index: Option<u32>,
    generations: u64,
    entities: u32,
    hash_only: bool,
    per_gen_stats: bool,
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut seed: Option<u64> = None;
    let mut master_seed: Option<u64> = None;
    let mut runs: u32 = 1;
    let mut run_index: Option<u32> = None;
    let mut generations: u64 = 200;
    let mut entities: u32 = 1000;
    let mut hash_only = false;
    let mut per_gen_stats = false;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut take = |name: &str| -> Result<String, String> {
            it.next().ok_or_else(|| format!("missing value for {name}"))
        };
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "--hash-only" => hash_only = true,
            "--per-gen-stats" => per_gen_stats = true,
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
        per_gen_stats,
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

    let want_per_gen = args.per_gen_stats && !args.hash_only;

    let mut results: Vec<RunStats> = Vec::with_capacity(indices.len());
    // Per-run, per-generation CSV rows (only populated when --per-gen-stats and not --hash-only).
    let mut per_gen: Vec<String> = Vec::with_capacity(if want_per_gen { indices.len() } else { 0 });
    for &i in &indices {
        let cfg = SimConfig {
            seed: derive_seed(args.master, u64::from(i)),
            generations: args.generations,
            entity_count: args.entities,
        };
        // The determinism hash always comes from the one-shot path (provably unchanged by --per-gen-stats):
        // `run_headless` is reset → step(generations) → run_stats. Per-gen stepping is collected separately.
        results.push(run_headless(&cfg));
        if want_per_gen {
            per_gen.push(collect_per_gen_csv(i, &cfg));
        }
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
            "  run {i:>4}  seed={:<20}  entities={:<7}  generations={:<6}  allele_freq={:<8.6}  hash={:016x}",
            r.seed, r.entity_count, r.generations, r.allele_freq, r.hash
        );
    }
    println!("combined_hash={combined:016x}");

    match write_run_artifacts(&args, &indices, &results, combined, &per_gen) {
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

/// CSV header for the per-generation columnar stats (SPEC §5). Fixed, deterministic column order.
const PER_GEN_HEADER: &str =
    "run_index,generation,population_size,allele_freq,growth_rate,reflectance,drought_tolerance,fecundity,kill_switch_linkage";

/// Drive the stepwise [`Simulation`] for run `i`, advancing ONE generation at a time and calling
/// `observe()` after each, building the per-generation CSV body (one row per generation; no header).
///
/// Stepping 1-gen-at-a-time `N` times is bit-identical to one-shot `step(N)` (proven by sim-core's
/// `simulation_stepwise_matches_one_shot` — one seeded stream, no re-seed) and `observe()` is pure, so
/// this does NOT influence the determinism hash, which comes from the one-shot `run_headless` path
/// (invariant #3). Trait values are pulled in fixed [`Trait::ALL`] order from each `Observation.phenotype`.
fn collect_per_gen_csv(i: u32, cfg: &SimConfig) -> String {
    let mut sim = Simulation::reset(cfg);
    // One data row per generation (generations 1..=cfg.generations), deterministic order.
    let mut body = String::new();
    for _ in 0..cfg.generations {
        sim.step(1);
        let o = sim.observe();
        let p = &o.phenotype;
        // Trait values in canonical Trait::ALL order; `0.0` for any (shouldn't happen) missing trait.
        let _ = writeln!(
            body,
            "{run},{gen},{pop},{af},{gr},{refl},{dt},{fec},{ksl}",
            run = i,
            gen = o.generation,
            pop = o.population_size,
            af = o.allele_freq,
            gr = p.get(Trait::GrowthRate).unwrap_or(0.0),
            refl = p.get(Trait::Reflectance).unwrap_or(0.0),
            dt = p.get(Trait::DroughtTolerance).unwrap_or(0.0),
            fec = p.get(Trait::Fecundity).unwrap_or(0.0),
            ksl = p.get(Trait::KillSwitchLinkage).unwrap_or(0.0),
        );
    }
    body
}

/// Write `data/runs/<run_id>/{seed.json,stats.ndjson}` (human-readable; replay contract seed, SPEC §5).
/// `run_id` is deterministic (no wall-clock) so the path itself is reproducible. When a single
/// `--run-index i` is selected (batch sharding, §W7) the id is keyed by that index (`_i{i}`) so parallel
/// batch jobs write to distinct, non-colliding directories; otherwise it is keyed by the run count.
fn write_run_artifacts(
    args: &Args,
    indices: &[u32],
    results: &[RunStats],
    combined: u64,
    per_gen: &[String],
) -> std::io::Result<PathBuf> {
    let run_id = match args.run_index {
        Some(i) => format!(
            "m{}_g{}_n{}_i{}",
            args.master, args.generations, args.entities, i
        ),
        None => format!(
            "m{}_g{}_n{}_r{}",
            args.master,
            args.generations,
            args.entities,
            indices.len()
        ),
    };
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
            "{{\"run_index\": {i}, \"seed\": {seed}, \"entity_count\": {ec}, \"generations\": {g}, \"allele_freq\": {af}, \"hash\": \"{h:016x}\"}}\n",
            seed = r.seed,
            ec = r.entity_count,
            g = r.generations,
            af = r.allele_freq,
            h = r.hash,
        ));
    }
    std::fs::write(dir.join("stats.ndjson"), ndjson)?;

    // Per-generation columnar stats (SPEC §5), only when --per-gen-stats was set. One header + one row
    // per generation per run, concatenated in stable run-index order (the Parquet step aggregates these).
    if !per_gen.is_empty() {
        let mut csv = String::with_capacity(PER_GEN_HEADER.len() + 1);
        csv.push_str(PER_GEN_HEADER);
        csv.push('\n');
        for rows in per_gen {
            csv.push_str(rows);
        }
        std::fs::write(dir.join("per_gen.csv"), csv)?;
    }

    Ok(dir)
}
