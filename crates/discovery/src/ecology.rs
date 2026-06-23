//! The D0 ecology scorer — the 6 metrics M1..M6, the gated combine, and the 12-dim fingerprint.
//!
//! Every value is integer / basis-point on the `SCALE = 10_000` grid (inv #3). Population sums and flow
//! aggregates promote through `u128` where `pop²` or summed flow could overflow `u64`. Species are addressed
//! by fixed index position — NO `HashMap` iteration anywhere on the ordered path. The scorer reads only the
//! exported [`PerGenTrace`] numbers (inv #2 — no biology).

use crate::fixed::{octave_log_bp, ratio_bp, SCALE};
use crate::trace::PerGenTrace;
use crate::{ScoreParams, ScoreVec, FP_DIMS, SCORE_SCALE};

/// Highest trophic-role ordinal (`{Autotroph..ObligateSymbiont}` = 0..5) — the divisor that maps a role onto
/// the fingerprint grid for the `end-dominant-role_bp` dim.
const MAX_ROLE: u64 = 5;

/// Score a trace with the given params. Pure, deterministic, RNG-free.
#[must_use]
pub fn score(p: &ScoreParams, t: &PerGenTrace) -> ScoreVec {
    let s = t.s as usize;
    let g = t.rows.len(); // captured generations (authoritative — may be < gens_requested on early-stop)

    // Degenerate: no species or no rows → everything zero.
    if s == 0 || g == 0 {
        return ScoreVec {
            quality: 0,
            breakdown: [0; 6],
            fingerprint: [0; FP_DIMS],
        };
    }

    // ---- stable window W = [g0 .. g) over the captured rows ----
    let g0 = (g as u64 * p.burn_in_bp / SCALE) as usize;
    // Guard: if burn-in would empty the window (very short runs), keep at least the last row.
    let g0 = g0.min(g.saturating_sub(1));
    let w: &[crate::trace::GenRow] = &t.rows[g0..g];
    let w_len = w.len() as u64;

    // pop(g_idx, i) with bounds tolerance (rows may have ragged pop lengths defensively).
    let pop = |row: &crate::trace::GenRow, i: usize| -> u64 {
        row.pop.get(i).copied().map(u64::from).unwrap_or(0)
    };

    // alive_gens_W[i] = # gens in W where species i has pop > 0.
    let mut alive_gens_w = vec![0u64; s];
    for row in w {
        for (i, slot) in alive_gens_w.iter_mut().enumerate() {
            if pop(row, i) > 0 {
                *slot += 1;
            }
        }
    }

    // persist_bp[i] = alive_gens_W[i] * SCALE / |W|  (∈ [0, SCALE]); persists iff ≥ persist threshold.
    let persist_threshold = w_len * p.persist_bp / SCALE;
    let mut persists = vec![false; s];
    let mut persist_bp = vec![0u64; s];
    for i in 0..s {
        persist_bp[i] = ratio_bp(alive_gens_w[i], w_len).min(SCALE);
        persists[i] = alive_gens_w[i] >= persist_threshold && alive_gens_w[i] > 0;
    }

    let m1 = metric_m1(p, s, &persists);
    let m2 = metric_m2(w, s);
    let (m3, _amps) = metric_m3(p, w, s, &persist_bp);
    let (m4, fp_roles) = metric_m4(p, t, w, s);
    let (m5, ev) = metric_m5(p, t, s);
    let m6 = metric_m6(p, t, s);

    // ---- combine ----
    let weighted = (p.w1 * m1 + p.w2 * m2 + p.w3 * m3 + p.w4 * m4 + p.w5 * m5) / p.wsum().max(1);
    let q_bp = weighted * m6 / SCALE; // multiplicative survival gate
    let quality = q_bp * SCORE_SCALE / SCALE; // → [0, SCORE_SCALE]

    let breakdown = [
        m1 as u16, m2 as u16, m3 as u16, m4 as u16, m5 as u16, m6 as u16,
    ];

    // ---- fingerprint (PINNED order) ----
    let survivors_end = {
        let last = &t.rows[g - 1];
        (0..s).filter(|&i| pop(last, i) > 0).count() as u64
    };
    let survivor_count_bp = ratio_bp(survivors_end, s as u64).min(SCALE);
    let end_dom_role_bp = (u64::from(fp_roles) * SCALE / MAX_ROLE).min(SCALE);

    let fingerprint: [u16; FP_DIMS] = [
        m1 as u16,
        m2 as u16,
        m3 as u16,
        m4 as u16,
        m5 as u16,
        m6 as u16,
        survivor_count_bp as u16,
        end_dom_role_bp as u16,
        octave_log_bp(ev.booms) as u16,
        octave_log_bp(ev.crashes) as u16,
        octave_log_bp(ev.takeovers) as u16,
        octave_log_bp(ev.immigrations) as u16,
    ];

    ScoreVec {
        quality,
        breakdown,
        fingerprint,
    }
}

/// M1 — Coexistence. `R = #persisting`; `R≤1 → 0`. `m1 = (min(R,cap)-1)*SCALE / max(min(S,cap)-1, 1)`.
fn metric_m1(p: &ScoreParams, s: usize, persists: &[bool]) -> u64 {
    let r = persists.iter().filter(|&&x| x).count() as u64;
    if r <= 1 {
        return 0;
    }
    let cap = p.rich_cap;
    let num = r.min(cap) - 1;
    let den = (s as u64).min(cap).saturating_sub(1).max(1);
    (num * SCALE / den).min(SCALE)
}

/// M2 — Evenness. Per gen `simpson_bp = SCALE − Σpop² * SCALE / N²` (= 1−Σpᵢ²); mean over W gens with N>0.
fn metric_m2(w: &[crate::trace::GenRow], s: usize) -> u64 {
    let mut sum_simpson: u64 = 0;
    let mut counted: u64 = 0;
    for row in w {
        let mut n: u128 = 0;
        let mut sumsq: u128 = 0;
        for i in 0..s {
            let pi = u128::from(row.pop.get(i).copied().unwrap_or(0));
            n += pi;
            sumsq += pi * pi;
        }
        if n == 0 {
            continue;
        }
        // simpson_bp = SCALE − sumsq*SCALE/N²  (∈ [0, SCALE]); monoculture → sumsq==N² → 0.
        let conc = (sumsq * u128::from(SCALE)) / (n * n); // Σpᵢ² in bp
        let simpson_bp = u128::from(SCALE).saturating_sub(conc);
        sum_simpson += simpson_bp as u64;
        counted += 1;
    }
    sum_simpson.checked_div(counted).unwrap_or(0)
}

/// M3 — Dynamism. Per species: `amp = (maxW−minW)*SCALE/(maxW+1)`; `turns` = #sign changes in Δpop over W
/// (dropping zero deltas); `turn_bp = min(SCALE, turns*SCALE/turn_target)`; `m3_i = (amp+turn_bp)/2`.
/// Persistence-weighted: `m3 = Σ m3_i*persist_bp[i] / (Σ persist_bp[i] + 1)`.
fn metric_m3(
    p: &ScoreParams,
    w: &[crate::trace::GenRow],
    s: usize,
    persist_bp: &[u64],
) -> (u64, Vec<u64>) {
    let mut amps = vec![0u64; s];
    let mut num: u128 = 0; // Σ m3_i * persist_bp[i]
    let mut den: u128 = 0; // Σ persist_bp[i]
    for i in 0..s {
        // amplitude
        let mut max_w: u64 = 0;
        let mut min_w: u64 = u64::MAX;
        for row in w {
            let v = u64::from(row.pop.get(i).copied().unwrap_or(0));
            max_w = max_w.max(v);
            min_w = min_w.min(v);
        }
        if min_w == u64::MAX {
            min_w = 0;
        }
        let amp = ratio_bp(max_w - min_w, max_w + 1).min(SCALE);
        amps[i] = amp;

        // turns = sign changes in the sequence of nonzero deltas of pop[i] over W.
        let mut last_sign: i64 = 0;
        let mut turns: u64 = 0;
        let mut prev: Option<u64> = None;
        for row in w {
            let v = u64::from(row.pop.get(i).copied().unwrap_or(0));
            if let Some(pv) = prev {
                let d = v as i64 - pv as i64;
                if d != 0 {
                    let sign = d.signum();
                    if last_sign != 0 && sign != last_sign {
                        turns += 1;
                    }
                    last_sign = sign;
                }
            }
            prev = Some(v);
        }
        let turn_bp = (turns * SCALE / p.turn_target).min(SCALE);
        let m3_i = (amp + turn_bp) / 2;

        num += u128::from(m3_i) * u128::from(persist_bp[i]);
        den += u128::from(persist_bp[i]);
    }
    let m3 = (num / (den + 1)) as u64;
    (m3.min(SCALE), amps)
}

/// M4 — Trophic structure. `Agg[i*s+j] = Σ_W flow(dest=i, src=j)` (i128). `E` = #off-diagonal Agg>0 edges;
/// `distinct_roles` = # role ordinals touching an edge; `total_flow` = Σ off-diagonal Agg. Returns
/// `(m4, end_dominant_role)` (the dominant species' role at the last captured gen, for the fingerprint).
fn metric_m4(p: &ScoreParams, t: &PerGenTrace, w: &[crate::trace::GenRow], s: usize) -> (u64, u8) {
    // Aggregate the sparse per-tick flow over W into a dense i128 matrix (dest-major: Agg[dest*s+src]).
    let mut agg = vec![0i128; s * s];
    for row in w {
        for &(dest, src, amount) in &row.flow {
            let (d, sc) = (dest as usize, src as usize);
            if d < s && sc < s && amount > 0 {
                agg[d * s + sc] += i128::from(amount);
            }
        }
    }

    let mut edges: u64 = 0;
    let mut total_flow: i128 = 0;
    // role ordinals touching an edge (either endpoint) — track via a small fixed-size presence array, not a map.
    let mut role_touched = [false; 256];
    for d in 0..s {
        for sc in 0..s {
            if d == sc {
                continue;
            }
            let a = agg[d * s + sc];
            if a > 0 {
                edges += 1;
                total_flow += a;
                let rd = t.species.get(d).map(|m| m.role).unwrap_or(0);
                let rsc = t.species.get(sc).map(|m| m.role).unwrap_or(0);
                role_touched[rd as usize] = true;
                role_touched[rsc as usize] = true;
            }
        }
    }
    let distinct_roles = role_touched.iter().filter(|&&x| x).count() as u64;

    // edge_bp from p.edge_target (tunable, inv #7); role denom fixed at 3 (three "levels"); flow via octave-log.
    let edge_bp = (edges * SCALE / p.edge_target.max(1)).min(SCALE);
    let role_bp = (distinct_roles * SCALE / 3).min(SCALE);
    let flow_bp = octave_log_bp(total_flow.max(0) as u64);
    let m4 = (edge_bp + role_bp + flow_bp) / 3;

    // dominant role at last captured gen.
    let end_role = {
        let last = t.rows.last();
        let mut best_i: usize = 0;
        let mut best_pop: u64 = 0;
        if let Some(last) = last {
            for i in 0..s {
                let v = u64::from(last.pop.get(i).copied().unwrap_or(0));
                if v > best_pop {
                    best_pop = v;
                    best_i = i;
                }
            }
        }
        if best_pop == 0 {
            0
        } else {
            t.species.get(best_i).map(|m| m.role).unwrap_or(0)
        }
    };
    (m4.min(SCALE), end_role)
}

/// Event tallies used by M5 + the fingerprint.
#[derive(Clone, Copy, Default)]
struct Events {
    booms: u64,
    crashes: u64,
    takeovers: u64,
    immigrations: u64,
    /// Σ of event magnitudes (in bp) → the M5 saturating numerator.
    raw: u64,
}

/// M5 — Emergent events over the FULL captured run (booms/crashes/takeovers from `pop`, immigrations from the
/// journal). `event_raw = Σ mags`; `m5 = min(SCALE, event_raw*SCALE/event_sat)`.
fn metric_m5(p: &ScoreParams, t: &PerGenTrace, s: usize) -> (u64, Events) {
    let mut ev = Events::default();
    let rows = &t.rows;
    let g = rows.len();

    // BOOM / CRASH per (g, i) on consecutive captured gens.
    for gi in 1..g {
        let cur = &rows[gi];
        let prev = &rows[gi - 1];
        for i in 0..s {
            let c = u64::from(cur.pop.get(i).copied().unwrap_or(0));
            let pv = u64::from(prev.pop.get(i).copied().unwrap_or(0));
            // BOOM: c ≥ pv*boom_k ∧ pv ≥ pop_floor.
            if pv >= p.pop_floor && c >= pv.saturating_mul(p.boom_k) {
                let mag = octave_log_bp(c / pv.max(1));
                ev.booms += 1;
                ev.raw = ev.raw.saturating_add(mag);
            }
            // CRASH: pv ≥ crash_from ∧ c ≤ pv/crash_k.
            if pv >= p.crash_from && c <= pv / p.crash_k {
                let mag = octave_log_bp(pv / c.max(1));
                ev.crashes += 1;
                ev.raw = ev.raw.saturating_add(mag);
            }
        }
        // TAKEOVER: rank-1 argmax flips between prev and cur (ties → lower SpeciesId), both N>0.
        let r_prev = rank1(prev, s);
        let r_cur = rank1(cur, s);
        if let (Some(a), Some(b)) = (r_prev, r_cur) {
            if a != b {
                ev.takeovers += 1;
                ev.raw = ev.raw.saturating_add(SCALE);
            }
        }
    }

    // IMMIGRATE_ESTABLISHED: per InocRec, the species is alive at the LAST captured gen (G-1).
    if let Some(last) = rows.last() {
        for inoc in &t.inoculations {
            let i = inoc.species_id as usize;
            if i < s && u64::from(last.pop.get(i).copied().unwrap_or(0)) > 0 {
                ev.immigrations += 1;
                ev.raw = ev.raw.saturating_add(SCALE);
            }
        }
    }

    let m5 = (ev.raw * SCALE / p.event_sat).min(SCALE);
    (m5, ev)
}

/// rank-1 species index = argmax pop (ties → lower index), only if its pop > 0. `None` if all-zero.
fn rank1(row: &crate::trace::GenRow, s: usize) -> Option<usize> {
    let mut best_i = 0usize;
    let mut best = 0u64;
    for i in 0..s {
        let v = u64::from(row.pop.get(i).copied().unwrap_or(0));
        if v > best {
            best = v;
            best_i = i;
        }
    }
    if best > 0 {
        Some(best_i)
    } else {
        None
    }
}

/// M6 — Survival GATE (multiplicative). `last_multi_gen` = last gen with ≥2 species alive;
/// `longevity_bp = last_multi_gen*SCALE/G`; `ran_long_bp = G*SCALE/max(1, gens_requested)`;
/// `m6 = min(longevity_bp, ran_long_bp)`. End-state extinction is NOT penalized; only EARLY total loss.
fn metric_m6(_p: &ScoreParams, t: &PerGenTrace, s: usize) -> u64 {
    let g = t.rows.len();
    if g == 0 {
        return 0;
    }
    // last_multi_gen as a 1-based count of gens up to (and incl) the last gen with ≥2 alive.
    let mut last_multi: u64 = 0;
    for (gi, row) in t.rows.iter().enumerate() {
        let alive = (0..s)
            .filter(|&i| u64::from(row.pop.get(i).copied().unwrap_or(0)) > 0)
            .count();
        if alive >= 2 {
            last_multi = (gi + 1) as u64;
        }
    }
    let g_u = g as u64;
    let longevity_bp = ratio_bp(last_multi, g_u).min(SCALE);
    let ran_long_bp = ratio_bp(g_u, u64::from(t.gens_requested).max(1)).min(SCALE);
    longevity_bp.min(ran_long_bp)
}

#[cfg(test)]
mod tests {
    // The 7-archetype oracle lives in `tests/oracle.rs` (integration) so it exercises the public API; here we
    // only keep tiny internal-unit sanity checks that need private fns.
    use super::*;
    use crate::trace::{GenRow, InocRec, SpeciesMeta};

    fn meta(id: u16, role: u8) -> SpeciesMeta {
        SpeciesMeta {
            id,
            key: format!("sp{id}"),
            role,
        }
    }

    fn row(gen: u32, pop: Vec<u32>) -> GenRow {
        GenRow {
            gen,
            allele_q: vec![0; pop.len()],
            pop,
            flow: vec![],
        }
    }

    #[test]
    fn m1_monoculture_is_zero() {
        // Two species but only one persists.
        let p = ScoreParams::default();
        let persists = [true, false];
        assert_eq!(metric_m1(&p, 2, &persists), 0);
        // Both persist → full coexistence at S=2.
        assert_eq!(metric_m1(&p, 2, &[true, true]), SCALE);
    }

    #[test]
    fn m6_early_death_crushes() {
        // 100 gens requested, dead (single species) by gen 3.
        let t = PerGenTrace {
            s: 2,
            g: 100,
            gens_requested: 100,
            species: vec![meta(0, 0), meta(1, 1)],
            rows: (0..100)
                .map(|gi| {
                    if gi < 3 {
                        row(gi, vec![10, 10])
                    } else {
                        row(gi, vec![10, 0])
                    }
                })
                .collect(),
            inoculations: vec![],
            seed: 1,
            recorded_hash: 0,
        };
        let m6 = metric_m6(&ScoreParams::default(), &t, 2);
        // last_multi = 3 (gens 0,1,2) → longevity ≈ 300 bp → tiny.
        assert!(m6 <= 400, "early death must crush m6, got {m6}");
    }

    #[test]
    fn m5_immigration_counts_only_if_established() {
        let p = ScoreParams::default();
        let mut t = PerGenTrace {
            s: 2,
            g: 5,
            gens_requested: 5,
            species: vec![meta(0, 0), meta(1, 1)],
            rows: (0..5).map(|gi| row(gi, vec![10, 10])).collect(),
            inoculations: vec![InocRec {
                gen: 1,
                species_id: 1,
                count: 5,
            }],
            seed: 1,
            recorded_hash: 0,
        };
        let (_m5, ev) = metric_m5(&p, &t, 2);
        assert_eq!(ev.immigrations, 1, "species 1 alive at end → established");
        // Now make species 1 die by the end → not established.
        t.rows[4] = row(4, vec![10, 0]);
        let (_m5b, evb) = metric_m5(&p, &t, 2);
        assert_eq!(evb.immigrations, 0);
    }
}
