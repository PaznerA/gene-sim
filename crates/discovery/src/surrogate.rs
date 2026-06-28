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
}
