//! The deep-edit determinism FIREWALL (ADR-017 S5) — the producer/consumer split that lets a non-bit-reproducible
//! FBA solve feed the deterministic sim WITHOUT moving the hash.
//!
//! ## The one-way quantized-integer crossing
//! The non-reproducible FBA solve is ALWAYS the **producer** (off-thread, off-hash). The deterministic sim only
//! ever **consumes** a **quantized integer** committed at a **fixed future epoch** via a **journaled
//! [`crate::Action::CommitEcoliImpact`]** — exactly like a player `ApplyEdit`. Arrival TIME is irrelevant: the
//! commit epoch is decided by epoch-counting off the `Tick` generation stream (never wall-clock), so a fast,
//! slow, absent, or differently-answering oracle all produce the IDENTICAL journal up to the commit, and (at
//! S5) the IDENTICAL hash even after — because the committed slot is applied as an **IDENTITY modifier**
//! (coefficient 1.0, no selection change). S6 is the deliberate re-pin that turns the coefficient on.
//!
//! ## Single-writer discipline (the #1 dispatch hazard)
//! The off-thread oracle NEVER mutates [`EditFirewall::pending`] or the journal. It only writes a completed
//! quantized payload into a per-`req_id` **mailbox** ([`OracleMailbox`]). ONLY the synchronous step loop, at a
//! fixed epoch boundary, reads the mailbox and emits the journaled `CommitEcoliImpact` (slipping deterministically
//! if the mailbox is empty). This is what makes the commit epoch solver-speed-independent within the lead window.
//!
//! ## Drain order (inv #3)
//! [`EditFirewall::pending`] is a `BTreeMap<u32, Vec<PendingImpact>>` keyed by `due_epoch`, drained in ascending
//! `(SpeciesId, req_id)` order — explicitly NOT a `HashMap` iterated in sim logic. `(SpeciesId, req_id)` is unique
//! per bucket so the order is total (a `debug_assert` of strict ordering guards the tie-break).

use std::collections::BTreeMap;

use crate::Action;

/// How many GENERATIONS a missed commit may slip before the firewall gives up and commits the neutral sentinel.
/// Counted in EPOCHS, never wall-clock seconds (the design's slip-cap, promoted to a required determinism-
/// completeness rule): a hung/crashed oracle must not stall the journal forever. At `due_epoch + MAX_SLIP_EPOCHS`
/// the firewall deterministically commits [`EcoliImpact::neutral`] so the journal ALWAYS terminates.
pub const MAX_SLIP_EPOCHS: u32 = 8;

/// The wild-type / neutral growth-ratio permille (1.0×). [`EcoliImpact::neutral`] uses this so a slip-cap
/// abandonment is selection-neutral (and at S5 every commit is neutral anyway — coefficient zero).
pub const NEUTRAL_GROWTH_RATIO_Q: u16 = 1000;

/// The quantized impact payload an oracle produces for one deep edit — already integers (floats never cross this
/// boundary). Content-hashed over the QUANTIZED BYTES so a tampered journal is rejected on replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EcoliImpact {
    /// Quantized growth-ratio factor (permille; 1000 = wild-type). At S5 UNREAD by selection (coefficient zero).
    pub growth_ratio_q: u16,
    /// Quantized exchange-flux deltas as `(exchange_index, signed_delta)`, in canonical exchange-index order.
    /// UNREAD at S5; the S6 modifier taps these into the decomposer mineralize_rate.
    pub exchange_deltas: Vec<(u16, i16)>,
}

impl EcoliImpact {
    /// The NEUTRAL/identity impact: wild-type growth ratio, no exchange deltas. Used as the slip-cap sentinel and
    /// (at S5) the universal committed value — applying it is a no-op (coefficient 1.0), so the hash never moves.
    #[must_use]
    pub fn neutral() -> Self {
        Self {
            growth_ratio_q: NEUTRAL_GROWTH_RATIO_Q,
            exchange_deltas: Vec::new(),
        }
    }

    /// Content hash over the QUANTIZED BYTES (`growth_ratio_q` + index-ordered `exchange_deltas`) — NEVER the
    /// floats, NEVER the FBA model-version string (that belongs in provenance, else a model re-bake silently
    /// re-pins). A pure deterministic integer hash (FNV-1a over the canonical byte layout), so it binds the
    /// committed integers identically on every platform. Replay recomputes this and rejects a journal whose
    /// recorded `content_hash` disagrees (`InvalidData`).
    #[must_use]
    pub fn content_hash(&self) -> u64 {
        // FNV-1a 64-bit over a canonical byte layout. Integer-only, order-stable.
        const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0000_0100_0000_01b3;
        let mut h = OFFSET;
        let eat = |byte: u8, h: &mut u64| {
            *h ^= u64::from(byte);
            *h = h.wrapping_mul(PRIME);
        };
        for b in self.growth_ratio_q.to_le_bytes() {
            eat(b, &mut h);
        }
        // Length-prefix the deltas so two different splittings can't collide, then each (index, delta) in order.
        for b in (self.exchange_deltas.len() as u64).to_le_bytes() {
            eat(b, &mut h);
        }
        for (idx, delta) in &self.exchange_deltas {
            for b in idx.to_le_bytes() {
                eat(b, &mut h);
            }
            for b in delta.to_le_bytes() {
                eat(b, &mut h);
            }
        }
        h
    }
}

/// A buffered deep-edit request awaiting its commit, keyed in [`EditFirewall::pending`] by `due_epoch` and
/// ordered within a bucket by `(species, req_id)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingImpact {
    /// Target species (operator/species granularity, inv #6). Raw `u16` (the Action's scaffold type) — the
    /// `(species, req_id)` pair is the deterministic drain key.
    pub species: u16,
    /// The deterministic monotonic occurrence index of the originating request (replay-stable; NEVER wall-clock).
    pub req_id: u32,
    /// The epoch this request was ORIGINALLY due at (so a slip can record `slipped_from`).
    pub original_due_epoch: u32,
}

/// The per-`req_id` mailbox the off-thread oracle writes into and the synchronous step loop reads from — the
/// single-writer seam. In this PoC the "off-thread" producer is modeled by an [`Oracle`] the driver calls and
/// stashes here; the discipline (producer writes mailbox, ONLY the step loop reads it + emits the journaled
/// commit) is the load-bearing rule, and the firewall logic is identical whether the producer is a real thread
/// or a synchronous stub (the determinism comes from epoch-counting, not from arrival).
#[derive(Debug, Default)]
pub struct OracleMailbox {
    /// `req_id -> the quantized payload the oracle produced` (an ordered map, never a `HashMap` — inv #3).
    ready: BTreeMap<u32, EcoliImpact>,
}

impl OracleMailbox {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The producer side: deposit a completed payload for `req_id`. Called by the driver from the (modeled)
    /// off-thread dispatch — NEVER touches `pending` or the journal.
    pub fn deposit(&mut self, req_id: u32, impact: EcoliImpact) {
        self.ready.insert(req_id, impact);
    }

    /// The consumer side: has `req_id`'s payload arrived yet? (Read by the step loop at an epoch boundary.)
    #[must_use]
    pub fn peek(&self, req_id: u32) -> Option<&EcoliImpact> {
        self.ready.get(&req_id)
    }
}

/// The determinism firewall: the `due_epoch`-keyed buffer of pending impacts plus the monotonic `req_id`
/// allocator. Lives in the harness/env layer (NOT an ECS resource), so it adds 0 bytes to `hash_world`.
#[derive(Debug, Default)]
pub struct EditFirewall {
    /// Pending impacts keyed by the epoch they are due to commit at, ordered within a bucket by `(species,
    /// req_id)` at drain time. A `BTreeMap` (ordered) — NEVER a `HashMap` iterated in sim logic (inv #3).
    pending: BTreeMap<u32, Vec<PendingImpact>>,
    /// The next `req_id` to hand out — a deterministic monotonic OCCURRENCE index into the `RequestEcoliEdit`
    /// stream, advanced on EVERY request (decoupled from credit, unlike the campaign `edits_used++`). Reset per
    /// episode (the firewall is constructed fresh at `reset()`), NEVER wall-clock/UUID/global-atomic.
    next_req_id: u32,
}

impl EditFirewall {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate the next deterministic `req_id` — a monotonic occurrence index over the requests the driver
    /// JOURNALS. Pure counter (advances on every call, never wall-clock); the env-layer driver calls it once per
    /// ACCEPTED request, after the spend gate, so refused requests are dropped pre-journal and consume no id.
    pub fn alloc_req_id(&mut self) -> u32 {
        let id = self.next_req_id;
        self.next_req_id += 1;
        id
    }

    /// Buffer a deep-edit request to commit at `due_epoch`. The off-thread oracle dispatch (writing the mailbox)
    /// is the driver's job; this only records the pending entry (single-writer: the firewall buffer is mutated
    /// ONLY by the synchronous loop).
    pub fn buffer_request(&mut self, species: u16, req_id: u32, due_epoch: u32) {
        self.pending
            .entry(due_epoch)
            .or_default()
            .push(PendingImpact {
                species,
                req_id,
                original_due_epoch: due_epoch,
            });
    }

    /// Whether any impacts remain buffered (the driver drains until empty at end-of-episode so the journal always
    /// terminates).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Drain every impact DUE at `epoch` (its bucket key `<= epoch`), in ascending `(species, req_id)` order,
    /// emitting one journaled [`Action::CommitEcoliImpact`] per drained request. For each due impact:
    ///
    /// * if its payload is READY in the `mailbox`, commit the quantized payload at `epoch` (with `slipped_from`
    ///   set iff `epoch != original_due_epoch`);
    /// * else if it has reached `original_due_epoch + MAX_SLIP_EPOCHS`, commit the NEUTRAL sentinel (the slip-cap
    ///   — the journal ALWAYS terminates, regardless of how long the oracle hangs);
    /// * else re-buffer it at `epoch + 1` (a deterministic SLIP to the next epoch).
    ///
    /// Returns the journaled commit Actions in drain order. PURE w.r.t. the sim hash at S5: the caller applies
    /// each commit as an identity modifier (coefficient zero). The decision is driven by epoch-counting, NOT by
    /// which thread message arrived first — so a slow oracle changes neither the result NOR its timing.
    #[must_use]
    pub fn drain_epoch(&mut self, epoch: u32, mailbox: &OracleMailbox) -> Vec<Action> {
        // Collect every bucket key <= epoch in ascending order (BTreeMap iterates sorted — never a HashMap).
        let due_keys: Vec<u32> = self.pending.range(..=epoch).map(|(k, _)| *k).collect();

        // Gather all due impacts, then sort by (species, req_id) for a TOTAL, deterministic drain order.
        let mut due: Vec<PendingImpact> = Vec::new();
        for k in &due_keys {
            if let Some(v) = self.pending.remove(k) {
                due.extend(v);
            }
        }
        due.sort_by(|a, b| a.species.cmp(&b.species).then(a.req_id.cmp(&b.req_id)));
        debug_assert!(
            due.windows(2)
                .all(|w| (w[0].species, w[0].req_id) < (w[1].species, w[1].req_id)),
            "drain order must be a strict total order on (species, req_id)"
        );

        let mut commits = Vec::with_capacity(due.len());
        for p in due {
            let slipped_from = if epoch == p.original_due_epoch {
                None
            } else {
                Some(p.original_due_epoch)
            };
            let at_slip_cap = epoch >= p.original_due_epoch.saturating_add(MAX_SLIP_EPOCHS);

            let impact = match mailbox.peek(p.req_id) {
                Some(ready) => ready.clone(),
                None if at_slip_cap => EcoliImpact::neutral(), // slip-cap: journal terminates deterministically
                None => {
                    // SLIP: re-buffer at the next epoch, carrying the ORIGINAL due epoch (self-describing).
                    self.pending.entry(epoch + 1).or_default().push(p);
                    continue;
                }
            };

            commits.push(Action::CommitEcoliImpact {
                species: p.species,
                req_id: p.req_id,
                due_epoch: epoch,
                slipped_from,
                content_hash: impact.content_hash(),
                growth_ratio_q: impact.growth_ratio_q,
                exchange_deltas: impact.exchange_deltas,
            });
        }
        commits
    }

    /// Drain EVERYTHING still pending, advancing the epoch one at a time until empty (used at end-of-episode so
    /// every buffered request has a paired commit — a `RequestEcoliEdit` without a `CommitEcoliImpact` is a HARD
    /// replay error, so the recorder must always emit both). Starts at `from_epoch` and steps up; the slip-cap
    /// guarantees termination even if the mailbox never fills (every pending impact neutral-commits by
    /// `original_due_epoch + MAX_SLIP_EPOCHS`).
    #[must_use]
    pub fn drain_to_completion(&mut self, from_epoch: u32, mailbox: &OracleMailbox) -> Vec<Action> {
        let mut out = Vec::new();
        let mut epoch = from_epoch;
        // Bound the loop defensively: the slip-cap guarantees the last pending entry resolves within MAX_SLIP
        // epochs of the highest original_due_epoch, but iterate until genuinely empty.
        while !self.pending.is_empty() {
            out.extend(self.drain_epoch(epoch, mailbox));
            epoch += 1;
        }
        out
    }
}

/// A producer of deep-edit impacts (inv #5 — the FBA science behind a trait). The DEFAULT impl is the frozen-table
/// `oracle-fba` lookup; tests inject absent / slow / chaotic / never-returning oracles to PROVE the firewall's
/// hash is invariant to the producer. The producer is OFF-hash (it only writes the mailbox).
pub trait Oracle {
    /// Produce the quantized impact for a request, or `None` if it is not ready yet (a slow/absent oracle).
    /// `req_id` identifies the request; `species`/`locus` echo the request payload. Returns ALREADY-QUANTIZED
    /// integers — a float never escapes the producer.
    fn produce(&mut self, req_id: u32, species: u16, locus: u32) -> Option<EcoliImpact>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An oracle that produces the NEUTRAL impact immediately (the S5 universal value — keeps the hash put).
    struct InstantNeutralOracle;
    impl Oracle for InstantNeutralOracle {
        fn produce(&mut self, _req_id: u32, _species: u16, _locus: u32) -> Option<EcoliImpact> {
            Some(EcoliImpact::neutral())
        }
    }

    /// An oracle that NEVER returns (models a hung/crashed FBA subprocess) — exercises the slip-cap.
    struct NeverReturnsOracle;
    impl Oracle for NeverReturnsOracle {
        fn produce(&mut self, _req_id: u32, _species: u16, _locus: u32) -> Option<EcoliImpact> {
            None
        }
    }

    #[test]
    fn req_id_is_a_monotonic_occurrence_index() {
        let mut fw = EditFirewall::new();
        assert_eq!(fw.alloc_req_id(), 0);
        assert_eq!(fw.alloc_req_id(), 1);
        assert_eq!(
            fw.alloc_req_id(),
            2,
            "alloc advances on every CALL (the driver calls it once per accepted/journaled request)"
        );
    }

    #[test]
    fn ready_impact_commits_at_due_epoch() {
        let mut fw = EditFirewall::new();
        let mut mb = OracleMailbox::new();
        let mut oracle = InstantNeutralOracle;

        let req_id = fw.alloc_req_id();
        fw.buffer_request(0, req_id, 5);
        // Producer (modeled off-thread) deposits into the mailbox.
        mb.deposit(req_id, oracle.produce(req_id, 0, 10).unwrap());

        // Nothing due before epoch 5.
        assert!(fw.drain_epoch(4, &mb).is_empty());
        let commits = fw.drain_epoch(5, &mb);
        assert_eq!(commits.len(), 1);
        match &commits[0] {
            Action::CommitEcoliImpact {
                species,
                req_id: rid,
                due_epoch,
                slipped_from,
                growth_ratio_q,
                ..
            } => {
                assert_eq!(*species, 0);
                assert_eq!(*rid, req_id);
                assert_eq!(*due_epoch, 5);
                assert_eq!(
                    *slipped_from, None,
                    "committed on its first scheduled epoch"
                );
                assert_eq!(*growth_ratio_q, NEUTRAL_GROWTH_RATIO_Q);
            }
            other => panic!("expected CommitEcoliImpact, got {other:?}"),
        }
        assert!(fw.is_empty());
    }

    #[test]
    fn slow_oracle_slips_with_self_describing_journal() {
        let mut fw = EditFirewall::new();
        let mb_empty = OracleMailbox::new();
        let mut mb_ready = OracleMailbox::new();

        let req_id = fw.alloc_req_id();
        fw.buffer_request(0, req_id, 5);

        // Mailbox empty at epoch 5 -> SLIP, no commit yet, re-buffered at 6.
        assert!(fw.drain_epoch(5, &mb_empty).is_empty());
        // Payload arrives before epoch 6.
        mb_ready.deposit(req_id, EcoliImpact::neutral());
        let commits = fw.drain_epoch(6, &mb_ready);
        assert_eq!(commits.len(), 1);
        match &commits[0] {
            Action::CommitEcoliImpact {
                due_epoch,
                slipped_from,
                ..
            } => {
                assert_eq!(*due_epoch, 6, "committed at the slipped epoch");
                assert_eq!(
                    *slipped_from,
                    Some(5),
                    "slip is self-describing in the journal"
                );
            }
            other => panic!("expected CommitEcoliImpact, got {other:?}"),
        }
    }

    #[test]
    fn slip_cap_terminates_with_neutral_sentinel() {
        // A NeverReturnsOracle must force a NEUTRAL commit at exactly original_due + MAX_SLIP_EPOCHS, so the
        // journal terminates deterministically no matter how long the subprocess hangs.
        let mut fw = EditFirewall::new();
        let mb = OracleMailbox::new(); // never filled
        let _oracle = NeverReturnsOracle;

        let req_id = fw.alloc_req_id();
        let due = 3u32;
        fw.buffer_request(0, req_id, due);

        let mut committed_at = None;
        for epoch in due..=(due + MAX_SLIP_EPOCHS + 2) {
            let commits = fw.drain_epoch(epoch, &mb);
            if !commits.is_empty() {
                assert_eq!(commits.len(), 1);
                match &commits[0] {
                    Action::CommitEcoliImpact {
                        due_epoch,
                        growth_ratio_q,
                        content_hash,
                        ..
                    } => {
                        committed_at = Some(*due_epoch);
                        assert_eq!(*growth_ratio_q, NEUTRAL_GROWTH_RATIO_Q, "neutral sentinel");
                        assert_eq!(
                            *content_hash,
                            EcoliImpact::neutral().content_hash(),
                            "fixed sentinel content_hash"
                        );
                    }
                    other => panic!("expected CommitEcoliImpact, got {other:?}"),
                }
                break;
            }
        }
        assert_eq!(
            committed_at,
            Some(due + MAX_SLIP_EPOCHS),
            "slip-cap commits at exactly original_due + MAX_SLIP_EPOCHS"
        );
        assert!(fw.is_empty(), "journal terminates");
    }

    #[test]
    fn drain_order_is_total_on_species_then_req_id() {
        let mut fw = EditFirewall::new();
        let mut mb = OracleMailbox::new();
        // Three requests sharing one due_epoch, buffered out of (species, req_id) order.
        let r0 = fw.alloc_req_id(); // 0
        let r1 = fw.alloc_req_id(); // 1
        let r2 = fw.alloc_req_id(); // 2
        fw.buffer_request(2, r2, 4);
        fw.buffer_request(0, r0, 4);
        fw.buffer_request(1, r1, 4);
        for r in [r0, r1, r2] {
            mb.deposit(r, EcoliImpact::neutral());
        }
        let commits = fw.drain_epoch(4, &mb);
        let order: Vec<(u16, u32)> = commits
            .iter()
            .map(|a| match a {
                Action::CommitEcoliImpact {
                    species, req_id, ..
                } => (*species, *req_id),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(
            order,
            vec![(0, r0), (1, r1), (2, r2)],
            "drained in ascending (species, req_id) order, not buffer/HashMap order"
        );
    }

    #[test]
    fn content_hash_binds_quantized_bytes_and_is_order_sensitive() {
        let a = EcoliImpact {
            growth_ratio_q: 800,
            exchange_deltas: vec![(3, -120), (11, 88)],
        };
        let b = EcoliImpact {
            growth_ratio_q: 800,
            exchange_deltas: vec![(11, 88), (3, -120)], // different order
        };
        assert_ne!(a.content_hash(), b.content_hash(), "order-sensitive");
        // Stable across calls.
        assert_eq!(a.content_hash(), a.content_hash());
        // Differs from neutral.
        assert_ne!(a.content_hash(), EcoliImpact::neutral().content_hash());
    }

    #[test]
    fn drain_to_completion_terminates_even_with_a_dead_oracle() {
        let mut fw = EditFirewall::new();
        let mb = OracleMailbox::new(); // dead oracle, never fills
        for _ in 0..3 {
            let r = fw.alloc_req_id();
            fw.buffer_request(0, r, 2);
        }
        let commits = fw.drain_to_completion(2, &mb);
        assert_eq!(
            commits.len(),
            3,
            "every buffered request gets a paired commit"
        );
        assert!(fw.is_empty());
        // All neutral (slip-capped).
        for c in &commits {
            if let Action::CommitEcoliImpact { growth_ratio_q, .. } = c {
                assert_eq!(*growth_ratio_q, NEUTRAL_GROWTH_RATIO_Q);
            }
        }
    }
}
