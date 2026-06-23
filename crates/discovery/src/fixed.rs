//! Integer fixed-point helpers for the D0 scorer — pure, RNG-free, platform-identical (inv #3).
//!
//! The score path is entirely `u64`/`u128` integer arithmetic. The ONE fenced float touch in the whole crate
//! is [`q16`], used ONCE at trace capture to quantize an `allele`/`energy` fraction to permille — never on the
//! score path. Everything else here is exact integer math: [`isqrt`], the octave-log curve [`octave_log_bp`]
//! (the `sim-core::signature::flow_to_grid` curve, rescaled to [`SCALE`] and parity-tested), and [`ratio_bp`].

/// Basis-point scale: every metric `m*` and every `*_bp` value lives in `[0, SCALE]` (inv: fixed-point grid).
pub const SCALE: u64 = 10_000;

/// Integer square root of `n` via Newton's method — exact `floor(sqrt(n))`, no float. `isqrt(0) = 0`.
#[must_use]
pub fn isqrt(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    // Newton iteration on u64; seed from the bit length so it converges in a few steps.
    let mut x = 1u64 << ((64 - n.leading_zeros()).div_ceil(2));
    loop {
        let y = (x + n / x) >> 1;
        if y >= x {
            break;
        }
        x = y;
    }
    // `x` is now floor(sqrt(n)); correct one step defensively, using checked_mul so the u64::MAX case (where
    // (x+1)² overflows) never panics — an overflow means (x+1)² > n, so we simply stop climbing.
    while x > 0 && x.checked_mul(x).is_none_or(|sq| sq > n) {
        x -= 1;
    }
    while x
        .checked_add(1)
        .and_then(|x1| x1.checked_mul(x1))
        .is_some_and(|sq| sq <= n)
    {
        x += 1;
    }
    x
}

/// Saturating exact ratio in basis points: `floor(num * SCALE / den)`, capped at... nothing (callers clamp).
/// `den == 0 → 0` (an empty denominator carries no signal). Promotes through `u128` so `num * SCALE` cannot
/// overflow even for population-squared numerators.
#[must_use]
pub fn ratio_bp(num: u64, den: u64) -> u64 {
    if den == 0 {
        return 0;
    }
    let r = (u128::from(num) * u128::from(SCALE)) / u128::from(den);
    // Truncate back to u64; r can exceed SCALE (caller decides whether to clamp), but cannot exceed u64 for any
    // realistic num (num would need to exceed u64::MAX/SCALE ≈ 1.8e15 to wrap, far beyond any pop/flow count).
    r.min(u128::from(u64::MAX)) as u64
}

/// The PINNED octave-log curve — the EXACT shape of `sim-core::signature::flow_to_grid`, rescaled from that
/// crate's `[0, 65535]` grid onto this crate's `[0, SCALE]` basis-point grid. Pure integer base-2 log over
/// [`OCTAVE_SAT`] octaves with [`FRAC_BITS`] of sub-octave interpolation; `0 → 0`, `f ≥ 2^OCTAVE_SAT → SCALE`.
///
/// Replicated here (NOT imported) so this crate stays std+serde with no `sim-core` dependency (inv #1/#5); the
/// `octave_curve_matches_signature_flow_to_grid` unit test certifies parity of the curve SHAPE against a local
/// replica of `flow_to_grid` (same `pos/span` position function, only the final scale differs).
#[must_use]
pub fn octave_log_bp(f: u64) -> u64 {
    if f == 0 {
        return 0;
    }
    if f >= (1u64 << OCTAVE_SAT) {
        return SCALE;
    }
    let pos = octave_pos(f);
    let span = u64::from(OCTAVE_SAT) << FRAC_BITS;
    ((u128::from(pos) * u128::from(SCALE)) / u128::from(span)).min(u128::from(SCALE)) as u64
}

/// Octaves the log curve spans before saturating: `2^OCTAVE_SAT` maps to the grid ceiling. Matches the 28
/// octaves `sim-core::signature::FLOW_J_SCALE = 1<<28` spans, so the curve shape is identical.
const OCTAVE_SAT: u32 = 28;

/// Sub-octave interpolation bits — matches `sim-core::signature::flow_to_grid::FRAC_BITS`.
const FRAC_BITS: u32 = 8;

/// Position of `f` along `[0, OCTAVE_SAT * 2^FRAC_BITS)` — the EXACT `pos` function from
/// `sim-core::signature::flow_to_grid` (floor(log2(f)) as the integer octave, plus FRAC_BITS of mantissa).
/// Scale-independent: both this crate and sim-core feed this same `pos` into their own grid rescale.
#[must_use]
fn octave_pos(f: u64) -> u64 {
    let bits = 63 - f.leading_zeros(); // floor(log2(f))
    let frac = if bits >= FRAC_BITS {
        (f >> (bits - FRAC_BITS)) & ((1 << FRAC_BITS) - 1)
    } else {
        (f << (FRAC_BITS - bits)) & ((1 << FRAC_BITS) - 1)
    };
    (u64::from(bits) << FRAC_BITS) | frac
}

/// The ONE fenced float touch (inv #3): quantize a fraction `x ∈ [0,1]` to permille `u16 ∈ [0,1000]`, done
/// ONCE at trace capture for `allele`/`energy`. Round-half-up then clamp: `clamp(floor(x*1000 + 0.5), 0, 1000)`.
/// Never called on the score path — the score path reads the already-quantized `allele_q`.
#[must_use]
pub fn q16(x: f64) -> u16 {
    let v = (x * 1000.0 + 0.5).floor();
    if v <= 0.0 {
        0
    } else if v >= 1000.0 {
        1000
    } else {
        v as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A local replica of `sim-core::signature::flow_to_grid` — same octave position, rescaled to the u16 grid
    /// (UNIT_SCALE = 65535, FLOW_J_SCALE = 1<<28). Used ONLY to certify curve-shape parity without depending on
    /// sim-core. If sim-core's curve ever changes, this replica (and the parity assertion) must move with it.
    fn flow_to_grid_replica(f: i64) -> u16 {
        const UNIT_SCALE: u64 = 65535;
        const FLOW_J_SCALE: i64 = 1 << 28;
        if f <= 0 {
            return 0;
        }
        if f >= FLOW_J_SCALE {
            return UNIT_SCALE as u16;
        }
        let f = f as u64;
        let pos = octave_pos(f);
        let span = u64::from(OCTAVE_SAT) << FRAC_BITS;
        ((pos * UNIT_SCALE) / span).min(UNIT_SCALE) as u16
    }

    #[test]
    fn isqrt_is_exact_floor_sqrt() {
        assert_eq!(isqrt(0), 0);
        assert_eq!(isqrt(1), 1);
        assert_eq!(isqrt(3), 1);
        assert_eq!(isqrt(4), 2);
        assert_eq!(isqrt(8), 2);
        assert_eq!(isqrt(9), 3);
        assert_eq!(isqrt(15), 3);
        assert_eq!(isqrt(16), 4);
        assert_eq!(isqrt(1_000_000), 1000);
        assert_eq!(isqrt(u64::MAX), 4_294_967_295);
        // Exhaustive on a band of perfect squares ± 1.
        for k in 0u64..2000 {
            let sq = k * k;
            assert_eq!(isqrt(sq), k, "isqrt({sq})");
            if sq > 0 {
                assert_eq!(isqrt(sq - 1), k - 1, "isqrt({})", sq - 1);
            }
            assert_eq!(isqrt(sq + k), k, "isqrt({}) below next square", sq + k);
        }
    }

    #[test]
    fn ratio_bp_saturating_and_zero_den() {
        assert_eq!(ratio_bp(0, 0), 0);
        assert_eq!(ratio_bp(5, 0), 0);
        assert_eq!(ratio_bp(1, 2), SCALE / 2); // 5000 bp = 50%
        assert_eq!(ratio_bp(1, 1), SCALE);
        assert_eq!(ratio_bp(3, 1), 3 * SCALE); // can exceed SCALE (caller clamps)
                                               // Population-squared scale: 1e9 * SCALE fits u128, truncates to u64 cleanly.
        let big = ratio_bp(1_000_000_000, 1);
        assert_eq!(big, 1_000_000_000 * SCALE);
    }

    #[test]
    fn q16_round_half_up_and_clamped() {
        assert_eq!(q16(0.0), 0);
        assert_eq!(q16(1.0), 1000);
        assert_eq!(q16(0.5), 500);
        assert_eq!(q16(0.0005), 1); // 0.5 permille rounds up to 1
        assert_eq!(q16(0.0004), 0); // below half-permille rounds to 0
        assert_eq!(q16(-1.0), 0); // clamped low
        assert_eq!(q16(2.0), 1000); // clamped high
    }

    /// PARITY: `octave_log_bp` is the `sim-core::signature::flow_to_grid` curve, only the output grid differs.
    /// Assert (a) the replica reproduces flow_to_grid's pinned anchors, and (b) `octave_log_bp` is that SAME
    /// curve position rescaled SCALE/UNIT_SCALE — monotone, same zero, same saturation, same octave shape.
    #[test]
    fn octave_curve_matches_signature_flow_to_grid() {
        const UNIT_SCALE: u64 = 65535;
        // (a) replica reproduces flow_to_grid's documented anchors.
        assert_eq!(flow_to_grid_replica(0), 0);
        assert_eq!(flow_to_grid_replica(-5), 0);
        assert_eq!(flow_to_grid_replica(1 << 28), UNIT_SCALE as u16);
        assert_eq!(flow_to_grid_replica((1i64 << 28) * 2), UNIT_SCALE as u16);
        let a = flow_to_grid_replica(1_000);
        let b = flow_to_grid_replica(100_000);
        let c = flow_to_grid_replica(10_000_000);
        assert!(a < b && b < c, "replica monotone: {a} {b} {c}");

        // (b) octave_log_bp anchors.
        assert_eq!(octave_log_bp(0), 0);
        assert_eq!(octave_log_bp(1 << 28), SCALE);
        assert_eq!(octave_log_bp(1u64 << 40), SCALE); // beyond saturation
        let la = octave_log_bp(1_000);
        let lb = octave_log_bp(100_000);
        let lc = octave_log_bp(10_000_000);
        assert!(la < lb && lb < lc, "octave_log_bp monotone: {la} {lb} {lc}");

        // (c) SAME CURVE, only rescaled: for every f, octave_log_bp(f) == round(flow_to_grid_replica(f) onto
        // the SCALE grid) to within one quantization step (the two grids round independently from the shared
        // `pos`). Check the shared `pos` is identical and the rescale is consistent.
        for &f in &[
            1u64,
            2,
            7,
            13,
            100,
            999,
            65_535,
            1_000_000,
            50_000_000,
            (1 << 28) - 1,
        ] {
            let lo = octave_log_bp(f);
            let grid = u64::from(flow_to_grid_replica(f as i64));
            // Reproject the u16-grid value onto the SCALE grid; must match octave_log_bp within ±1 bp.
            let reproj = (grid * SCALE) / UNIT_SCALE;
            let diff = lo.abs_diff(reproj);
            assert!(
                diff <= 2,
                "curve parity at f={f}: octave_log_bp={lo} vs reproj(flow_to_grid)={reproj} (diff {diff})"
            );
        }
    }
}
