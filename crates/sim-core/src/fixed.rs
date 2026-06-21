//! Deterministic fixed-point primitives — the determinism backbone of the ecology substrate
//! (ADR-013 CHEMOSTAT-J, phase **F-1**).
//!
//! Everything load-bearing in the substrate is an integer quantum of a single conserved "joule" currency
//! (`i64`). This module owns the ONE canonical way to **divide / apportion** those quanta so that the
//! resource pools, the genome allocation budget, diffusion remainders, and trophic transfers all round
//! IDENTICALLY and bit-reproducibly across platforms — invariant #3: no transcendentals, no float divide in
//! the sim path, deterministic tie-breaks, no `HashMap` iteration. It draws **zero** from the `SimRng`
//! stream and is pure integer math, so wiring it in is **hash-neutral** until a later phase actually divides
//! a real pool with it.
//!
//! The core primitive is the **largest-remainder method** (Hamilton apportionment): floor every share, then
//! hand the leftover quanta to the largest fractional remainders, breaking ties toward the LOWEST index.
//! Crucially it **conserves the total exactly** — `sum(apportion(total, w)) == total` — so the ledger never
//! gains or loses a quantum to rounding (the property the `ledger_closes` invariant of F0a/F3 will assert).

/// Parts per thousand — the fixed denominator of a genome allocation budget (ADR-013 F2). A `Strategy.budget`
/// is `[u16; N]` summing to exactly `PERMILLE`.
pub const PERMILLE: u32 = 1000;

/// Fixed-point scale for quantizing a `[0,1]` real (e.g. a genome parameter or an affinity) into a `u16`.
/// `u16::MAX` so the whole `u16` range is used and `0.0 -> 0`, `1.0 -> 65535`.
pub const UNIT_SCALE: u16 = u16::MAX;

/// Apportion a non-negative integer `total` across `weights` by the **largest-remainder method**, with a
/// deterministic tie-break: leftover quanta go to the largest fractional remainders, ties to the LOWEST
/// index. Guarantees (for `total >= 0`):
/// * **conservation** — `sum(result) == total` when `Σweights > 0` (else all-zero);
/// * each `result[i]` is `floor(total*weights[i] / Σweights)` plus at most one extra quantum;
/// * `result[i] >= 0`.
///
/// Pure integer math (`u128` intermediates, so no overflow for any realistic joule total) — byte-identical
/// on every platform. A non-positive `total` yields all zeros (the substrate never apportions a debit; the
/// caller handles withdrawals explicitly).
pub fn apportion(total: i64, weights: &[u64]) -> Vec<i64> {
    let n = weights.len();
    let mut out = vec![0i64; n];
    if n == 0 || total <= 0 {
        return out;
    }
    let sum: u128 = weights.iter().map(|&w| u128::from(w)).sum();
    if sum == 0 {
        return out;
    }
    let total_u = total as u128;
    // Floor share + fractional remainder per index, tracking how many quanta remain to distribute.
    let mut remainders: Vec<(u128, usize)> = Vec::with_capacity(n);
    let mut allocated: u128 = 0;
    for (i, &w) in weights.iter().enumerate() {
        let numer = total_u * u128::from(w);
        let q = numer / sum;
        let r = numer - q * sum;
        out[i] = q as i64;
        allocated += q;
        remainders.push((r, i));
    }
    let mut leftover = total_u - allocated; // strictly < n
                                            // Largest remainder first; tie -> lowest index. Total order over (remainder desc, index asc) — fully
                                            // deterministic, no HashMap, no platform-dependent ordering.
    remainders.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut k = 0usize;
    while leftover > 0 {
        out[remainders[k].1] += 1;
        leftover -= 1;
        k += 1;
    }
    out
}

/// Split `total` joules across an allocation `budget` of permille shares (typically summing to [`PERMILLE`],
/// but any non-negative weights are apportioned proportionally). Conserves `total` exactly (ADR-013 F2 —
/// the genome budget never leaks energy). Thin wrapper over [`apportion`].
pub fn split_budget(total: i64, budget: &[u16]) -> Vec<i64> {
    let w: Vec<u64> = budget.iter().map(|&b| u64::from(b)).collect();
    apportion(total, &w)
}

/// Normalize arbitrary non-negative `weights` into a permille budget summing to EXACTLY [`PERMILLE`] (1000),
/// via the same largest-remainder apportionment. The genome→strategy boundary (ADR-013 F2) uses this so a
/// `Strategy.budget` is always a conserved 1000-permille simplex regardless of the raw genome magnitudes.
/// All-zero (or empty) weights yield an all-zero budget (caller treats as "no expressed strategy").
pub fn normalize_permille(weights: &[u64]) -> Vec<u16> {
    apportion(i64::from(PERMILLE as i32), weights)
        .into_iter()
        .map(|x| x as u16)
        .collect()
}

/// Quantize a real in `[0, 1]` to the `u16` fixed-point grid `[0, UNIT_SCALE]` by flooring. This is the single
/// audited chokepoint where a genome `f64` becomes an integer (ADR-013 keystone Q3 option b: `f64` stays on
/// disk, converted at expression). IEEE-754 multiply is correctly-rounded and platform-stable, and flooring
/// then casting to `u16` after clamping is deterministic — no transcendental, no cross-platform divergence.
/// Out-of-range inputs clamp to the grid ends.
pub fn to_unit_u16(x: f64) -> u16 {
    let c = if x <= 0.0 {
        0.0
    } else if x >= 1.0 {
        f64::from(UNIT_SCALE)
    } else {
        (x * f64::from(UNIT_SCALE)).floor()
    };
    c as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apportion_conserves_total() {
        for (total, w) in [
            (10i64, vec![1u64, 1, 1]),
            (1000, vec![3, 3, 3, 1]),
            (7, vec![5, 5]),
            (1_000_000_000, vec![1, 2, 3, 4, 5]),
        ] {
            let out = apportion(total, &w);
            assert_eq!(
                out.iter().sum::<i64>(),
                total,
                "must conserve total {total}"
            );
            assert!(out.iter().all(|&x| x >= 0), "no negative shares");
        }
    }

    #[test]
    fn apportion_largest_remainder_ties_to_lowest_index() {
        // 10 across three equal weights: floor 3 each (=9), one leftover quantum to the LOWEST index.
        assert_eq!(apportion(10, &[1, 1, 1]), vec![4, 3, 3]);
        // 1 across two equal weights: the single quantum goes to index 0.
        assert_eq!(apportion(1, &[1, 1]), vec![1, 0]);
    }

    #[test]
    fn apportion_proportional() {
        // 100 across 1:4 -> 20 / 80 exactly.
        assert_eq!(apportion(100, &[1, 4]), vec![20, 80]);
    }

    #[test]
    fn apportion_degenerate_inputs() {
        assert_eq!(apportion(0, &[1, 2, 3]), vec![0, 0, 0]);
        assert_eq!(apportion(-5, &[1, 1]), vec![0, 0]);
        assert_eq!(apportion(10, &[0, 0]), vec![0, 0], "zero weights -> zeros");
        assert!(apportion(10, &[]).is_empty());
    }

    #[test]
    fn normalize_permille_sums_to_1000() {
        for w in [
            vec![1u64, 1, 1],
            vec![7, 0, 3],
            vec![999, 1],
            vec![5, 5, 5, 5, 5],
        ] {
            let b = normalize_permille(&w);
            assert_eq!(
                b.iter().map(|&x| u32::from(x)).sum::<u32>(),
                PERMILLE,
                "budget {w:?}"
            );
        }
        assert_eq!(
            normalize_permille(&[0, 0]),
            vec![0, 0],
            "no expressed strategy"
        );
    }

    #[test]
    fn split_budget_conserves() {
        let out = split_budget(1_000_000, &[500, 300, 200]);
        assert_eq!(out, vec![500_000, 300_000, 200_000]);
        assert_eq!(out.iter().sum::<i64>(), 1_000_000);
    }

    #[test]
    fn to_unit_u16_grid() {
        assert_eq!(to_unit_u16(0.0), 0);
        assert_eq!(to_unit_u16(1.0), UNIT_SCALE);
        assert_eq!(to_unit_u16(-3.0), 0, "clamp low");
        assert_eq!(to_unit_u16(2.0), UNIT_SCALE, "clamp high");
        assert_eq!(to_unit_u16(0.5), 32767); // floor(0.5 * 65535) = floor(32767.5)
    }

    #[cfg(feature = "proptest")]
    mod prop {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            // The load-bearing contract: apportionment ALWAYS conserves the total and never produces a share
            // outside [floor, floor+1] of the ideal — for any joule total and any weights.
            #[test]
            fn apportion_conserves_and_bounds(
                total in 0i64..1_000_000_000_000,
                weights in proptest::collection::vec(0u64..1_000_000, 1..12),
            ) {
                let out = apportion(total, &weights);
                let sum: u128 = weights.iter().map(|&w| u128::from(w)).sum();
                if sum > 0 {
                    prop_assert_eq!(out.iter().sum::<i64>(), total, "conservation");
                }
                for (i, &share) in out.iter().enumerate() {
                    prop_assert!(share >= 0, "non-negative");
                    if sum > 0 {
                        let floor = ((total as u128) * u128::from(weights[i]) / sum) as i64;
                        prop_assert!(share == floor || share == floor + 1, "share within [floor, floor+1]");
                    }
                }
            }

            // Quantization stays on the grid and is monotonic non-decreasing in its input.
            #[test]
            fn to_unit_u16_in_range_and_monotonic(a in -1.0f64..2.0, b in -1.0f64..2.0) {
                let (qa, qb) = (to_unit_u16(a), to_unit_u16(b));
                prop_assert!(qa <= UNIT_SCALE && qb <= UNIT_SCALE);
                if a <= b { prop_assert!(qa <= qb, "monotonic"); }
            }
        }
    }
}
