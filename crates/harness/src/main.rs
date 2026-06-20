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

use crispr::{default_cas_variants, EditOutcome, GuideSequence, RegionEditOutcome};
use genome::LocusId;
use harness::{Action, EditAction, Env, GeneSimEnv};
use sim_core::{derive_seed, run_headless, Observation, RunStats, SimConfig, Simulation, Trait};

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
    --snapshots <DIR>     Write per-cell render snapshots (snap_<gen>.bin) to <DIR> every epoch (SPEC §W10)
    --grid <W>x<H>        Snapshot grid size for --snapshots. Default: 64x64
    --specimens <DIR>     Write specimens.json (baseline + edited species-genome trait vectors) for the
                          renderer's L-system plant view (SPEC §W10, S4.5). Read-only; no hash impact.
    --hash-only           Print only the combined determinism hash (no files written)
    --record-episode <DIR>  Record a journaled reset+Advance+ApplyEdit episode to <DIR>/<run_id>/ (seed.json +
                          actions.ndjson) and print its hash — the live-session replay contract (R6/P1, ADR-010).
                          With --snapshots (and optional --grid) it ALSO writes per-cell snap_<gen>.bin +
                          injections.json (stamped injection generations) for the renderer timeline (P2)
    --replay <DIR>        Replay a recorded episode dir (seed.json + actions.ndjson) and print its stats hash;
                          equals the recorded hash bit-for-bit on the same build (SPEC §6, inv #3)
    -h, --help            Show this help

Examples:
    harness --seed 42 --runs 1 --generations 200
    harness --seed 1234 --generations 300 --hash-only
    harness --master-seed 42 --run-index 3 --generations 500 --per-gen-stats
    harness --seed 7 --generations 50 --snapshots data/runs/snaptest --grid 32x32
    harness --record-episode data/runs --seed 7 --entities 300 --snapshots . --grid 48x48
";

/// How often (in generations) a snapshot is written when `--snapshots` is set; the final generation is
/// always written too. Read-only (does not affect the determinism hash — invariant #3).
const SNAPSHOT_EPOCH: u64 = 10;

struct Args {
    master: u64,
    runs: u32,
    run_index: Option<u32>,
    generations: u64,
    entities: u32,
    hash_only: bool,
    per_gen_stats: bool,
    snapshots: Option<PathBuf>,
    grid: (u32, u32),
    specimens: Option<PathBuf>,
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
    let mut snapshots: Option<PathBuf> = None;
    let mut grid: (u32, u32) = (64, 64);
    let mut specimens: Option<PathBuf> = None;

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
            "--snapshots" => snapshots = Some(PathBuf::from(take("--snapshots")?)),
            "--grid" => grid = parse_grid(&take("--grid")?)?,
            "--specimens" => specimens = Some(PathBuf::from(take("--specimens")?)),
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
        snapshots,
        grid,
        specimens,
    }))
}

/// Parse a `<W>x<H>` grid spec (e.g. `32x32`) into `(width, height)`; both must be non-zero.
fn parse_grid(s: &str) -> Result<(u32, u32), String> {
    let (w, h) = s
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("invalid --grid {s:?} (expected <W>x<H>, e.g. 64x64)"))?;
    let width: u32 = parse_num(w, "--grid width")?;
    let height: u32 = parse_num(h, "--grid height")?;
    if width == 0 || height == 0 {
        return Err(format!(
            "invalid --grid {s:?}: width and height must be > 0"
        ));
    }
    Ok((width, height))
}

fn parse_num<T: std::str::FromStr>(s: &str, name: &str) -> Result<T, String> {
    s.parse::<T>()
        .map_err(|_| format!("invalid value for {name}: {s:?}"))
}

fn main() -> ExitCode {
    // Replay subcommands (roadmap R6/P1): the determinism/replay contract that the live-sim `LiveSim` node
    // (gdext, ADR-010) will satisfy, exposed on the CLI and provable headless without Godot.
    if let Some(code) = handle_replay_subcommands() {
        return code;
    }

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
    let want_snapshots = args.snapshots.is_some() && !args.hash_only;
    let want_specimens = args.specimens.is_some() && !args.hash_only;

    let mut results: Vec<RunStats> = Vec::with_capacity(indices.len());
    // Per-run, per-generation CSV rows (only populated when --per-gen-stats and not --hash-only).
    let mut per_gen: Vec<String> = Vec::with_capacity(if want_per_gen { indices.len() } else { 0 });
    let multi_run = indices.len() > 1;
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
        if want_snapshots {
            // ADDITIVE & read-only: snapshots derive from a fresh stepwise Simulation and never feed the
            // hash (snapshot() draws no RNG, mutates nothing — invariant #3).
            if let Some(dir) = &args.snapshots {
                if let Err(e) = write_snapshots(dir, i, multi_run, &cfg, args.grid) {
                    eprintln!("warning: could not write snapshots for run {i}: {e}");
                }
            }
        }
        if want_specimens {
            // ADDITIVE & read-only: specimens come from a SEPARATE GeneSimEnv instance (its own RNG), never
            // the hashed run — so exporting them cannot change the determinism hash (invariant #3).
            if let Some(dir) = &args.specimens {
                if let Err(e) = write_specimens(dir, i, multi_run, &cfg) {
                    eprintln!("warning: could not write specimens for run {i}: {e}");
                }
            }
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

/// Handle the replay subcommands (`--replay <dir>` / `--record-episode <dir>`), returning `Some(code)` if
/// one was present. These expose `harness::replay`'s record/replay contract (SPEC §5/§6) on the CLI — the
/// determinism foundation the live-sim `LiveSim` node (ADR-010) journals into and replays bit-identically.
fn handle_replay_subcommands() -> Option<ExitCode> {
    let argv: Vec<String> = std::env::args().collect();
    let val = |flag: &str| -> Option<String> {
        argv.iter()
            .position(|a| a == flag)
            .and_then(|i| argv.get(i + 1).cloned())
    };

    if let Some(dir) = val("--replay") {
        return Some(match harness::replay::replay(&dir) {
            Ok(hash) => {
                // Single deterministic line — the replayed stats hash (compare against the recorded one).
                println!("{hash:016x}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("replay error: {e}");
                ExitCode::from(1)
            }
        });
    }

    if let Some(dir) = val("--record-episode") {
        let seed = val("--seed").and_then(|s| s.parse().ok()).unwrap_or(42);
        let entities = val("--entities")
            .and_then(|s| s.parse().ok())
            .unwrap_or(500);
        // P2: when --snapshots/--grid accompany --record-episode, ALSO drive the same journaled episode
        // stepwise to emit per-cell snapshots + stamped injection generations into the run dir (the marker
        // source the renderer reads). Read-only w.r.t. the determinism hash (inv #3).
        let snapshot_grid = val("--snapshots").is_some().then(|| {
            val("--grid")
                .and_then(|g| parse_grid(&g).ok())
                .unwrap_or((64, 64))
        });
        return Some(record_demo_episode(&dir, seed, entities, snapshot_grid));
    }

    None
}

/// The representative demo action sequence (reset + Advance/ApplyEdit mix) — the SINGLE source for both the
/// journaled `actions.ndjson` and the stepwise snapshot/injection drive, so the two always line up in
/// generation. `Advance(20)` blocks separate the two edits, so the edits are stamped at gen 20 and gen 40.
fn demo_episode_actions() -> Vec<Action> {
    let cas = |name: &str| default_cas_variants().into_iter().find(|v| v.name == name);
    let mut actions = vec![Action::Advance(20)];
    if let (Some(sp), Ok(g)) = (cas("SpCas9"), GuideSequence::new(*b"ACGTGGACGTTTTAGGCCGG")) {
        actions.push(Action::ApplyEdit(EditAction {
            cas: sp.id,
            target: LocusId(0),
            guide: g,
        }));
    }
    actions.push(Action::Advance(20));
    if let (Some(asc), Ok(g)) = (
        cas("AsCas12a"),
        GuideSequence::new(*b"TTTACCGGTTTAGGGCAAAC"),
    ) {
        actions.push(Action::ApplyEdit(EditAction {
            cas: asc.id,
            target: LocusId(1),
            guide: g,
        }));
    }
    actions.push(Action::Advance(20));
    actions
}

/// Record a representative journaled episode (reset + Advance/ApplyEdit mix) to `<dir>/<run_id>/` — the same
/// shape a live `LiveSim` session produces — so `--replay` can reproduce its hash bit-identically.
///
/// When `snapshot_grid` is `Some((w, h))` (i.e. `--snapshots`/`--grid` accompanied `--record-episode`), the
/// SAME journaled episode is then driven stepwise through a separate [`GeneSimEnv`] to ALSO write per-cell
/// `snap_<gen>.bin` snapshots and a stamped `injections.json` into the run dir, so a renderer can draw
/// injection markers without re-deriving the generations from the log. That export is read-only w.r.t. the
/// determinism hash (inv #3): the hash comes from `record_episode`'s own `run_stats` fold; snapshots draw no
/// RNG and the injection stamps are plain generation counters off the SAME single seeded stream.
fn record_demo_episode(
    dir: &str,
    seed: u64,
    entities: u32,
    snapshot_grid: Option<(u32, u32)>,
) -> ExitCode {
    use harness::replay::{record_episode, EnvConfig};

    let actions = demo_episode_actions();

    let env = EnvConfig {
        entity_count: entities,
        ..Default::default()
    };
    match record_episode(&env, seed, &actions, dir) {
        Ok(r) => {
            println!("recorded {} (hash {:016x})", r.dir.display(), r.hash);
            if let Some(grid) = snapshot_grid {
                if let Err(e) =
                    write_episode_snapshots_and_injections(&r.dir, seed, entities, &actions, grid)
                {
                    eprintln!("warning: could not write episode snapshots/injections: {e}");
                    return ExitCode::from(1);
                }
                println!(
                    "wrote snapshots + injections.json to {} ({}x{} grid)",
                    r.dir.display(),
                    grid.0,
                    grid.1
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("record error: {e}");
            ExitCode::from(1)
        }
    }
}

/// Drive the journaled demo episode stepwise through a fresh [`GeneSimEnv`], writing per-cell render
/// snapshots (`snap_<gen>.bin`) at the post-`Advance` generations and a stamped `injections.json` (one entry
/// per [`Action::ApplyEdit`]) into `run_dir` — the data a renderer timeline needs to draw injection markers
/// aligned to the snapshots (P2; ADR-010, SPEC §5/§W10).
///
/// The drive replays the *same* `(seed, actions)` the recorded episode used, so snapshots/injections line up
/// in generation with `actions.ndjson` by construction. It is **read-only & ADDITIVE** w.r.t. the
/// determinism hash (invariant #3): `snapshot()` draws no RNG, and an injection entry is just the running
/// `Advance` cumulative plus the edit's Applied/Failed outcome (from [`GeneSimEnv::last_edit`]) — both off
/// the hash path (the recorded hash already came from `record_episode`'s own `run_stats` fold).
///
/// `injections.json` schema — a JSON array of objects:
/// `[{ "generation": <u64>, "label": <string>, "applied": <bool> }, ...]`, one per ApplyEdit, in order.
fn write_episode_snapshots_and_injections(
    run_dir: &std::path::Path,
    seed: u64,
    entities: u32,
    actions: &[Action],
    grid: (u32, u32),
) -> std::io::Result<()> {
    use harness::{Env, GeneSimEnv};

    std::fs::create_dir_all(run_dir)?;
    let (w, h) = grid;

    let mut env = GeneSimEnv::new(entities);
    env.reset(seed);

    // Generation 0 (the initial state, before any action) — so the timeline has a starting frame.
    env.snapshot(w, h).write_to(run_dir.join("snap_0.bin"))?;

    // Running cumulative of advanced generations (the single seeded stream's generation counter), kept in
    // lock-step with the recorded episode's `Advance` actions so every stamp/snapshot lines up.
    let mut generation: u64 = 0;
    let mut injections: Vec<(u64, String, bool)> = Vec::new();
    for action in actions {
        match action {
            Action::Advance(n) => {
                env.step(Action::Advance(*n));
                generation += *n;
                // A post-Advance snapshot so the snapshot's `generation` matches the journaled cumulative.
                env.snapshot(w, h)
                    .write_to(run_dir.join(format!("snap_{generation}.bin")))?;
            }
            Action::ApplyEdit(edit) => {
                env.step(Action::ApplyEdit(edit.clone()));
                // Stamp the injection at the CURRENT cumulative generation (the edit is applied "now", in
                // between Advance blocks — the renderer marks it on that frame). `applied` reflects whether
                // crispr cleared the gate (Applied) vs an explicit failure (Failed) — never a silent no-op.
                let applied = matches!(env.last_edit(), Some(EditOutcome::Applied { .. }));
                injections.push((generation, edit_label(edit), applied));
            }
            Action::ApplyEditRegion(edit, region) => {
                env.step(Action::ApplyEditRegion(edit.clone(), *region));
                let applied = matches!(
                    env.last_region_edit(),
                    Some((RegionEditOutcome::Applied { .. }, _))
                );
                injections.push((generation, format!("{} @region", edit_label(edit)), applied));
            }
        }
    }

    write_injections_json(&run_dir.join("injections.json"), &injections)
}

/// A short human-readable label for an injection entry: the Cas variant name (resolved against the seed
/// table) plus the targeted species `LocusId` (e.g. `"SpCas9 → locus 0"`). Resolution is for display only —
/// no biology is computed here (the genotype→phenotype map stays in the core, inv #2).
fn edit_label(edit: &EditAction) -> String {
    let cas_name = default_cas_variants()
        .into_iter()
        .find(|v| v.id == edit.cas)
        .map_or_else(|| format!("cas#{}", edit.cas.0), |v| v.name.to_string());
    format!("{cas_name} → locus {}", edit.target.0)
}

/// Serialize the stamped injections to a pretty JSON array — the renderer's injection-marker source.
/// Schema: `[{ "generation": <u64>, "label": <string>, "applied": <bool> }, ...]` (additive; off the hash
/// path — inv #3). Labels are JSON-escaped so an arbitrary Cas name cannot corrupt the file.
fn write_injections_json(
    path: &std::path::Path,
    injections: &[(u64, String, bool)],
) -> std::io::Result<()> {
    let mut json = String::from("[\n");
    for (idx, (generation, label, applied)) in injections.iter().enumerate() {
        if idx > 0 {
            json.push_str(",\n");
        }
        let _ = write!(
            json,
            "  {{\"generation\": {generation}, \"label\": \"{}\", \"applied\": {applied}}}",
            json_escape(label),
        );
    }
    json.push_str("\n]\n");
    std::fs::write(path, json)
}

/// Minimal JSON string escaping (quotes / backslashes / control chars) so a label can never break the
/// `injections.json` array. std-only (no serde dependency added here).
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
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

/// Drive a fresh stepwise [`Simulation`] for run `i` and write a compact per-cell render snapshot
/// (`snap_<generation>.bin`, SPEC §5/§W10) every [`SNAPSHOT_EPOCH`] generations and at the final
/// generation. With multiple run indices the files are namespaced by run (`run<i>/`) to avoid collisions.
///
/// Read-only & ADDITIVE: `snapshot()` draws no RNG and mutates nothing, and the determinism hash comes
/// solely from the one-shot `run_headless` path above — so emitting snapshots cannot change it (inv. #3).
/// Generation `0` (the initial state, before any step) is also captured.
fn write_snapshots(
    base: &std::path::Path,
    i: u32,
    multi_run: bool,
    cfg: &SimConfig,
    grid: (u32, u32),
) -> std::io::Result<()> {
    let dir = if multi_run {
        base.join(format!("run{i}"))
    } else {
        base.to_path_buf()
    };
    std::fs::create_dir_all(&dir)?;

    let (w, h) = grid;
    let mut sim = Simulation::reset(cfg);
    let write_one = |sim: &mut Simulation, gen: u64| -> std::io::Result<()> {
        sim.snapshot(w, h)
            .write_to(dir.join(format!("snap_{gen}.bin")))
    };

    // Initial state, then one snapshot per epoch and a final one (deduped if the end lands on an epoch).
    write_one(&mut sim, 0)?;
    for gen in 1..=cfg.generations {
        sim.step(1);
        if gen % SNAPSHOT_EPOCH == 0 || gen == cfg.generations {
            write_one(&mut sim, gen)?;
        }
    }
    Ok(())
}

/// Demo CRISPR edits exported as named specimens (a Cas + the species-genome locus + a guide that targets
/// it). These are deliberately fixed presets that exercise the on-target path on the sample genome's two
/// loci; any outcome (Applied **or** Failed) mutates the genome, so each specimen's phenotype differs from
/// the baseline — that is exactly the "an edit visibly changes morphology" demo the renderer draws (S4.5).
fn demo_edits() -> Vec<(&'static str, EditAction)> {
    let cas = |name: &str| default_cas_variants().into_iter().find(|v| v.name == name);
    let mut out = Vec::new();
    if let (Some(sp), Ok(g)) = (cas("SpCas9"), GuideSequence::new(*b"ACGTGGACGTTTTAGGCCGG")) {
        out.push((
            "SpCas9 → growth_locus",
            EditAction {
                cas: sp.id,
                target: LocusId(0),
                guide: g,
            },
        ));
    }
    if let (Some(asc), Ok(g)) = (
        cas("AsCas12a"),
        GuideSequence::new(*b"TTTACCGGTTTAGGGCAAAC"),
    ) {
        out.push((
            "AsCas12a → killswitch_locus",
            EditAction {
                cas: asc.id,
                target: LocusId(1),
                guide: g,
            },
        ));
    }
    out
}

/// JSON of the baseline **species genome**: each locus' name, ontology tags (Sequence Ontology / Gene
/// Ontology term ids, formatted `SO:0000704` / `GO:0008150`), and unit-scaled parameter values. The renderer
/// surfaces these in its detail panel — track-B (Stage 5 ontology) prep. Read-only; no biology in the export
/// beyond what the core already defines (`genome::sample_genome` is the species baseline used at reset).
fn genome_json() -> String {
    let g = genome::sample_genome();
    let mut loci = String::new();
    for (i, locus) in g.loci.iter().enumerate() {
        let go = locus
            .tags
            .go_refs
            .iter()
            .map(|r| format!("\"GO:{:07}\"", r.0))
            .collect::<Vec<_>>()
            .join(",");
        let params = locus
            .parameters
            .iter()
            .map(|p| format!("{:.4}", p.value.as_unit_scalar()))
            .collect::<Vec<_>>()
            .join(",");
        if i > 0 {
            loci.push_str(",\n");
        }
        let _ = write!(
            loci,
            "      {{\"name\":\"{}\",\"so_term\":\"SO:{:07}\",\"go_refs\":[{}],\"params\":[{}]}}",
            locus.name, locus.tags.so_term.0, go, params
        );
    }
    format!("{{\"loci\": [\n{loci}\n    ]}}")
}

/// JSON object of the 5 trait values (canonical [`Trait::ALL`] order) carried by an [`Observation`].
fn traits_json(o: &Observation) -> String {
    let p = &o.phenotype;
    format!(
        concat!(
            "{{\"growth_rate\":{:.6},\"reflectance\":{:.6},\"drought_tolerance\":{:.6},",
            "\"fecundity\":{:.6},\"kill_switch_linkage\":{:.6}}}"
        ),
        p.get(Trait::GrowthRate).unwrap_or(0.0),
        p.get(Trait::Reflectance).unwrap_or(0.0),
        p.get(Trait::DroughtTolerance).unwrap_or(0.0),
        p.get(Trait::Fecundity).unwrap_or(0.0),
        p.get(Trait::KillSwitchLinkage).unwrap_or(0.0),
    )
}

/// Write `specimens.json`: the baseline species-genome phenotype plus one phenotype per demo edit, for the
/// renderer's L-system plant view (S4.5). The genotype→phenotype map (invariant #2) runs in the core via a
/// [`GeneSimEnv`]; the renderer only reads these trait vectors and maps them to plant visuals.
///
/// Read-only & ADDITIVE: the env is a SEPARATE instance with its own seeded RNG (it never touches the hashed
/// `run_headless` stream), so this cannot change the determinism hash (invariant #3). For a fixed
/// `(seed, entity_count)` the bytes are reproducible.
fn write_specimens(
    base: &std::path::Path,
    i: u32,
    multi_run: bool,
    cfg: &SimConfig,
) -> std::io::Result<()> {
    let dir = if multi_run {
        base.join(format!("run{i}"))
    } else {
        base.to_path_buf()
    };
    std::fs::create_dir_all(&dir)?;

    // Baseline: the unedited species-genome phenotype (env.reset returns the gen-0 observation).
    let mut env = GeneSimEnv::new(cfg.entity_count);
    let baseline = env.reset(cfg.seed);

    let mut edits_json = String::new();
    for (idx, (label, edit)) in demo_edits().into_iter().enumerate() {
        // Fresh env per edit so each is applied to the BASELINE genome (independent, not cumulative).
        let mut e = GeneSimEnv::new(cfg.entity_count);
        e.reset(cfg.seed);
        let after = e.step(Action::ApplyEdit(edit)).obs;
        let applied = matches!(e.last_edit(), Some(crispr::EditOutcome::Applied { .. }));
        if idx > 0 {
            edits_json.push_str(",\n");
        }
        let _ = write!(
            edits_json,
            "    {{\"label\":\"{label}\",\"applied\":{applied},\"traits\":{}}}",
            traits_json(&after)
        );
    }

    let json = format!(
        "{{\n  \"baseline\": {{\"label\":\"baseline\",\"traits\":{}}},\n  \"genome\": {},\n  \"edits\": [\n{}\n  ]\n}}\n",
        traits_json(&baseline),
        genome_json(),
        edits_json
    );
    std::fs::write(dir.join("specimens.json"), json)
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
