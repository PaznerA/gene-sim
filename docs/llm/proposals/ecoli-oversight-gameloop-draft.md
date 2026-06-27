# ADR-017 S4/S5/S6 — the Oversight game-loop: earned-credit economy + the deep-edit determinism firewall

> **STATUS — S4 (earned-credit economy) + S5 (journaled deep-edit Actions + the determinism firewall) have
> LARGELY MERGED.** See DECISIONS.md (ADR-018 BiGG licensing accepted) and the OVERSIGHT earned-edit loop
> Slice A/B on `main` (the inert→wired `RequestEcoliEdit`/`CommitEcoliImpact` Actions, the harness
> `CreditLedger`, the `due_epoch` firewall, `crates/oracle-fba`, the `EcoliEditModifier` scaffold). **S6 — the
> load-bearing full `EcoliEditModifier` activation (a deliberate re-pin) — is DEFERRED and is the main thing
> still to build.** This doc is the design reference future S6 work must honor.

---

## The determinism FIREWALL (the contract S6 must honor)

The one-way quantized-integer crossing: the non-bit-reproducible FBA solve is ALWAYS the **producer**
(off-thread, off-hash). The deterministic sim only ever **consumes** a **quantized integer** committed at a
**fixed future epoch** via a **journaled Action** — exactly like a player `ApplyEdit`.

- **Epoch clock — `Tick`, never wall-clock.** `due_epoch` is a function of the `Tick(u64)` generation counter.
  **Zero** `Instant`/`SystemTime`/`now()` on the hash path. Cadence is fixed every-N-generations with a minimum
  lead so a slow oracle has slack before the first slip.
- **Single-writer dispatch.** The background `oracle-fba` thread NEVER mutates `pending`/the journal — it only
  writes a completed quantized payload into a per-`req_id` mailbox. ONLY the synchronous step loop, at a fixed
  epoch boundary, reads the mailbox and emits the journaled `CommitEcoliImpact` (slipping deterministically if
  empty). Arrival time is irrelevant; the commit epoch is decided by epoch-counting.
- **Slip + slip-cap, journaled.** A missed `due_epoch` deterministically SLIPS to the next epoch; the slip is
  itself journaled (`slipped_from: Option<u32>` inline in `CommitEcoliImpact`). The pending buffer is a
  `BTreeMap<u32, Vec<PendingImpact>>` drained in ascending `(SpeciesId, req_id)` order (NOT a `HashMap`, inv
  #3). A **max-slip of K EPOCHS** (never seconds): at `due_epoch + K` the firewall commits a NEUTRAL/identity
  impact with a fixed sentinel `content_hash`, so the journal ALWAYS terminates deterministically.
- **Quantize-before-return + content-hash.** `crates/oracle-fba` (a std-only structural clone of `oracle-slim`,
  links nothing — inv #1) emits already-quantized `u16`/`i16` text; the parent parses integers only (no float
  survives into any ordering across arches). For single-gene edits it is a FROZEN-TABLE KO lookup, not a live
  solve. `EcoliImpact{ growth_ratio_q:u16, exchange_deltas:Vec<(u16,i16)> }` is content-hashed over the
  QUANTIZED BYTES in canonical exchange-index order; a tampered journal whose `content_hash` disagrees is
  rejected on replay as `InvalidData`. The model-version string stays OUT of the hashed bytes (it lives in
  `Sourced` provenance, else a re-bake silently re-pins).
- **The credit LEDGER.** `CreditLedger{ credit:u64, accrued_total:u64 }` in `oversight.rs`, in the harness/env
  layer (0 bytes into `hash_world` — the `edits_used` precedent), an RNG-free integer fold over the per-gen
  stats stream (Term A `region_allele`-toward-objective + Term B `FlowMatrix`-health delta). Quantize each
  gen's term to `u16` FIRST, then difference the integers (never difference f64 means). The spend DECISION is
  journaled (replay reads the gate outcome, not recomputes it). `req_id` is a deterministic monotonic
  occurrence index into the `RequestEcoliEdit` stream, reset per episode.

The journaled Action shapes (landed):

```rust
Action::RequestEcoliEdit { species: u16, locus: genome::LocusId, edit_kind: crispr::EditKind,
                           due_epoch: u32, req_id: u32 }
Action::CommitEcoliImpact { species: u16, req_id: u32, due_epoch: u32, slipped_from: Option<u32>,
                           content_hash: u64, growth_ratio_q: u16, exchange_deltas: Vec<(u16, i16)> }
```

---

## S6 — the load-bearing wire (DEFERRED, a deliberate RE-PIN, 🛑 human sign-off REQUIRED)

A new `EcoliEditModifier` behind the inv-#5 modifier seam that READS the per-species committed
`growth_ratio_q`/`exchange_deltas` slot (currently WRITTEN-BUT-UNREAD, coefficient zero — the F2-Strategy
precedent) and turns it load-bearing:

- Returns a strictly-positive `[0.5,1.5]` **integer-permille** factor that biases DEMAND **PRE-apportion**
  (the F3 invariant: never an f64 multiply on the i64 J path, never the granted amount), looked up per species
  by `SpeciesId` in stable order.
- The `exchange_deltas` become an ordered `ResourceField` tap routed through the `Ledger` so `ledger_closes`
  holds (and `FlowMatrix` row-sum == 0).
- Fold the `content_hash` + committed slot into `hash_world`, beside the existing `FlowMatrix` fold.
- **KEPT SELECTION-NEUTRAL ON INTRODUCTION** at the neutral factor (1000 permille = 1.0×) until the player
  actually edits — stage the wire neutrally, then re-pin only when a non-zero coefficient goes live.
- **Activating the non-zero coefficient IS the re-pin:** implement → run `determinism_hash_is_pinned
  --nocapture` on x86_64 AND aarch64 → replace the pinned literal at `sim-core/src/lib.rs:2010` + append a
  dated ledger note → regenerate byte-identical on both arches → `tools/gate.sh` green. The ADR-011 procedure
  F3/F4/F3.4 followed.

**The ripple (the payoff, on the LANDED F4 decomposer loop):** player earns credit → `RequestEcoliEdit` on a
real K-12 gene (pta / gltA / ptsG) → background FBA computes quantized `growth_ratio_q` + `exchange_deltas` →
at `due_epoch` the committed impact flows through two existing seams: `growth_ratio_q` as the
`EcoliEditModifier` factor, and `exchange_deltas` as the decomposer's `mineralize_rate` tap into
detritus→free_nutrient. Because F4 made `free_nutrient` endogenous, throttling E. coli mineralization
measurably drops `free_nutrient` → plant uptake starves → plant population declines. MEASURED in the live
`FlowMatrix`, CONSERVED, and EMERGENT — not scripted. The `due_epoch` delay is the diegetic
"computing in background…" window, not lag.

**Residual leak to surface at sign-off:** under a non-zero coefficient, the SAME player inputs on a machine
whose solver exceeds the minimum-lead window RECORD a different commit epoch → a different journal → a
different hash. Replay stays deterministic (it reads the journal); recording is not machine-speed-invariant
beyond the lead. The slip-cap (in epochs) makes the commit epoch solver-speed-independent *within* the lead.

**S6 gate:** behind F2-ontology-rekey for the evidence-complete E. coli (an unedited `gp.rs` flat-index
mis-expresses a non-canonical genome). The 7-property `firewall_determinism.rs` acceptance test (presence/
absence/different-bytes invariance, wall-clock independence, replay-never-re-runs-FBA, content-hash binding,
slip-cap termination, `req_id` determinism, economy hash-neutrality) is the S5 deliverable S6 INHERITS — S6
only flips the read coefficient on.

---

## Open questions carried to S6 sign-off

1. **Epoch cadence + lead** — confirm fixed every-N-generations; pin N + minimum lead (game-design tuning).
2. **Slip-cap mechanism** — timeout-to-neutral (sentinel `content_hash`) vs journaled CANCEL (in EPOCHS). Are deep edits PERMANENT for the PoC, or is a reversible un-commit Action + credit-refund needed?
3. **Tuning allowlist** — `PER_GEN_CAP`/`ECOLI_EDIT_COST`/`score_curve` need a DECISIONS.md provenance entry DISTINCT from biological constants.
4. **`content_hash` scope** — confirm the FBA model-version string stays OUT of the hashed bytes (in `Sourced` provenance), else a model re-bake silently re-pins. HARD requirement.
5. **F2 state** — confirm F2-ontology-rekey gives biologically-correct per-species E. coli expression at S6, or whether the `gp.rs` flat-index hazard remains.

---

*S4 economy + S5 firewall/Actions scaffolding are merged (see DECISIONS.md ADR-018 + the OVERSIGHT loop Slice
A/B on `main`). The load-bearing `EcoliEditModifier` (S6) remains a deliberate, ledgered, multi-ISA-validated
re-pin of the pinned determinism literal requiring human sign-off.*
