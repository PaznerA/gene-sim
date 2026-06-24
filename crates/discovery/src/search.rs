//! D2a STAGE 1 — the SEARCH types: the config / proposal / gem data model (NO engine).
//!
//! ## Boundary (inv #1/#5)
//! std + serde ONLY — exactly like the rest of `discovery`. A [`SearchConfig`] is a DETERMINISTIC, serializable
//! DESCRIPTION of one headless run (roster + env + containment); it carries NO `sim-core` / `harness` types, so
//! the actual capture/replay engine (D2b) lives on the other side of the seam and consumes a plain config.
//!
//! ## Determinism (inv #3)
//! The proposal sampler [`propose`] uses a std-only splitmix64 integer hash of `(search_seed, trial, field)` —
//! NO `rand` / `rand_chacha` crate, NO thread-local/global RNG. Same `(search_seed, trial)` → byte-identical
//! [`SearchConfig`]. The [`GemLibrary`] keep/dedup logic is pure integer + ordered (`Vec`, no `HashMap`
//! iteration), with a fully-specified deterministic tie-break — so the kept set is order-independent of
//! insertion. Captions are derived purely from the integer score signals (inv #2 — no biology).

use crate::fixed::SCALE;
use crate::{novelty_l1, ScoreVec, FP_DIMS};
use serde::{Deserialize, Serialize};

/// A DETERMINISTIC description of one headless run: which species + how many, plus the env knobs. Replaying the
/// engine on the same `master_seed` + this config reproduces the run byte-identically (the gem reproducibility
/// contract). `temp_q` is q16 permille (`0..=1000` → `0.0..=1.0`); `season` is the season ordinal.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchConfig {
    /// The run's master seed (derives every sub-seed in the engine — inv #3).
    pub master_seed: u64,
    /// Roster: `(species key/stem, starting count)`, in the [`SearchSpace`] species order (deterministic).
    pub roster: Vec<(String, u32)>,
    /// Containment level (`0..=3`: Sealed → Open) — drives deterministic airborne immigration.
    pub containment_level: u8,
    /// Temperature as q16 permille (`0..=1000` ↔ `0.0..=1.0`).
    pub temp_q: u16,
    /// Season ordinal (`0..=3`: Spring/Summer/Autumn/Winter).
    pub season: u8,
}

/// One species axis of the search: its key/stem + the inclusive `[lo, hi]` starting-count range to draw from.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpeciesAxis {
    /// Species key/stem (matches the roster key consumed by the engine: `default`/`ecoli`/`bacillus`/...).
    pub key: String,
    /// Inclusive minimum starting count.
    pub count_lo: u32,
    /// Inclusive maximum starting count.
    pub count_hi: u32,
}

/// The bounded config space the sampler draws from — pins the species set + per-field ranges. [`Default`] is the
/// Primordial anchor (the `data/presets/primordial.json` roster + env knobs, widened into ranges to search).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchSpace {
    /// The species axes, in a FIXED order (the proposed roster preserves this order — deterministic).
    pub species: Vec<SpeciesAxis>,
    /// Inclusive containment-level range (`[lo, hi]` within `0..=3`).
    pub containment_lo: u8,
    /// Inclusive containment-level upper bound.
    pub containment_hi: u8,
    /// Inclusive temperature range, q16 permille.
    pub temp_lo: u16,
    /// Inclusive temperature upper bound, q16 permille.
    pub temp_hi: u16,
    /// Inclusive season-ordinal range (`[lo, hi]` within `0..=3`).
    pub season_lo: u8,
    /// Inclusive season-ordinal upper bound.
    pub season_hi: u8,
}

impl Default for SearchSpace {
    fn default() -> Self {
        // The Primordial anchor (data/presets/primordial.json), widened into search ranges. Producer-heavy
        // pyramid: plant >> decomposers > predator. Order is FIXED (drives the deterministic roster order).
        SearchSpace {
            species: vec![
                SpeciesAxis {
                    key: "default".to_string(),
                    count_lo: 200,
                    count_hi: 1200,
                },
                SpeciesAxis {
                    key: "ecoli".to_string(),
                    count_lo: 50,
                    count_hi: 600,
                },
                SpeciesAxis {
                    key: "bacillus".to_string(),
                    count_lo: 30,
                    count_hi: 400,
                },
                SpeciesAxis {
                    key: "bdellovibrio".to_string(),
                    count_lo: 10,
                    count_hi: 200,
                },
            ],
            containment_lo: 0,
            containment_hi: 3,
            // temp 0.20..=0.80 (q16 permille) — a livable band around the preset's 0.50.
            temp_lo: 200,
            temp_hi: 800,
            // all four seasons.
            season_lo: 0,
            season_hi: 3,
        }
    }
}

/// splitmix64 — the canonical std-only integer scrambler (NO `rand` crate). A pure function of its input word:
/// avalanches every input bit, so `mix64(stream(seed, trial, field))` gives an independent, reproducible draw
/// per field. Public for tests/callers that want the same stream the sampler uses.
#[must_use]
pub fn mix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Combine `(search_seed, trial, field)` into one stream word, then avalanche it. Distinct `field` indices give
/// independent draws within a trial; distinct `trial`s give independent configs; `search_seed` shifts the whole
/// search. Order of mixing is fixed → byte-stable.
#[must_use]
fn draw(search_seed: u64, trial: u64, field: u64) -> u64 {
    // Fold the three coordinates through the mixer in a fixed order so every coordinate avalanches the rest.
    let a = mix64(search_seed ^ 0xA076_1D64_78BD_642F);
    let b = mix64(a ^ trial.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    mix64(b ^ field.wrapping_mul(0xD1B5_4A32_D192_ED03))
}

/// Map a raw scrambled word uniformly onto the inclusive `[lo, hi]` integer range. `lo >= hi → lo` (degenerate
/// axis collapses to its single value). Uses the `u128` widening-multiply trick (Lemire) for an unbiased,
/// division-only reduction — exact + deterministic.
#[must_use]
fn in_range_u64(r: u64, lo: u64, hi: u64) -> u64 {
    if lo >= hi {
        return lo;
    }
    let span = hi - lo + 1; // inclusive width (hi >= lo, and span <= u64::MAX since lo>0 cases are small here)
    let offset = ((u128::from(r) * u128::from(span)) >> 64) as u64;
    lo + offset
}

/// DETERMINISTIC proposal: draw a [`SearchConfig`] from `space` for `(search_seed, trial)`. Same `(search_seed,
/// trial)` → byte-identical config; different `trial`s generally differ. Each field draws from its own
/// `(.., field_index)` stream, so adding a field never perturbs the earlier ones. NO RNG crate — pure splitmix.
#[must_use]
pub fn propose(search_seed: u64, trial: u64, space: &SearchSpace) -> SearchConfig {
    // Field index allocation (fixed — never reorder, or stored configs stop reproducing):
    //   0          → master_seed
    //   1..=N      → per-species count (N = species.len())
    //   1+N        → containment
    //   2+N        → temp
    //   3+N        → season
    let n = space.species.len() as u64;

    // The run's master seed is itself a deterministic draw (full 64-bit word — every run gets its own seed).
    let master_seed = draw(search_seed, trial, 0);

    let mut roster: Vec<(String, u32)> = Vec::with_capacity(space.species.len());
    for (i, axis) in space.species.iter().enumerate() {
        let r = draw(search_seed, trial, 1 + i as u64);
        let count = in_range_u64(r, u64::from(axis.count_lo), u64::from(axis.count_hi)) as u32;
        roster.push((axis.key.clone(), count));
    }

    let containment_level = in_range_u64(
        draw(search_seed, trial, 1 + n),
        u64::from(space.containment_lo),
        u64::from(space.containment_hi),
    ) as u8;
    let temp_q = in_range_u64(
        draw(search_seed, trial, 2 + n),
        u64::from(space.temp_lo),
        u64::from(space.temp_hi),
    ) as u16;
    let season = in_range_u64(
        draw(search_seed, trial, 3 + n),
        u64::from(space.season_lo),
        u64::from(space.season_hi),
    ) as u8;

    SearchConfig {
        master_seed,
        roster,
        containment_level,
        temp_q,
        season,
    }
}

/// A saved emergent run — the gem. It bundles the reproducible [`SearchConfig`] with the integer score signals
/// (quality, novelty-adjusted final `score`, per-metric `breakdown`, novelty fingerprint), the engine
/// reproducibility anchor (`recorded_hash` + `build_id`, inv #7), an auto one-liner caption, and the gens run.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Gem {
    /// The reproducible run description (master_seed + roster + env).
    pub config: SearchConfig,
    /// Novelty-adjusted FINAL score (what gems are ranked by). See [`crate::final_score`].
    pub score: u64,
    /// Gated combined quality `Q ∈ [0, SCORE_SCALE]` (pre-novelty).
    pub quality: u64,
    /// Novelty basis points at save time (`min(SCALE, nn*SCALE/nov_sat)`).
    pub novelty: u16,
    /// The six metric values `[m1, m2, m3, m4, m5, m6]` (explainability).
    pub breakdown: [u16; 6],
    /// The 12-dim novelty fingerprint (PINNED order — drives [`GemLibrary`] dedup).
    pub fingerprint: [u16; FP_DIMS],
    /// The `hash_world` the recording produced — the byte-identical-replay contract anchor (inv #3).
    pub recorded_hash: u64,
    /// The pinned-build fingerprint (inv #7). A re-pin invalidates stored scores (recompute by replay).
    pub build_id: String,
    /// Auto one-liner from the integer breakdown (no biology) — see [`caption`].
    pub caption: String,
    /// Generations the run actually executed.
    pub gens: u32,
}

/// An auto one-liner describing a run, derived PURELY from the integer score signals + the roster size — no
/// biology, no float. Form: `"<shape> · <N> spp · <events>"`, e.g. `"limit-cycle · 3 spp · 2 takeovers"`. The
/// shape is read off M3 (dynamism) vs M1/M2 (coexistence/evenness); the event tail off the fingerprint's
/// boom/crash/takeover/immig octave dims (indices 8..=11). Stable: same inputs → same string.
#[must_use]
pub fn caption(s: &ScoreVec, cfg: &SearchConfig) -> String {
    let [m1, m2, m3, _m4, m5, _m6] = s.breakdown;
    // species count = roster entries with a positive starting count (the run's nominal richness).
    let spp = cfg.roster.iter().filter(|(_, c)| *c > 0).count();

    // --- shape: read off dynamism (m3) and coexistence (m1)+evenness (m2) ---
    // High m3 = oscillation/drama; high m1+m2 = sustained even multi-species; low everything = flat/dead.
    let half = (SCALE / 2) as u16;
    let lo = (SCALE / 5) as u16; // 2000 bp
    let shape = if m3 >= half && m1 >= half {
        "limit-cycle"
    } else if m3 >= half {
        "boom-bust"
    } else if m1 >= half && m2 >= half {
        "coexistence"
    } else if m5 >= half {
        "eventful"
    } else if m1 <= lo && m3 <= lo {
        "flat"
    } else {
        "drift"
    };

    // --- event tail: the dominant event family from the fingerprint octave dims (8 boom, 9 crash, 10 takeover,
    // 11 immig). Report the single largest non-zero family as a terse phrase. ---
    let fp = &s.fingerprint;
    let families: [(u16, &str, &str); 4] = [
        (fp[10], "takeover", "takeovers"),
        (fp[8], "boom", "booms"),
        (fp[9], "crash", "crashes"),
        (fp[11], "immigration", "immigrations"),
    ];
    // pick the max-magnitude family deterministically (first wins on a tie — fixed array order).
    let mut best: Option<(u16, &str, &str)> = None;
    for &fam in &families {
        if fam.0 > 0 && best.map(|b| fam.0 > b.0).unwrap_or(true) {
            best = Some(fam);
        }
    }
    // Translate the octave magnitude back into a small count word via the same octave grid the fingerprint uses
    // (it is octave_log_bp(count) rescaled to SCALE). We don't have the exact count, so report the family with a
    // qualitative magnitude bucket: a present family reads as its plural with a magnitude tier from the bp.
    let event = match best {
        // magnitude tier from the octave bp: any positive bp means ≥1 event of that family; a strong (≥half-
        // SCALE, i.e. a few octaves' worth) reading reads as "many <plural>", otherwise the terse plural.
        Some((mag, _sing, plural)) if mag >= half => format!("many {plural}"),
        Some((_, _sing, plural)) => plural.to_string(),
        None => "steady".to_string(),
    };

    format!("{shape} · {spp} spp · {event}")
}

/// A bounded, deduped library of the top-K gems by final `score`. Insertion is deterministic + order-independent
/// of the call sequence: a candidate too close to a kept gem (`nn < dedup_min`) is REJECTED; otherwise it is
/// inserted and the set is trimmed to the best `keep` by `(score desc, recorded_hash asc, master_seed asc)`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GemLibrary {
    /// The kept gems, always sorted best-first by the deterministic ranking key.
    pub gems: Vec<Gem>,
    /// Top-K cut: at most this many gems are retained.
    pub keep: usize,
    /// Dedup threshold: a candidate whose nearest-neighbour fingerprint L1 distance is `< dedup_min` is rejected
    /// as a near-duplicate. `SCALE` by the spec (the pinned `DEDUP_MIN`).
    pub dedup_min: u64,
}

/// The deterministic ranking key: best score first, then lowest `recorded_hash`, then lowest `master_seed`. A
/// total order over distinct gems (recorded_hash + seed break any score tie), so the kept set is unique +
/// insertion-order-independent.
fn rank_key(g: &Gem) -> (core::cmp::Reverse<u64>, u64, u64) {
    (
        core::cmp::Reverse(g.score),
        g.recorded_hash,
        g.config.master_seed,
    )
}

impl GemLibrary {
    /// A fresh library keeping the top-`keep` with the spec's pinned `dedup_min = SCALE`.
    #[must_use]
    pub fn new(keep: usize) -> Self {
        GemLibrary {
            gems: Vec::new(),
            keep,
            dedup_min: SCALE,
        }
    }

    /// A library with an explicit `dedup_min` (for tuning / tests).
    #[must_use]
    pub fn with_dedup(keep: usize, dedup_min: u64) -> Self {
        GemLibrary {
            gems: Vec::new(),
            keep,
            dedup_min,
        }
    }

    /// The currently-kept fingerprints, in `gems` order (for novelty scoring of the next candidate).
    #[must_use]
    pub fn fingerprints(&self) -> Vec<[u16; FP_DIMS]> {
        self.gems.iter().map(|g| g.fingerprint).collect()
    }

    /// Consider a candidate gem. Returns `true` iff it was kept (inserted, possibly evicting a weaker gem).
    ///
    /// Rules (deterministic): (0) an EXACT-record duplicate (a gem with the same [`rank_key`] — same score,
    /// recorded_hash, and master_seed — already present) is idempotently rejected, so re-considering the same
    /// gem never grows the set (keeps `consider` order-independent over a multiset of inputs). (1) measure
    /// `nn = novelty_l1(candidate.fp, kept fps)`; if `nn < dedup_min` REJECT (near-duplicate of an existing
    /// gem). (2) Otherwise insert, re-sort by [`rank_key`], and trim to `keep`. Returns whether the candidate
    /// survived the cut.
    pub fn consider(&mut self, candidate: Gem) -> bool {
        if self.keep == 0 {
            return false;
        }
        let cand_key = rank_key(&candidate);
        // (0) idempotent on an exact-record duplicate (full ranking key already kept).
        if self.gems.iter().any(|g| rank_key(g) == cand_key) {
            return false;
        }
        let nn = novelty_l1(&candidate.fingerprint, &self.fingerprints());
        if nn < self.dedup_min {
            return false;
        }
        self.gems.push(candidate);
        // Deterministic total-order sort (no HashMap; stable key with full tie-break).
        self.gems.sort_by_key(rank_key);
        if self.gems.len() > self.keep {
            self.gems.truncate(self.keep);
        }
        // The candidate was kept iff a gem with its exact ranking key is still present after the trim.
        self.gems.iter().any(|g| rank_key(g) == cand_key)
    }

    /// The number of gems currently kept.
    #[must_use]
    pub fn len(&self) -> usize {
        self.gems.len()
    }

    /// Whether the library is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.gems.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fp_const(v: u16) -> [u16; FP_DIMS] {
        [v; FP_DIMS]
    }

    fn gem_with(score: u64, fp: [u16; FP_DIMS], hash: u64, seed: u64) -> Gem {
        Gem {
            config: SearchConfig {
                master_seed: seed,
                roster: vec![("default".to_string(), 100)],
                containment_level: 0,
                temp_q: 500,
                season: 0,
            },
            score,
            quality: score,
            novelty: 0,
            breakdown: [0; 6],
            fingerprint: fp,
            recorded_hash: hash,
            build_id: "test-build".to_string(),
            caption: "x".to_string(),
            gens: 200,
        }
    }

    // ---- propose determinism ----

    #[test]
    fn propose_is_byte_identical_for_same_seed_trial() {
        let space = SearchSpace::default();
        let a = propose(42, 7, &space);
        let b = propose(42, 7, &space);
        assert_eq!(a, b, "same (seed,trial) must produce byte-identical config");
    }

    #[test]
    fn propose_differs_across_trials() {
        let space = SearchSpace::default();
        let base = propose(42, 0, &space);
        // Across a swath of trials, the vast majority differ from trial 0 (independent draws).
        let mut differ = 0;
        for trial in 1..=64 {
            if propose(42, trial, &space) != base {
                differ += 1;
            }
        }
        assert!(
            differ >= 60,
            "different trials should generally differ from trial 0 (got {differ}/64)"
        );
    }

    #[test]
    fn propose_differs_across_seeds() {
        let space = SearchSpace::default();
        let a = propose(1, 5, &space);
        let b = propose(2, 5, &space);
        assert_ne!(a, b, "different search seeds should shift the config");
    }

    #[test]
    fn propose_respects_space_bounds() {
        let space = SearchSpace::default();
        for trial in 0..256u64 {
            let cfg = propose(123, trial, &space);
            assert_eq!(cfg.roster.len(), space.species.len());
            for (axis, (key, count)) in space.species.iter().zip(cfg.roster.iter()) {
                assert_eq!(key, &axis.key, "roster order/key must match the space");
                assert!(
                    *count >= axis.count_lo && *count <= axis.count_hi,
                    "{key} count {count} out of [{},{}]",
                    axis.count_lo,
                    axis.count_hi
                );
            }
            assert!(
                cfg.containment_level >= space.containment_lo
                    && cfg.containment_level <= space.containment_hi
            );
            assert!(cfg.temp_q >= space.temp_lo && cfg.temp_q <= space.temp_hi);
            assert!(cfg.season >= space.season_lo && cfg.season <= space.season_hi);
        }
    }

    #[test]
    fn propose_covers_the_range() {
        // Over many trials, draws should span a good fraction of each range (not collapse to a constant).
        let space = SearchSpace::default();
        let mut min_c = u32::MAX;
        let mut max_c = 0u32;
        let mut seen_cont = [false; 4];
        let mut seen_season = [false; 4];
        for trial in 0..512u64 {
            let cfg = propose(9, trial, &space);
            let c = cfg.roster[0].1; // "default" count in [200,1200]
            min_c = min_c.min(c);
            max_c = max_c.max(c);
            seen_cont[cfg.containment_level as usize] = true;
            seen_season[cfg.season as usize] = true;
        }
        assert!(
            max_c - min_c > 800,
            "count range too narrow: {min_c}..{max_c}"
        );
        assert!(
            seen_cont.iter().all(|&b| b),
            "not all containment levels seen"
        );
        assert!(seen_season.iter().all(|&b| b), "not all seasons seen");
    }

    #[test]
    fn in_range_degenerate_axis_collapses() {
        assert_eq!(in_range_u64(0, 5, 5), 5);
        assert_eq!(in_range_u64(u64::MAX, 5, 5), 5);
        assert_eq!(in_range_u64(u64::MAX, 7, 3), 7); // lo > hi → lo
    }

    // ---- caption stability ----

    fn scorevec(breakdown: [u16; 6], fp: [u16; FP_DIMS]) -> ScoreVec {
        ScoreVec {
            quality: 0,
            breakdown,
            fingerprint: fp,
        }
    }

    #[test]
    fn caption_is_stable_and_reads_the_signals() {
        let cfg = SearchConfig {
            master_seed: 1,
            roster: vec![
                ("default".to_string(), 800),
                ("ecoli".to_string(), 250),
                ("bdellovibrio".to_string(), 50),
            ],
            containment_level: 0,
            temp_q: 500,
            season: 0,
        };
        // limit-cycle: high m3 + high m1; takeover-dominated fingerprint (dim 10).
        let mut fp = [0u16; FP_DIMS];
        fp[10] = 9000;
        let sv = scorevec([6000, 4000, 7000, 3000, 2000, 9000], fp);
        let c1 = caption(&sv, &cfg);
        let c2 = caption(&sv, &cfg);
        assert_eq!(c1, c2, "caption must be deterministic");
        assert!(c1.starts_with("limit-cycle"), "got: {c1}");
        assert!(c1.contains("3 spp"), "got: {c1}");
        assert!(c1.contains("takeover"), "got: {c1}");

        // flat monoculture-ish: everything low.
        let flat = caption(&scorevec([500, 200, 300, 0, 0, 100], [0; FP_DIMS]), &cfg);
        assert!(flat.starts_with("flat"), "got: {flat}");
        assert!(flat.contains("steady"), "got: {flat}");
    }

    #[test]
    fn caption_counts_only_positive_roster() {
        let cfg = SearchConfig {
            master_seed: 1,
            roster: vec![
                ("default".to_string(), 800),
                ("ecoli".to_string(), 0), // zero-count species not counted
                ("bacillus".to_string(), 50),
            ],
            containment_level: 0,
            temp_q: 500,
            season: 0,
        };
        let c = caption(&scorevec([6000, 6000, 1000, 0, 0, 100], [0; FP_DIMS]), &cfg);
        assert!(c.contains("2 spp"), "got: {c}");
    }

    // ---- GemLibrary: top-K, dedup, order-independence ----

    #[test]
    fn library_keeps_top_k_by_score() {
        let mut lib = GemLibrary::with_dedup(3, 0); // dedup off — test the K cut alone
                                                    // distinct fingerprints so nothing is a duplicate; varied scores.
        for (i, score) in [10u64, 50, 30, 70, 20, 60].iter().enumerate() {
            lib.consider(gem_with(
                *score,
                fp_const(i as u16 * 100),
                i as u64,
                i as u64,
            ));
        }
        assert_eq!(lib.len(), 3);
        let scores: Vec<u64> = lib.gems.iter().map(|g| g.score).collect();
        assert_eq!(scores, vec![70, 60, 50], "top-3 by score, best first");
    }

    #[test]
    fn library_rejects_duplicate_fingerprint() {
        let mut lib = GemLibrary::new(8); // dedup_min = SCALE
        assert!(lib.consider(gem_with(100, fp_const(1000), 1, 1)));
        // identical fingerprint → nn = 0 < SCALE → rejected even with a higher score.
        assert!(!lib.consider(gem_with(999, fp_const(1000), 2, 2)));
        assert_eq!(lib.len(), 1);
        // a fingerprint just inside the dedup ball (L1 < SCALE) is also rejected.
        let mut near = fp_const(1000);
        near[0] = near[0].wrapping_add(100); // L1 distance 100 < SCALE
        assert!(!lib.consider(gem_with(999, near, 3, 3)));
        // a fingerprint far enough out (L1 >= SCALE) is accepted.
        let mut far = fp_const(1000);
        far[0] = far[0].wrapping_add(SCALE as u16); // L1 distance == SCALE
        assert!(lib.consider(gem_with(50, far, 4, 4)));
        assert_eq!(lib.len(), 2);
    }

    #[test]
    fn library_final_set_is_insertion_order_independent() {
        // Build a pool of distinct-fingerprint gems and feed them in several permutations; the kept set + order
        // must be identical (deterministic top-K + tie-break).
        let pool: Vec<Gem> = (0..8)
            .map(|i| {
                gem_with(
                    [15u64, 80, 40, 80, 25, 80, 5, 99][i],
                    fp_const(i as u16 * 500),
                    (i as u64) * 7,
                    i as u64,
                )
            })
            .collect();

        let mut orders = vec![
            vec![0, 1, 2, 3, 4, 5, 6, 7],
            vec![7, 6, 5, 4, 3, 2, 1, 0],
            vec![3, 1, 4, 7, 0, 6, 2, 5],
            vec![5, 5, 1, 1, 7, 7, 0, 0, 2, 3, 4, 6], // with repeats
        ];

        let mut canonical: Option<Vec<(u64, u64, u64)>> = None;
        for order in orders.drain(..) {
            let mut lib = GemLibrary::with_dedup(4, 0); // dedup off; pure K + tie-break
            for &i in &order {
                lib.consider(pool[i].clone());
            }
            let snapshot: Vec<(u64, u64, u64)> = lib
                .gems
                .iter()
                .map(|g| (g.score, g.recorded_hash, g.config.master_seed))
                .collect();
            match &canonical {
                None => canonical = Some(snapshot),
                Some(c) => assert_eq!(c, &snapshot, "kept set must be insertion-order independent"),
            }
        }
        // The three score-80 gems tie; the tie-break is (recorded_hash asc, seed asc). Indices 1,3,5 → hashes
        // 7,21,35 → all kept (top-4 = three 80s + the 99? no, 99 is score, indices: score 99 at index 7).
        let kept = canonical.unwrap();
        assert_eq!(kept.len(), 4);
        // best first: score 99 (idx7), then the three 80s ordered by recorded_hash asc (idx1 h7, idx3 h21, idx5 h35).
        assert_eq!(kept[0].0, 99);
        assert_eq!(kept[1], (80, 7, 1));
        assert_eq!(kept[2], (80, 21, 3));
        assert_eq!(kept[3], (80, 35, 5));
    }

    #[test]
    fn library_zero_keep_rejects_all() {
        let mut lib = GemLibrary::new(0);
        assert!(!lib.consider(gem_with(100, fp_const(1), 1, 1)));
        assert!(lib.is_empty());
    }

    #[test]
    fn library_clone_eq_is_stable() {
        let mut lib = GemLibrary::new(4);
        lib.consider(gem_with(100, fp_const(1000), 1, 1));
        lib.consider(gem_with(50, fp_const(5000), 2, 2));
        // Clone + Eq is the determinism harness for the kept set (no I/O dependency in this crate's tests).
        assert_eq!(lib, lib.clone());
    }
}
