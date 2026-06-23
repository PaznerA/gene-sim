# Pinned spec — D0 interestingness scorer + D1 trace (`crates/discovery`)

> Output of the `emergent-scorer-design` workflow (3-lens panel → judge). The buildable spec for the
> emergent-discovery epic's first phase. Implementer + ADR-023 consume this. Companion:
> [emergent-discovery-harness-draft.md](emergent-discovery-harness-draft.md).
>
> **Invariants:** #1 std-only crate (no GPL, like `relations-index`/`oracle-fba`). #2 the scorer only READS
> exports — no biology in the scorer. #3 every metric is an INTEGER/quantized, RNG-free function of the trace;
> trace capture is off `hash_world` (proven by `per_gen_stats`) → the pinned literal `0x47a0_3c8f_6701_f240`
> CANNOT move. #4 headless. #5 the metric set is pluggable behind `InterestingnessScorer`. #6 config/operator level.

## Constants
`SCALE = 10_000` (basis points; every `m*` ∈ [0,SCALE]). `SCORE_SCALE = 1_000_000` (Q micro-units). `FP_DIMS = 12`.
All score-path arithmetic is `u64`/`u128`-promoted then truncated; **no `f64` in the score path** (the lone fenced
float touch is the `allele`/`energy` → permille `q16()` quantization done ONCE at capture). No HashMap iteration —
fixed field order.

## The 6 metrics (all over the STABLE WINDOW `W = [g0..G)`, `g0 = G * BURN_IN_BP/SCALE`, BURN_IN_BP=2000 → drop first 20%)

- **M1 — Coexistence** (W1=14). A species *persists* iff `alive_gens_W[i] ≥ |W|*PERSIST_BP/SCALE` (PERSIST_BP=8000
  → 80%). `R = #persisting`. `m1 = (min(R,RICH_CAP).saturating_sub(1))*SCALE / max(min(S,RICH_CAP)-1,1)`, RICH_CAP=6.
  `R≤1 → 0` (a monoculture earns ZERO — coexistence, not mere survival, is the signal).
- **M2 — Evenness** (W2=14). Per gen `N[g]=Σ_i pop`, `sumsq[g]=Σ_i pop²` (u128); `simpson_bp[g] = SCALE −
  sumsq*SCALE/N²` ( = 1−Σpᵢ² ). `m2 = mean over W of simpson_bp[g]` (gens with N>0). Monoculture → 0.
- **M3 — Dynamism** (W3=22, tied-highest). Per species: `amp[i] = (maxW−minW)*SCALE/(maxW+1)`; `turns[i]` = #sign
  changes in `Δpop` over W (dropping zero deltas); `turn_bp[i] = min(SCALE, turns*SCALE/TURN_TARGET)`, TURN_TARGET=8.
  `m3_i = (amp+turn_bp)/2`. Persistence-weighted: `m3 = Σ m3_i*persist_bp[i] / (Σ persist_bp[i] + 1)`. **Flat line →
  0; single monotone boom → high amp but turns≈0 → capped ~SCALE/2; limit cycle → high amp AND turns → near SCALE.**
  The anti-"single-boom-is-maximal" guard + the "reward drama over forced stability" term.
- **M4 — Trophic structure** (W4=18). `Agg[i*S+j] = Σ_W flow[g][i*S+j]` (i128). `E = #off-diagonal Agg>0 edges`;
  `distinct_roles` = # role ordinals touching an edge; `total_flow = Σ off-diagonal Agg`. `edge_bp = min(SCALE,
  E*SCALE/EDGE_TARGET)` (EDGE_TARGET=4); `role_bp = min(SCALE, distinct_roles*SCALE/3)`; `flow_bp =
  octave_log_bp(total_flow)` (the `signature.rs::flow_to_grid` octave curve, parity-tested). `m4 =
  (edge_bp+role_bp+flow_bp)/3`. All-zero matrix → 0.
- **M5 — Emergent events** (W5=18; derived from `pop` only, no core event journal). BOOM `(g,i)`: `pop[g][i] ≥
  pop[g-1][i]*BOOM_K ∧ pop[g-1][i] ≥ POP_FLOOR` (BOOM_K=3, POP_FLOOR=5), `mag = octave_log_bp(pop[g]/pop[g-1])`.
  CRASH: `pop[g-1] ≥ CRASH_FROM ∧ pop[g] ≤ pop[g-1]/CRASH_K` (CRASH_FROM=20, CRASH_K=4), `mag =
  octave_log_bp(pop[g-1]/pop[g])`. TAKEOVER `g`: rank-1 `argmax_i pop` flips (ties → lower SpeciesId), both N>0, `mag
  = SCALE`. IMMIGRATE_ESTABLISHED: per `InocRec`, species alive at G-1, `mag = SCALE`. `event_raw = Σ mags`; `m5 =
  min(SCALE, event_raw*SCALE/EVENT_SAT)`, EVENT_SAT=6*SCALE (saturating — 100 booms can't run away; POP_FLOOR/
  CRASH_FROM gate tiny-base jitter).
- **M6 — Survival GATE** (multiplicative, NOT in the weighted sum). `last_multi_gen` = last gen with ≥2 species
  alive. `longevity_bp = last_multi_gen*SCALE/G`; `ran_long_bp = G*SCALE/max(1,gens_requested)`; `m6 =
  min(longevity_bp, ran_long_bp)`. **End-state extinction is NOT penalized** — only EARLY total loss of multi-species
  dynamics (dead by gen 5 of 500 → m6 ≈ tens of bp → crushes Q). Encodes the open-system memory: extinction is valid,
  instant death is not.

## Combine
`weighted = (W1*m1 + W2*m2 + W3*m3 + W4*m4 + W5*m5) / WSUM`, WSUM = 86. Gate: `Q_bp = weighted * m6 / SCALE`. Scale:
`Q = Q_bp * SCORE_SCALE / SCALE ∈ [0, 1_000_000]`. Ship `breakdown = [m1..m6]` alongside for explainability.

**Novelty** (applied at SAVE time as a MULTIPLIER on Q vs the saved-gem fingerprint set — never creates score from a
boring run; only protects gem-set diversity among already-good runs). FINGERPRINT = `u16[12]` (pinned order): `[m1,
m2, m3, m4, m5, m6, survivor_count_bp, end-dominant-role_bp, octlog(boom#), octlog(crash#), octlog(takeover#),
octlog(immig#)]`. `nn = min_g L1(fp, gem_g)` (integer L1); `novelty_bp = min(SCALE, nn*SCALE/NOV_SAT)`, NOV_SAT=3*SCALE;
empty set → SCALE. `final_score = Q * (NOV_FLOOR + (SCALE−NOV_FLOOR)*novelty_bp/SCALE)/SCALE`, NOV_FLOOR=4000 (a
redundant gem keeps 40% of Q). DEDUP: reject a candidate with `nn < DEDUP_MIN = SCALE`. (Gem-library persistence is
D2; D0 ships `fingerprint()` + `final_score()` + a unit-tested `novelty_l1`.)

**Gem validity** (reproducibility contract): a gem = `(seed:u64, env:EnvConfig, journal:Vec<Action>)`; saving runs
`record_episode → assert replay == recorded_hash` BEFORE scoring; a failed round-trip is DROPPED. Score stored with
`build_id` (the pinned-hash fingerprint, inv #7) — a re-pin invalidates stored scores (recomputed by replay).

## D1 trace schema (`PerGenTrace`, defined IN `crates/discovery`, std-only; the harness POPULATES it off-hash)
```
struct PerGenTrace { s:u16, g:u32, gens_requested:u32, species:Vec<SpeciesMeta>, rows:Vec<GenRow>,
                     inoculations:Vec<InocRec>, seed:u64, recorded_hash:u64 }
struct SpeciesMeta { id:u16, key:String, role:u8 }   // role = TrophicRole ordinal, const per run
struct GenRow { gen:u32, pop:Vec<u32>, allele_q:Vec<u16> /*q16 permille, reserved*/, flow:Vec<(u16,u16,i64)> /*sparse (dest,src,amount>0)*/ }
struct InocRec { gen:u32, species_id:u16, count:u32 }   // from actions.ndjson RegionInoculate
```
Capture loop: `env.reset(seed)`; per gen `env.step(action)` then push a `GenRow` from `observe_all()` +
`flow_matrix()`; roles from `observe_all()[i].role`; `inoculations` from the journal; early-stop on `Σpop==0`. Both
`observe_all`/`flow_matrix` are PROVEN hash-neutral (zero SimRng, never folded into `hash_world`; `per_gen_stats`
proves stepping-with-reads is hash-neutral) → capture cannot move `0x47a0_3c8f_6701_f240`.

## Trait shape (inv #5 pluggable)
```rust
pub trait InterestingnessScorer { fn score(&self, t:&PerGenTrace) -> ScoreVec; fn id(&self) -> &'static str; }
pub struct ScoreVec { pub quality:u64, pub breakdown:[u16;6], pub fingerprint:[u16;FP_DIMS] }   // PartialEq+Eq
pub struct DefaultScorer { pub params: ScoreParams }   // id = "ecology-d0"; Default = the pinned values below
pub fn final_score(s:&impl InterestingnessScorer, t:&PerGenTrace, saved:&[[u16;FP_DIMS]]) -> ScoredRun;
pub struct ScoreParams { /* every threshold/weight below, so re-tuning needs no code edit (ADR-pinned, inv #7) */ }
```
Modules: `fixed` (isqrt, `octave_log_bp` parity-tested vs `signature.rs::flow_to_grid`, `ratio_bp`, `q16`), `trace`,
`ecology` (DefaultScorer). `Cargo.toml` deps: std + serde (trace I/O) ONLY. Everything `#[must_use]` + `Eq` for
determinism unit tests.

## Pinned ScoreParams (the tunable starting point — ADR-023 records these; `ScoreParams` lets them change without code)
WSUM split `[W1=14, W2=14, W3=22, W4=18, W5=18]`; M6 = multiplicative gate. SCALE=10_000, SCORE_SCALE=1_000_000,
BURN_IN_BP=2000, PERSIST_BP=8000, RICH_CAP=6, TURN_TARGET=8, EDGE_TARGET=4, BOOM_K=3, CRASH_K=4, POP_FLOOR=5,
CRASH_FROM=20, EVENT_SAT=6×SCALE, NOV_SAT=3×SCALE, NOV_FLOOR=4000, DEDUP_MIN=SCALE, FP_DIMS=12.

## Test oracle (the behavior contract — synthetic fixtures + ≥1 real headless run to ground it)
- **A** predator–prey limit cycle → **HIGH** (≥600_000): all terms light up.
- **B** contamination recovery (a journaled immigrant establishes + reshapes the web) → **HIGH**; distinct fingerprint from A → high novelty.
- **C** trophic cascade with rebound → **HIGH** (≥450_000); a TEMPORARY collapse-with-rebound ranks above a permanent one.
- **D** instant collapse (dead by ~gen 5 of 500) → **LOW** (≈0): the M6 gate crushes Q.
- **E** flat monoculture → **LOW**: M1=M2=M4=M5=0.
- **F** converged steady-state coexistence (the forced-stability trap) → **LOW-ish and STRICTLY below A** — the single
  most important ordering test (encodes "don't tune to forced stability"; a live limit cycle MUST beat frozen coexistence).
- **G** single boom then plateau → **LOW**: M3's turn-gating + M5's saturation + M2/M4 zeros pin "a single boom is NOT maximal".

## Open questions (defaults chosen; revisit when tuning / at D2)
1. The weights/thresholds above are the pinned-but-tunable starting point (favouring M3+M5 drama = 40/86, per the
   open-system memory). Tunable via `ScoreParams`.
2. M5 uses a saturating event-magnitude SUM (not an edge-walked cascade CHAIN) — simpler/testable for D0; true
   multi-link cascade detection deferred to a D2 follow-up.
3. M4 uses `distinct_roles` (not integer DAG-longest-walk depth) — cheaper/testable proxy for "multi-level"; true
   trophic depth deferred.
4. Trace stores the SPARSE per-tick FlowMatrix (needed for M4/M5); windowed running-aggregate compaction deferred
   until G/S strain trace size.
5. `allele_q` carried but UNUSED by M1..M6 (reserved for future genetic-sweep metrics + fingerprint) — kept for
   forward-compat.
6. Crate = `crates/discovery`, std+serde only (scorer takes a plain `PerGenTrace`); the harness owns the capture seam
   (cleanest inv #1/#5 boundary).
7. Novelty/dedup gem-library persistence (saved-fingerprint store, top-K cut) is D2 — D0 stops at scorer + fingerprint
   + a unit-tested `novelty_l1`.
