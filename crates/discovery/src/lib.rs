//! `discovery` — D0 interestingness scorer + D1 trace types for the emergent-discovery harness.
//!
//! ## Boundary (inv #1/#5)
//! std + serde ONLY. The scorer takes a plain [`PerGenTrace`] (defined in [`trace`]) — it has NO dependency on
//! `sim-core` or `harness`, so the metric set is pluggable behind [`InterestingnessScorer`] and the harness
//! owns the capture seam. The scorer only READS exported numbers (inv #2 — no biology here).
//!
//! ## Determinism (inv #3)
//! Every metric is an INTEGER / quantized, RNG-free function of the trace. No `HashMap` iteration on any
//! ordered path — species are addressed by their fixed index position. Same trace bytes → byte-identical
//! [`ScoreVec`] (which is `Eq`, so determinism is a unit-test assertion). The lone fenced float is
//! [`fixed::q16`], used ONCE at capture, never on the score path.
//!
//! ## What ships at D0
//! [`DefaultScorer`] (`id = "ecology-d0"`) implementing the 6 metrics M1..M6 (see [`ecology`]), the gated
//! combine → `quality ∈ [0, SCORE_SCALE]`, the 12-dim [`fingerprint`](ScoreVec::fingerprint), a unit-tested
//! [`novelty_l1`], and [`final_score`].
//!
//! ## What ships at D2a (STAGE 1)
//! The SEARCH data model in [`search`] (no engine): [`SearchConfig`] (a deterministic description of one run),
//! a std-only splitmix64 sampler [`propose`] over a bounded [`SearchSpace`] (NO `rand` crate — inv #3), the
//! [`Gem`] type + [`caption`], and the deduped top-K [`GemLibrary`]. All integer / ordered / serde — the
//! same std+serde boundary as the rest of the crate.

#![forbid(unsafe_code)]

pub mod ecology;
pub mod fixed;
pub mod search;
pub mod surrogate;
pub mod trace;

pub use search::{
    caption, crossover, mutate, propose, propose_evolved, EvalRecord, Gem, GemLibrary,
    SearchConfig, SearchSpace, SpeciesAxis,
};
pub use surrogate::{encode, FeatureVec, ENCODER_ID, FEAT_DIMS};
pub use trace::{GenRow, InocRec, PerGenTrace, SpeciesMeta};

use fixed::SCALE;

/// Fingerprint dimensionality (PINNED): `[m1, m2, m3, m4, m5, m6, survivor_count_bp, end-dominant-role_bp,
/// octlog(boom#), octlog(crash#), octlog(takeover#), octlog(immig#)]`.
pub const FP_DIMS: usize = 12;

/// Q micro-units scale: `quality ∈ [0, SCORE_SCALE]`.
pub const SCORE_SCALE: u64 = 1_000_000;

/// The pluggable scoring interface (inv #5). An impl turns a [`PerGenTrace`] into a [`ScoreVec`]; swapping
/// impls never touches sim-core or the trace.
pub trait InterestingnessScorer {
    /// Score a trace into quality + per-metric breakdown + fingerprint.
    #[must_use]
    fn score(&self, t: &PerGenTrace) -> ScoreVec;
    /// Stable identifier of this scorer (e.g. `"ecology-d0"`), recorded alongside saved scores.
    fn id(&self) -> &'static str;
}

/// The output of a scorer: a single `quality` plus the explainable per-metric breakdown and the novelty
/// fingerprint. `Eq` so determinism is a byte-for-byte unit-test assertion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScoreVec {
    /// Gated combined quality `Q ∈ [0, SCORE_SCALE]`.
    pub quality: u64,
    /// The six metric values `[m1, m2, m3, m4, m5, m6]`, each in `[0, SCALE]` (explainability).
    pub breakdown: [u16; 6],
    /// The 12-dim novelty fingerprint (PINNED order).
    pub fingerprint: [u16; FP_DIMS],
}

/// A scored run: the [`ScoreVec`], the novelty multiplier applied at save time, and the resulting
/// `final_score` (Q after the novelty multiplier). See [`final_score`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScoredRun {
    /// The raw scorer output.
    pub score: ScoreVec,
    /// Nearest-neighbour integer L1 distance to the saved gem set (`SCALE` if the set is empty).
    pub nn: u64,
    /// Novelty multiplier in basis points (`min(SCALE, nn*SCALE/NOV_SAT)`).
    pub novelty_bp: u64,
    /// `quality` after the novelty multiplier — the value gems are ranked by at save time.
    pub final_score: u64,
}

/// Every pinned threshold / weight as a field so re-tuning needs NO code edit (ADR-pinned, inv #7). [`Default`]
/// returns the spec's pinned starting point. The scorer reads only these fields — change them, re-run, done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScoreParams {
    // --- combine weights (WSUM = sum of W1..W5; M6 is a multiplicative gate, NOT in the sum) ---
    /// M1 (coexistence) weight.
    pub w1: u64,
    /// M2 (evenness) weight.
    pub w2: u64,
    /// M3 (dynamism) weight.
    pub w3: u64,
    /// M4 (trophic structure) weight.
    pub w4: u64,
    /// M5 (events) weight.
    pub w5: u64,

    // --- stable window ---
    /// Burn-in fraction in bp: `g0 = G * burn_in_bp / SCALE` (drop the first 20%).
    pub burn_in_bp: u64,
    /// Persistence fraction in bp: a species persists iff it is alive ≥ `|W|*persist_bp/SCALE` gens of W.
    pub persist_bp: u64,

    // --- M1 ---
    /// Richness cap for the coexistence normalization.
    pub rich_cap: u64,

    // --- M3 ---
    /// Turn target: `turns` saturates the turn term at this many sign-changes.
    pub turn_target: u64,

    // --- M4 ---
    /// Edge target: off-diagonal edge count saturates the edge term here.
    pub edge_target: u64,

    // --- M5 ---
    /// Boom multiplier: `pop[g] ≥ pop[g-1]*boom_k`.
    pub boom_k: u64,
    /// Crash divisor: `pop[g] ≤ pop[g-1]/crash_k`.
    pub crash_k: u64,
    /// Population floor below which a boom base is ignored (jitter guard).
    pub pop_floor: u64,
    /// Crash-from floor below which a crash base is ignored (jitter guard).
    pub crash_from: u64,
    /// Event saturation (in raw magnitude units): `m5 = min(SCALE, event_raw*SCALE/event_sat)`.
    pub event_sat: u64,

    // --- novelty / dedup ---
    /// Novelty saturation: `novelty_bp = min(SCALE, nn*SCALE/nov_sat)`.
    pub nov_sat: u64,
    /// Novelty floor in bp: a redundant gem keeps `nov_floor/SCALE` of Q.
    pub nov_floor: u64,
    /// Dedup minimum: a candidate with `nn < dedup_min` is a duplicate (rejected at save in D2).
    pub dedup_min: u64,
}

impl Default for ScoreParams {
    fn default() -> Self {
        // The spec's pinned starting point (ADR-023). Tunable without code edits.
        ScoreParams {
            w1: 14,
            w2: 14,
            w3: 22,
            w4: 18,
            w5: 18,
            burn_in_bp: 2_000,
            persist_bp: 8_000,
            rich_cap: 6,
            turn_target: 8,
            edge_target: 4,
            boom_k: 3,
            crash_k: 4,
            pop_floor: 5,
            crash_from: 20,
            event_sat: 6 * SCALE,
            nov_sat: 3 * SCALE,
            nov_floor: 4_000,
            dedup_min: SCALE,
        }
    }
}

impl ScoreParams {
    /// `WSUM = W1+W2+W3+W4+W5` — the weighted-sum denominator (M6 is excluded; it is a multiplicative gate).
    #[must_use]
    pub fn wsum(&self) -> u64 {
        self.w1 + self.w2 + self.w3 + self.w4 + self.w5
    }
}

/// The pinned D0 scorer (`id = "ecology-d0"`). [`Default`] uses the pinned [`ScoreParams`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DefaultScorer {
    /// The tunable parameter set.
    pub params: ScoreParams,
}

impl InterestingnessScorer for DefaultScorer {
    fn score(&self, t: &PerGenTrace) -> ScoreVec {
        ecology::score(&self.params, t)
    }
    fn id(&self) -> &'static str {
        "ecology-d0"
    }
}

/// Integer L1 distance between a candidate fingerprint and the nearest saved gem fingerprint. An empty gem set
/// returns [`SCALE`] (maximal novelty — nothing to be redundant with). Pure integer; no `HashMap`, fixed order.
#[must_use]
pub fn novelty_l1(fp: &[u16; FP_DIMS], saved: &[[u16; FP_DIMS]]) -> u64 {
    if saved.is_empty() {
        return SCALE;
    }
    let mut best = u64::MAX;
    for gem in saved {
        let mut d: u64 = 0;
        for k in 0..FP_DIMS {
            d += u64::from(fp[k].abs_diff(gem[k]));
        }
        if d < best {
            best = d;
        }
    }
    best
}

/// Score a run, then apply the SAVE-time novelty multiplier vs the saved-gem fingerprint set. Novelty only
/// PROTECTS gem-set diversity among already-good runs — it never manufactures score from a boring run (the
/// multiplier floors at `nov_floor/SCALE`, and `quality` is already 0 for boring runs). Returns the full
/// [`ScoredRun`] (raw score + nn + novelty + final).
#[must_use]
pub fn final_score(
    s: &impl InterestingnessScorer,
    t: &PerGenTrace,
    saved: &[[u16; FP_DIMS]],
) -> ScoredRun {
    let params = ScoreParams::default();
    final_score_with(s, &params, t, saved)
}

/// [`final_score`] with explicit [`ScoreParams`] (so a re-tuned scorer's novelty constants match its metrics).
#[must_use]
pub fn final_score_with(
    s: &impl InterestingnessScorer,
    params: &ScoreParams,
    t: &PerGenTrace,
    saved: &[[u16; FP_DIMS]],
) -> ScoredRun {
    let score = s.score(t);
    let nn = novelty_l1(&score.fingerprint, saved);
    let novelty_bp = (nn * SCALE / params.nov_sat).min(SCALE);
    // multiplier = nov_floor + (SCALE - nov_floor) * novelty_bp / SCALE   (in bp)
    let mult_bp = params.nov_floor + (SCALE - params.nov_floor) * novelty_bp / SCALE;
    let final_score = score.quality * mult_bp / SCALE;
    ScoredRun {
        score,
        nn,
        novelty_bp,
        final_score,
    }
}
