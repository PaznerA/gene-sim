# Pinned spec — D3 surrogate model (the "brute-force gradient")

> Output of the `surrogate-model-design` 3-lens panel → judge. The buildable spec for D3 of the emergent-discovery
> epic. Awaiting human sign-off on the two flagged decisions (steering target + model choice) before implementation.
> Companion: ADR-023/024/025 (the D0/D1 scorer + trace, in DECISIONS.md).
>
> **Invariants:** #1 std+serde, GPL-clean (heavy ML stays at the process boundary — never linked). #2 reads config +
> score NUMBERS only, no biology. #3 integer/deterministic/off-hash (the pinned literal `0x47a0_3c8f_6701_f240`
> cannot move — the surrogate never builds a `SimRng`/`GeneSimEnv`). #4 headless. #5 swappable behind a `Surrogate`
> trait. #6 config/operator level.

## The idea
Learn `config-features → predicted DRAMA` from the accumulated `(config, score)` evaluations, and use it to **pre-filter
candidate proposals** before paying for the expensive real headless run — biasing the D2b evolutionary search toward
**dramatic** runs (limit-cycles/cascades = high M3 dynamism + M5 events), not just the stable coexistence Q rewards.

## D3-A — the eval log (PREREQUISITE slice, ships first, hash-neutral) — ✅ DONE
The discover loop currently saves only the top-K gems; the surrogate needs ALL evaluations. Add `EvalRecord {config,
quality, breakdown:[u16;6], fingerprint, recorded_hash}` (std+serde, in `discovery::search`) and a `--save-evals`
option that writes every evaluated `(config → ScoreVec)` to `data/runs/evals/<search_seed>.jsonl` (off-hash;
`data/runs/*` already gitignored). A test asserts the log is byte-reproducible per seed.

## D3-B — the surrogate + steering

**Feature encoding** — `encode(cfg, space) -> FeatureVec([i32; 28])` (pure, integer, in `discovery::surrogate`),
on the bp grid (SCALE=10_000). Layout PINNED (changing FEAT_DIMS invalidates stored models; guarded by an
`encoder_id`): `[0]` bias · `[1..=7]` presence bit per species axis · `[8..=14]` normalized count per axis ·
`[15]` richness · `[16]` **predator×prey** (AND-gated bdellovibrio-present × prey-share — the one interaction a
linear model can't otherwise represent) · `[17]` autotroph share · `[18..=21]` containment one-hot · `[22..=25]`
season one-hot · `[26]` temp · `[27]` **temp-extremity** (edge climates drive drama). `master_seed` is EXCLUDED
(entropy, not steerable — two configs differing only in seed must predict identically).

**Model** — `RidgeInt`: an integer ridge LINEAR regressor. Rationale: the regime is tens-to-hundreds of evals over
28 features → variance-bound, so the lowest-variance model (linear) wins; the hand-crafted interaction features
supply the only nonlinearity drama needs. Trained by **fixed-point GRADIENT DESCENT** (NOT float matrix inversion,
which is cross-platform non-deterministic): `θ` as i64 on `THETA_SHIFT=16`, a PINNED `N_ITERS=2000` (no float
early-stop), ridge MSE, dataset sorted once → row-order-independent. **Zero f64** on train or predict (pure integer
dot-product). Serde + `build_id` anchor (a re-pin self-invalidates stale models). Upgrade path (deferred, same
trait): `BoostStumpInt` (tiny integer GBT) when the log exceeds ~300 rows; heavy ML (XGBoost/LightGBM) stays at a
**subprocess boundary** crate (`crates/oracle-surrogate`, the oracle-slim/oracle-fba pattern) — never linked.

**Target** — predict a **DRAMA-weighted** `D`, not raw Q: `D = (Σ wᵢMᵢ for i∈1..5)/Σw × M6/SCALE`, with
`DramaWeights = [m1=8, m2=4, m3=40, m4=32→m5=32, m4=8]` → **78% of the weight on dynamism (M3) + events (M5)** (vs
46% in Q), M6 the unchanged instant-death gate. **Clean separation:** the surrogate STEERS by predicting `D`, but
gems are still CURATED by `final_score` (Q × novelty) — so the search hunts drama while the library keeps the
curation criterion. Weights live in a serialized `DramaWeights` struct (ADR-pinned, retune-without-code, like
`ScoreParams`). This encodes memory `no-hardcoded-balance-open-system` (steer toward living dynamics, not forced
stability).

**Search integration** — a NEW sibling `discover_evolved_steered` (keep `discover_evolved` intact as the
`NullSurrogate` base case). Per generation (once the log ≥ `min_samples`, else COLD-START PASSTHROUGH = behave like
`discover_evolved`): **OVERSAMPLE** `pop*4` candidate configs (cheap, config-structs only) → **PREDICT** each →
**SELECT** `pop` to actually run: a reserved `EXPLORE_RUN_BP=2500` (25%) quota of fresh explorers the surrogate
CANNOT veto (the hard novelty floor), then exploit-fill the rest by `(D̂ desc, seed asc, step asc)`. RUN only the
survivors through the UNCHANGED `capture_and_consider` + round-trip gem write. Retrain each generation on the
growing log. CLI `--steer` (+ `--oversample`).

**Surrogate trait** (inv #5): `fn fit(&mut self, x:&[FeatureVec], y:&[u64], seed:u64)`, `fn predict(&self,
x:&FeatureVec)->u64`, `fn id(&self)->&'static str`, `fn min_samples(&self)->usize`. Impls: `NullSurrogate` (base
case = `discover_evolved`), `RidgeInt` (default), `BoostStumpInt` (deferred).

**Test oracle:** encode deterministic + bounded + FEAT_DIMS==28; drama-target strictly monotone in M3 & M5, M6→0
crushes it; RidgeInt fit/predict byte-identical + row-order-independent; RidgeInt recovers a planted
`D = a·predator×prey + b·temp-extremity + noise`; `NullSurrogate` steered run is BYTE-IDENTICAL to
`discover_evolved` (the base-case guard); eval-log reproducible; **hash-neutrality with the full path armed**
(steered + save-evals + retrain → the pinned config still hashes `0x47a0…`); `cargo tree -p discovery` = std+serde
only.

## Open questions (the design's defaults are sensible + philosophy-aligned; flagged for sign-off)
1. **Steering target:** drama-weighted `D` (M3+M5 ≈ 78%) vs raw Q. Judge picked **drama** + steer/curate separation.
2. **Model:** `RidgeInt` linear-first (GBT as a data-grows upgrade) vs an integer GBT now. Judge picked **RidgeInt**.
3. **Boundary ML:** ship pure-Rust `RidgeInt` (no decision needed); defer the `crates/oracle-surrogate` subprocess
   scaffold? Judge: **defer**.
4. **GD hyperparameters** (`N_ITERS=2000`, `THETA_SHIFT=16`, λ, lr) — determinism-load-bearing; need one empirical
   convergence-tuning pass on a real eval log.
5. **FEAT_DIMS=28** — dropped 3 weak/collinear features; keep cut (good for linear) or bump now for the eventual GBT.
