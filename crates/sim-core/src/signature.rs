//! Per-species relations **signature** export (ADR-014 re-grounded — the MEASURED-FlowMatrix design).
//!
//! A read-only, OFF-HASH projection of the run into a fixed-order `u16[D]` vector per species, plus a
//! categorical `role:u8` carried ALONGSIDE the vector (a label/filter — NEVER a distance dimension; an
//! `Autotroph` and a `Decomposer` are not metrically "close", so mixing role into L1 would corrupt the
//! metric). This is the substrate the boundary `crates/relations-index` consumes for nearest-species k-NN +
//! single-link guild clustering; the output is VIEW-ONLY and never re-enters `selection()`/`metabolism()`/
//! `hash_world`.
//!
//! ## Hash-neutral by structure (invariant #3)
//! [`species_signatures`] is a PURE projection: it draws ZERO `SimRng`, mutates nothing, and is NEVER folded
//! into `hash_world`. Two sources, both already certified hash-neutral to read:
//!  * **Block A (Strategy)** reads the cached [`gp::Strategy`] in each `SpeciesEntry` (ADR-013 F2) — UNREAD by
//!    selection, F3 metabolism is its first reader, and a read cannot perturb the run.
//!  * **Block B (measured interaction)** reads a projection of the recorded `FlowMatrix` (ADR-013 F4) — folded
//!    into `hash_world` ONCE in F4 in fixed row-major order; READING it here adds no new hash input.
//!
//! Crucially, **no float ever enters the signature**: `base_growth` (the only f64 in scope) is DROPPED — it is
//! already echoed by the budget+affinity block. The only quantization is integer rescaling (permille → the u16
//! grid) and the pinned integer log/clamp of Block B, both pure-integer at the export boundary.
//!
//! ## Layout (PINNED — append-only so a stored sidecar index stays valid)
//! `D = 12`, every dim on the shared u16 grid `[0, UNIT_SCALE = 65535]` so L1 is not dominated by one block.
//!
//! ```text
//! BLOCK A — STRATEGY / metabolic identity (9 dims, from the cached gp::Strategy):
//!   [0..5)  budget[5] {acq, grow, repro, maint, def} — permille rescaled to the u16 grid
//!   [5..8)  affinity[3] {light, free_nutrient, detritus} — already u16 on [0, UNIT_SCALE]
//!   [8]     mineralize_rate — permille rescaled to the u16 grid (the F4 detritus-loop lever)
//!
//! BLOCK B — MEASURED interaction profile (3 dims, from a flow_matrix() projection):
//!   [9]     in_flow  = Σ_{j≠i} max(0, flat[i*s+j])  (J species i GAINED), log/clamp → u16
//!   [10]    out_flow = Σ_{j≠i} max(0, flat[j*s+i])  (J species i GAVE),   log/clamp → u16
//!   [11]    degree   = count of nonzero off-diagonal partners, scaled to the u16 grid by (s − 1)
//! ```
//!
//! `role:u8` = the [`gp::TrophicRole`] ordinal `{Autotroph 0, Heterotroph 1, Mixotroph 2, Decomposer 3}`,
//! carried beside the vector so "nearest decomposer" is expressible as a FILTER.

use crate::gp;
use crate::resource::RESOURCE_CHANNELS;

/// Signature dimensionality — PINNED (ADR-014). Block A (9) + Block B (3). Append-only: any future growth
/// appends dims past `[11]` so a stored sidecar index built against this layout stays valid.
pub const SIGNATURE_DIMS: usize = 12;

/// The shared fixed-point grid every signature dim lives on (`u16::MAX`), mirroring [`crate::fixed::UNIT_SCALE`].
/// Re-stated as a `u16` const here so the projection's intent (one shared scale → L1 is block-balanced) is local
/// and explicit; it equals `fixed::UNIT_SCALE` by construction.
pub const SIGNATURE_UNIT_SCALE: u16 = u16::MAX;

/// The permille denominator a permille value (`budget`, `mineralize_rate`) is rescaled FROM (`*UNIT_SCALE/1000`).
const PERMILLE: u32 = 1000;

/// Block B in/out flow log/clamp denominator — the PINNED integer J-scale (ADR-014). An accumulated flow `f`
/// (i64 joules) maps to the u16 grid via a fixed integer log curve against THIS const, NEVER a per-call max-abs
/// normalization (which would make signatures non-comparable across snapshots). Sized so the per-generation
/// net flows seen in the shipped roster (tens of thousands of J/edge) land across the low-to-mid grid, leaving
/// headroom — the log compression keeps even very large flows on-scale.
const FLOW_J_SCALE: i64 = 1 << 28; // 268_435_456 J — the saturation point (maps to the grid ceiling).

/// Rescale a permille value (`[0, 1000]`) onto the shared u16 grid (`[0, UNIT_SCALE]`) by an exact integer
/// `*UNIT_SCALE/1000` (floored once). Clamps a malformed `> 1000` input to the grid ceiling defensively.
#[must_use]
fn permille_to_grid(permille: u16) -> u16 {
    let v = u32::from(permille).min(PERMILLE);
    ((v * u32::from(SIGNATURE_UNIT_SCALE)) / PERMILLE) as u16
}

/// Map an accumulated non-negative flow `f` (i64 joules) onto the u16 grid via a PINNED integer base-2 log
/// curve against [`FLOW_J_SCALE`] — pure integer, no transcendental, identical on every platform. `0 → 0`;
/// `f ≥ FLOW_J_SCALE → UNIT_SCALE`; in between, `floor(log2(f+1))` linearly fills the 28 octaves the scale
/// spans, then the fractional octave is interpolated by the high bits. This compresses the heavy-tailed flow
/// magnitudes onto a comparable grid WITHOUT a per-call max-abs (so the same flow maps to the same dim value in
/// every snapshot — the cross-snapshot comparability inv #3 demands).
#[must_use]
fn flow_to_grid(f: i64) -> u16 {
    if f <= 0 {
        return 0;
    }
    if f >= FLOW_J_SCALE {
        return SIGNATURE_UNIT_SCALE;
    }
    // 28 octaves span [1, FLOW_J_SCALE). `bits` = floor(log2(f)) ∈ [0, 27]; the next `FRAC_BITS` below the
    // leading bit interpolate inside the octave. Result = (octave * 2^FRAC_BITS + frac) scaled onto the grid.
    let f = f as u64;
    const OCTAVES: u32 = 28; // log2(FLOW_J_SCALE)
    const FRAC_BITS: u32 = 8;
    let bits = 63 - f.leading_zeros(); // floor(log2(f)) ∈ [0, 27]
                                       // Fractional part: the FRAC_BITS just below the leading 1-bit.
    let frac = if bits >= FRAC_BITS {
        (f >> (bits - FRAC_BITS)) & ((1 << FRAC_BITS) - 1)
    } else {
        (f << (FRAC_BITS - bits)) & ((1 << FRAC_BITS) - 1)
    };
    // Position in [0, OCTAVES * 2^FRAC_BITS) → scale onto [0, UNIT_SCALE].
    let pos = (u64::from(bits) << FRAC_BITS) | frac;
    let span = u64::from(OCTAVES) << FRAC_BITS;
    let scaled = (pos * u64::from(SIGNATURE_UNIT_SCALE)) / span;
    scaled.min(u64::from(SIGNATURE_UNIT_SCALE)) as u16
}

/// The Block-B measured-interaction triple for one species `i`, derived from the flat row-major FlowMatrix
/// `flat` (`flat[a*s + b]` = NET J from `b` into `a`). All integer, ordered (`j` ascending), HashMap-free.
///
/// * `in_flow`  = Σ_{j≠i} max(0, flat[i*s+j])  (J species i GAINED across the row)
/// * `out_flow` = Σ_{j≠i} max(0, flat[j*s+i])  (J species i GAVE across the column)
/// * `degree`   = count of nonzero off-diagonal partners (either direction) — interaction topology
#[must_use]
fn block_b(flat: &[i64], s: usize, i: usize) -> [u16; 3] {
    if s == 0 || flat.len() != s * s {
        return [0, 0, 0];
    }
    let mut in_flow: i64 = 0;
    let mut out_flow: i64 = 0;
    let mut degree: u32 = 0;
    for j in 0..s {
        if j == i {
            continue;
        }
        let row = flat[i * s + j]; // net J from j into i
        let col = flat[j * s + i]; // net J from i into j
        in_flow = in_flow.saturating_add(row.max(0));
        out_flow = out_flow.saturating_add(col.max(0));
        if row != 0 || col != 0 {
            degree += 1;
        }
    }
    // `degree` scaled onto the u16 grid by the max possible partners (s − 1); s == 1 → 0 (no partners).
    let degree_grid = if s > 1 {
        ((u64::from(degree) * u64::from(SIGNATURE_UNIT_SCALE)) / (s as u64 - 1)) as u16
    } else {
        0
    };
    [flow_to_grid(in_flow), flow_to_grid(out_flow), degree_grid]
}

/// Assemble one species' full `u16[SIGNATURE_DIMS]` signature row from its cached [`gp::Strategy`] (Block A) and
/// the flat FlowMatrix projection for its index (Block B). Pure integer; no f64 enters the row.
#[must_use]
pub(crate) fn signature_row(
    strategy: &gp::Strategy,
    flat: &[i64],
    s: usize,
    i: usize,
) -> [u16; SIGNATURE_DIMS] {
    let mut row = [0u16; SIGNATURE_DIMS];
    // [0..5) budget permille → grid.
    for (slot, &b) in row[0..5].iter_mut().zip(strategy.budget.iter()) {
        *slot = permille_to_grid(b);
    }
    // [5..8) affinity — already on the u16 grid.
    row[5..5 + RESOURCE_CHANNELS].copy_from_slice(&strategy.affinity);
    // [8] mineralize_rate permille → grid.
    row[8] = permille_to_grid(strategy.mineralize_rate);
    // [9..12) measured interaction.
    let b = block_b(flat, s, i);
    row[9] = b[0];
    row[10] = b[1];
    row[11] = b[2];
    row
}

/// The categorical role ordinal `{Autotroph 0, Heterotroph 1, Mixotroph 2, Decomposer 3}` carried beside the
/// vector (a FILTER, not a distance dim). Matches the [`gp::TrophicRole`] declaration order.
#[must_use]
pub(crate) fn role_ordinal(role: gp::TrophicRole) -> u8 {
    match role {
        gp::TrophicRole::Autotroph => 0,
        gp::TrophicRole::Heterotroph => 1,
        gp::TrophicRole::Mixotroph => 2,
        gp::TrophicRole::Decomposer => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permille_rescale_spans_the_grid() {
        assert_eq!(permille_to_grid(0), 0);
        assert_eq!(permille_to_grid(1000), SIGNATURE_UNIT_SCALE);
        // Monotone and in-grid in between.
        let mid = permille_to_grid(500);
        assert!(mid > 0 && mid < SIGNATURE_UNIT_SCALE);
        // Defensive clamp of a malformed > 1000 permille.
        assert_eq!(permille_to_grid(2000), SIGNATURE_UNIT_SCALE);
    }

    #[test]
    fn flow_log_is_monotone_and_clamped() {
        assert_eq!(flow_to_grid(0), 0);
        assert_eq!(flow_to_grid(-5), 0); // negatives floored to 0 (in/out are pre-max'd anyway)
        assert_eq!(flow_to_grid(FLOW_J_SCALE), SIGNATURE_UNIT_SCALE);
        assert_eq!(flow_to_grid(FLOW_J_SCALE * 2), SIGNATURE_UNIT_SCALE);
        // Strictly monotone across a few octaves.
        let a = flow_to_grid(1_000);
        let b = flow_to_grid(100_000);
        let c = flow_to_grid(10_000_000);
        assert!(a < b && b < c, "log curve must be monotone: {a} {b} {c}");
    }

    #[test]
    fn block_b_tracks_a_known_synthetic_matrix() {
        // 3 species; species 1 GIVES 100 J to species 0 (so row 0 gains, row 1 loses; row-sum==0).
        // flat[a*s+b] = net J from b into a. Encode: 0 gains 100 from 1; 1 loses 100 (gives to 0).
        let s = 3;
        let mut flat = vec![0i64; s * s];
        flat[1] = 100; // flat[0*s+1]: net J from 1 into 0 = +100  (0 GAINED 100)
        flat[s] = -100; // flat[1*s+0]: net J from 0 into 1 = -100 (1 GAVE 100)
                        // Species 0: in_flow=100 (gained), out_flow=0, degree=1 partner.
        let b0 = block_b(&flat, s, 0);
        assert_eq!(b0[0], flow_to_grid(100));
        assert_eq!(b0[1], flow_to_grid(0));
        assert!(b0[2] > 0, "species 0 has one nonzero partner");
        // Species 1: in_flow=0, out_flow=100 (gave to 0), degree=1.
        let b1 = block_b(&flat, s, 1);
        assert_eq!(b1[0], flow_to_grid(0));
        assert_eq!(b1[1], flow_to_grid(100));
        assert!(b1[2] > 0);
        // Species 2: isolated → all zero.
        assert_eq!(block_b(&flat, s, 2), [0, 0, 0]);
    }

    #[test]
    fn role_ordinals_match_declaration_order() {
        assert_eq!(role_ordinal(gp::TrophicRole::Autotroph), 0);
        assert_eq!(role_ordinal(gp::TrophicRole::Heterotroph), 1);
        assert_eq!(role_ordinal(gp::TrophicRole::Mixotroph), 2);
        assert_eq!(role_ordinal(gp::TrophicRole::Decomposer), 3);
    }

    #[test]
    fn signature_row_is_integer_and_fixed_length() {
        let strat = gp::Strategy {
            budget: [200, 200, 200, 200, 200],
            role: gp::TrophicRole::Autotroph,
            affinity: [60000, 0, 0],
            mineralize_rate: 0,
        };
        let row = signature_row(&strat, &[], 0, 0); // no flow matrix → Block B all zero
        assert_eq!(row.len(), SIGNATURE_DIMS);
        // Budget 200 permille → grid.
        assert_eq!(row[0], permille_to_grid(200));
        // Affinity passed through unchanged (already on the grid).
        assert_eq!(row[5], 60000);
        // Block B with no matrix → zeros.
        assert_eq!(&row[9..12], &[0, 0, 0]);
    }
}
