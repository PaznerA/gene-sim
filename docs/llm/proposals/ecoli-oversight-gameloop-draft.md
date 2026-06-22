# ADR-017 S4/S5 — the Oversight game-loop: earned-credit economy + the deep-edit determinism firewall

> **DRAFT — synthesis of three architectures + a three-reviewer adversarial pass. DESIGN ONLY for the
> load-bearing wire (S6); the hash-neutral S4 economy + S5 firewall/Actions scaffolding land autonomously.
> Builds on ADR-013 (CHEMOSTAT-J, F3/F4/F3.4 LANDED) and the ADR-017 layered architecture draft.**

This draft pins the design for **S4 (the earned-credit economy)** and **S5 (the journaled deep-edit Actions +
the determinism firewall + its CI acceptance test)**, and specifies **S6 (the load-bearing `EcoliEditModifier`
wire) as a LATER deliberate re-pin** requiring human sign-off. S4 + S5 are **hash-neutral by construction** —
the pinned literal `0x4e4d_0520_722a_a069` (sim-core/src/lib.rs:2010) stays unchanged, and *that
unchanged-ness IS the neutrality proof*. The brief's two inert `Action` variants are **already landed** as
part of this batch (see §6).

---

## 0. Ground truth (VERIFIED against the tree, 2026-06-22)

The single most important correction the synthesis makes over the older ADR-017 architecture draft: **F3, F4,
and F3.4 have LANDED.** The architecture draft still cites the **stale** pinned literal `0xf795_eac4_112f_acd5`
— that hash no longer exists. The live, verified facts this design rests on:

- **Pinned determinism literal: `0x4e4d_0520_722a_a069`** at `sim-core/src/lib.rs:2010`
  (`assert_eq!(run_headless(&cfg).hash, …)` inside `determinism_hash_is_pinned`). The F3.4 chemostat-tuning
  re-pin. The whole S4/S5 neutrality argument is "this literal does not move."
- **The decomposer loop is live.** E. coli is re-roled `Decomposer` via `niche.trophic_role`,
  `free_nutrient` is endogenous (minted only by the decomposer mineralization loop), and the measured S×S
  `FlowMatrix` is folded into `hash_world` (ADR-013 F4 ledger). `LiveSim::flow_matrix()` is the read-only export.
- **`harness::Action` is externally-tagged** (`crates/harness/src/lib.rs:99`), currently
  `Advance` / `ApplyEdit` / `ApplyEditRegion`. Adding variants is purely additive — every existing
  `actions.ndjson` line still deserializes to the same variant (round-trip test at lib.rs:620).
- **The `SeedJson` `#[serde(default)]` precedent is exact** — `lat`/`lon`/`avg_temp`/`season` are all
  `#[serde(default …)]` (`replay.rs:93-100`), so a new `game_mode` field loads byte-identically from every
  existing `seed.json`. **But** `SeedJson` is built as a struct literal at `replay.rs:188`, `:292`, and `:500`
  — those sites must add the field (or `..Default::default()`) or they fail to compile (a serde-default does
  NOT cover construction). This is a known S5 chore, deferred with the rest of `game_mode`.
- **License boundary is `scripts/check_license.sh:59`** (NOT `tools/check_license.sh` — that file does not
  exist; `gate.sh` calls `./scripts/check_license.sh`). Its `BOUNDARY_CRATES="oracle-slim oracle-fba
  relations-index"` **already lists `oracle-fba`** (ADR-017 S0 is DONE). The new boundary is mechanically
  enforced the moment the crate lands.
- **`Action::ApplyEdit` DRAWS from `SimRng`** — it calls `sim.with_genome_and_rng(|g,rng| apply_edit(…))`
  (`lib.rs:356-368`). This is the load-bearing correction from the adversarial pass: the two oversight
  variants must be modeled on **`Advance(0)`** (a no-op that draws **zero** words), NEVER on `ApplyEdit`.
- **`fixed::to_unit_u16`** (`sim-core/src/fixed.rs`) is the single audited float→integer chokepoint
  (floor-based, F-1 landed). It is the quantization contract for everything that crosses the firewall.
- **F2-Strategy "expressed-but-unread" precedent**: `gp.rs` caches a `Strategy{budget,role,…}` that selection
  does not yet read — proof that a written-but-unread slot has **coefficient zero** and is hash-neutral.
- **`edits_used`-in-`ScenarioResult` precedent** (`campaign.rs`): a pure integer counter that lives in the
  harness/env layer and adds **zero** bytes to `hash_world`. The credit economy is the same pattern.

---

## 1. Winning architecture (the synthesis)

Three candidate architectures converged on the **same correct core**; the synthesis takes **Arch 2 verbatim**
on the economy, the firewall, and the Action shapes, and **grafts** the safety gaps the reviewers found.

### The one-way quantized-integer crossing

> The non-bit-reproducible FBA solve is ALWAYS the **producer** (off-thread, off-hash). The deterministic sim
> only ever **consumes** a **quantized integer** committed at a **fixed future epoch** via a **journaled
> Action** — exactly like a player `ApplyEdit`.

INSIDE the hash live ONLY: quantized integers committed at an epoch boundary via a journaled Action
(deterministic inputs) + the `content_hash`. OUTSIDE live: the FBA/MOMA solve, all raw fluxes, the subprocess,
the wall-clock arrival, and every float before `fixed::to_unit_u16`. At S4/S5 **nothing crosses inward**
(coefficient zero), so the firewall STRUCTURE is pinned before S6 makes anything load-bearing.

### The two layers

- **Economy (S4)** — score→credit accrual lives in the **harness/env layer** (a new
  `crates/harness/src/oversight.rs`), **never** an ECS `World` resource, so it adds **0 bytes** to `hash_world`
  by construction (the `edits_used` precedent). It is a **pure integer fold** over the per-gen stats stream the
  engine ALREADY produces (`region_allele` / `Observation` / `flow_matrix` — all verified RNG-free read-only
  projections), recomputed deterministically on replay from `(seed, actions)`.
- **Firewall (S5)** — a producer/consumer split with a **`due_epoch` buffer**: the FBA solve produces a
  quantized integer; the deterministic sim consumes it only at a fixed future epoch. A new
  `crates/oracle-fba` (a structural clone of `oracle-slim`) is the std-only subprocess boundary that
  **quantizes before returning** (floats never escape the child). A new `crates/harness/src/firewall.rs`
  `EditFirewall{ pending: BTreeMap<u32, Vec<PendingImpact>> }` drains at each epoch boundary in
  `(SpeciesId, req_id)` order.

### The credit economy, pinned

`CreditLedger{ credit:u64, accrued_total:u64 }` in `oversight.rs`. Per `Advance`:

```
credit += clamp(quantize(objective_progress_delta_this_gen), 0, per_gen_cap)
```

computed by stepping a fresh `GeneSimEnv` one `Advance(1)` at a time exactly as `campaign::evaluate` does
(`campaign.rs:189-212`), reading `region_allele` after each. The composite signal (grafted from the open
questions, **REQUIRED quantize discipline** from the reviewers):

1. **Term A — `region_allele`-toward-objective** (works today; `Simulation::region_allele` is verified live).
2. **Term B — a `FlowMatrix`-health delta** (both plant↔E. coli off-diagonals positive = a healthy
   mineralization loop). `flow_matrix()` is exposed and F4 has landed.

**Quantize discipline (the cross-ISA hazard the reviewers flagged):** quantize **each gen's term to `u16`
FIRST** via `fixed::to_unit_u16`, then difference the **integers** (`q_now − q_prev`) — NEVER difference the
f64 means and quantize the delta. Sample `flow_matrix()` at a **fixed post-`Advance(1)` point** (before the
next step resets it). Pin a sentinel for the empty-region / zero-population path (`populated_cells == 0`).
Both terms are independently RNG-free. Pin the composite formula + term order in the S4 commit.

### Two-tier spend gate

Cheap `ApplyEdit`/`ApplyEditRegion` stay free/frequent. The rare, expensive `RequestEcoliEdit` is gated by
`credit >= ecoli_edit_cost`, structurally identical to the VERIFIED `campaign.rs` refusal
(`if edits_used < scenario.edit_budget` → else refused, not replayed).

**Journal the spend DECISION, do not recompute the gate on replay (grafted from a reviewer).** Recomputing
credit at a `RequestEcoliEdit` is the one place credit non-determinism could become a replay break (a
borderline request accepts on record but refuses on replay). The robust rule: the recorder writes the spend
OUTCOME, and replay reads the decision from the journal — exactly as it reads the `CommitEcoliImpact` payload
from the journal rather than re-solving FBA. Replay still recomputes credit for display, but the gate decision
is journaled. (Pin the exact mechanism in the S4/S5 commit.)

### `CreditPolicy` provenance

`CreditPolicy` (PER_GEN_CAP / ECOLI_EDIT_COST / score_curve) is loaded from `data/oversight/<name>.json` (the
`load_campaign` precedent) with **its own DECISIONS.md provenance allowlist DISTINCT from biological
constants** — it is assumption-class game-design tuning, not science. See open question [4].

---

## 2. The firewall (S5), pinned

### Epoch clock — `Tick`, never wall-clock

`due_epoch` is a function of the **`Tick(u64)` generation counter** (VERIFIED `sim-core/src/lib.rs:83`,
advanced by `advance_tick` at `:477`), NOT wall-clock. There is **zero** `Instant`/`SystemTime`/`now()` in the
hash path. Cadence is **fixed every-N-generations** (cleaner determinism than completion-relative), with a
**minimum lead** (epochs between request and earliest possible commit) so a slow oracle has slack before the
first slip. N + the lead are game-design tuning knobs (open question [1]).

### Slip rule — self-describing, journaled

If the deep job misses its `due_epoch`, the commit deterministically **SLIPS** to the next epoch and the slip
is ITSELF a journaled fact (`slipped_from: Option<u32>` **inline** in `CommitEcoliImpact`), so solver speed
changes neither the result NOR its timing. The pending buffer is a **`BTreeMap<u32, Vec<PendingImpact>>`**
keyed by `due_epoch`, drained in ascending **`(SpeciesId, req_id)`** order — explicitly **NOT** a `HashMap`
iterated in sim logic (honors inv #3 by name). A `debug_assert` of strict ordering on drain guards the
tie-break; `(SpeciesId, req_id)` is unique per epoch bucket so the order is total.

### SLIP-CAP — promoted from open question to a PINNED, REQUIRED design

All three reviewers independently flagged that a hung/crashed `oracle-fba` subprocess slips the commit
**forever** and stalls the journal, and that this is a **determinism-completeness requirement**, not optional
polish. **PINNED:** a **max-slip of K generations**, counted in **EPOCHS, never wall-clock seconds**. At
`due_epoch + K`, the firewall deterministically commits a **NEUTRAL/identity impact** with a **fixed sentinel
`content_hash`** (`growth_ratio_q` = the 1.0× permille, empty `exchange_deltas`). The abandonment is itself
journaled, so the journal **always terminates deterministically** regardless of how long the subprocess hangs.
This is acceptance-test property **(5)** below. (Timeout-to-neutral vs a journaled CANCEL is the remaining
sub-choice — open question [2]; whichever is chosen is counted in epochs.)

### Boundary crate — `crates/oracle-fba`

A structural clone of the VERIFIED `oracle-slim` template: `#![forbid(unsafe_code)]`, a `resolve_fba_bin`
`$FBA_BIN→pinned→PATH` resolver, `FbaError{Io,Spawn,NonZeroExit,MissingOutput}` mirroring `SlimError`,
std-only, shells out, links nothing (inv #1). **CRITICAL difference from `oracle-slim`: it QUANTIZES BEFORE
RETURNING.** The child process emits **already-quantized `u16`/`i16` text**; the parent parses **integers
only** — no float ever participates in any ordering/comparison the harness does (a libm float-formatting
difference could otherwise survive into the quantized integer across arches). For single-gene edits it is a
**FROZEN-TABLE KO lookup**, not a live solve — collapsing the FBA non-reproducibility hazard entirely. Already
allowlisted in `scripts/check_license.sh:59` (S0 done).

### The background dispatch (the #1 hazard — pinned single-writer discipline)

All three reviewers flagged this as the headline risk: **there is ZERO `Instant`/`SystemTime`/`thread::spawn`/
`mpsc`/`channel` in `crates/harness/src` today** (grep empty). The off-thread `oracle-fba` dispatch is
**net-new concurrency** in the crate that owns the journal. **PINNED single-writer rule:**

> The background thread **NEVER** mutates `pending`/the journal. It only writes a completed quantized payload
> into a per-`req_id` **mailbox**. **ONLY** the synchronous step loop, at a **fixed epoch boundary**, reads the
> mailbox and emits the journaled `CommitEcoliImpact` (slipping deterministically if the mailbox is empty).

This makes arrival-time **irrelevant**: the commit epoch is decided by epoch-counting, never by which thread
message arrives first, never by "whatever finished by now." The dispatch lives in the `GeneSimEnv` harness
driver (NOT `godot/` — inv #2; NOT the single-threaded `World` — ADR-002). The harness owns producing the
`CommitEcoliImpact`.

### Quantize + freeze + content-hash

`EcoliImpact{ growth_ratio_q:u16, exchange_deltas:Vec<(u16,i16)> }` is content-hashed over the **QUANTIZED
BYTES** (never the floats). `exchange_deltas` MUST be in **canonical exchange-index order** at production time
(the producer emits a fixed order; replay's recompute assumes the same order). The `content_hash` binds the
committed integers; a tampered journal whose `content_hash` disagrees with the recomputed quantized-bytes hash
is REJECTED on replay as `InvalidData` (property (4)). **The FBA model-version string is NOT in the hashed
bytes** — it belongs in `Sourced` provenance for the INSPECT view, else a model re-bake silently re-pins
(open question [6], a HARD requirement).

### `req_id` allocation — pinned

`req_id` is a **deterministic monotonic occurrence index into the `RequestEcoliEdit` stream**, reset per
episode at `reset()`, NEVER wall-clock/UUID/global-atomic-derived. The reviewers found a contradiction in the
draft: it must advance on **every** `RequestEcoliEdit` occurrence (the raw stream position), **NOT** only on
accepted ones (the campaign `edits_used++` rule is the OPPOSITE and would couple `req_id` to the credit
computation being bit-identical). **PINNED: occurrence-index, decoupled from credit.** This is acceptance-test
property (6).

### Request-without-commit — a HARD replay error

On replay, credit is recomputed from the stats stream while the impact is read from the journal. A
`RequestEcoliEdit` recorded **without** its paired `CommitEcoliImpact` (crash mid-episode, truncated ndjson)
must **FAIL replay with `InvalidData`** — replay must NEVER invent a slip or a neutral commit. The slip-cap
default only legitimizes a commit the RECORDER wrote; the REPLAYER never fabricates one. The recorder writes
both, both are counted in `action_count`. (Acceptance-test property: request-without-commit ⇒ `InvalidData`.)

---

## 3. The firewall acceptance test (the S5 deliverable, folded into `tools/gate.sh`)

A new `crates/harness/tests/firewall_determinism.rs`. It MUST drive the **REAL record→replay harness path**
(not a hand-rolled buffer) — the leak hides in thread-scheduling, not in a synchronous fold, so a mocked
synchronous stub proves nothing about the real dispatch.

**Acceptance core:** `Simulation::run_stats().hash` stays **byte-identical** to the pinned
`0x4e4d_0520_722a_a069` whether `oracle-fba` is **ABSENT, SLOW, PRESENT-returning-A, or
PRESENT-returning-different-bytes-B**, for the SAME `(seed, actions)` Oversight episode — UNTIL the impact is
committed at its `due_epoch`, after which the committed integer is consumed from `actions.ndjson` WITHOUT
re-running any solve.

The test asserts **SEVEN properties** (the design's four + the three the reviewers required):

1. **PRESENCE/ABSENCE/DIFFERENT-BYTES INVARIANCE.** Run the same episode three ways: `$FBA_BIN` unset so spawn
   fails (graceful skip, mirroring `oracle-slim`); a `FakeOracle` returning payload-A; a `ChaosOracle`
   returning a different payload-B each call. At S4/S5 the committed slot is UNREAD by selection (coefficient
   zero), so all three hashes equal the pinned literal **AND** (the early tripwire) the identical `DrawCount`
   (`hash_world` folds `draw_count` + a terminal `SimRng.next_u64()` at `lib.rs:1960-1962`, so DrawCount
   divergence is the earliest, most localized leak signal).
2. **WALL-CLOCK INDEPENDENCE.** A `LatencyInjector` drives the same `(seed, actions)` twice in LIVE mode —
   once INSTANT, once with an injected delay forcing a `due_epoch` SLIP. Assert IDENTICAL hash AND identical
   journaled `CommitEcoliImpact` (same final `due_epoch`, same `slipped_from`). Two DIFFERENT latencies that
   slip to the SAME recorded epoch produce the identical journal-replayed hash. **The test must drive the
   ACTUAL background-dispatch path** (not a synchronous stub). Assert no `Instant`/`SystemTime` on the
   dispatch→commit path.
3. **REPLAY NEVER RE-RUNS FBA.** Record an Oversight episode (inline-quantized `CommitEcoliImpact` written to
   `actions.ndjson`), then `--replay` with `$FBA_BIN=/bin/false` (a `PanicOracle` that FAILS if invoked).
   Assert replay hash == recorded hash AND a spawn-counter at zero — the committed integer is consumed
   straight from the journal. Extends the verified read-only-export-is-replay-stable property
   (`episode_injections.rs`, `observe_all_is_read_only_does_not_change_hash` at `lib.rs:2423`).
4. **CONTENT-HASH BINDING.** A `CommitEcoliImpact` whose `content_hash` disagrees with its recomputed
   quantized-bytes hash (`growth_ratio_q` + index-ordered `exchange_deltas`) is REJECTED on replay as
   `io::ErrorKind::InvalidData` — mirroring the verified malformed-guide / `action_count`-mismatch rejections
   (`replay.rs:255/265, 486`).
5. **SLIP-CAP TERMINATION (reviewer-required).** A `NeverReturnsOracle` that never returns must force the
   journal to terminate deterministically at the pinned `max_slip_epoch`: assert a NEUTRAL/identity
   `CommitEcoliImpact` with the fixed sentinel `content_hash` (or a journaled CANCEL) lands at exactly
   `due_epoch + K`, and that the recorded journal is identical regardless of how long the subprocess hangs
   (the cap is counted in epochs, not seconds). Replay reproduces it byte-for-byte without ever spawning the
   oracle.
6. **`req_id` DETERMINISM (reviewer-required).** Two `RequestEcoliEdit` in the same episode record `req_id`s
   that are a deterministic monotonic occurrence index; replay reproduces the identical `(SpeciesId, req_id)`
   drain order. Assert byte-identical journal across two record runs on the same inputs.
7. **ECONOMY HASH-NEUTRALITY (reviewer-required).** Assert that enabling credit accrual (the per-gen
   `region_allele` + `FlowMatrix`-health fold) leaves `run_stats().hash == 0x4e4d_0520_722a_a069`
   **unchanged** — NOT merely that accrual is reproducible — mirroring `per_gen_stats.rs`'s
   `per_gen_stats_preserves_determinism_hash`. ALSO assert a borderline-credit `RequestEcoliEdit` replays to
   the SAME accept/refuse decision (the journaled-spend-decision property).

The test is the deliverable of S5; **S6 INHERITS it** and only flips the read coefficient on.

---

## 4. Vision fit — the earned-edit OVERSIGHT mode

This realizes the player-agency payoff of the north star (MEMORY `ecoli-layered-ecosystem-vision`): the fast
abstract sim runs continuously; the player **EARNS** credit from ecosystem-improvement signals; **SPENDS** it
to edit the **REAL K-12 E. coli** (the soil decomposer); the impact is computed in the **BACKGROUND** by FBA;
and it **RIPPLES** across the ecosystem through the conserved-J economy — *the substrate it ripples through is
ALREADY BUILT* (F3/F4 landed).

**The ripple path (ties to the LANDED F4 decomposer loop):** player earns credit → spends `RequestEcoliEdit`
targeting a real K-12 gene (pta / gltA / ptsG — acetate-overflow / TCA / glucose-uptake levers) → background
FBA computes quantized `growth_ratio_q` + `exchange_deltas` → at `due_epoch` the committed impact (at **S6**)
flows through TWO existing seams: (1) `growth_ratio_q` as a strictly-positive `[0.5,1.5]` `EcoliEditModifier`
factor (the `soil::EnvironmentModifier` / `climate::ClimateModifier` / F4 `ResourceModifier` product
precedent), and (2) `exchange_deltas` as the decomposer's `mineralize_rate` tap into detritus→free_nutrient.
Because F4 made `free_nutrient` endogenous (only the decomposer mints it), throttling E. coli mineralization
measurably DROPS `free_nutrient` over the cross-tick frozen-snapshot lag → plant uptake starves → plant
population visibly declines. The ripple magnitude is MEASURED in the live `FlowMatrix` (both plant↔E.coli
off-diagonals collapse), CONSERVED (`ledger_closes` + `FlowMatrix` row-sum==0), and EMERGENT — not scripted.

**The delay is a FEATURE, not lag.** The firewall's `due_epoch` buffer extends F4's existing cross-tick
frozen-snapshot lag to the deep-compute layer. The "computing in background…" window IS the producer/consumer
gap made diegetic and journaled — the player watches the ecosystem absorb a consequential edit over several
ticks instead of seeing an instant magic number. The two fidelities share ONE conserved-J substrate — ADR-017's
whole premise, now realizable because F4 built it.

---

## 5. The exact slice breakdown

| Slice | Scope | Hash status | Sign-off |
|---|---|---|---|
| **S4 — economy** | `crates/harness/src/oversight.rs`: `CreditLedger` + `CreditPolicy` (data-loaded) + the RNG-free integer accrual fold (Term A region_allele + Term B FlowMatrix-health, quantize-each-then-difference). `GameMode` serde-default on `SeedJson`. The economy hash-neutrality test. | **HASH-NEUTRAL** (pinned literal unchanged IS the proof) | autonomous, gate-green |
| **S5 — firewall + Actions + acceptance test** | `crates/oracle-fba` (FbaError mirroring SlimError, quantize-before-return, frozen-KO lookup). The two journaled Actions **wired to the firewall** (request→buffer, commit→committed-slot, drain). `crates/harness/src/firewall.rs` `EditFirewall{BTreeMap<u32,Vec<PendingImpact>>}` + single-writer background dispatch + slip-cap. The `firewall_determinism.rs` 7-property acceptance test, folded into `tools/gate.sh`. `main.rs` demo OVERSIGHT episode (request→buffer→commit→drain) recorded + replayed in CI. | **HASH-NEUTRAL** (committed slot WRITTEN but UNREAD — coefficient zero; the F2-Strategy precedent) | autonomous, gate-green |
| **S6 — load-bearing wire** | A new `EcoliEditModifier` behind the inv-#5 modifier seam that READS the committed `growth_ratio_q`/`exchange_deltas` slot and returns a `[0.5,1.5]` integer-permille factor biasing DEMAND PRE-apportion (F3 invariant: never an f64 multiply on the i64 J path). The `exchange_deltas` become an ordered `ResourceField` tap through the `Ledger`. Fold the `content_hash` + slot into `hash_world`. **Activating the non-zero coefficient IS the re-pin.** | **RE-PIN of `0x4e4d_0520_722a_a069`** | 🛑 **human sign-off REQUIRED** + multi-ISA (x86_64 + aarch64) validation |

**This batch delivers the hash-neutral S4 + S5 only; it STOPS at S5.** The brief's INERT `Action` scaffolding
(§6) is the first concrete step of S5, landed now because it is unambiguously hash-neutral.

---

## 6. What landed THIS batch (the INERT Action scaffolding)

The two additive variants on the externally-tagged `harness::Action` enum (`crates/harness/src/lib.rs`),
landed as **inert scaffolding** — parsed / round-tripped / journaled but **not yet acted on**:

```rust
Action::RequestEcoliEdit {
    species: u16,            // operator/species granularity (inv #6); → SpeciesId at S5 (see note)
    locus: genome::LocusId,
    edit_kind: crispr::EditKind,   // the LANDED Knockout/Knockdown/Activate (commit 41a7f48)
    due_epoch: u32,
    req_id: u32,             // deterministic monotonic occurrence index — replay-stable
}
Action::CommitEcoliImpact {
    species: u16,
    req_id: u32,
    due_epoch: u32,
    slipped_from: Option<u32>,     // self-describing slip — replay is exact
    content_hash: u64,             // binds the quantized bytes
    growth_ratio_q: u16,           // fixed::to_unit_u16 scale; UNREAD at S4/S5
    exchange_deltas: Vec<(u16, i16)>,  // canonical exchange-index order; UNREAD at S4/S5
}
```

**Why the `CommitEcoliImpact` shape wins (Arch 2's graft):** it carries the quantized payload INLINE plus
`slipped_from`, so replay reads the impact straight from `actions.ndjson` and NEVER re-runs the deep compute,
and the slip is self-describing. Arch 0's commit carried only `(req_id, due_epoch, content_hash)` and relied
on re-reading the buffered request to recover the payload — strictly more fragile; rejected.

**Why `species: u16` and not `SpeciesId`:** the core's `SpeciesId(u16)` (sim-core/src/lib.rs:175) has a
**private** inner field, **no** `Serialize`/`Deserialize` derive, and **no** public constructor. Adding serde
+ a constructor to a core type is a deliberate public-surface change that the `determinism_hash_is_pinned`
test region itself constructs (`SpeciesId(c.species)` etc.), so it belongs in the **signed-off S5 slice**, not
in this hash-neutral scaffolding batch. The variants use a raw `u16` placeholder now; **S5 promotes it to a
serde-derived `SpeciesId`** alongside wiring the firewall. This is the only deviation from the design's
literal `species: SpeciesId` shape, and it is a strict no-op for back-compat (the variants are unconstructed
in any replay corpus today).

**Hash-neutrality, proven:**

- **Externally-tagged additive variants** — every existing `actions.ndjson` line still deserializes to its
  same variant (`{"Advance":10}` etc. untouched). Round-trip + back-compat asserted by the new test
  `ecoli_oversight_actions_round_trip_and_are_back_compat` (alongside the existing
  `action_and_edit_action_round_trip_through_serde`).
- **Strict NO-OP `step` arms** — `Action::RequestEcoliEdit { .. } => {}` and
  `Action::CommitEcoliImpact { .. } => {}` call neither `sim.step()` nor `with_genome_and_rng()`. They draw
  **zero** `SimRng` words and mutate **no** hashed component — modeled on `Advance(0)`, NOT on `ApplyEdit`
  (which DRAWS). The test asserts stepping them leaves the observation generation unchanged.
- **Inv #6 type guard extended** — both variants are added to the `action_space_is_species_granular`
  compile-time match, destructuring every field, asserting they target a `species` + a `locus`, never a
  per-organism handle. A future organism-handle field would stop the test compiling and force a review.
- **`campaign.rs::evaluate` and `main.rs` injection loops** — each gains a no-op arm that steps the oversight
  actions through (themselves no-ops) so a journaled stream containing them stays consistent. These are the
  exact insertion points S5 grafts the epoch-boundary firewall drain into.
- **The pinned literal `0x4e4d_0520_722a_a069` is UNCHANGED** — `cargo test -p sim-core
  determinism_hash_is_pinned` passes untouched. That unchanged-ness IS the neutrality proof.

---

## 7. What stays hash-neutral vs what re-pins

**HASH-NEUTRAL NOW (S4 + S5 — autonomous, gate-green, no sign-off),** each proven by the UNCHANGED pinned
literal `0x4e4d_0520_722a_a069`:

- `GameMode` + `CreditPolicy` + score→credit accrual: entirely in harness/env (`oversight.rs`), a pure fold
  over the stats stream, ZERO bytes into `hash_world` (the `edits_used` precedent). The economy test asserts
  the hash is unchanged with/without credit accrual.
- The two Actions: serde-additive to the journal. `RequestEcoliEdit` advances no RNG; `CommitEcoliImpact`
  writes a per-species committed slot WRITTEN BUT UNREAD by selection (coefficient ZERO — the F2-Strategy
  expressed-but-unread precedent).
- `crates/oracle-fba` boundary crate + the firewall scheduler: OFF the hash by construction (producer side).
  S0 license enforcement already in place (`scripts/check_license.sh:59`).
- The firewall ACCEPTANCE TEST is itself the S5 deliverable, written now so S6 inherits it.

**LOAD-BEARING LATER (S6 — DELIBERATE RE-PIN, 🛑 human sign-off REQUIRED):**

- A new `EcoliEditModifier` behind the inv-#5 modifier seam. Reads the per-species committed
  `growth_ratio_q`/`exchange_deltas` slot, returns a strictly-positive `[0.5,1.5]` INTEGER-PERMILLE factor
  that biases DEMAND PRE-apportion (the F3 invariant: never an f64 multiply on the i64 J path, never the
  granted amount), looked up per species by `SpeciesId` in stable order. The `exchange_deltas` become an
  ordered `ResourceField` tap routed through the `Ledger` so `ledger_closes` holds.
- Folding the `content_hash` (and the committed slot) into `hash_world` — beside the verified `FlowMatrix`
  fold.
- **KEPT SELECTION-NEUTRAL ON INTRODUCTION:** the modifier is introduced at the neutral factor (committed
  delta == 1000 permille = 1.0×) until the player actually edits — the climate-extremity neutral-at-default
  precedent. Even S6 can STAGE the wire neutrally, then re-pin only when a non-zero coefficient goes live.
- **ACTIVATING the non-zero coefficient IS the re-pin:** implement → run `determinism_hash_is_pinned
  --nocapture` on x86_64 AND aarch64 → replace the literal at `lib.rs:2010` + append a dated ledger note →
  regenerate byte-identical on both arches → `tools/gate.sh` green. The exact ADR-011 procedure F3/F4/F3.4
  followed four times.

**Residual recording-time wall-clock sensitivity at S6 (surfaced, not buried).** Under a non-zero coefficient,
the SAME player inputs on a machine where the solver exceeds the minimum-lead window will RECORD a different
commit epoch → a different journal → a different hash. **Replay stays deterministic** (it reads the journal),
but **recording is not machine-speed-invariant beyond the lead.** This is a real residual leak that bounds the
lead/slip-cap tuning; it must be surfaced to the human at S6 sign-off. The slip-cap (counted in epochs) makes
the commit epoch solver-speed-independent *within* the lead window; beyond it, two machines can record
different journals.

**Blast radius:** one isolated, ledgered, signed-off commit. S6 is additionally gated behind F2-ontology-rekey
being complete for the evidence-complete 134/136-locus E. coli (an unedited `gp.rs` flat-index mis-expresses a
non-canonical genome → deterministic-but-biologically-WRONG); a VALUE-only E. coli stand-in prototypes the
economy + firewall (S4/S5) before F2, which is exactly why this batch STOPS at S5.

---

## 8. Open questions (carried to S5/S6 sign-off)

1. **EPOCH CADENCE + LEAD** — confirm fixed every-N-generations (N from the `Tick` counter) over
   completion-relative; pin N + a minimum lead so a slow oracle has slack and "computing in background" is a
   visible-but-not-excessive tell. Game-design tuning.
2. **SLIP CAP mechanism** — timeout-to-neutral (fixed sentinel `content_hash`) vs a journaled CANCEL. Either
   way counted in EPOCHS; the journal MUST terminate deterministically and the abandonment MUST itself be
   journaled. Also: are deep edits PERMANENT for the PoC (a reversible edit needs a second un-commit Action +
   a credit-refund economy)?
3. **`req_id` allocation** — confirmed occurrence-index (advances on every `RequestEcoliEdit`, decoupled from
   credit), reset per-episode at `reset()`. (Resolved in §2; flagged here for the sign-off checklist.)
4. **ASSUMPTION-CLASS TUNING ALLOWLIST** — `PER_GEN_CAP` / `ECOLI_EDIT_COST` / `score_curve` from
   `data/oversight/<name>.json` need a DECISIONS.md provenance allowlist entry DISTINCT from biological
   constants. Who signs off; is an OVERSIGHT-economy ADR the right home?
5. **CREDIT-ACCRUAL SIGNAL** — pin the composite formula: Term A `region_allele`-toward-objective + Term B
   `FlowMatrix`-health delta, each independently RNG-free, **quantize-each-to-u16-then-difference** (not
   difference-then-quantize), `flow_matrix()` sampled at a fixed post-`Advance(1)` point, with an empty-region
   sentinel.
6. **CONTENT_HASH SCOPE** — `content_hash` is over the quantized bytes ONLY. Confirm the FBA model-version
   string is NOT in the hashed bytes (it belongs in `Sourced` provenance for INSPECT) — else a model re-bake
   silently re-pins. HARD requirement.
7. **DATA-LICENSING STOP-THE-LINE (blocks S6/S2, NOT this batch)** — the BiGG `e_coli_core`/`iML1515` UCSD
   non-commercial clause vs the inv-#1 commercial-freedom rationale. The `oracle-fba` CODE is clean (std-only,
   already in `BOUNDARY_CRATES`); the open gate is the DATA license of any vendored FBA KO table. The
   VALUE-only S4/S5 stand-in vendors no model file, so it is unblocked — confirm the human ruling is sequenced
   before any GEM is vendored. (Note: TASKS.md records "human-accepted the academic non-commercial clause,
   2026-06-21" for `e_coli_core` — confirm this covers a vendored KO table under inv #1's commercial-freedom
   rationale, since accepting an academic clause is in tension with a future closed release.)
8. **F2 STATE** — confirm whether F2-ontology-rekey is complete (needed so S6 per-species E. coli expression
   is biologically correct) or whether S6 reads through a VALUE-only stand-in. F3/F4/F3.4 are VERIFIED landed;
   F2 status gates the load-bearing wire. (TASKS.md shows F2-1 + F2-2 + B-1/B-2 DONE — the 136-gene genome
   expresses microbe traits; confirm this is sufficient for S6 or whether the `gp.rs` flat-index hazard
   remains.)
9. **REPLAY CONSISTENCY** — a recorded `RequestEcoliEdit`'s spend must reproduce AND its paired
   `CommitEcoliImpact` must already be in `actions.ndjson`. A request WITHOUT its commit is a journaling bug
   (HARD `InvalidData` on replay), not a silent slip. The recorder writes both; replay reads the commit
   straight from the journal while recomputing credit (for display) from the stats stream and reading the
   spend DECISION from the journal.
10. **`SeedJson` `game_mode` construction sites** — adding the `#[serde(default)]` `game_mode` field requires
    `..Default::default()` (or the field) at the three struct-literal sites `replay.rs:188/292/500`; a
    serde-default does not cover construction. Deferred to S5 with the rest of `game_mode`.

---

*Draft authored for the ADR-017 S4/S5 design phase. The hash-neutral S4 economy + S5 firewall scaffolding land
autonomously and gate-green; the load-bearing `EcoliEditModifier` (S6) is a deliberate, ledgered,
multi-ISA-validated re-pin of `0x4e4d_0520_722a_a069` requiring human sign-off.*
