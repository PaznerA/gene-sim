//! The 7-archetype behavior oracle (the D0 contract) + determinism + novelty tests.
//!
//! Each archetype is a HAND-BUILT synthetic `PerGenTrace` (no sim-core, no RNG). The assertions encode the
//! spec's ordering contract — most importantly `A.quality > F.quality` (a live limit cycle beats frozen
//! coexistence: the open-system "don't tune to forced stability" memory).

use discovery::trace::{GenRow, InocRec, PerGenTrace, SpeciesMeta};
use discovery::{final_score, novelty_l1, DefaultScorer, InterestingnessScorer, ScoreVec, FP_DIMS};

const G: u32 = 200;

fn meta(id: u16, role: u8) -> SpeciesMeta {
    SpeciesMeta {
        id,
        key: format!("sp{id}"),
        role,
    }
}

/// A GenRow with a symmetric predator-prey flow edge sized to the populations (so M4 lights up). `flow` is
/// `(dest, src, amount>0)`. When `pred`/`prey` populations are both positive, prey→predator energy flows.
fn row_flow(gen: u32, pop: Vec<u32>, flow: Vec<(u16, u16, i64)>) -> GenRow {
    GenRow {
        gen,
        allele_q: vec![0; pop.len()],
        pop,
        flow,
    }
}

fn row(gen: u32, pop: Vec<u32>) -> GenRow {
    row_flow(gen, pop, vec![])
}

fn trace(
    s: u16,
    species: Vec<SpeciesMeta>,
    rows: Vec<GenRow>,
    inoculations: Vec<InocRec>,
) -> PerGenTrace {
    let g = rows.len() as u32;
    PerGenTrace {
        s,
        g,
        gens_requested: G,
        species,
        rows,
        inoculations,
        seed: 42,
        recorded_hash: 0xdead_beef,
    }
}

/// Integer triangle-wave oscillator in `[lo, hi]` with `period` gens, phase-shifted by `phase`.
fn tri(gen: u32, lo: u32, hi: u32, period: u32, phase: u32) -> u32 {
    let span = hi - lo;
    let p = (gen + phase) % period;
    let half = period / 2;
    let up = if p < half {
        // rising
        (p * span) / half
    } else {
        // falling
        span - ((p - half) * span) / half
    };
    lo + up
}

// ---------------------------------------------------------------------------
// A — predator–prey limit cycle (anti-phase, with trophic flow). Expect HIGH.
// ---------------------------------------------------------------------------
fn archetype_a() -> PerGenTrace {
    let species = vec![meta(0, 0), meta(1, 4)]; // prey = autotroph(0), predator = predator(4)
    let rows: Vec<GenRow> = (0..G)
        .map(|gi| {
            // Prey leads, predator lags by a quarter period — classic anti-phase cycle.
            let prey = tri(gi, 20, 200, 40, 0);
            let pred = tri(gi, 10, 120, 40, 10);
            // prey → predator energy flow proportional to predator size (a real trophic edge).
            let flow = vec![(1u16, 0u16, i64::from(pred) * 50)];
            row_flow(gi, vec![prey, pred], flow)
        })
        .collect();
    trace(2, species, rows, vec![])
}

// ---------------------------------------------------------------------------
// B — contamination recovery: an immigrant establishes mid-run + reshapes the web. Expect HIGH.
// ---------------------------------------------------------------------------
fn archetype_b() -> PerGenTrace {
    // 3 species: a resident producer + consumer, plus an immigrant decomposer introduced at gen 60.
    let species = vec![meta(0, 0), meta(1, 1), meta(2, 3)];
    let rows: Vec<GenRow> = (0..G)
        .map(|gi| {
            let prod = tri(gi, 40, 160, 50, 0);
            let cons = tri(gi, 20, 110, 50, 14);
            // immigrant absent until gen 60, then grows and oscillates (establishes).
            let immig = if gi < 60 {
                0
            } else {
                tri(gi - 60, 15, 90, 45, 0).max(8)
            };
            let mut flow = vec![(1u16, 0u16, i64::from(cons) * 40)];
            if immig > 0 {
                // decomposer pulls detritus from both → reshapes the web (two new edges).
                flow.push((2u16, 0u16, i64::from(immig) * 20));
                flow.push((2u16, 1u16, i64::from(immig) * 20));
            }
            row_flow(gi, vec![prod, cons, immig], flow)
        })
        .collect();
    let inoc = vec![InocRec {
        gen: 60,
        species_id: 2,
        count: 8,
    }];
    trace(3, species, rows, inoc)
}

// ---------------------------------------------------------------------------
// C — trophic cascade with rebound: temporary collapse then recovery. Expect HIGH.
// ---------------------------------------------------------------------------
fn archetype_c() -> PerGenTrace {
    let species = vec![meta(0, 0), meta(1, 1), meta(2, 4)];
    let rows: Vec<GenRow> = (0..G)
        .map(|gi| {
            // A cascade: predator crashes ~gen 70, prey booms then over-grazes producer, all rebound by ~gen 130.
            let (prod, cons, pred) = if gi < 70 {
                (
                    tri(gi, 60, 140, 60, 0),
                    tri(gi, 40, 90, 60, 12),
                    tri(gi, 30, 70, 60, 24),
                )
            } else if gi < 100 {
                // collapse window: predator crashes, consumer booms, producer crashes.
                (
                    140u32.saturating_sub((gi - 70) * 4),
                    90 + (gi - 70) * 3,
                    70u32.saturating_sub((gi - 70) * 2).max(3),
                )
            } else {
                // rebound: everything climbs back into oscillation.
                let k = gi - 100;
                (
                    (20 + k * 2).min(140),
                    (180u32.saturating_sub(k * 2)).max(40),
                    (10 + k).min(70),
                )
            };
            let flow = vec![
                (1u16, 0u16, i64::from(cons) * 30),
                (2u16, 1u16, i64::from(pred) * 30),
            ];
            row_flow(gi, vec![prod, cons, pred], flow)
        })
        .collect();
    trace(3, species, rows, vec![])
}

// ---------------------------------------------------------------------------
// D — instant collapse: dead by ~gen 5 of 500. Expect LOW (≈0; M6 gate crushes Q).
// ---------------------------------------------------------------------------
fn archetype_d() -> PerGenTrace {
    let species = vec![meta(0, 0), meta(1, 1)];
    // Captured run early-stops at gen 5 (Σpop==0). gens_requested stays 500.
    let mut rows: Vec<GenRow> = vec![
        row(0, vec![30, 20]),
        row(1, vec![25, 15]),
        row(2, vec![15, 8]),
        row(3, vec![6, 2]),
        row(4, vec![2, 0]),
        row(5, vec![0, 0]),
    ];
    rows.shrink_to_fit();
    let g = rows.len() as u32;
    PerGenTrace {
        s: 2,
        g,
        gens_requested: 500,
        species,
        rows,
        inoculations: vec![],
        seed: 42,
        recorded_hash: 0,
    }
}

// ---------------------------------------------------------------------------
// E — flat monoculture: one species, perfectly flat. Expect LOW (M1=M2=M4=M5=0).
// ---------------------------------------------------------------------------
fn archetype_e() -> PerGenTrace {
    let species = vec![meta(0, 0), meta(1, 1)];
    // Species 0 flat at 100; species 1 dead from the start.
    let rows: Vec<GenRow> = (0..G).map(|gi| row(gi, vec![100, 0])).collect();
    trace(2, species, rows, vec![])
}

// ---------------------------------------------------------------------------
// F — converged steady-state coexistence (the forced-stability trap). Expect LOW-ish, STRICTLY below A.
// ---------------------------------------------------------------------------
fn archetype_f() -> PerGenTrace {
    let species = vec![meta(0, 0), meta(1, 4)];
    // Both alive, even, with a steady trophic flow — but FROZEN (no dynamism, no events).
    let rows: Vec<GenRow> = (0..G)
        .map(|gi| row_flow(gi, vec![100, 80], vec![(1u16, 0u16, 80 * 50)]))
        .collect();
    trace(2, species, rows, vec![])
}

// ---------------------------------------------------------------------------
// G — single boom then plateau. Expect LOW (turn-gating + saturation + M2/M4 zeros).
// ---------------------------------------------------------------------------
fn archetype_g() -> PerGenTrace {
    let species = vec![meta(0, 0), meta(1, 1)];
    let rows: Vec<GenRow> = (0..G)
        .map(|gi| {
            // Monotone boom to a plateau; second species dead (so no coexistence / evenness / trophic).
            let p0 = if gi < 30 { 10 + gi * 6 } else { 190 };
            row(gi, vec![p0, 0])
        })
        .collect();
    trace(2, species, rows, vec![])
}

fn q(t: &PerGenTrace) -> ScoreVec {
    DefaultScorer::default().score(t)
}

#[test]
fn seven_archetype_ordering_contract() {
    let a = q(&archetype_a());
    let b = q(&archetype_b());
    let c = q(&archetype_c());
    let d = q(&archetype_d());
    let e = q(&archetype_e());
    let f = q(&archetype_f());
    let g = q(&archetype_g());

    eprintln!(
        "A={} B={} C={} D={} E={} F={} G={}",
        a.quality, b.quality, c.quality, d.quality, e.quality, f.quality, g.quality
    );
    eprintln!("A breakdown {:?}", a.breakdown);
    eprintln!("B breakdown {:?}", b.breakdown);
    eprintln!("C breakdown {:?}", c.breakdown);
    eprintln!("F breakdown {:?}", f.breakdown);

    // HIGH archetypes (above the spec gates).
    assert!(
        a.quality >= 600_000,
        "A (limit cycle) must be HIGH: {}",
        a.quality
    );
    assert!(
        b.quality >= 600_000,
        "B (contamination) must be HIGH: {}",
        b.quality
    );
    assert!(
        c.quality >= 450_000,
        "C (cascade+rebound) must be HIGH: {}",
        c.quality
    );

    // LOW archetypes.
    assert!(
        d.quality <= 50_000,
        "D (instant collapse) must be LOW: {}",
        d.quality
    );
    assert!(
        e.quality <= 50_000,
        "E (monoculture) must be LOW: {}",
        e.quality
    );
    assert!(
        g.quality <= 250_000,
        "G (single boom) must be LOW: {}",
        g.quality
    );

    // F is the forced-stability trap: low-ish, and STRICTLY below A — the single most important ordering.
    assert!(
        f.quality < a.quality,
        "A({}) must strictly beat F({})",
        a.quality,
        f.quality
    );
    // F must also clearly rank below the HIGH band (it is the trap, not a gem).
    assert!(
        f.quality < 450_000,
        "F (frozen coexistence) must be LOW-ish: {}",
        f.quality
    );
}

#[test]
fn determinism_same_bytes_byte_identical_scorevec() {
    let t = archetype_a();
    let s1 = q(&t);
    // Re-build an identical trace from scratch and score again.
    let s2 = q(&archetype_a());
    assert_eq!(s1, s2, "same trace → byte-identical ScoreVec (Eq)");
    // And re-scoring the very same value is stable.
    assert_eq!(q(&t), s1);
}

#[test]
fn novelty_l1_empty_set_is_scale_and_near_dup_is_small() {
    let zero = [0u16; FP_DIMS];
    // Empty gem set → maximal novelty = SCALE.
    assert_eq!(novelty_l1(&zero, &[]), 10_000);

    let a = q(&archetype_a());
    // A near-duplicate of A's fingerprint (one dim off by 3) → small nn.
    let mut dup = a.fingerprint;
    dup[0] = dup[0].wrapping_add(3);
    let nn = novelty_l1(&a.fingerprint, &[dup]);
    assert_eq!(nn, 3, "near-duplicate fingerprint → nn == 3");

    // Monotonicity: adding a far gem doesn't reduce nn below the nearest; the nearest stays the duplicate.
    let far = [60_000u16; FP_DIMS];
    let nn2 = novelty_l1(&a.fingerprint, &[far, dup]);
    assert_eq!(nn2, 3, "nearest neighbour is still the near-duplicate");
    assert!(
        novelty_l1(&a.fingerprint, &[far]) > nn,
        "a distinct-only gem set yields larger nn than a near-duplicate"
    );
}

#[test]
fn final_score_applies_novelty_multiplier() {
    let scorer = DefaultScorer::default();
    let t = archetype_a();
    // Empty gem set → nn == SCALE; novelty_bp = min(SCALE, SCALE*SCALE/NOV_SAT) = 10000*10000/30000 = 3333.
    let fresh = final_score(&scorer, &t, &[]);
    assert_eq!(fresh.nn, 10_000);
    assert_eq!(fresh.novelty_bp, 3_333);
    // mult = NOV_FLOOR + (SCALE-NOV_FLOOR)*novelty_bp/SCALE = 4000 + 6000*3333/10000 = 5999.
    let mult = 4_000 + 6_000 * 3_333 / 10_000;
    assert_eq!(fresh.final_score, fresh.score.quality * mult / 10_000);

    // A redundant gem (identical fingerprint) → nn == 0 → multiplier floors at NOV_FLOOR (40%).
    let same = fresh.score.fingerprint;
    let redundant = final_score(&scorer, &t, &[same]);
    assert_eq!(redundant.nn, 0);
    assert_eq!(redundant.novelty_bp, 0);
    // final = quality * 4000/10000 = 40% of quality (the NOV_FLOOR — a redundant gem keeps 40%).
    assert_eq!(
        redundant.final_score,
        redundant.score.quality * 4_000 / 10_000
    );

    // A far-away gem set → novelty saturates at SCALE → multiplier == 1.0 → final_score == quality.
    let far = [60_000u16; FP_DIMS];
    let novel = final_score(&scorer, &t, &[far]);
    assert!(
        novel.nn >= 30_000,
        "far gem → nn ≥ NOV_SAT, got {}",
        novel.nn
    );
    assert_eq!(novel.novelty_bp, 10_000);
    assert_eq!(novel.final_score, novel.score.quality);
}

#[test]
fn b_is_distinct_from_a_high_novelty() {
    // B's fingerprint should be far enough from A's to clear the dedup floor (different web/role profile).
    let a = q(&archetype_a());
    let b = q(&archetype_b());
    let nn = novelty_l1(&b.fingerprint, &[a.fingerprint]);
    assert!(
        nn >= 10_000,
        "B must be distinct from A (nn={nn} ≥ DEDUP_MIN)"
    );
}
