//! The OVERSIGHT earned-credit economy (ADR-017 S4) — a HASH-NEUTRAL integer fold over the per-gen stats stream.
//!
//! The player runs the fast abstract sim and **earns credit** from ecosystem-improvement signals, then **spends**
//! it on a rare, expensive deep E. coli edit (the ADR-017 S5 firewall). This module owns the EARN half:
//! [`CreditLedger`] accrues credit deterministically from signals the engine ALREADY produces.
//!
//! ## Hash-neutral by construction (the `edits_used` precedent)
//! The ledger lives in the **harness/env layer**, NEVER an ECS `World` resource — exactly like
//! [`crate::campaign::ScenarioResult::edits_used`]. So it adds **0 bytes** to `sim_core`'s `hash_world`: the
//! pinned determinism literal `0x4e4d_0520_722a_a069` is unchanged whether or not credit is being accrued. That
//! unchanged-ness IS the neutrality proof (asserted by `oversight_accrual_is_hash_neutral`). It is a **pure
//! integer fold** over RNG-free read-only projections (`region_allele` Term A + `flow_matrix` Term B), so it
//! recomputes byte-identically on replay from `(seed, actions)` (inv #3).
//!
//! ## Quantize discipline (the cross-ISA hazard the design flagged)
//! Term A (`region_allele` toward the objective) is an `f64` mean. We **quantize each gen's mean to `u16`
//! FIRST** via [`sim_core::fixed::to_unit_u16`] (the single audited float→int chokepoint, floor-based,
//! platform-stable), then **difference the integers** (`q_now − q_prev`) — NEVER difference the f64 means and
//! quantize the delta (a cross-arch divergence hazard). Term B (`flow_matrix`) is already `i64`, so its health
//! signal is integer-native. An empty region (`populated_cells == 0`) pins a fixed sentinel (quantized 0), so the
//! zero-population path is deterministic.

use sim_core::fixed::to_unit_u16;

use crate::firewall::{EditFirewall, Oracle, OracleMailbox};
use crate::{Action, Env, GeneSimEnv};

/// Generations per OVERSIGHT epoch — the fixed every-N-generations cadence (off the `Tick` generation counter,
/// NEVER wall-clock). `due_epoch` for a request issued at generation `g` is `epoch_of(g) + EPOCH_LEAD`. Game-
/// design tuning (pinned in code for S5; the design carries a `data/oversight/<name>.json` path for S6).
pub const EPOCH_LEN: u32 = 10;

/// Minimum lead (in epochs) between a request and its earliest possible commit — slack so a slow oracle has time
/// to produce before the first slip. `due_epoch = epoch_of(request_gen) + EPOCH_LEAD`.
pub const EPOCH_LEAD: u32 = 1;

/// The epoch a generation falls in (fixed every-`EPOCH_LEN`-generations cadence).
#[must_use]
pub fn epoch_of(generation: u64) -> u32 {
    (generation / u64::from(EPOCH_LEN)) as u32
}

/// Per-episode tuning for the credit economy (ADR-017 S4 — assumption-class GAME-DESIGN tuning, distinct from
/// biological constants). Pinned here in code for S4; the design carries a `data/oversight/<name>.json` load
/// path (the `load_campaign` precedent) for S5/S6 with its own provenance allowlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreditPolicy {
    /// Maximum credit accrued in a SINGLE generation — clamps a noisy spike so one lucky gen cannot bankroll an
    /// edit. The per-gen term is clamped to `[0, per_gen_cap]` before it is folded in.
    pub per_gen_cap: u64,
    /// Cost of one [`crate::Action::RequestEcoliEdit`] deep edit. The two-tier gate spends this; a request is
    /// REFUSED (journaled, not replayed) when `credit < ecoli_edit_cost` (the `campaign.rs` edit_budget rule).
    pub ecoli_edit_cost: u64,
    /// Weight (multiplier) on Term A — the `region_allele`-toward-objective improvement, in quantized u16 units.
    pub term_a_weight: u64,
    /// Weight (multiplier) on Term B — the FlowMatrix mineralization-health improvement, in flow `i64` units.
    pub term_b_weight: u64,
}

impl Default for CreditPolicy {
    /// A modest default economy: a healthy mineralization gen earns a few credits; an `ecoli_edit_cost` of 100
    /// means a player must sustain improvement for a while before unlocking a deep edit. Pure tuning.
    fn default() -> Self {
        Self {
            per_gen_cap: 50,
            ecoli_edit_cost: 100,
            term_a_weight: 1,
            term_b_weight: 1,
        }
    }
}

/// A single generation's RNG-free, already-quantized stats sample — the input the fold consumes. Built by the
/// harness driver from `region_allele` + `flow_matrix` AFTER each `Advance(1)` (a fixed post-step point, before
/// the next step resets the per-gen FlowMatrix). All fields are integers so the fold never touches a float.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenSample {
    /// Term A: the region's mean allele frequency, quantized to `u16` via [`to_unit_u16`] at THIS gen. `0` when
    /// the region is empty (the pinned empty-region sentinel).
    pub region_allele_q: u16,
    /// Term B: a FlowMatrix mineralization-health scalar — the sum of the off-diagonal plant↔decomposer flows
    /// (both positive = a healthy loop), already `i64` from the integer-native flow ledger.
    pub flow_health: i64,
}

impl GenSample {
    /// Build a sample from the RNG-free read-only projections. `region` is the [`sim_core::RegionReadout`] of the
    /// objective zone; `(s, flat)` is the [`sim_core::Simulation::flow_matrix`] export. Quantize-each-FIRST:
    /// the f64 mean becomes a `u16` HERE (never differenced as a float). An empty region → the `0` sentinel.
    #[must_use]
    pub fn from_projections(region: &sim_core::RegionReadout, s: usize, flat: &[i64]) -> Self {
        let region_allele_q = if region.populated_cells == 0 {
            0 // pinned empty-region sentinel (deterministic zero-population path)
        } else {
            to_unit_u16(region.mean)
        };
        Self {
            region_allele_q,
            flow_health: flow_health(s, flat),
        }
    }
}

/// The FlowMatrix mineralization-health scalar: the sum of the strictly-positive OFF-DIAGONAL flows (the
/// cross-species mineralization loop — plant↔decomposer transfers). Negative/zero off-diagonals contribute
/// nothing. Pure integer over the flat row-major `i64` matrix (`s × s`), so it is byte-identical cross-platform.
/// Iterates the matrix in flat index order — NOT a `HashMap` (inv #3).
#[must_use]
fn flow_health(s: usize, flat: &[i64]) -> i64 {
    let mut acc: i64 = 0;
    for r in 0..s {
        for c in 0..s {
            if r == c {
                continue; // diagonals are self-flow, not a cross-species loop signal
            }
            let v = flat[r * s + c];
            if v > 0 {
                acc = acc.saturating_add(v);
            }
        }
    }
    acc
}

/// The earned-credit ledger (ADR-017 S4). Two `u64` counters in the harness/env layer (the `edits_used`
/// precedent) — adds 0 bytes to `hash_world`. RNG-free: every mutation is a deterministic integer fold over the
/// stats stream, so replay recomputes it byte-identically from `(seed, actions)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CreditLedger {
    /// Spendable credit currently held.
    pub credit: u64,
    /// Total credit ever accrued (monotonic; for the INSPECT view / debugging — never decremented by a spend).
    pub accrued_total: u64,
}

impl CreditLedger {
    /// A fresh ledger (zero credit). Reset at the start of each episode (`reset()`), like `req_id`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold ONE generation's improvement into the ledger. Given the PREVIOUS gen's sample and the CURRENT gen's
    /// sample, computes the per-gen objective/FlowMatrix-health DELTA, quantizes+clamps it to `[0, per_gen_cap]`,
    /// and adds it to both counters:
    ///
    /// ```text
    /// credit += clamp(term_a_weight·max(0, qA_now − qA_prev) + term_b_weight·max(0, flow_now − flow_prev),
    ///                 0, per_gen_cap)
    /// ```
    ///
    /// **Quantize-each-then-difference:** the region-allele term differences the ALREADY-QUANTIZED `u16`s
    /// (`region_allele_q`), never the f64 means. Only IMPROVEMENT is rewarded (each term floored at 0), so a
    /// regressing gen earns nothing rather than a negative (the ledger never goes down on accrual).
    pub fn accrue_gen(&mut self, prev: &GenSample, now: &GenSample, policy: &CreditPolicy) {
        // Term A: improvement in the quantized region-allele toward the objective (integer difference of u16s).
        let a_delta = i64::from(now.region_allele_q) - i64::from(prev.region_allele_q);
        let term_a = if a_delta > 0 {
            policy.term_a_weight.saturating_mul(a_delta as u64)
        } else {
            0
        };
        // Term B: improvement in mineralization health (integer flow delta).
        let b_delta = now.flow_health - prev.flow_health;
        let term_b = if b_delta > 0 {
            policy.term_b_weight.saturating_mul(b_delta as u64)
        } else {
            0
        };
        let gained = term_a.saturating_add(term_b).min(policy.per_gen_cap);
        self.credit = self.credit.saturating_add(gained);
        self.accrued_total = self.accrued_total.saturating_add(gained);
    }

    /// Whether a deep-edit request can be afforded right now (`credit >= ecoli_edit_cost`). The two-tier gate
    /// uses this to decide the spend OUTCOME; the recorder JOURNALS the decision and replay reads it from the
    /// journal (the design's journaled-spend-decision rule), so a borderline credit cannot accept-on-record /
    /// refuse-on-replay.
    #[must_use]
    pub fn can_afford(&self, policy: &CreditPolicy) -> bool {
        self.credit >= policy.ecoli_edit_cost
    }

    /// Spend `ecoli_edit_cost` if affordable, returning whether the spend happened. Structurally identical to the
    /// `campaign.rs` `edits_used < edit_budget` refusal: an unaffordable request is REFUSED (credit untouched),
    /// not replayed.
    pub fn try_spend(&mut self, policy: &CreditPolicy) -> bool {
        if self.can_afford(policy) {
            self.credit -= policy.ecoli_edit_cost;
            true
        } else {
            false
        }
    }
}

/// The OVERSIGHT episode DRIVER (ADR-017 S5) — runs the fast abstract sim while threading the earned-credit
/// economy (S4) and the deep-edit firewall (S5) end-to-end, and PRODUCES the journaled action stream (with the
/// firewall's `CommitEcoliImpact` actions spliced in at their committed epochs).
///
/// This is where the off-thread oracle dispatch lives (the harness/env layer — NOT `godot/`, inv #2; NOT the
/// single-threaded `World`, ADR-002). The single-writer discipline holds: the [`Oracle`] (the modeled off-thread
/// producer) only ever writes the [`OracleMailbox`]; ONLY this synchronous loop, at a fixed epoch boundary, reads
/// the mailbox and emits the journaled `CommitEcoliImpact`. The commit epoch is decided by epoch-counting off the
/// generation `Tick` stream, so a fast / slow / absent / chaotic oracle all produce the IDENTICAL journal up to
/// the commit — and at S5, where the commit is applied as an IDENTITY modifier, the IDENTICAL hash even after.
pub struct OversightEpisode<O: Oracle> {
    /// The deterministic env the episode drives (a fresh seeded `GeneSimEnv`).
    env: GeneSimEnv,
    /// The earned-credit ledger (S4) — off-hash, recomputed from the stats stream.
    ledger: CreditLedger,
    /// The deep-edit firewall buffer + `req_id` allocator (S5) — off-hash.
    firewall: EditFirewall,
    /// The single-writer mailbox the oracle writes and this loop reads.
    mailbox: OracleMailbox,
    /// The (modeled off-thread) impact producer.
    oracle: O,
    /// The credit-economy tuning.
    policy: CreditPolicy,
    /// The objective region the credit Term A reads (the renderer/campaign zone).
    region: sim_core::Region,
    /// The snapshot grid the region is read on.
    grid: (u32, u32),
    /// Cumulative generations advanced (the single seeded stream's generation counter == the `Tick`).
    generation: u64,
    /// The previous gen's stats sample (for the quantize-each-then-difference credit fold).
    prev_sample: GenSample,
    /// The highest epoch boundary already drained (so each boundary drains exactly once).
    last_drained_epoch: u32,
}

/// The journaled outcome of running one OVERSIGHT episode: the full action stream (input actions with the
/// firewall's `CommitEcoliImpact`s spliced in at their committed epochs) plus the final stats hash + ledger.
#[derive(Debug, Clone)]
pub struct OversightOutcome {
    /// The complete journaled action stream — exactly what is written to `actions.ndjson` so a replay reproduces
    /// the episode (including the committed impacts, read straight from the journal, NEVER re-solving FBA).
    pub journal: Vec<Action>,
    /// The final [`sim_core::RunStats::hash`] — at S5 byte-identical to the pinned literal (commits are identity).
    pub hash: u64,
    /// The final credit ledger (for the INSPECT view / tests).
    pub ledger: CreditLedger,
}

impl<O: Oracle> OversightEpisode<O> {
    /// Start an episode: reset the env from `seed`, build a fresh ledger + firewall + mailbox, and read the gen-0
    /// stats sample as the credit baseline.
    #[must_use]
    pub fn start(
        mut env: GeneSimEnv,
        seed: u64,
        oracle: O,
        policy: CreditPolicy,
        region: sim_core::Region,
        grid: (u32, u32),
    ) -> Self {
        env.reset(seed);
        let prev_sample = sample_now(&mut env, region, grid);
        Self {
            env,
            ledger: CreditLedger::new(),
            firewall: EditFirewall::new(),
            mailbox: OracleMailbox::new(),
            oracle,
            policy,
            region,
            grid,
            generation: 0,
            prev_sample,
            last_drained_epoch: 0,
        }
    }

    /// The credit currently held (for the INSPECT view / the spend gate display).
    #[must_use]
    pub fn credit(&self) -> u64 {
        self.ledger.credit
    }

    /// Run the INPUT action stream, producing the full journal (with committed impacts spliced in). Each input
    /// `Advance(n)` is stepped ONE generation at a time (like `campaign::evaluate`), accruing credit and draining
    /// the firewall at every epoch boundary. A `RequestEcoliEdit` is gated by credit, buffered, and dispatched to
    /// the oracle off-thread (written into the mailbox). Existing `CommitEcoliImpact` lines in the INPUT stream
    /// are passed through inert (replay reads them from the journal). At end-of-episode the firewall is drained to
    /// completion so every request has a paired commit (the journal always terminates).
    #[must_use]
    pub fn run(mut self, actions: &[Action]) -> OversightOutcome {
        // REPLAY DETECTION: a journal that ALREADY carries `CommitEcoliImpact` lines is a recorded stream being
        // replayed — the firewall already ran when it was recorded. On replay the driver MUST NOT re-dispatch the
        // oracle (replay never re-runs FBA) nor re-buffer/re-drain (the commits ride in the journal). It passes
        // every action through inert, consuming the committed integers straight from the stream. This is the
        // `record_episode`/`replay` contract: same `(seed, journal)` → same hash, oracle untouched.
        let is_replay = actions
            .iter()
            .any(|a| matches!(a, Action::CommitEcoliImpact { .. }));
        if is_replay {
            return self.replay(actions);
        }

        let mut journal: Vec<Action> = Vec::new();

        for action in actions {
            match action {
                Action::Advance(n) => {
                    journal.push(Action::Advance(*n));
                    for _ in 0..*n {
                        self.env.step(Action::Advance(1));
                        self.generation += 1;
                        // Credit accrual: quantize THIS gen, difference the integers vs the previous sample.
                        let now = sample_now(&mut self.env, self.region, self.grid);
                        self.ledger
                            .accrue_gen(&self.prev_sample, &now, &self.policy);
                        self.prev_sample = now;
                        // Epoch boundary? Drain due commits in (species, req_id) order, splice into the journal.
                        self.drain_boundaries_into(&mut journal);
                    }
                }
                edit @ (Action::ApplyEdit(_) | Action::ApplyEditRegion(_, _)) => {
                    journal.push(edit.clone());
                    self.env.step(edit.clone());
                }
                Action::RequestEcoliEdit {
                    species,
                    locus,
                    edit_kind,
                    due_epoch: _,
                    req_id: _,
                } => {
                    // Two-tier spend gate: gated by credit, structurally like campaign edit_budget. The spend
                    // DECISION is journaled (the request line is emitted only if afforded) — replay reads the
                    // decision from the journal, never re-deciding on a recomputed credit (the design's
                    // journaled-spend rule). A refused request is NOT replayed (dropped from the journal).
                    if !self.ledger.try_spend(&self.policy) {
                        continue; // refused — not buffered, not journaled
                    }
                    // Allocate the deterministic occurrence-index req_id (advances on every ACCEPTED request in
                    // the produced journal; the journal is the replay source of truth).
                    let req_id = self.firewall.alloc_req_id();
                    let due_epoch = epoch_of(self.generation) + EPOCH_LEAD;
                    // Re-emit the request with the firewall-allocated req_id + computed due_epoch so the journal
                    // is self-contained (replay does not recompute these).
                    journal.push(Action::RequestEcoliEdit {
                        species: *species,
                        locus: *locus,
                        edit_kind: *edit_kind,
                        due_epoch,
                        req_id,
                    });
                    self.firewall.buffer_request(*species, req_id, due_epoch);
                    // Dispatch the (modeled off-thread) oracle: it writes ONLY the mailbox (single-writer).
                    if let Some(impact) = self.oracle.produce(req_id, *species, locus.0) {
                        self.mailbox.deposit(req_id, impact);
                    }
                }
                Action::CommitEcoliImpact { .. } => {
                    // Unreachable in a FRESH record (the `is_replay` guard routes any stream containing a commit
                    // to `replay`). Kept exhaustive for the match.
                    journal.push(action.clone());
                    self.env.step(action.clone());
                }
            }
        }

        // Drain to completion so every buffered request has a paired commit (the slip-cap guarantees this
        // terminates even if the oracle never returned). Splice the trailing commits in at their epochs.
        let from_epoch = self.last_drained_epoch + 1;
        for commit in self.firewall.drain_to_completion(from_epoch, &self.mailbox) {
            journal.push(commit.clone());
            self.env.step(commit);
        }

        let hash = self.env.run_stats().hash;
        OversightOutcome {
            journal,
            hash,
            ledger: self.ledger,
        }
    }

    /// REPLAY a recorded journal: pass every action through to the env inert, NEVER dispatching the oracle and
    /// NEVER re-buffering/re-draining the firewall (the commits ride in the journal). The credit ledger is still
    /// recomputed from the stats stream (for the INSPECT view), but the spend DECISION is read from the journal
    /// (a refused request was never recorded, so every `RequestEcoliEdit` in the journal was accepted). Returns
    /// the SAME journal it was handed plus the reproduced hash — the `record_episode`/`replay` determinism
    /// contract (inv #3): same `(seed, journal)` → byte-identical hash, with the oracle untouched.
    fn replay(mut self, actions: &[Action]) -> OversightOutcome {
        for action in actions {
            match action {
                Action::Advance(n) => {
                    for _ in 0..*n {
                        self.env.step(Action::Advance(1));
                        self.generation += 1;
                        let now = sample_now(&mut self.env, self.region, self.grid);
                        self.ledger
                            .accrue_gen(&self.prev_sample, &now, &self.policy);
                        self.prev_sample = now;
                    }
                }
                // Edits, journaled requests, and committed impacts all step through inert (the request draws no
                // RNG; the commit applies the S5 identity modifier — both hash-neutral). The oracle is NEVER
                // called; the committed integers are consumed straight from the journal.
                other => {
                    self.env.step(other.clone());
                }
            }
        }
        let hash = self.env.run_stats().hash;
        OversightOutcome {
            journal: actions.to_vec(),
            hash,
            ledger: self.ledger,
        }
    }

    /// Drain every epoch boundary newly crossed by the current `generation`, splicing the committed impacts into
    /// `journal` and applying each (an IDENTITY modifier at S5) to the env so the journal stays in step.
    fn drain_boundaries_into(&mut self, journal: &mut Vec<Action>) {
        let current_epoch = epoch_of(self.generation);
        while self.last_drained_epoch < current_epoch {
            self.last_drained_epoch += 1;
            let epoch = self.last_drained_epoch;
            for commit in self.firewall.drain_epoch(epoch, &self.mailbox) {
                journal.push(commit.clone());
                self.env.step(commit); // S5: applies the identity modifier (no-op) — hash unchanged
            }
        }
    }
}

/// Read the RNG-free credit sample at the CURRENT env state (a fixed post-`Advance(1)` point, before the next
/// step resets the per-gen FlowMatrix). Pure read-only projection (no RNG, no mutation).
fn sample_now(env: &mut GeneSimEnv, region: sim_core::Region, grid: (u32, u32)) -> GenSample {
    let readout = env.region_allele(region, grid.0, grid.1);
    let (s, flat) = env.flow_matrix();
    GenSample::from_projections(&readout, s, &flat)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(q: u16, flow: i64) -> GenSample {
        GenSample {
            region_allele_q: q,
            flow_health: flow,
        }
    }

    #[test]
    fn accrual_rewards_improvement_only_and_clamps() {
        let policy = CreditPolicy {
            per_gen_cap: 50,
            ecoli_edit_cost: 100,
            term_a_weight: 1,
            term_b_weight: 1,
        };
        let mut led = CreditLedger::new();

        // Improving gen: qA +10, flow +5 -> 15, under the cap -> +15 credit.
        led.accrue_gen(&sample(100, 0), &sample(110, 5), &policy);
        assert_eq!(led.credit, 15);
        assert_eq!(led.accrued_total, 15);

        // Regressing gen: qA -20, flow -3 -> both floored at 0 -> +0 (the ledger never drops on accrual).
        led.accrue_gen(&sample(110, 5), &sample(90, 2), &policy);
        assert_eq!(led.credit, 15, "a regressing gen earns nothing");

        // A huge improvement is CLAMPED to per_gen_cap (40000 q-delta -> capped at 50).
        led.accrue_gen(&sample(0, 0), &sample(40000, 0), &policy);
        assert_eq!(led.credit, 15 + 50, "per-gen gain is capped");
    }

    #[test]
    fn spend_gate_refuses_when_unaffordable() {
        let policy = CreditPolicy::default(); // cost 100
        let mut led = CreditLedger {
            credit: 90,
            accrued_total: 90,
        };
        assert!(!led.can_afford(&policy));
        assert!(!led.try_spend(&policy), "unaffordable request is refused");
        assert_eq!(led.credit, 90, "refused spend leaves credit untouched");

        led.credit = 120;
        assert!(led.can_afford(&policy));
        assert!(led.try_spend(&policy));
        assert_eq!(led.credit, 20, "afforded spend deducts the cost");
        assert_eq!(
            led.accrued_total, 90,
            "accrued_total is monotonic, untouched by a spend"
        );
    }

    #[test]
    fn flow_health_sums_positive_off_diagonals_only() {
        // 2x2: diag ignored; off-diag (0,1)=+7 and (1,0)=+3 -> 10; a negative off-diag contributes 0.
        let flat = vec![
            99, 7, // row 0
            3, -5, // row 1 (the diag -5 is ignored anyway)
        ];
        assert_eq!(flow_health(2, &flat), 10);

        let with_neg = vec![99, -7, 3, 99];
        assert_eq!(
            flow_health(2, &with_neg),
            3,
            "negative off-diagonal ignored"
        );
    }

    #[test]
    fn empty_region_pins_zero_sentinel() {
        let empty = sim_core::RegionReadout {
            mean: 0.73, // mean is meaningless when populated_cells == 0
            populated_cells: 0,
        };
        let s = GenSample::from_projections(&empty, 1, &[0]);
        assert_eq!(
            s.region_allele_q, 0,
            "empty region pins the 0 sentinel, not the stale mean"
        );
    }

    #[test]
    fn accrual_is_a_pure_deterministic_fold() {
        // Same sample stream => byte-identical ledger (inv #3 — the fold is RNG-free integer math).
        let policy = CreditPolicy::default();
        let stream = [
            sample(0, 0),
            sample(50, 10),
            sample(80, 30),
            sample(80, 25),
            sample(120, 40),
        ];
        let fold = || {
            let mut led = CreditLedger::new();
            for w in stream.windows(2) {
                led.accrue_gen(&w[0], &w[1], &policy);
            }
            led
        };
        assert_eq!(fold(), fold());
    }
}
