//! D1 trace-capture integration tests (Stage 2): the off-hash capture seam grounded on a REAL headless run.
//!
//! Two load-bearing properties (the slice acceptance criteria):
//!  1. CAPTURE IS OFF-HASH (inv #3). A real multi-species run captured into a `PerGenTrace` produces the
//!     byte-identical `run_stats().hash` as the same run WITHOUT capture — `observe_all`/`flow_matrix` draw
//!     zero `SimRng` and are never folded into `hash_world`. And the per-generation reads `capture_trace`
//!     performs leave the PINNED literal `0x47a0_3c8f_6701_f240` unmoved (proven directly against sim-core's
//!     pinned `Simulation` config, where `generations` is the hashed metadata that anchors the literal).
//!  2. The synthetic D0 oracle, GROUNDED on a real trace: a living multi-species predator/prey run scores a
//!     sane NON-degenerate `quality`; a dead/monoculture run scores ≈0 (the M6 gate / M1=M2=M4=M5 zeros).

use discovery::{DefaultScorer, InterestingnessScorer};
use genome::spec::BuiltSpecies;
use harness::capture::capture_trace;
use harness::{Action, Env, GeneSimEnv};
use sim_core::{SimConfig, Simulation};

/// The pinned determinism config (sim-core's `determinism_hash_is_pinned` test): seed / 1000 entities / 50
/// generations → `0x47a0_3c8f_6701_f240`. `generations` is folded into `hash_world`, so the literal is
/// anchored to a `Simulation` driven with `generations: 50` (NOT the env path, which carries `generations:
/// 0` metadata — a distinct-but-valid deterministic hash).
const PINNED_SEED: u64 = 13_679_457_532_755_275_413;
const PINNED_GENS: u32 = 50;
const PINNED_HASH: u64 = 0x47a0_3c8f_6701_f240;

/// Load a baked species spec from `data/species/<stem>.json` (the byte-mover boundary; mirrors the lib/replay
/// test helpers).
fn load_stem(stem: &str) -> BuiltSpecies {
    harness::species::load_species_file(format!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/species/{}.json"),
        stem
    ))
    .unwrap_or_else(|e| panic!("{stem}.json loads: {e}"))
}

/// A real predator/prey/producer roster: the abstract plant (autotroph) + E. coli (decomposer prey) +
/// Bdellovibrio (the F6 predator). This closes plant→detritus→decomposer→predator, so a captured run shows a
/// non-trivial FlowMatrix and multi-species dynamics — the real grounding for the D0 oracle.
fn predator_prey_roster() -> Vec<(BuiltSpecies, u32)> {
    vec![
        (load_stem("default"), 600),
        (load_stem("ecoli"), 400),
        (load_stem("bdellovibrio"), 120),
    ]
}

#[test]
fn capture_is_hash_neutral_on_a_real_multi_species_run() {
    // The run WITHOUT capture: reset → one Advance(GENS) → run_stats (the normal episode path).
    let gens = 120u32;
    let seed = 2024u64;

    let mut plain = GeneSimEnv::new(200);
    plain.set_roster(predator_prey_roster());
    plain.reset(seed);
    plain.step(Action::Advance(u64::from(gens)));
    let plain_hash = plain.run_stats().hash;

    // The SAME run WITH capture: capture_trace steps Advance(1) GENS times and reads observe_all/flow_matrix
    // after each — the off-hash projections. The final run_stats().hash MUST be byte-identical (Advance(1)*N
    // drives the identical schedule runs / RNG draws as Advance(N); the reads draw zero SimRng).
    let mut captured = GeneSimEnv::new(200);
    captured.set_roster(predator_prey_roster());
    let trace = capture_trace(&mut captured, seed, gens, &[]);

    assert_eq!(
        trace.recorded_hash, plain_hash,
        "capturing a trace must be HASH-NEUTRAL: Advance(1)*N + reads == Advance(N) (inv #3)"
    );

    // The captured trace is internally consistent with the schema.
    assert_eq!(trace.s as usize, trace.species.len());
    assert_eq!(trace.gens_requested, gens);
    assert_eq!(trace.g as usize, trace.rows.len());
    assert_eq!(trace.s, 3, "the predator/prey roster has three species");
    assert_eq!(trace.seed, seed);
    for row in &trace.rows {
        assert_eq!(row.pop.len(), trace.species.len(), "pop is per-species");
        assert_eq!(row.allele_q.len(), trace.species.len());
    }
    // The Bdellovibrio entry carries the Predator role ordinal (4) — the role mapping is correct.
    assert!(
        trace.species.iter().any(|m| m.role == 4),
        "the roster must include the Predator role ordinal (Bdellovibrio)"
    );
}

#[test]
fn capture_reads_keep_the_pinned_literal_unmoved() {
    // INVARIANT #3 anchor: the per-generation reads `capture_trace` performs — `observe_all()` +
    // `flow_matrix()` after each `step(1)` — are exactly the reads sim-core proves off-hash. Driving the
    // PINNED config (`generations: 50`, the hashed metadata that anchors the literal) one generation at a
    // time WITH those reads must leave `0x47a0_3c8f_6701_f240` UNMOVED. This is the harness-side mirror of
    // sim-core's `species_signatures_export_is_hash_neutral`, using the precise capture read pattern.
    let cfg = SimConfig {
        seed: PINNED_SEED,
        generations: u64::from(PINNED_GENS),
        entity_count: 1000,
    };
    let mut sim = Simulation::reset(&cfg);
    for _ in 0..cfg.generations {
        // The capture reads, taken mid-run — pure projections that draw zero SimRng and are never hashed.
        let obs = sim.observe_all();
        let (s, flat) = sim.flow_matrix();
        assert_eq!(flat.len(), s * s, "flow matrix is s*s");
        assert!(!obs.is_empty());
        sim.step(1);
    }
    assert_eq!(
        sim.run_stats().hash,
        PINNED_HASH,
        "the capture read pattern is hash-neutral: 0x47a0_3c8f_6701_f240 is UNMOVED (inv #3)"
    );
}

#[test]
fn monoculture_capture_hash_is_stable_under_capture() {
    // The env-side pinned single-species PLANT path: capturing it generation-by-generation is hash-neutral
    // w.r.t. the env's OWN deterministic hash (the env carries `generations: 0` metadata, so its literal
    // differs from the sim-core `generations: 50` literal — but capture must not perturb it).
    let mut plain = GeneSimEnv::new(1000);
    plain.reset(PINNED_SEED);
    plain.step(Action::Advance(u64::from(PINNED_GENS)));
    let plain_hash = plain.run_stats().hash;

    let mut captured = GeneSimEnv::new(1000);
    let trace = capture_trace(&mut captured, PINNED_SEED, PINNED_GENS, &[]);
    assert_eq!(
        trace.recorded_hash, plain_hash,
        "capturing the single-species plant run must be hash-neutral (captured == un-captured)"
    );
    assert_eq!(
        trace.s, 1,
        "the env single-species path is monoculture (plant)"
    );
}

#[test]
fn d0_scorer_grounds_on_real_traces_living_high_dead_low() {
    // GROUND the synthetic oracle on REAL captured traces: a living multi-species predator/prey run earns a
    // sane non-degenerate quality; a monoculture run scores ≈0.
    let scorer = DefaultScorer::default();

    // (A) A living predator/prey/producer roster run — multi-species, non-trivial FlowMatrix.
    let mut living = GeneSimEnv::new(200);
    living.set_roster(predator_prey_roster());
    let live_trace = capture_trace(&mut living, 2024, 200, &[]);
    let live = scorer.score(&live_trace);

    // It actually RAN (no instant collapse): the trace captured a healthy fraction of the requested horizon.
    assert!(
        f64::from(live_trace.g) >= f64::from(live_trace.gens_requested) * 0.5,
        "a living run must capture a healthy fraction of its horizon (got g={} of {})",
        live_trace.g,
        live_trace.gens_requested
    );
    // A real living multi-species ecology earns a sane, non-degenerate quality (well clear of the dead floor).
    assert!(
        live.quality >= 50_000,
        "a living multi-species run must earn a non-degenerate Q (got {}, breakdown {:?})",
        live.quality,
        live.breakdown
    );

    // (D) A MONOCULTURE: the single-species plant config. Coexistence/evenness/trophic/events are all zero by
    // construction (one species), so Q is ≈0 — the synthetic oracle's "mere survival is not interesting".
    let mut mono = GeneSimEnv::new(1000);
    let mono_trace = capture_trace(&mut mono, PINNED_SEED, PINNED_GENS, &[]);
    let monoc = scorer.score(&mono_trace);
    assert_eq!(
        mono_trace.s, 1,
        "the grounding monoculture is single-species"
    );
    assert!(
        monoc.quality < 1_000,
        "a monoculture must score ≈0 (got {}, breakdown {:?})",
        monoc.quality,
        monoc.breakdown
    );

    // The ORDERING is the point: a living multi-species ecology strictly out-scores a monoculture.
    assert!(
        live.quality > monoc.quality,
        "a living multi-species run ({}) must out-score a monoculture ({})",
        live.quality,
        monoc.quality
    );
}
