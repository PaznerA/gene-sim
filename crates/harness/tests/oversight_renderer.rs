//! Renderer-driven OVERSIGHT plumbing determinism (ADR-017 S4–S6).
//!
//! The godot-sim `oversight_state` / `preview_ecoli_edit` / `commit_ecoli_edit` `#[func]`s are THIN marshalling
//! over THIS harness surface (`GeneSimEnv::enable_oversight` / `oversight_status` / `preview_ecoli_edit` /
//! `commit_ecoli_edit`) — every economy/biology decision stays in the core/harness (inv #2). This test pins the
//! two determinism properties the binding relies on, WITHOUT a Godot runtime (the builtins need an engine; the
//! load-bearing facts are harness-level):
//!
//!   1. **A committed deep edit is byte-deterministic (replay-equal).** The binding records the
//!      `RequestEcoliEdit`/`CommitEcoliImpact` pair `commit_ecoli_edit` returns into the session journal; a FRESH
//!      env (NO oversight — the committed integer rides inline in the journal) replays it to the SAME
//!      `run_stats().hash` (inv #3). A non-neutral commit also MOVES the hash off the no-edit baseline, so the
//!      replay-equality is non-trivial (the S6 wire is live).
//!   2. **The plumbing is hash-neutral.** Enabling the earn loop + accruing credit on a run that commits NOTHING
//!      is byte-identical to the oversight-disabled run (accrual is a pure off-hash integer fold), and the
//!      sim-core PINNED config still produces `0x47a0_3c8f_6701_f240` (the plumbing lives in the harness/binding,
//!      never sim-core — the literal is structurally unmoved).

use harness::oversight::CreditPolicy;
use harness::{Action, Env, GeneSimEnv};

const SEED: u64 = 2024;
const ENTITIES: u32 = 300;

/// The renderer policy with a ZERO spend gate so the deep edit is always affordable — the path under test is the
/// firewall CROSSING + journaling, not the credit magnitude (that is covered by the `oversight` unit tests). The
/// binding enables the DEFAULT policy; the zero-cost variant here just removes the accrual-magnitude dependency.
fn renderer_policy() -> CreditPolicy {
    CreditPolicy {
        per_gen_cap: 50,
        ecoli_edit_cost: 0,
        term_a_weight: 1,
        term_b_weight: 1,
    }
}

#[test]
fn renderer_committed_edit_is_replay_equal() {
    // LIVE: enable oversight (the binding does this at `reset`), advance, commit a NON-neutral deep edit (the
    // binding journals the returned Request/Commit pair), advance again so the committed factor is consumed. We
    // build the SAME journal the binding keeps.
    let mut journal: Vec<Action> = Vec::new();
    let mut live = GeneSimEnv::new(ENTITIES);
    live.enable_oversight(renderer_policy());
    live.reset(SEED);

    live.step(Action::Advance(20));
    journal.push(Action::Advance(20));

    // A read-only preview must not perturb the run (it draws zero SimRng): the hash is taken at the end and is
    // proven equal to the replay, which never previews.
    let _preview = live.preview_ecoli_edit(0, 700);
    let status_before = live.oversight_status();
    assert!(status_before.committed.is_empty(), "nothing committed yet");

    let commit = live.commit_ecoli_edit(0, 700, 0); // species 0, KO-grade ratio (<1000), due-epoch floor
    assert!(commit.applied, "the zero-cost spend gate accepts the edit");
    assert_eq!(
        commit.journaled.len(),
        2,
        "an applied commit journals a RequestEcoliEdit + CommitEcoliImpact pair"
    );
    journal.extend(commit.journaled.iter().cloned());

    // The INSPECT view now reflects the committed edit (the renderer reads this through `oversight_state`).
    let status_after = live.oversight_status();
    assert_eq!(status_after.committed.len(), 1, "one committed deep edit");
    assert_eq!(status_after.committed[0].growth_ratio_q, 700);

    live.step(Action::Advance(20)); // the committed factor throttles species 0 over THIS advance
    journal.push(Action::Advance(20));
    let live_hash = live.run_stats().hash;

    // REPLAY: a FRESH env (NO oversight — the committed integer is inline in the journaled CommitEcoliImpact)
    // replays the recorded journal and must reproduce the live hash byte-for-byte (inv #3).
    let mut replay = GeneSimEnv::new(ENTITIES);
    replay.reset(SEED);
    for action in &journal {
        let _ = replay.step(action.clone());
    }
    let replay_hash = replay.run_stats().hash;
    assert_eq!(
        replay_hash, live_hash,
        "the committed oversight edit replays byte-identically from the journal (inv #3)"
    );

    // The wire is LIVE: the SAME advance count (40 gens) with NO edit yields a DIFFERENT hash — so replay-equality
    // above is a real reproduction of an effectful edit, not a trivial pass on an inert no-op.
    let mut baseline = GeneSimEnv::new(ENTITIES);
    baseline.reset(SEED);
    baseline.step(Action::Advance(40));
    let baseline_hash = baseline.run_stats().hash;
    assert_ne!(
        live_hash, baseline_hash,
        "a committed non-neutral deep edit actually perturbs the run (the S6 firewall wire)"
    );
}

#[test]
fn oversight_plumbing_is_hash_neutral() {
    // Enabling the earn loop + accruing credit on a run that COMMITS NOTHING is byte-identical to the same run
    // with oversight disabled (accrual is a pure off-hash integer fold over RNG-free read-only projections, inv #3).
    let mut with = GeneSimEnv::new(ENTITIES);
    with.enable_oversight(renderer_policy());
    with.reset(SEED);
    with.step(Action::Advance(40));
    let with_hash = with.run_stats().hash;

    let mut without = GeneSimEnv::new(ENTITIES);
    without.reset(SEED);
    without.step(Action::Advance(40));
    let without_hash = without.run_stats().hash;
    assert_eq!(
        with_hash, without_hash,
        "enabling the oversight earn loop is hash-neutral when no edit commits (inv #3)"
    );

    // The sim-core PINNED config still produces the canonical literal: the oversight plumbing lives in the
    // harness/binding, never sim-core, so `0x47a0_3c8f_6701_f240` is structurally UNMOVED (inv #3).
    let cfg = sim_core::SimConfig {
        seed: 13_679_457_532_755_275_413,
        generations: 50,
        entity_count: 1000,
    };
    assert_eq!(
        sim_core::run_headless(&cfg).hash,
        0x47a0_3c8f_6701_f240,
        "the pinned literal is UNMOVED by the oversight plumbing (inv #3)"
    );
}
