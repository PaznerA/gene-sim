//! D3-B STAGE — the SURROGATE FEATURE ENCODER (sub-slice D3-B.1).
//!
//! A PURE INTEGER function `(cfg, space) -> FeatureVec([i32; FEAT_DIMS])` on the bp grid ([`crate::fixed::SCALE`]),
//! the shared input the D3-B `RidgeInt` surrogate (later sub-slice) trains on. The layout is PINNED
//! (changing [`FEAT_DIMS`] or the feature order invalidates stored models — guarded by [`ENCODER_ID`]).
//!
//! ## Boundary (inv #1/#5)
//! std + serde ONLY — no new dep, no `sim-core`. Reads [`SearchConfig`] / [`SearchSpace`] NUMBERS only
//! (inv #2 — no biology). A later `Surrogate` trait + `RidgeInt` impl sit on the same seam; the encoder
//! is the shared input, so it ships first, with the smallest surface.
//!
//! ## Determinism (inv #3)
//! The encoder is a PURE INTEGER function of `(cfg, space)`. NO `SimRng` / `hash_world` touched — the
//! pinned literal `0x47a0_3c8f_6701_f240` is unmoved (the encoder never builds a sim env). NO `HashMap`
//! iteration (uses `Vec`/slices/arrays only). NO float. Same `(cfg, space)` → byte-identical [`FeatureVec`].
//! `master_seed` is EXCLUDED (entropy, not steerable — two configs differing only in `master_seed` encode
//! identically; asserted by `encode_excludes_master_seed`).
//!
//! ## Steering target (sub-slice D3-B.2)
//! [`drama_target`] computes the DRAMA-weighted `D` the surrogate STEERS by: the `ScoreParams` Q-combine
//! SHAPE (`ecology::score` — `weighted = Σwᵢmᵢ/wsum; D = weighted*m6/SCALE`) re-weighted via the pinned
//! [`DramaWeights`] so 78% of the mass sits on dynamism (M3) + events (M5). It is a SEPARATE target from gem
//! CURATION (`final_score` = Q × novelty, which is UNCHANGED): the search hunts drama while the library keeps
//! the curation criterion (memory `no-hardcoded-balance-open-system`). Pure integer, RNG-free, `f64`-free;
//! `m6 == 0` crushes `D` to 0 (the same instant-death survival gate Q uses).

use crate::fixed::{ratio_bp, SCALE};
use crate::search::{SearchConfig, SearchSpace};
use serde::{Deserialize, Serialize};

/// PINNED feature dimensionality. Changing this (or the feature order) invalidates stored surrogate
/// models — bump [`ENCODER_ID`] in lockstep.
pub const FEAT_DIMS: usize = 28;

/// Stable encoder version string, recorded alongside saved surrogate models. Bumped on any layout change
/// (FEAT_DIMS, feature order, or normalization) so a stale model is detected and re-fit instead of being
/// silently mis-read.
pub const ENCODER_ID: &str = "encode-v1@28";

/// The encoded feature vector — a fixed-width `i32` array newtype. Wrapped so the type carries the
/// PINNED dimensionality ([`FEAT_DIMS`]) and is `Eq` (determinism is a unit-test assertion). `Serialize`
/// /`Deserialize` so a trained surrogate's inputs round-trip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureVec(pub [i32; FEAT_DIMS]);

impl FeatureVec {
    /// Read the feature at index `i` (panics out-of-range, mirroring `[i32; N]` indexing).
    #[inline]
    #[must_use]
    pub fn idx(self, i: usize) -> i32 {
        self.0[i]
    }
}

/// The pure integer encoder. Layout (PINNED — never reorder or stored models invalidate):
///
/// | idx | feature                                              | range (i32)            |
/// |-----|------------------------------------------------------|------------------------|
/// |  0  | bias (the linear-model intercept term, always 1)     | `{1}`                  |
/// | 1..=7 | presence bit per species axis (1 if count > 0)     | `{0,1}`                |
/// | 8..=14| normalized count per axis (`count/count_hi` in bp) | `[0, SCALE]`            |
/// | 15  | richness (number of present species)                | `[0, 7]`               |
/// | 16  | predator×prey (AND-gated bdellovibrio × prey-share) | `[0, SCALE]`            |
/// | 17  | autotroph share (`default` count / total, bp)        | `[0, SCALE]`            |
/// |18..=21| containment one-hot (level 0..=3 → 4 bits)        | `{0,1}`                |
/// |22..=25| season one-hot (season 0..=3 → 4 bits)             | `{0,1}`                |
/// | 26  | temp (`temp_q / 1000` in bp, q16 permille → bp)     | `[0, SCALE]`            |
/// | 27  | temp-extremity (distance from 500, scaled to bp)    | `[0, SCALE]`            |
///
/// `master_seed` is EXCLUDED — see `encode_excludes_master_seed`.
#[must_use]
pub fn encode(cfg: &SearchConfig, space: &SearchSpace) -> FeatureVec {
    let mut f = [0i32; FEAT_DIMS];

    // [0] bias — the linear-model intercept term.
    f[0] = 1;

    let n = space.species.len();

    // Per-axis count aligned by INDEX (the roster preserves space order). A roster shorter than the
    // space, or a key mismatch at an index, reads as 0 (absent) — the encoder is robust to a parent
    // built under a different space (mirrors the evolutionary operators' alignment rule).
    let axis_count = |i: usize| -> u32 {
        let axis = &space.species[i];
        cfg.roster
            .get(i)
            .filter(|(k, _)| k == &axis.key)
            .map(|(_, c)| *c)
            .or_else(|| {
                cfg.roster
                    .iter()
                    .find(|(k, _)| k == &axis.key)
                    .map(|(_, c)| *c)
            })
            .unwrap_or(0)
    };

    // [1..=7] presence bit per axis; [8..=14] normalized count per axis.
    let mut total_count: u64 = 0;
    let mut counts: Vec<u32> = Vec::with_capacity(n);
    for i in 0..n {
        let c = axis_count(i);
        counts.push(c);
        total_count = total_count.saturating_add(u64::from(c));
        let present = c > 0;
        // presence slots: [1..=7] (7 axes).
        if i < 7 {
            f[1 + i] = if present { 1 } else { 0 };
        }
        // normalized count: count / count_hi in bp, capped at SCALE. 0 if absent.
        if i < 7 {
            let count_hi = u64::from(space.species[i].count_hi).max(1);
            let bp = if c == 0 {
                0
            } else {
                ratio_bp(u64::from(c), count_hi).min(SCALE)
            };
            f[8 + i] = bp as i32;
        }
    }

    // [15] richness — number of present species.
    let richness = counts.iter().filter(|&&c| c > 0).count();
    f[15] = richness as i32;

    // [16] predator×prey — AND-gated bdellovibrio-present × prey-share. The predator is the axis with
    // key "bdellovibrio" (the last axis in the default space). Prey = all species EXCEPT bdellovibrio.
    // prey_share_bp = ratio_bp(total_prey_count, total_count). 0 if no predator or no prey.
    let bdel_idx = space.species.iter().position(|a| a.key == "bdellovibrio");
    let (prey_share_bp, bdel_present) = match bdel_idx {
        Some(bi) => {
            let bdel_count = counts.get(bi).copied().unwrap_or(0);
            let bdel_present = bdel_count > 0;
            let total_prey = total_count.saturating_sub(u64::from(bdel_count));
            let share = ratio_bp(total_prey, total_count).min(SCALE);
            (share, bdel_present)
        }
        None => (0, false),
    };
    f[16] = if bdel_present {
        prey_share_bp as i32
    } else {
        0
    };

    // [17] autotroph share — the first axis (key "default", space.species[0]) count / total, in bp.
    let auto_count = counts.first().copied().unwrap_or(0);
    let auto_share = ratio_bp(u64::from(auto_count), total_count).min(SCALE);
    f[17] = auto_share as i32;

    // [18..=21] containment one-hot (level 0..=3 → 4 bits). Clamp to 3 defensively (bad config ≠ panic).
    let cont = cfg.containment_level.min(3) as usize;
    for i in 0..4 {
        f[18 + i] = if i == cont { 1 } else { 0 };
    }

    // [22..=25] season one-hot (season 0..=3 → 4 bits). Clamp to 3.
    let season = cfg.season.min(3) as usize;
    for i in 0..4 {
        f[22 + i] = if i == season { 1 } else { 0 };
    }

    // [26] temp — temp_q is q16 permille [0, 1000]; temp_bp = temp_q * SCALE / 1000 (= ratio_bp(temp_q, 1000)).
    let temp_q = u64::from(cfg.temp_q).min(1000);
    let temp_bp = ratio_bp(temp_q, 1000).min(SCALE);
    f[26] = temp_bp as i32;

    // [27] temp-extremity — distance from the middle (500) scaled to [0, SCALE]: 0 at temp_q=500,
    // SCALE at temp_q=0 or 1000. i64 to avoid underflow; clamp to [0, SCALE].
    let extremity_bp =
        ((i64::from(cfg.temp_q) - 500).abs() * 2 * SCALE as i64 / 1000).clamp(0, SCALE as i64);
    f[27] = extremity_bp as i32;

    FeatureVec(f)
}

// ============================================================================
// D3-B.2 — the DRAMA-weighted steering target `D`
// ============================================================================

/// PINNED drama-weights version. Bumped on any re-pin of the [`DramaWeights`] default so a model that
/// serialized its training-target weights can detect a stale pin: the [`DramaWeights::version`] field
/// travels WITH the serialized struct (a self-invalidation anchor, like `Gem::build_id` / [`ENCODER_ID`]).
pub const DRAMA_WEIGHTS_VERSION: u32 = 1;

fn default_drama_version() -> u32 {
    DRAMA_WEIGHTS_VERSION
}

/// The DRAMA-weighted steering target's tunable weights (ADR-pinned, inv #7) — the surrogate's STEER
/// criterion, kept CLEANLY SEPARATE from the gem-curation [`ScoreParams`](crate::ScoreParams) (Q × novelty).
/// Mirrors `ScoreParams`'s combine weights and `wsum()` exactly, but re-weighted so 78% of the mass sits on
/// dynamism (M3) + events (M5) — biasing the search toward LIVING dynamics (limit-cycles/cascades) over the
/// placid coexistence Q rewards (memory `no-hardcoded-balance-open-system`). [`Default`] is the pinned
/// starting point; retune without a code edit. `Serialize`/`Deserialize` so a trained surrogate's target
/// weights round-trip byte-stable (the [`version`](Self::version) anchor self-invalidates a stale pin).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DramaWeights {
    /// M1 (coexistence) weight.
    pub w1: u64,
    /// M2 (evenness) weight.
    pub w2: u64,
    /// M3 (dynamism) weight — the dominant drama term.
    pub w3: u64,
    /// M4 (trophic structure) weight.
    pub w4: u64,
    /// M5 (events) weight — the second drama term.
    pub w5: u64,
    /// Pinned-weights version anchor (defaults to [`DRAMA_WEIGHTS_VERSION`]). An old serialization that
    /// predates this field deserializes to the current version; a re-pin bumps it so a stale model self-detects.
    #[serde(default = "default_drama_version")]
    pub version: u32,
}

impl Default for DramaWeights {
    fn default() -> Self {
        // The pinned drama weights (this slice's literal): 78% of the weight on dynamism (M3) + events (M5).
        // w3 + w5 = 72, WSUM = 92, (w3+w5)*100/WSUM = 78. Tunable without a code edit (ADR-pinned, inv #7).
        DramaWeights {
            w1: 8,
            w2: 4,
            w3: 40,
            w4: 8,
            w5: 32,
            version: DRAMA_WEIGHTS_VERSION,
        }
    }
}

impl DramaWeights {
    /// `WSUM = w1+w2+w3+w4+w5` — the weighted-average denominator (M6 is EXCLUDED; it is the multiplicative
    /// survival gate, mirroring [`ScoreParams::wsum`](crate::ScoreParams::wsum)).
    #[must_use]
    pub fn wsum(&self) -> u64 {
        self.w1 + self.w2 + self.w3 + self.w4 + self.w5
    }
}

/// Compute the DRAMA-weighted steering target `D` from a metric breakdown `[m1..m6]`. EXACTLY the Q-combine
/// SHAPE (`ecology::score`: `weighted = Σwᵢmᵢ/wsum.max(1); D = weighted*m6/SCALE`) but with [`DramaWeights`]
/// instead of the curation weights — so `D` re-ranks runs by DRAMA while Q / `final_score` curate gems
/// UNCHANGED. `m6` is the multiplicative survival gate: `m6 == 0` crushes `D` to 0 (the instant-death gate).
/// PURE INTEGER, RNG-free, no `f64` — same `(breakdown, weights)` → byte-identical `D ∈ [0, SCALE]`.
#[must_use]
pub fn drama_target(breakdown: &[u16; 6], w: &DramaWeights) -> u64 {
    let m1 = u64::from(breakdown[0]);
    let m2 = u64::from(breakdown[1]);
    let m3 = u64::from(breakdown[2]);
    let m4 = u64::from(breakdown[3]);
    let m5 = u64::from(breakdown[4]);
    let m6 = u64::from(breakdown[5]);
    // Mirror `ecology::score`'s combine shape (the weighted average, then the multiplicative m6 gate).
    let weighted = (w.w1 * m1 + w.w2 * m2 + w.w3 * m3 + w.w4 * m4 + w.w5 * m5) / w.wsum().max(1);
    weighted * m6 / SCALE
}

/// [`drama_target`] convenience that forwards a [`ScoreVec`](crate::ScoreVec)'s `breakdown` — so a caller
/// holding a scored run steers off its metrics without unpacking the array.
#[must_use]
pub fn drama_target_from(score: &crate::ScoreVec, w: &DramaWeights) -> u64 {
    drama_target(&score.breakdown, w)
}

// ============================================================================
// D3-B.3 — the `Surrogate` trait + the integer `RidgeInt` regressor
// ============================================================================
//
// The inv #5 seam: a tiny pluggable interface that turns the accumulated `(config-features → drama)`
// evaluations into a cheap `D̂` predictor, so the D2b search (D3-B.4) can pre-filter candidate configs before
// paying for the expensive headless run. Impls are SWAPPABLE without touching `search` — the steered loop holds
// a `&mut dyn Surrogate`. Two impls ship here: [`NullSurrogate`] (the passthrough base case) + [`RidgeInt`]
// (the integer ridge linear regressor, the default). `BoostStumpInt` is the deferred upgrade (same trait).
//
// ## Determinism (inv #3) — ZERO f64
// `RidgeInt` is trained by FIXED-POINT gradient descent (NOT float matrix inversion, which is cross-platform
// non-deterministic and FORBIDDEN): `θ` is `i64` on [`THETA_SHIFT`], every dot-product / gradient sum runs in
// an `i128` accumulator (exact, overflow-safe), and the dataset is SORTED ONCE so accumulation is row-order
// independent. There is NO `f32`/`f64` anywhere on the train or predict path and NO RNG (the `seed` param is
// accepted for the trait contract but unused by batch GD — reserved for a future minibatch shuffle). Same rows
// + same constants → byte-identical `θ` → byte-identical predictions. OFF-HASH: the surrogate never builds a
// `SimRng`/sim env, so the pinned literal `0x47a0_3c8f_6701_f240` is untouched.
//
// ## Pins (inv #7)
// [`THETA_SHIFT`], [`N_ITERS`], [`LR_SHIFT`], [`RIDGE_LAMBDA_SHIFT`] (the ridge λ) and [`RIDGE_MIN_SAMPLES`] are
// determinism-load-bearing constants; [`RIDGE_BUILD_ID`] travels WITH a serialized model (a self-invalidation
// anchor, like [`ENCODER_ID`] / [`DRAMA_WEIGHTS_VERSION`]) so a re-pin self-detects a stale model.

/// The pluggable surrogate interface (inv #5). An impl learns `features → predicted drama` from the eval log
/// and predicts a candidate's `D̂` so the steered search can pre-filter. Swapping impls never touches `search`.
pub trait Surrogate {
    /// Fit the model to the `(x, y)` training rows (`x[j]` = [`encode`]d config, `y[j]` = [`drama_target`]).
    /// DETERMINISTIC + row-order-independent: same rows + same constants → identical model, regardless of the
    /// order the rows arrive in. `seed` is part of the contract (future minibatch shuffle); batch impls ignore it.
    fn fit(&mut self, x: &[FeatureVec], y: &[u64], seed: u64);
    /// Predict the drama `D̂ ∈ [0, SCALE]` for a single encoded config. PURE INTEGER, no `f64`.
    #[must_use]
    fn predict(&self, x: &FeatureVec) -> u64;
    /// Stable identifier (recorded alongside saved models / logs).
    fn id(&self) -> &'static str;
    /// The minimum number of training rows before this surrogate should STEER. Below it, the steered loop
    /// COLD-STARTS to passthrough (behaves like `discover_evolved`). [`NullSurrogate`] sets this to
    /// [`usize::MAX`] so it NEVER steers — the byte-identical base case.
    fn min_samples(&self) -> usize;
}

/// The base-case surrogate (inv #5): a no-op that NEVER steers. `fit` is a no-op, `predict` returns a constant
/// `0`, and [`min_samples`](Surrogate::min_samples) is [`usize::MAX`] so the D3-B.4 steered loop always
/// COLD-STARTS to passthrough — a `NullSurrogate`-steered run is BYTE-IDENTICAL to `discover_evolved`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NullSurrogate;

impl Surrogate for NullSurrogate {
    #[inline]
    fn fit(&mut self, _x: &[FeatureVec], _y: &[u64], _seed: u64) {
        // no-op: the base case never learns.
    }
    #[inline]
    fn predict(&self, _x: &FeatureVec) -> u64 {
        0
    }
    #[inline]
    fn id(&self) -> &'static str {
        "null-v1"
    }
    #[inline]
    fn min_samples(&self) -> usize {
        usize::MAX
    }
}

/// PINNED fixed-point shift for the `θ` coefficients (inv #7): a stored `θ_i` represents the real coefficient
/// `θ_i / 2^THETA_SHIFT`. Predict is a pure integer dot product `(θ·x) >> THETA_SHIFT`.
pub const THETA_SHIFT: u32 = 16;

/// PINNED gradient-descent iteration count (inv #7) — a fixed budget, NO float early-stop (an early-stop on a
/// float loss would be cross-platform non-deterministic).
pub const N_ITERS: usize = 2000;

/// PINNED learning-rate shift (inv #7): the per-iteration data step is `Σⱼ rⱼ·xⱼᵢ / (n · 2^LR_SHIFT)`. Tuned
/// for the bp-grid feature scale (`E[x²] ≈ SCALE²/3`) — roughly a 1/6-of-Newton step, so the dominant drama
/// features (predator×prey, temp-extremity, …) converge in tens of iterations, well inside [`N_ITERS`]. The
/// heterogeneous binary features (presence/one-hot) learn slowly under a single shift; a real-eval-log
/// convergence pass / per-feature scaling is open-question #4, deferred.
pub const LR_SHIFT: u32 = 11;

/// PINNED ridge λ as a per-iteration weight-decay shift (inv #7): each non-bias `θ_i` is shrunk toward zero by
/// `θ_i / 2^RIDGE_LAMBDA_SHIFT` each iteration (decoupled L2 / ridge). This is the "ridge" in `RidgeInt`; the
/// bias term (idx 0) is left unregularized (standard).
pub const RIDGE_LAMBDA_SHIFT: u32 = 8;

/// The minimum training rows before [`RidgeInt`] should steer (≥ the feature count, so the linear system is not
/// wildly under-determined). The D3-B.4 steered loop cold-starts to passthrough below this.
pub const RIDGE_MIN_SAMPLES: usize = FEAT_DIMS;

/// PINNED model build anchor (inv #7), serialized WITH a fitted [`RidgeInt`]. Encodes the load-bearing pins
/// (dims, shift, iters) so a re-pin of any of them self-invalidates a stale serialized model — mirrors
/// [`ENCODER_ID`] / `Gem::build_id`. Bump in lockstep with any pin change above.
pub const RIDGE_BUILD_ID: &str = "ridgeint-v1@dims28-shift16-iters2000";

/// The stable [`Surrogate::id`] string for [`RidgeInt`].
pub const RIDGE_ID: &str = "ridgeint-v1";

fn default_ridge_build_id() -> String {
    RIDGE_BUILD_ID.to_string()
}

/// The integer ridge LINEAR regressor (inv #5 default impl). `θ` (length [`FEAT_DIMS`], on [`THETA_SHIFT`]) is
/// fit by fixed-point gradient descent on the ridge MSE; predict is a pure integer dot product. Zero `f64` on
/// any path (inv #3). `serde` so a fitted model round-trips; the [`build_id`](Self::build_id) anchor
/// self-invalidates a stale pin (inv #7).
///
/// ## Fit (fixed-point GD, `i128`-accumulated)
/// 1. SORT the `(x, y)` rows ONCE by `(y, features)` — a deterministic canonical order. (Integer batch sums are
///    already exactly order-independent; the sort is the belt-and-suspenders ROW-ORDER-INDEPENDENCE guarantee.)
/// 2. For [`N_ITERS`] iterations: predict every row (`ŷⱼ = (θ·xⱼ) >> THETA_SHIFT`, UNclamped for the gradient),
///    form residuals `rⱼ = ŷⱼ − yⱼ`, accumulate the per-feature gradient `Gᵢ = Σⱼ rⱼ·xⱼᵢ` in an `i128`
///    (exact, overflow-safe), then step `θᵢ −= Gᵢ / (n · 2^LR_SHIFT)` and apply the ridge decay (non-bias).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RidgeInt {
    /// The fitted coefficients (length [`FEAT_DIMS`]), each an `i64` on [`THETA_SHIFT`]. Zero before [`fit`].
    theta: Vec<i64>,
    /// The pinned-build anchor (inv #7) — an old serialization predating this field defaults to
    /// [`RIDGE_BUILD_ID`]; a re-pin bumps the constant so a stale model self-detects via [`is_current_build`].
    ///
    /// [`fit`]: RidgeInt::fit
    /// [`is_current_build`]: RidgeInt::is_current_build
    #[serde(default = "default_ridge_build_id")]
    build_id: String,
}

impl Default for RidgeInt {
    fn default() -> Self {
        Self::new()
    }
}

impl RidgeInt {
    /// A fresh, unfit model — `θ` all zero (so `predict` returns 0 until [`fit`](Surrogate::fit)).
    #[must_use]
    pub fn new() -> Self {
        Self {
            theta: vec![0i64; FEAT_DIMS],
            build_id: RIDGE_BUILD_ID.to_string(),
        }
    }

    /// The fitted coefficients (length [`FEAT_DIMS`], on [`THETA_SHIFT`]) — introspection for tests / steering.
    #[must_use]
    pub fn theta(&self) -> &[i64] {
        &self.theta
    }

    /// This model's build anchor (inv #7).
    #[must_use]
    pub fn build_id(&self) -> &str {
        &self.build_id
    }

    /// Whether this model was built under the CURRENT pinned build — a stale serialized model is detectable.
    #[must_use]
    pub fn is_current_build(&self) -> bool {
        self.build_id == RIDGE_BUILD_ID
    }
}

impl Surrogate for RidgeInt {
    fn fit(&mut self, x: &[FeatureVec], y: &[u64], _seed: u64) {
        // _seed: batch GD is deterministic and seed-independent (reserved for a future minibatch shuffle).
        let n = x.len().min(y.len());
        // Fresh fit: reset θ to zero (the search retrains each generation).
        self.theta = vec![0i64; FEAT_DIMS];
        if n == 0 {
            return;
        }

        // (1) SORT the row indices ONCE by a deterministic key `(y, features)` → a canonical accumulation order.
        // Integer batch sums are already exactly order-independent; the sort makes ROW-ORDER-INDEPENDENCE a
        // structural guarantee (robust to any future order-sensitive op).
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&a, &b| (y[a], x[a].0).cmp(&(y[b], x[b].0)));

        // The combined `n · 2^LR_SHIFT` denominator (mean-gradient + learning rate folded into one division).
        let lr_den: i128 = (n as i128) << LR_SHIFT;

        for _iter in 0..N_ITERS {
            // Per-feature gradient sum `Gᵢ = Σⱼ rⱼ·xⱼᵢ`, accumulated in i128 (exact, overflow-safe).
            let mut grad = [0i128; FEAT_DIMS];
            for &j in &order {
                let xj = &x[j].0;
                // ŷⱼ = (θ·xⱼ) >> THETA_SHIFT — UNclamped (clamping would zero the gradient outside range).
                let mut acc: i128 = 0;
                for (&t, &xi) in self.theta.iter().zip(xj.iter()) {
                    acc += i128::from(t) * i128::from(xi);
                }
                let pred = acc >> THETA_SHIFT;
                let resid = pred - i128::from(y[j]);
                for (g, &xi) in grad.iter_mut().zip(xj.iter()) {
                    *g += resid * i128::from(xi);
                }
            }
            // Apply the data step + ridge decay.
            for (i, t) in self.theta.iter_mut().enumerate() {
                let step = grad[i] / lr_den; // mean-grad / 2^LR_SHIFT, truncates toward zero
                let mut v = i128::from(*t) - step;
                if i != 0 {
                    // Ridge: decoupled L2 weight-decay toward zero (bias term left unregularized).
                    v -= v / (1i128 << RIDGE_LAMBDA_SHIFT);
                }
                *t = v as i64; // i64 has ample headroom for bp-scale coefficients
            }
        }
    }

    fn predict(&self, x: &FeatureVec) -> u64 {
        // The dot product θ·x in i128 (zip naturally stops at min(theta.len(), FEAT_DIMS) — robust to a stale
        // model with a wrong-length θ vector).
        let mut acc: i128 = 0;
        for (&t, &xi) in self.theta.iter().zip(x.0.iter()) {
            acc += i128::from(t) * i128::from(xi);
        }
        let pred = acc >> THETA_SHIFT;
        pred.clamp(0, SCALE as i128) as u64
    }

    fn id(&self) -> &'static str {
        RIDGE_ID
    }

    fn min_samples(&self) -> usize {
        RIDGE_MIN_SAMPLES
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_space() -> SearchSpace {
        SearchSpace::default()
    }

    /// A config mirroring `parent_a` from search.rs tests — a realistic multi-species roster with the
    /// predator present (so the predator×prey feature fires).
    fn real_cfg() -> SearchConfig {
        SearchConfig {
            master_seed: 0xDEAD_BEEF,
            roster: vec![
                ("default".to_string(), 600),
                ("ecoli".to_string(), 200),
                ("bacillus".to_string(), 0),
                ("pseudomonas".to_string(), 100),
                ("staph".to_string(), 0),
                ("aspergillus-niger".to_string(), 0),
                ("bdellovibrio".to_string(), 40),
            ],
            containment_level: 1,
            temp_q: 500,
            season: 1,
            edits: Vec::new(),
        }
    }

    /// A minimal config: only the autotroph (the never-empty fallback shape). Predator absent.
    fn autotroph_only_cfg() -> SearchConfig {
        SearchConfig {
            master_seed: 1,
            roster: vec![
                ("default".to_string(), 100),
                ("ecoli".to_string(), 0),
                ("bacillus".to_string(), 0),
                ("pseudomonas".to_string(), 0),
                ("staph".to_string(), 0),
                ("aspergillus-niger".to_string(), 0),
                ("bdellovibrio".to_string(), 0),
            ],
            containment_level: 0,
            temp_q: 500,
            season: 0,
            edits: Vec::new(),
        }
    }

    // ---- structural ----

    #[test]
    fn encode_returns_correct_dims() {
        let f = encode(&real_cfg(), &default_space());
        assert_eq!(f.0.len(), FEAT_DIMS);
        assert_eq!(FEAT_DIMS, 28);
    }

    #[test]
    fn encode_bias_is_one() {
        let f = encode(&real_cfg(), &default_space());
        assert_eq!(f.idx(0), 1, "bias slot must always be 1");
    }

    #[test]
    fn encoder_id_is_stable() {
        // Changing ENCODER_ID invalidates stored surrogate models — pin the exact string.
        assert_eq!(ENCODER_ID, "encode-v1@28");
        assert_eq!(FEAT_DIMS, 28);
    }

    // ---- master_seed exclusion (the entropy-not-steerable contract) ----

    #[test]
    fn encode_excludes_master_seed() {
        // Two configs identical except master_seed must encode byte-identically.
        let mut a = real_cfg();
        let mut b = real_cfg();
        a.master_seed = 1;
        b.master_seed = 2;
        assert_eq!(encode(&a, &default_space()), encode(&b, &default_space()));
        // And a wildly different seed still matches.
        let mut c = real_cfg();
        c.master_seed = 0xCAFE_BABE_DEAD_BEEF;
        assert_eq!(encode(&a, &default_space()), encode(&c, &default_space()));
    }

    // ---- presence bits ----

    #[test]
    fn encode_presence_bits_match_roster() {
        let cfg = real_cfg();
        let f = encode(&cfg, &default_space());
        // roster: default+, ecoli+, bacillus-, pseudomonas+, staph-, aspergillus-, bdellovibrio+.
        let expected = [1, 1, 0, 1, 0, 0, 1];
        for (i, &exp) in expected.iter().enumerate() {
            assert_eq!(
                f.idx(1 + i),
                exp,
                "presence bit axis {i} = {} expected {exp}",
                f.idx(1 + i)
            );
        }
    }

    #[test]
    fn encode_presence_zero_when_all_absent_but_autotroph() {
        let cfg = autotroph_only_cfg();
        let f = encode(&cfg, &default_space());
        // axis 0 present (autotroph fallback), axes 1..=6 absent.
        assert_eq!(f.idx(1), 1);
        for i in 1..7 {
            assert_eq!(f.idx(1 + i), 0, "axis {i} absent");
        }
    }

    // ---- normalized count ----

    #[test]
    fn encode_normalized_count_in_range_and_zero_when_absent() {
        let cfg = real_cfg();
        let f = encode(&cfg, &default_space());
        for i in 0..7 {
            let v = f.idx(8 + i);
            assert!(
                (0..=SCALE as i32).contains(&v),
                "norm count axis {i} = {v} out of [0, SCALE]"
            );
        }
        // absent axes (bacillus, staph, aspergillus) must read 0.
        assert_eq!(f.idx(8 + 2), 0, "bacillus absent → 0");
        assert_eq!(f.idx(8 + 4), 0, "staph absent → 0");
        assert_eq!(f.idx(8 + 5), 0, "aspergillus absent → 0");
    }

    #[test]
    fn encode_normalized_count_saturates_at_scale() {
        // A count exceeding count_hi clamps to SCALE. Build a config with default count > count_hi.
        let space = default_space();
        let mut cfg = real_cfg();
        // default count_hi = 1500; push to 9999.
        cfg.roster[0].1 = 9999;
        let f = encode(&cfg, &space);
        assert_eq!(
            f.idx(8),
            SCALE as i32,
            "count over count_hi must clamp to SCALE"
        );
    }

    // ---- richness ----

    #[test]
    fn encode_richness_counts_present_species() {
        let cfg = real_cfg();
        let f = encode(&cfg, &default_space());
        // default, ecoli, pseudomonas, bdellovibrio present → richness 4.
        assert_eq!(f.idx(15), 4, "richness = present-species count");
    }

    #[test]
    fn encode_richness_one_when_only_autotroph() {
        let f = encode(&autotroph_only_cfg(), &default_space());
        assert_eq!(f.idx(15), 1);
    }

    // ---- predator × prey (the one hand-crafted interaction feature) ----

    #[test]
    fn encode_predator_prey_is_and_gated() {
        let space = default_space();

        // (a) predator absent → feature is 0 even with prey present.
        let mut no_pred = real_cfg();
        no_pred.roster[6].1 = 0; // bdellovibrio absent
        let f0 = encode(&no_pred, &space);
        assert_eq!(f0.idx(16), 0, "predator absent → predator×prey = 0");

        // (b) predator present + prey present → feature = prey_share_bp.
        let cfg = real_cfg(); // bdellovibrio=40, prey = 600+200+100 = 900, total = 940.
        let f = encode(&cfg, &space);
        let total: u64 = cfg.roster.iter().map(|(_, c)| u64::from(*c)).sum();
        let prey: u64 = total - 40;
        let expected = ratio_bp(prey, total).min(SCALE) as i32;
        assert_eq!(
            f.idx(16),
            expected,
            "predator present + prey → prey_share_bp"
        );
        assert!(f.idx(16) > 0, "prey_share must be >0 when prey present");
    }

    #[test]
    fn encode_predator_prey_zero_when_predator_alone() {
        // Degenerate: only the predator present (no prey). prey_share = 0/total.
        let space = default_space();
        let cfg = SearchConfig {
            master_seed: 1,
            roster: vec![
                ("default".to_string(), 0),
                ("ecoli".to_string(), 0),
                ("bacillus".to_string(), 0),
                ("pseudomonas".to_string(), 0),
                ("staph".to_string(), 0),
                ("aspergillus-niger".to_string(), 0),
                ("bdellovibrio".to_string(), 50),
            ],
            containment_level: 0,
            temp_q: 500,
            season: 0,
            edits: Vec::new(),
        };
        let f = encode(&cfg, &space);
        // predator present but no prey → prey_count = 0 → prey_share = 0.
        assert_eq!(f.idx(16), 0, "predator alone (no prey) → 0");
    }

    // ---- autotroph share ----

    #[test]
    fn encode_autotroph_share() {
        let space = default_space();
        let cfg = real_cfg(); // default=600, total=940.
        let f = encode(&cfg, &space);
        let total: u64 = cfg.roster.iter().map(|(_, c)| u64::from(*c)).sum();
        let expected = ratio_bp(600, total).min(SCALE) as i32;
        assert_eq!(f.idx(17), expected);
        assert!(f.idx(17) > 0);
    }

    #[test]
    fn encode_autotroph_share_full_when_only_autotroph() {
        let f = encode(&autotroph_only_cfg(), &default_space());
        assert_eq!(f.idx(17), SCALE as i32, "only autotroph → share = SCALE");
    }

    // ---- one-hots ----

    #[test]
    fn encode_containment_one_hot() {
        let space = default_space();
        for level in 0..=3u8 {
            let mut cfg = real_cfg();
            cfg.containment_level = level;
            let f = encode(&cfg, &space);
            for i in 0..4 {
                let want = if i == level as usize { 1 } else { 0 };
                assert_eq!(
                    f.idx(18 + i),
                    want,
                    "containment level {level} bit {i} = {} expected {want}",
                    f.idx(18 + i)
                );
            }
        }
    }

    #[test]
    fn encode_containment_clamps_bad_level() {
        let space = default_space();
        let mut cfg = real_cfg();
        cfg.containment_level = 9; // out of range — must clamp to 3, not panic.
        let f = encode(&cfg, &space);
        assert_eq!(f.idx(21), 1, "level 9 clamps to 3 → bit 3 set");
        for i in 0..3 {
            assert_eq!(f.idx(18 + i), 0);
        }
    }

    #[test]
    fn encode_season_one_hot() {
        let space = default_space();
        for season in 0..=3u8 {
            let mut cfg = real_cfg();
            cfg.season = season;
            let f = encode(&cfg, &space);
            for i in 0..4 {
                let want = if i == season as usize { 1 } else { 0 };
                assert_eq!(
                    f.idx(22 + i),
                    want,
                    "season {season} bit {i} = {} expected {want}",
                    f.idx(22 + i)
                );
            }
        }
    }

    #[test]
    fn encode_season_clamps_bad_value() {
        let space = default_space();
        let mut cfg = real_cfg();
        cfg.season = 7; // clamps to 3.
        let f = encode(&cfg, &space);
        assert_eq!(f.idx(25), 1, "season 7 clamps to 3 → bit 3 set");
    }

    // ---- temp + temp-extremity ----

    #[test]
    fn encode_temp_normalized() {
        let mut cfg = real_cfg();

        cfg.temp_q = 0;
        assert_eq!(encode(&cfg, &default_space()).idx(26), 0, "temp_q=0 → 0");

        cfg.temp_q = 1000;
        assert_eq!(
            encode(&cfg, &default_space()).idx(26),
            SCALE as i32,
            "temp_q=1000 → SCALE"
        );

        cfg.temp_q = 500;
        assert_eq!(
            encode(&cfg, &default_space()).idx(26),
            SCALE as i32 / 2,
            "temp_q=500 → SCALE/2"
        );

        // in range generally.
        cfg.temp_q = 250;
        assert_eq!(
            encode(&cfg, &default_space()).idx(26),
            SCALE as i32 / 4,
            "temp_q=250 → SCALE/4"
        );
    }

    #[test]
    fn encode_temp_extremity() {
        let mut cfg = real_cfg();

        cfg.temp_q = 500;
        assert_eq!(
            encode(&cfg, &default_space()).idx(27),
            0,
            "temp_q=500 → extremity 0 (middle)"
        );

        cfg.temp_q = 0;
        assert_eq!(
            encode(&cfg, &default_space()).idx(27),
            SCALE as i32,
            "temp_q=0 → extremity SCALE"
        );

        cfg.temp_q = 1000;
        assert_eq!(
            encode(&cfg, &default_space()).idx(27),
            SCALE as i32,
            "temp_q=1000 → extremity SCALE"
        );

        // symmetric around 500.
        cfg.temp_q = 400;
        let lo = encode(&cfg, &default_space()).idx(27);
        cfg.temp_q = 600;
        let hi = encode(&cfg, &default_space()).idx(27);
        assert_eq!(lo, hi, "extremity symmetric around 500");
        assert!(lo > 0 && lo < SCALE as i32);
    }

    // ---- determinism + boundedness ----

    #[test]
    fn encode_is_deterministic() {
        let space = default_space();
        let cfg = real_cfg();
        let a = encode(&cfg, &space);
        let b = encode(&cfg, &space);
        assert_eq!(a, b, "same (cfg, space) → byte-identical FeatureVec");
    }

    #[test]
    fn encode_is_bounded() {
        let space = default_space();
        // Sweep a realistic config across many temp/season/containment combos and assert each feature
        // stays in its valid range.
        for temp_q in [0u16, 100, 250, 500, 750, 900, 1000] {
            for season in 0..=3u8 {
                for cont in 0..=3u8 {
                    let mut cfg = real_cfg();
                    cfg.temp_q = temp_q;
                    cfg.season = season;
                    cfg.containment_level = cont;
                    let f = encode(&cfg, &space);

                    assert_eq!(f.idx(0), 1, "bias");
                    for i in 0..7 {
                        let v = f.idx(1 + i);
                        assert!(v == 0 || v == 1, "presence axis {i} = {v}");
                    }
                    for i in 0..7 {
                        let v = f.idx(8 + i);
                        assert!((0..=SCALE as i32).contains(&v), "norm count axis {i} = {v}");
                    }
                    assert!((0..=7).contains(&f.idx(15)), "richness = {}", f.idx(15));
                    assert!(
                        (0..=SCALE as i32).contains(&f.idx(16)),
                        "predator×prey = {}",
                        f.idx(16)
                    );
                    assert!((0..=SCALE as i32).contains(&f.idx(17)), "autotroph share");
                    for i in 0..4 {
                        let v = f.idx(18 + i);
                        assert!(v == 0 || v == 1, "containment bit {i} = {v}");
                    }
                    for i in 0..4 {
                        let v = f.idx(22 + i);
                        assert!(v == 0 || v == 1, "season bit {i} = {v}");
                    }
                    assert!((0..=SCALE as i32).contains(&f.idx(26)), "temp");
                    assert!((0..=SCALE as i32).contains(&f.idx(27)), "temp-extremity");
                }
            }
        }
    }

    // ---- realistic config sanity ----

    #[test]
    fn encode_real_config() {
        let space = default_space();
        let cfg = real_cfg();
        let f = encode(&cfg, &space);

        // Non-zero richness (multiple species present).
        assert_eq!(
            f.idx(15),
            4,
            "richness = 4 (default, ecoli, pseudomonas, bdellovibrio)"
        );
        // Predator present + prey present → predator×prey fires (>0).
        assert!(
            f.idx(16) > 0,
            "predator×prey must fire when bdellovibrio + prey present"
        );
        // Autotroph present → autotroph share > 0.
        assert!(f.idx(17) > 0, "autotroph share must be > 0");
        // Containment one-hot sums to 1.
        let cont_sum: i32 = (0..4).map(|i| f.idx(18 + i)).sum();
        assert_eq!(cont_sum, 1, "containment one-hot sums to 1");
        // Season one-hot sums to 1.
        let season_sum: i32 = (0..4).map(|i| f.idx(22 + i)).sum();
        assert_eq!(season_sum, 1, "season one-hot sums to 1");
        // temp_q=500 → temp = SCALE/2, extremity = 0.
        assert_eq!(f.idx(26), SCALE as i32 / 2);
        assert_eq!(f.idx(27), 0);
    }

    // ---- serde round-trip (the stored-model input contract) ----

    #[test]
    fn feature_vec_serde_roundtrip() {
        let f = encode(&real_cfg(), &default_space());
        let json = serde_json::to_string(&f).expect("serialize");
        let back: FeatureVec = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(f, back, "FeatureVec must round-trip via serde");
    }

    // ========================================================================
    // D3-B.2 — the DRAMA-weighted steering target `D`
    // ========================================================================

    /// (d) The pinned default weights satisfy the 78% drama share + the WSUM contract.
    #[test]
    fn drama_weights_default_is_pinned_78pct() {
        let w = DramaWeights::default();
        assert_eq!(
            [w.w1, w.w2, w.w3, w.w4, w.w5],
            [8, 4, 40, 8, 32],
            "pinned weights"
        );
        assert_eq!(w.w3 + w.w5, 72, "M3+M5 weight");
        assert_eq!(w.wsum(), 92, "WSUM = w1+..+w5");
        assert_eq!((w.w3 + w.w5) * 100 / w.wsum(), 78, "drama share = 78%");
    }

    /// (a) `D` is STRICTLY MONOTONE increasing in M3 (all else equal), up to saturation. `m6 = SCALE` so the
    /// gate is fully open and `D == weighted`, making each +Δm3 step visibly raise `D`.
    #[test]
    fn drama_target_strictly_monotone_in_m3() {
        let w = DramaWeights::default();
        let mut prev: Option<u64> = None;
        let mut steps = 0;
        for m3 in (0..=SCALE).step_by(500) {
            let b: [u16; 6] = [3000, 3000, m3 as u16, 3000, 3000, SCALE as u16];
            let d = drama_target(&b, &w);
            if let Some(p) = prev {
                assert!(
                    d > p,
                    "D must strictly increase with M3: {p} -> {d} (m3={m3})"
                );
                steps += 1;
            }
            prev = Some(d);
        }
        assert!(steps > 0, "sweep produced no comparisons");
    }

    /// (a) `D` is STRICTLY MONOTONE increasing in M5 (all else equal), up to saturation.
    #[test]
    fn drama_target_strictly_monotone_in_m5() {
        let w = DramaWeights::default();
        let mut prev: Option<u64> = None;
        let mut steps = 0;
        for m5 in (0..=SCALE).step_by(500) {
            let b: [u16; 6] = [3000, 3000, 3000, 3000, m5 as u16, SCALE as u16];
            let d = drama_target(&b, &w);
            if let Some(p) = prev {
                assert!(
                    d > p,
                    "D must strictly increase with M5: {p} -> {d} (m5={m5})"
                );
                steps += 1;
            }
            prev = Some(d);
        }
        assert!(steps > 0, "sweep produced no comparisons");
    }

    /// (b) `m6 == 0` crushes `D` to 0 (the instant-death gate), even at maxed-out drama metrics.
    #[test]
    fn drama_target_m6_zero_crushes_to_zero() {
        let w = DramaWeights::default();
        let max = SCALE as u16;
        let dead: [u16; 6] = [max, max, max, max, max, 0];
        assert_eq!(drama_target(&dead, &w), 0, "m6==0 must crush D to 0");
        // Sanity: the SAME drama metrics with the gate open are strongly positive.
        let alive: [u16; 6] = [max, max, max, max, max, max];
        assert!(drama_target(&alive, &w) > 0, "open gate → D > 0");
    }

    /// (c) Deterministic + pure integer: same input → byte-identical output (no RNG, no `f64`).
    #[test]
    fn drama_target_is_deterministic() {
        let w = DramaWeights::default();
        let b: [u16; 6] = [1234, 5678, 9012, 3456, 7890, 9999];
        assert_eq!(drama_target(&b, &w), drama_target(&b, &w));
    }

    /// `D` reproduces the `ecology::score` Q-combine SHAPE exactly (the longhand formula) — only the weights differ.
    #[test]
    fn drama_target_matches_combine_shape() {
        let w = DramaWeights::default();
        let b: [u16; 6] = [1000, 2000, 3000, 4000, 5000, 8000];
        let m = |i: usize| u64::from(b[i]);
        let weighted =
            (w.w1 * m(0) + w.w2 * m(1) + w.w3 * m(2) + w.w4 * m(3) + w.w5 * m(4)) / w.wsum().max(1);
        let expected = weighted * m(5) / SCALE;
        assert_eq!(drama_target(&b, &w), expected);
    }

    /// `drama_target_from` forwards a `ScoreVec`'s breakdown.
    #[test]
    fn drama_target_from_forwards_breakdown() {
        let w = DramaWeights::default();
        let sv = crate::ScoreVec {
            quality: 0,
            breakdown: [1000, 2000, 3000, 4000, 5000, 9000],
            fingerprint: [0; crate::FP_DIMS],
        };
        assert_eq!(drama_target_from(&sv, &w), drama_target(&sv.breakdown, &w));
    }

    /// (e) `DramaWeights` serde round-trips byte-stable.
    #[test]
    fn drama_weights_serde_roundtrip_byte_stable() {
        let w = DramaWeights::default();
        let json = serde_json::to_string(&w).expect("serialize");
        let back: DramaWeights = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(w, back, "DramaWeights must round-trip via serde");
        let json2 = serde_json::to_string(&back).expect("re-serialize");
        assert_eq!(
            json, json2,
            "serde form must be byte-stable across a round-trip"
        );
    }

    /// (f) The whole point: on a drama-heavy vs a placid-but-even breakdown, `D` ranks the DYNAMIC run ABOVE
    /// the placid one, while the curation Q-combine ranks them the OTHER way. Proves steer ≠ curate.
    #[test]
    fn drama_target_ranks_dynamic_above_placid_where_q_does_not() {
        use crate::ScoreParams;
        // The Q-combine bp value (the curation criterion's core, pre-SCORE_SCALE/novelty) — same shape as
        // `ecology::score`, used here purely to RANK two breakdowns under the curation weights.
        fn q_combine(b: &[u16; 6], p: &ScoreParams) -> u64 {
            let m = |i: usize| u64::from(b[i]);
            let weighted = (p.w1 * m(0) + p.w2 * m(1) + p.w3 * m(2) + p.w4 * m(3) + p.w5 * m(4))
                / p.wsum().max(1);
            weighted * m(5) / SCALE
        }
        let qp = ScoreParams::default();
        let dw = DramaWeights::default();

        // placid: perfect coexistence + evenness, NO dynamism/events; survives.
        let placid: [u16; 6] = [10_000, 10_000, 0, 0, 0, 10_000];
        // dynamic: NO coexistence/evenness, strong dynamism + events; survives.
        let dynamic: [u16; 6] = [0, 0, 6_000, 0, 6_000, 10_000];

        // Q (curation) ranks the PLACID run above the dynamic one...
        assert!(
            q_combine(&placid, &qp) > q_combine(&dynamic, &qp),
            "Q must rank placid above dynamic (placid={}, dynamic={})",
            q_combine(&placid, &qp),
            q_combine(&dynamic, &qp)
        );
        // ...but D (steering) ranks the DYNAMIC run strictly above the placid one — the whole point.
        assert!(
            drama_target(&dynamic, &dw) > drama_target(&placid, &dw),
            "D must rank dynamic above placid (dynamic={}, placid={})",
            drama_target(&dynamic, &dw),
            drama_target(&placid, &dw)
        );
    }

    // ========================================================================
    // D3-B.3 — the `Surrogate` trait + the integer `RidgeInt` regressor
    // ========================================================================

    /// A planted-signal dataset: `y = (A·feat[16] + B·feat[27]) / SCALE + bounded noise`, with all other
    /// features zero (bias = 1). feat[16] = predator×prey, feat[27] = temp-extremity — the two drama features
    /// the surrogate must learn. Deterministic, pure integer (a splitmix64 supplies bounded noise — NO RNG crate).
    const PLANT_A: i64 = 6000; // coefficient on feat[16]
    const PLANT_B: i64 = 3000; // coefficient on feat[27]

    fn planted_dataset() -> (Vec<FeatureVec>, Vec<u64>) {
        let m = 96usize;
        let mut xs = Vec::with_capacity(m);
        let mut ys = Vec::with_capacity(m);
        let mut noise: u64 = 0x1234_5678_9abc_def0;
        for r in 0..m {
            // Spread feat16/feat27 across [0, 8000] via coprime steps (deterministic, well-conditioned).
            let x16 = ((r as i64 * 101 + 37) % 8001) as i32;
            let x27 = ((r as i64 * 263 + 11) % 8001) as i32;
            let mut f = [0i32; FEAT_DIMS];
            f[0] = 1; // bias
            f[16] = x16;
            f[27] = x27;
            // bounded deterministic noise in [-150, 150].
            noise = noise
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let nz = ((noise >> 33) % 301) as i64 - 150;
            let clean = (PLANT_A * x16 as i64 + PLANT_B * x27 as i64) / SCALE as i64;
            let y = (clean + nz).clamp(0, SCALE as i64) as u64;
            xs.push(FeatureVec(f));
            ys.push(y);
        }
        (xs, ys)
    }

    /// The noiseless planted target for a `(feat16, feat27)` pair — the relationship `predict` must track.
    fn planted_clean(p: i32, q: i32) -> u64 {
        ((PLANT_A * p as i64 + PLANT_B * q as i64) / SCALE as i64).clamp(0, SCALE as i64) as u64
    }

    // ---- pinned constants (inv #7) ----

    #[test]
    fn surrogate_constants_are_pinned() {
        assert_eq!(THETA_SHIFT, 16);
        assert_eq!(N_ITERS, 2000);
        assert_eq!(LR_SHIFT, 11);
        assert_eq!(RIDGE_LAMBDA_SHIFT, 8);
        assert_eq!(RIDGE_MIN_SAMPLES, FEAT_DIMS);
        assert_eq!(RIDGE_BUILD_ID, "ridgeint-v1@dims28-shift16-iters2000");
        assert_eq!(RIDGE_ID, "ridgeint-v1");
    }

    // ---- (f) NullSurrogate base case ----

    #[test]
    fn null_surrogate_is_passthrough_base_case() {
        let (xs, ys) = planted_dataset();
        let mut s = NullSurrogate;
        // predict is a constant 0 before AND after fit (fit is a no-op).
        assert_eq!(s.predict(&xs[0]), 0);
        s.fit(&xs, &ys, 123);
        assert_eq!(s.predict(&xs[0]), 0);
        assert_eq!(s.predict(&xs[5]), 0);
        // min_samples = usize::MAX → the steered loop NEVER steers (cold-start passthrough).
        assert_eq!(s.min_samples(), usize::MAX);
        assert_eq!(s.id(), "null-v1");
    }

    // ---- (a) deterministic + byte-identical ----

    #[test]
    fn ridge_fit_predict_is_deterministic() {
        let (xs, ys) = planted_dataset();
        let mut a = RidgeInt::new();
        let mut b = RidgeInt::new();
        a.fit(&xs, &ys, 42);
        b.fit(&xs, &ys, 42);
        assert_eq!(a, b, "same rows + seed → byte-identical model");
        assert_eq!(a.theta(), b.theta());
        for x in &xs {
            assert_eq!(a.predict(x), b.predict(x), "predictions must be identical");
        }
    }

    /// Batch GD is seed-independent (the `seed` param is reserved for a future minibatch shuffle).
    #[test]
    fn ridge_fit_is_seed_independent() {
        let (xs, ys) = planted_dataset();
        let mut a = RidgeInt::new();
        let mut b = RidgeInt::new();
        a.fit(&xs, &ys, 1);
        b.fit(&xs, &ys, 0xFFFF_FFFF_FFFF_FFFF);
        assert_eq!(a.theta(), b.theta(), "batch GD ignores the seed");
    }

    // ---- (b) row-order-independent (the sort-once guarantee) ----

    #[test]
    fn ridge_fit_is_row_order_independent() {
        let (xs, ys) = planted_dataset();
        let mut base = RidgeInt::new();
        base.fit(&xs, &ys, 7);

        // (i) reversed order — pairs kept together.
        let mut xr: Vec<_> = xs.clone();
        let mut yr: Vec<_> = ys.clone();
        xr.reverse();
        yr.reverse();
        let mut rev = RidgeInt::new();
        rev.fit(&xr, &yr, 7);
        assert_eq!(base.theta(), rev.theta(), "reversed rows → identical θ");

        // (ii) a non-trivial deterministic permutation (stride interleave).
        let nrows = xs.len();
        let mut perm: Vec<usize> = Vec::with_capacity(nrows);
        let mut k = 0usize;
        for _ in 0..nrows {
            perm.push(k);
            k = (k + 37) % nrows; // 37 coprime with 96 → a full cycle (a true permutation)
        }
        let xp: Vec<_> = perm.iter().map(|&i| xs[i]).collect();
        let yp: Vec<_> = perm.iter().map(|&i| ys[i]).collect();
        let mut permuted = RidgeInt::new();
        permuted.fit(&xp, &yp, 7);
        assert_eq!(
            base.theta(),
            permuted.theta(),
            "permuted rows → identical θ"
        );
    }

    // ---- (c) recovers a planted linear signal ----

    #[test]
    fn ridge_recovers_planted_signal() {
        let (xs, ys) = planted_dataset();
        let mut m = RidgeInt::new();
        m.fit(&xs, &ys, 0xABCD);
        let theta = m.theta();

        // Ideal coefficients on THETA_SHIFT: A/SCALE and B/SCALE scaled by 2^16.
        let ideal16 = (PLANT_A << THETA_SHIFT) / SCALE as i64; // ≈ 39321
        let ideal27 = (PLANT_B << THETA_SHIFT) / SCALE as i64; // ≈ 19660

        // The two planted features are recovered within 20% (a mild ridge shrink + bounded noise).
        assert!(
            (theta[16] - ideal16).abs() < ideal16 / 5,
            "θ[16]={} not within 20% of ideal {ideal16}",
            theta[16]
        );
        assert!(
            (theta[27] - ideal27).abs() < ideal27 / 5,
            "θ[27]={} not within 20% of ideal {ideal27}",
            theta[27]
        );

        // The two planted features DOMINATE θ — every other coefficient is smaller in magnitude.
        for i in 0..FEAT_DIMS {
            if i == 16 || i == 27 {
                continue;
            }
            assert!(
                theta[i].abs() < theta[27].abs(),
                "θ[{i}]={} must be dominated by the planted features (θ[27]={})",
                theta[i],
                theta[27]
            );
        }

        // predict TRACKS the noiseless planted relationship within tolerance on held-out points.
        for &(p, q) in &[
            (2000i32, 1000i32),
            (5000, 4000),
            (8000, 0),
            (0, 8000),
            (4000, 4000),
            (7000, 7000),
        ] {
            let mut f = [0i32; FEAT_DIMS];
            f[0] = 1;
            f[16] = p;
            f[27] = q;
            let pred = m.predict(&FeatureVec(f));
            let clean = planted_clean(p, q);
            let err = pred.abs_diff(clean);
            assert!(
                err < 500,
                "predict({p},{q})={pred} vs planted {clean} — err {err} > 500"
            );
        }
    }

    /// Edge cases: an empty dataset leaves θ zero (predict 0); a single row does not panic.
    #[test]
    fn ridge_handles_degenerate_datasets() {
        let mut empty = RidgeInt::new();
        empty.fit(&[], &[], 0);
        assert!(empty.theta().iter().all(|&t| t == 0));
        assert_eq!(empty.predict(&planted_dataset().0[0]), 0);

        let (xs, ys) = planted_dataset();
        let mut single = RidgeInt::new();
        single.fit(&xs[..1], &ys[..1], 0);
        // does not panic; predict stays clamped in range.
        let p = single.predict(&xs[0]);
        assert!(p <= SCALE, "prediction must stay clamped to [0, SCALE]");
    }

    /// predict is clamped to `[0, SCALE]` even on out-of-distribution / extreme inputs.
    #[test]
    fn ridge_predict_is_clamped() {
        let (xs, ys) = planted_dataset();
        let mut m = RidgeInt::new();
        m.fit(&xs, &ys, 0);
        // Saturate both planted features → the planted relation would exceed SCALE; predict clamps.
        let mut f = [0i32; FEAT_DIMS];
        f[0] = 1;
        f[16] = SCALE as i32;
        f[27] = SCALE as i32;
        let pred = m.predict(&FeatureVec(f));
        assert!(pred <= SCALE, "predict={pred} must clamp to SCALE");
    }

    // ---- (e) serde round-trip + build_id self-invalidation ----

    #[test]
    fn ridge_serde_roundtrip_byte_stable() {
        let (xs, ys) = planted_dataset();
        let mut m = RidgeInt::new();
        m.fit(&xs, &ys, 0);

        let json = serde_json::to_string(&m).expect("serialize");
        let back: RidgeInt = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(m, back, "RidgeInt must round-trip via serde");
        let json2 = serde_json::to_string(&back).expect("re-serialize");
        assert_eq!(json, json2, "serde form must be byte-stable");

        // A fitted model carries the CURRENT build anchor.
        assert!(back.is_current_build());
        assert_eq!(back.build_id(), RIDGE_BUILD_ID);
    }

    #[test]
    fn ridge_build_id_mismatch_is_detectable() {
        let mut m = RidgeInt::new();
        m.fit(&planted_dataset().0, &planted_dataset().1, 0);
        let json = serde_json::to_string(&m).expect("serialize");

        // A model serialized under a DIFFERENT (stale) build is detectable on load.
        let stale_json = json.replace(RIDGE_BUILD_ID, "ridgeint-v0@STALE");
        let stale: RidgeInt = serde_json::from_str(&stale_json).expect("deserialize stale");
        assert!(
            !stale.is_current_build(),
            "a stale build_id must be detectable"
        );
        assert_ne!(stale.build_id(), RIDGE_BUILD_ID);

        // A model serialized WITHOUT the field (predates the anchor) defaults to the current build.
        let theta_json = serde_json::to_string(m.theta()).expect("serialize theta");
        let no_anchor = format!("{{\"theta\":{theta_json}}}");
        let defaulted: RidgeInt = serde_json::from_str(&no_anchor).expect("deserialize no-anchor");
        assert!(
            defaulted.is_current_build(),
            "missing anchor → current build"
        );
        assert_eq!(defaulted.theta(), m.theta());
    }

    /// The trait is object-safe — the D3-B.4 steered loop holds a `&mut dyn Surrogate`.
    #[test]
    fn surrogate_trait_is_object_safe() {
        let (xs, ys) = planted_dataset();
        let mut models: Vec<Box<dyn Surrogate>> =
            vec![Box::new(NullSurrogate), Box::new(RidgeInt::new())];
        for m in &mut models {
            m.fit(&xs, &ys, 0);
            let _ = m.predict(&xs[0]);
            let _ = m.id();
            let _ = m.min_samples();
        }
    }
}
