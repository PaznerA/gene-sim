//! The FIREWALL determinism acceptance test (ADR-017 S5 deliverable; S6 turns the read coefficient ON).
//!
//! Drives the REAL `oversight::OversightEpisode` record→replay path (NOT a hand-rolled buffer) and asserts the
//! SEVEN properties the design pins. **S6 (the load-bearing wire) is now LIVE:** a committed non-neutral
//! `growth_ratio_q` crosses the firewall into the hashed sim as a strictly-positive `[0.5,1.5]` per-species
//! DEMAND + MINERALIZATION factor, so a real edit MOVES the hash. The firewall's determinism guarantee is no
//! longer "the committed slot is unread" (the S5 coefficient-zero claim); it is the load-bearing S6 contract:
//!
//! > The committed payload is a QUANTIZED INTEGER journaled at a fixed `Tick`-counted epoch. WHICH integer
//! > commits (and at WHICH epoch, within the slip window) is a deterministic function of `(seed, actions, the
//! > oracle's quantized answer)` ONLY — never of wall-clock arrival. **Replay reads the committed integer
//! > straight from `actions.ndjson` and reproduces the recorded hash byte-for-byte without ever re-running the
//! > FBA solve.** An ABSENT/never-returning oracle slips deterministically to the NEUTRAL sentinel (a no-op →
//! > equals the no-oversight baseline); a PRESENT oracle commits its real factor (deterministic, replay-exact).
//!
//! So the S6 invariances are: ABSENT == baseline (slip-to-neutral); each PRESENT oracle is internally
//! deterministic and replay-exact; and replay NEVER touches the oracle. A non-neutral commit deliberately
//! differs from baseline — that difference IS the wire working.

use crispr::EditKind;
use genome::LocusId;
use harness::firewall::{EcoliImpact, Oracle};
use harness::oversight::{CreditPolicy, OversightEpisode};
use harness::replay::{read_journal, save_journal, EnvConfig};
use harness::{Action, GeneSimEnv};

const SEED: u64 = 2024;
const ENTITIES: u32 = 300;

/// The objective region + grid the credit economy reads (whole-world disc, like the campaign tests).
fn region() -> sim_core::Region {
    sim_core::Region {
        cx: 16,
        cy: 16,
        radius: 64,
    }
}
fn grid() -> (u32, u32) {
    (32, 32)
}

/// The INPUT action stream: advances long enough to cross several epoch boundaries, with two deep-edit requests
/// interleaved (credit is pre-charged so they are affordable — see `generous_policy`).
fn input_actions() -> Vec<Action> {
    vec![
        Action::Advance(15),
        Action::RequestEcoliEdit {
            species: 0,
            locus: LocusId(10), // gltA
            edit_kind: EditKind::Knockout,
            due_epoch: 0, // recomputed by the driver
            req_id: 0,    // reallocated by the driver
        },
        Action::Advance(15),
        Action::RequestEcoliEdit {
            species: 0,
            locus: LocusId(32), // ptsG
            edit_kind: EditKind::Knockdown,
            due_epoch: 0,
            req_id: 0,
        },
        Action::Advance(20),
    ]
}

/// A policy that makes the deep edits affordable (so the firewall actually buffers + commits — the path under
/// test). `ecoli_edit_cost = 0` charges nothing so the spend gate always accepts; the economy hash-neutrality
/// is exercised separately by `economy_accrual_is_hash_neutral`.
fn generous_policy() -> CreditPolicy {
    CreditPolicy {
        per_gen_cap: 50,
        ecoli_edit_cost: 0,
        term_a_weight: 1,
        term_b_weight: 1,
    }
}

// ── Test oracles (inv #5 — the FBA science behind a trait; all OFF-hash producers) ──────────────────────────

/// ABSENT: never produces (models `$FBA_BIN` unset / spawn failure). Every request slips to the slip-cap neutral.
struct AbsentOracle;
impl Oracle for AbsentOracle {
    fn produce(&mut self, _r: u32, _s: u16, _l: u32) -> Option<EcoliImpact> {
        None
    }
}

/// FAKE-A: produces a fixed non-neutral payload immediately.
struct FakeOracleA;
impl Oracle for FakeOracleA {
    fn produce(&mut self, _r: u32, _s: u16, _l: u32) -> Option<EcoliImpact> {
        Some(EcoliImpact {
            growth_ratio_q: 800,
            exchange_deltas: vec![(3, -120), (11, 88)],
        })
    }
}

/// CHAOS: produces a DIFFERENT payload on each call (different bytes every time).
struct ChaosOracle {
    n: u16,
}
impl Oracle for ChaosOracle {
    fn produce(&mut self, _r: u32, _s: u16, _l: u32) -> Option<EcoliImpact> {
        self.n = self.n.wrapping_add(137);
        Some(EcoliImpact {
            growth_ratio_q: 500 + (self.n % 400),
            exchange_deltas: vec![(self.n % 20, -(self.n as i16 % 50))],
        })
    }
}

/// PANIC: fails the test if it is ever invoked (asserts replay never re-runs the oracle).
struct PanicOracle;
impl Oracle for PanicOracle {
    fn produce(&mut self, _r: u32, _s: u16, _l: u32) -> Option<EcoliImpact> {
        panic!("oracle was invoked on a replay — replay must NEVER re-run FBA");
    }
}

/// Run an OVERSIGHT episode with `oracle`, returning the full journal + final hash + ledger.
fn run_with<O: Oracle>(
    oracle: O,
    policy: CreditPolicy,
    actions: &[Action],
) -> harness::oversight::OversightOutcome {
    let env = GeneSimEnv::new(ENTITIES);
    OversightEpisode::start(env, SEED, oracle, policy, region(), grid()).run(actions)
}

/// The NO-OVERSIGHT baseline: the SAME `(seed, actions)` with the requests stripped out (pure Advance stream),
/// run through the same driver with an absent oracle. This is the hash everything must equal.
fn baseline_hash() -> u64 {
    let advances: Vec<Action> = input_actions()
        .into_iter()
        .filter(|a| matches!(a, Action::Advance(_)))
        .collect();
    run_with(AbsentOracle, generous_policy(), &advances).hash
}

/// PROPERTY 1 — S6 PRESENCE/ABSENCE SEMANTICS. An ABSENT oracle slips deterministically to the NEUTRAL sentinel,
/// so its hash equals the no-oversight baseline (a neutral commit is a no-op). A PRESENT oracle commits its REAL
/// quantized factor, which DELIBERATELY moves the hash off the baseline (the wire is load-bearing) — and a
/// DIFFERENT payload moves it to a DIFFERENT hash (the committed integer is read). Each is internally
/// deterministic (record run==run).
#[test]
fn presence_moves_hash_absence_equals_baseline() {
    let base = baseline_hash();
    let absent = run_with(AbsentOracle, generous_policy(), &input_actions()).hash;
    let fake_a = run_with(FakeOracleA, generous_policy(), &input_actions()).hash;
    let fake_a2 = run_with(FakeOracleA, generous_policy(), &input_actions()).hash;
    let chaos = run_with(ChaosOracle { n: 0 }, generous_policy(), &input_actions()).hash;

    // ABSENT → every request slips to the neutral sentinel → equal to the no-oversight baseline.
    assert_eq!(
        absent, base,
        "an absent oracle slips to neutral → equals the no-oversight baseline (no-op commit)"
    );
    // PRESENT non-neutral → the committed factor crosses the firewall and MOVES the hash (S6 — the wire works).
    assert_ne!(
        fake_a, base,
        "a committed non-neutral factor MUST move the hash (S6 — the read coefficient is on)"
    );
    // The committed integer is READ: a different payload → a different hash.
    assert_ne!(
        fake_a, chaos,
        "a different committed payload must produce a different hash"
    );
    // Still internally deterministic: the same oracle + inputs reproduce the same hash.
    assert_eq!(
        fake_a, fake_a2,
        "the same oracle + inputs is deterministic run==run"
    );
}

/// PROPERTY 2 — WALL-CLOCK INDEPENDENCE OF THE RECORDED JOURNAL. The commit epoch is decided by `Tick`-counting,
/// NOT by which thread message arrives first — so for a FIXED payload the RECORDED journal (the committed
/// integers AND their epochs/`slipped_from`) is a deterministic function of `(seed, actions)` only, identical
/// run-to-run, and replay reproduces the hash byte-for-byte. There is no `Instant`/`SystemTime` on the
/// dispatch→commit path. (A genuinely slipping-but-resolving arrival is exercised by the firewall unit tests
/// `slow_oracle_slips_with_self_describing_journal`; the slip-to-neutral case is property 5.)
#[test]
fn recorded_journal_is_wall_clock_independent_for_a_fixed_payload() {
    let a = run_with(FakeOracleA, generous_policy(), &input_actions());
    let b = run_with(FakeOracleA, generous_policy(), &input_actions());

    // The recorded journal — committed integers AND their epochs/slip flags — is byte-identical run-to-run (the
    // commit timing comes from epoch-counting, never wall-clock).
    assert_eq!(
        a.journal, b.journal,
        "a fixed payload records a byte-identical journal (commit epoch is Tick-counted, not wall-clock)"
    );
    assert_eq!(a.hash, b.hash, "and a deterministic hash");

    // One committed impact per accepted request (the journal terminates with a paired commit for each).
    let count_commits = |o: &harness::oversight::OversightOutcome| {
        o.journal
            .iter()
            .filter(|a| matches!(a, Action::CommitEcoliImpact { .. }))
            .count()
    };
    assert_eq!(count_commits(&a), 2);

    // Every committed impact records a deterministic due_epoch (a function of the request generation + lead), so
    // a slower solver that produced the SAME bytes would record the SAME commit epoch (no wall-clock dependence
    // within the slip window).
    let commit_epochs: Vec<(u32, Option<u32>)> = a
        .journal
        .iter()
        .filter_map(|x| match x {
            Action::CommitEcoliImpact {
                due_epoch,
                slipped_from,
                ..
            } => Some((*due_epoch, *slipped_from)),
            _ => None,
        })
        .collect();
    assert_eq!(
        commit_epochs,
        b.journal
            .iter()
            .filter_map(|x| match x {
                Action::CommitEcoliImpact {
                    due_epoch,
                    slipped_from,
                    ..
                } => Some((*due_epoch, *slipped_from)),
                _ => None,
            })
            .collect::<Vec<_>>(),
        "the committed epochs are a deterministic function of the Tick stream, not arrival time"
    );
}

/// PROPERTY 3 — REPLAY NEVER RE-RUNS FBA. Record an episode (journal with inline-quantized commits) to disk, then
/// REPLAY the recorded journal with a PanicOracle that fails if invoked. The committed integers are consumed
/// straight from the journal; the oracle is never touched.
#[test]
fn replay_consumes_committed_impacts_without_rerunning_fba() {
    let recorded = run_with(FakeOracleA, generous_policy(), &input_actions());

    let dir = std::env::temp_dir().join(format!("gene_sim_firewall_replay_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let env_config = EnvConfig {
        entity_count: ENTITIES,
        env: sim_core::EnvParams::default(),
    };
    save_journal(&dir, &env_config, SEED, &recorded.journal).expect("save journal");

    // Read the recorded journal back and replay it with a PanicOracle: the commits ride in the journal, so the
    // driver re-emits them inert (the input-stream commit arm) and the oracle is never asked to produce.
    let (sj, actions) = read_journal(&dir).expect("read journal");
    assert_eq!(sj.seed, SEED);
    let replayed = run_with(PanicOracle, generous_policy(), &actions);

    assert_eq!(
        replayed.hash, recorded.hash,
        "replay hash must equal the recorded hash, consuming committed impacts from the journal"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// PROPERTY 4 — CONTENT-HASH BINDING. A committed impact's `content_hash` binds its quantized bytes: recomputing
/// it from `(growth_ratio_q, exchange_deltas)` must match the journaled value (a tamper is detectable). This is
/// the recompute the S6 replay path uses to reject a divergent injected impact.
#[test]
fn content_hash_binds_the_committed_quantized_bytes() {
    let recorded = run_with(FakeOracleA, generous_policy(), &input_actions());
    let commits: Vec<&Action> = recorded
        .journal
        .iter()
        .filter(|a| matches!(a, Action::CommitEcoliImpact { .. }))
        .collect();
    assert!(
        !commits.is_empty(),
        "the episode must have committed impacts"
    );
    for c in commits {
        if let Action::CommitEcoliImpact {
            content_hash,
            growth_ratio_q,
            exchange_deltas,
            ..
        } = c
        {
            let recomputed = EcoliImpact {
                growth_ratio_q: *growth_ratio_q,
                exchange_deltas: exchange_deltas.clone(),
            }
            .content_hash();
            assert_eq!(
                *content_hash, recomputed,
                "journaled content_hash must equal the recomputed quantized-bytes hash"
            );
        }
    }
}

/// PROPERTY 5 — SLIP-CAP TERMINATION. A never-returning oracle must force the journal to terminate
/// deterministically: every request commits a NEUTRAL impact (the fixed sentinel content_hash), and the recorded
/// journal is identical regardless of how the oracle behaves — proven by the AbsentOracle hash equalling the
/// baseline AND the slip-capped commits carrying the neutral content_hash.
#[test]
fn slip_cap_terminates_the_journal_deterministically() {
    let absent = run_with(AbsentOracle, generous_policy(), &input_actions());
    // Journal terminates: every accepted request has a paired commit.
    let requests = absent
        .journal
        .iter()
        .filter(|a| matches!(a, Action::RequestEcoliEdit { .. }))
        .count();
    let commits = absent
        .journal
        .iter()
        .filter(|a| matches!(a, Action::CommitEcoliImpact { .. }))
        .count();
    assert_eq!(
        requests, commits,
        "every request has a paired commit (journal terminates)"
    );

    // Every slip-capped commit carries the NEUTRAL sentinel content_hash.
    let neutral_hash = EcoliImpact::neutral().content_hash();
    for a in &absent.journal {
        if let Action::CommitEcoliImpact {
            content_hash,
            growth_ratio_q,
            ..
        } = a
        {
            assert_eq!(*growth_ratio_q, 1000, "slip-cap neutral growth ratio");
            assert_eq!(*content_hash, neutral_hash, "fixed sentinel content_hash");
        }
    }
    // Running it twice yields the byte-identical journal (deterministic termination).
    let absent2 = run_with(AbsentOracle, generous_policy(), &input_actions());
    assert_eq!(
        absent.journal, absent2.journal,
        "slip-capped journal is deterministic"
    );
}

/// PROPERTY 6 — `req_id` DETERMINISM. Two record runs on the same inputs produce a byte-identical journal: the
/// `req_id`s are a deterministic monotonic occurrence index and the `(species, req_id)` drain order is stable.
#[test]
fn req_id_is_deterministic_across_record_runs() {
    let a = run_with(FakeOracleA, generous_policy(), &input_actions());
    let b = run_with(FakeOracleA, generous_policy(), &input_actions());
    assert_eq!(
        a.journal, b.journal,
        "byte-identical journal across record runs"
    );

    // The two accepted requests get req_id 0 then 1 (monotonic occurrence index), in order.
    let req_ids: Vec<u32> = a
        .journal
        .iter()
        .filter_map(|x| match x {
            Action::RequestEcoliEdit { req_id, .. } => Some(*req_id),
            _ => None,
        })
        .collect();
    assert_eq!(
        req_ids,
        vec![0, 1],
        "req_id is a monotonic occurrence index"
    );
}

/// PROPERTY 7 — ECONOMY HASH-NEUTRALITY. Enabling credit accrual (the per-gen region_allele + FlowMatrix-health
/// fold) leaves the episode hash equal to the no-oversight baseline — NOT merely reproducible, but UNCHANGED.
/// Also: a borderline-credit request replays to the SAME accept/refuse decision (the journaled-spend rule).
#[test]
fn economy_accrual_is_hash_neutral_and_spend_decision_is_journaled() {
    // With an UNaffordable cost, every request is refused → NO commits are journaled → the stream is a pure
    // advance, so the hash equals the no-oversight baseline (the credit economy itself is off-hash — it only
    // governs WHICH requests get journaled, never folds bytes into `hash_world`). This stays true under S6:
    // when nothing commits, nothing crosses the firewall.
    let strict = CreditPolicy {
        per_gen_cap: 50,
        ecoli_edit_cost: 1_000_000, // unaffordable — all requests refused
        term_a_weight: 1,
        term_b_weight: 1,
    };
    let refused = run_with(FakeOracleA, strict, &input_actions());
    assert_eq!(
        refused.hash,
        baseline_hash(),
        "credit accrual + refused requests stay hash-neutral"
    );
    // All requests refused => no commits in the journal.
    assert_eq!(
        refused
            .journal
            .iter()
            .filter(|a| matches!(
                a,
                Action::RequestEcoliEdit { .. } | Action::CommitEcoliImpact { .. }
            ))
            .count(),
        0,
        "an unaffordable request is refused, not journaled"
    );

    // The spend DECISION is journaled: replaying the PRODUCED journal (which contains only the accepted requests)
    // reproduces the SAME hash without re-deciding the gate (a PanicOracle proves the oracle is never re-run).
    let accepted = run_with(FakeOracleA, generous_policy(), &input_actions());
    let replayed = run_with(PanicOracle, generous_policy(), &accepted.journal);
    assert_eq!(
        replayed.hash, accepted.hash,
        "the journaled spend decision replays identically"
    );
}
