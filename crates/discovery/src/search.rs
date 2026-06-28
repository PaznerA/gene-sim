//! D2a/D2b STAGE 1 — the SEARCH types: the config / proposal / gem data model + the EVOLUTIONARY operators
//! (NO engine).
//!
//! ## Boundary (inv #1/#5)
//! std + serde ONLY — exactly like the rest of `discovery`. A [`SearchConfig`] is a DETERMINISTIC, serializable
//! DESCRIPTION of one headless run (roster + env + containment); it carries NO `sim-core` / `harness` types, so
//! the actual capture/replay engine (D2b) lives on the other side of the seam and consumes a plain config.
//!
//! ## Determinism (inv #3)
//! The proposal sampler [`propose`] and the evolutionary operators [`mutate`] / [`crossover`] /
//! [`propose_evolved`] all use a std-only splitmix64 integer hash of `(search_seed, step/trial, field)` — NO
//! `rand` / `rand_chacha` crate, NO thread-local/global RNG. Same `(search_seed, step)` → byte-identical
//! [`SearchConfig`]. The [`GemLibrary`] keep/dedup logic is pure integer + ordered (`Vec`, no `HashMap`
//! iteration), with a fully-specified deterministic tie-break — so the kept set is order-independent of
//! insertion. Captions are derived purely from the integer score signals (inv #2 — no biology).
//!
//! ## Diverse communities (D2b)
//! [`SearchSpace::default`] spans ~7 FREE-LIVING species axes, each with a per-species PRESENCE draw
//! ([`SpeciesAxis::include_bp`]) so proposed rosters differ in the species MIX — the search explores diverse
//! COMMUNITIES, not just count-tweaks of one fixed set. A roster that draws all-absent falls back to the
//! autotroph (the first axis) so a run is never empty.

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
    /// The mid-run CRISPR edit schedule (Variant Lab D) — a list of [`EditGene`] DESCRIPTIONS the replay
    /// engine fires during the run. `#[serde(default, skip_serializing_if)]` makes this field HASH-NEUTRAL:
    /// - an OLD gem/config written WITHOUT the field deserializes to an EMPTY schedule (no migration), and
    /// - an EMPTY schedule serializes to NO `edits` key at all (skip-if-empty), so the [`EvalRecord`]/[`Gem`]
    ///   JSONL bytes are BYTE-IDENTICAL to pre-D logs.
    ///
    /// EMPTY whenever [`SearchSpace::edit_budget`] is `0` (the default search): [`propose`]/[`mutate`] draw
    /// ZERO edit fields, so the entire config — and the eval log — is byte-identical to before this slice.
    /// Edits enter ONLY when a caller raises `edit_budget`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edits: Vec<EditGene>,
}

/// One scheduled mid-run CRISPR edit — a SERIALIZABLE DESCRIPTION of a single edit, carrying NO
/// `sim-core`/`harness`/`genome`/`crispr` type so it stays on the clean std+serde side of the seam, EXACTLY
/// like [`SearchConfig`] (inv #1/#5). The Variant-Lab-E replay engine maps each gene onto a
/// `harness::Action::ApplyEdit(EditAction { target: genome::LocusId(target), guide, species })` — the fields
/// here are the bare integer / string reprs of that `EditAction` so this crate needs no biology dep (inv #2).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditGene {
    /// WHEN the edit fires, as a q16 FRACTION of the run: the absolute generation is
    /// `gen * gens / `[`EDIT_GEN_Q16_DEN`]` (`gen ∈ [0, EDIT_GEN_Q16_DEN)` ↔ `0.0..<1.0`). SPAN-INDEPENDENT —
    /// the gem replays at the same RELATIVE point regardless of the configured run length, and (crucially)
    /// [`propose`]/[`mutate`] never need the run length, so their 3-arg signatures (and every existing draw)
    /// are unchanged. The mapped generation is always `< gens` for `gens ≥ 1`.
    pub gen: u32,
    /// WHICH species' genome to edit: an index into the proposed [`SearchConfig::roster`] (operator/species
    /// granularity, inv #6 — never a per-organism handle). The replay engine resolves it against the run's
    /// species (`< roster.len()` by construction).
    pub species_index: u16,
    /// The genome locus to target — the bare [`genome::LocusId`] integer (a plain `u32` so this crate carries
    /// NO genome dep). Drawn in `[0, `[`EDIT_TARGET_LOCI`]`)`; the engine resolves/clamps it against the chosen
    /// species' ACTUAL loci count at replay (genomes range from a few to ~136 loci — this crate is
    /// genome-agnostic, inv #2/#5).
    pub target: u32,
    /// The guide (spacer) as a validated-ACGT string ([`EDIT_GUIDE_LEN`] bases) — the engine rebuilds it into a
    /// `crispr::GuideSequence` (which would reject a non-ACGT base, but [`draw_guide`] only emits `A/C/G/T`).
    pub guide: String,
}

/// One species axis of the search: its key/stem, the inclusive `[lo, hi]` starting-count range to draw from
/// when PRESENT, and the per-species presence probability ([`include_bp`](Self::include_bp)). The presence draw
/// is the D2b "diverse communities" knob — when a species is ABSENT its roster count is `0` (the engine drops
/// zero-count entries), so different proposals differ in the species MIX, not just the counts of a fixed set.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpeciesAxis {
    /// Species key/stem (matches the roster key consumed by the engine: `default`/`ecoli`/`bacillus`/...).
    pub key: String,
    /// Inclusive minimum starting count (when present).
    pub count_lo: u32,
    /// Inclusive maximum starting count (when present).
    pub count_hi: u32,
    /// Presence probability in basis points (`0..=SCALE` ↔ `0%..=100%`): the species is PRESENT in a proposed
    /// roster iff its presence draw is `< include_bp`. `SCALE` → always present (e.g. the anchor autotroph);
    /// a lower value lets the species drop out, so rosters explore DIFFERENT community mixes (D2b).
    pub include_bp: u16,
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
    /// MAX number of mid-run edits a proposed config may schedule (Variant Lab D). THE HASH-NEUTRALITY KNOB:
    /// `0` (the [`Default`]) → [`propose`]/[`mutate`] draw ZERO edit fields and emit `edits: vec![]`, so the
    /// default search — and every existing eval-log byte — is unchanged. A caller raising this above `0` turns
    /// the mid-run-edit axis ON; each proposal then draws `[0, edit_budget]` [`EditGene`]s from field words
    /// allocated AFTER the env knobs (so the count/presence/env draws never shift).
    pub edit_budget: u8,
}

impl Default for SearchSpace {
    fn default() -> Self {
        // D2b WIDENED space: ~7 FREE-LIVING species axes (the host-dependent symbionts carsonella/syn3 are
        // EXCLUDED — they cannot persist alone). The Primordial autotroph (`default`) anchors every roster
        // (include_bp = SCALE → always present, so a run is never empty); the other six draw their PRESENCE
        // per-proposal (include_bp < SCALE), so proposed rosters differ in the species MIX — the search
        // explores DIVERSE communities, not just count tweaks of one fixed set. Count ranges are BROADER than
        // the D2a narrow space. Order is FIXED (drives the deterministic roster + field order — never reorder
        // or stored configs stop reproducing).
        let scale = SCALE as u16;
        SearchSpace {
            species: vec![
                // The autotroph anchor — ALWAYS present (the producer base of any community).
                SpeciesAxis {
                    key: "default".to_string(),
                    count_lo: 100,
                    count_hi: 1500,
                    include_bp: scale,
                },
                SpeciesAxis {
                    key: "ecoli".to_string(),
                    count_lo: 30,
                    count_hi: 800,
                    include_bp: 7_000,
                },
                SpeciesAxis {
                    key: "bacillus".to_string(),
                    count_lo: 20,
                    count_hi: 600,
                    include_bp: 6_000,
                },
                SpeciesAxis {
                    key: "pseudomonas".to_string(),
                    count_lo: 20,
                    count_hi: 600,
                    include_bp: 5_500,
                },
                SpeciesAxis {
                    key: "staph".to_string(),
                    count_lo: 20,
                    count_hi: 500,
                    include_bp: 5_000,
                },
                SpeciesAxis {
                    key: "aspergillus-niger".to_string(),
                    count_lo: 10,
                    count_hi: 400,
                    include_bp: 4_500,
                },
                // The predator — present less often (a top-level consumer is rarer / more fragile).
                SpeciesAxis {
                    key: "bdellovibrio".to_string(),
                    count_lo: 5,
                    count_hi: 300,
                    include_bp: 4_000,
                },
            ],
            containment_lo: 0,
            containment_hi: 3,
            // temp 0.15..=0.85 (q16 permille) — a livable band, wider than the D2a 0.20..=0.80.
            temp_lo: 150,
            temp_hi: 850,
            // all four seasons.
            season_lo: 0,
            season_hi: 3,
            // NO mid-run edits by default — the hash-neutral knob. propose/mutate emit `edits: vec![]`, so the
            // default search + every existing eval-log byte is byte-identical to pre-Variant-Lab-D.
            edit_budget: 0,
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

// ── Field-index allocation (FIXED — never reorder, or stored configs stop reproducing) ──────────────────────
//   0              → master_seed
//   1 + 2*i        → species i count       (i in 0..N)
//   2 + 2*i        → species i presence    (i in 0..N)
//   1 + 2*N        → containment
//   2 + 2*N        → temp
//   3 + 2*N        → season
//   4 + 2*N        → edit COUNT          (Variant Lab D — drawn on the EDIT_SALT stream only)
//   5 + 2*N + 4*k  → edit k gen
//   6 + 2*N + 4*k  → edit k species_index
//   7 + 2*N + 4*k  → edit k target
//   8 + 2*N + 4*k  → edit k guide-seed
//
// The edit words live AFTER season + are drawn on a DISTINCT EDIT_SALT stream, so adding the mid-run-edit axis
// shifts NO existing draw: with `edit_budget == 0` the edit fields are never touched and the rest of the config
// (master_seed / roster / env) is byte-identical to pre-D.
//
// The evolutionary operators ([`mutate`]) reuse the SAME per-field stream coordinates (offset by a per-operator
// base so a mutation step is independent of a propose at the same index) — kept in these helpers so the count /
// presence / env coordinates have ONE definition.

/// Field word for species `i`'s starting count.
#[inline]
fn fi_count(i: usize) -> u64 {
    1 + 2 * i as u64
}
/// Field word for species `i`'s presence draw.
#[inline]
fn fi_presence(i: usize) -> u64 {
    2 + 2 * i as u64
}
/// Field word for the containment knob (after the `2*N` per-species words).
#[inline]
fn fi_containment(n: usize) -> u64 {
    1 + 2 * n as u64
}
/// Field word for the temperature knob.
#[inline]
fn fi_temp(n: usize) -> u64 {
    2 + 2 * n as u64
}
/// Field word for the season knob.
#[inline]
fn fi_season(n: usize) -> u64 {
    3 + 2 * n as u64
}

// ── Mid-run edit axis (Variant Lab D) ─────────────────────────────────────────────────────────────────────────
//
// The edit schedule is drawn on a stream SALTED by `EDIT_SALT` (XORed into the operator salt) so it is disjoint
// from the count/presence/env draws AND from the other operators — adding the axis perturbs nothing. With
// `edit_budget == 0` (the default) NO edit field is drawn and the schedule is empty (hash-neutral).

/// Stream salt for the edit-schedule draws — XORed into the operator salt so the edit words are disjoint from
/// the count/presence/env words and from each operator's main stream.
const EDIT_SALT: u64 = 0x4564_6974_5363_0004;
/// Stream words consumed per [`EditGene`] (gen, species_index, target, guide-seed).
const EDIT_FIELDS_PER_GENE: u64 = 4;
/// q16 denominator for [`EditGene::gen`]: the fraction is `gen / EDIT_GEN_Q16_DEN` of the run. `gen` is drawn in
/// `[0, EDIT_GEN_Q16_DEN)`, so the mapped generation `gen * gens / EDIT_GEN_Q16_DEN` is always `< gens`.
pub const EDIT_GEN_Q16_DEN: u32 = 1 << 16;
/// Search resolution for [`EditGene::target`]: the bare `LocusId` is drawn in `[0, EDIT_TARGET_LOCI)`. The
/// replay engine resolves/clamps it against the chosen species' ACTUAL loci count (this crate is
/// genome-agnostic, inv #2/#5), so this is a fixed search granularity, not a hard genome bound.
pub const EDIT_TARGET_LOCI: u32 = 32;
/// Length (nt) of a drawn [`EditGene::guide`] — the standard SpCas9 spacer length.
pub const EDIT_GUIDE_LEN: usize = 20;

/// Field word base for edit gene `k`, AFTER the edit COUNT word (`4 + 2*N`). Gene `k` occupies the four words
/// `[base, base+3]` (gen / species_index / target / guide-seed).
#[inline]
fn fi_edit_gene(n: usize, k: usize) -> u64 {
    5 + 2 * n as u64 + k as u64 * EDIT_FIELDS_PER_GENE
}

/// Derive a fixed-length validated-ACGT guide string from one scrambled word: 2 bits per base
/// (`0→A,1→C,2→G,3→T`), re-mixing the word every 32 bases so longer guides keep avalanching. The default
/// [`EDIT_GUIDE_LEN`] (20) needs 40 bits — one word suffices. Always returns `EDIT_GUIDE_LEN` upper-case ACGT
/// bytes, i.e. a string the engine can always rebuild into a `crispr::GuideSequence`.
fn draw_guide(seed: u64) -> String {
    const BASES: [u8; 4] = [b'A', b'C', b'G', b'T'];
    let mut s = String::with_capacity(EDIT_GUIDE_LEN);
    let mut w = seed;
    for i in 0..EDIT_GUIDE_LEN {
        if i > 0 && i % 32 == 0 {
            w = mix64(w);
        }
        let sym = (w >> ((i as u64 % 32) * 2)) & 0b11;
        s.push(BASES[sym as usize] as char);
    }
    s
}

/// Draw the mid-run edit schedule for a proposal/mutation. `edit_draw(field) -> u64` is the operator's
/// EDIT-stream draw (`op_draw(op_salt ^ EDIT_SALT, ..)`), disjoint from the count/presence/env draws. Returns
/// at most `space.edit_budget` genes; an EMPTY `Vec` (drawing NO edit field at all) when the budget is `0` — the
/// hash-neutral default. `roster_len` bounds `species_index`. Every gene is in range: `gen < EDIT_GEN_Q16_DEN`,
/// `species_index < roster_len`, `target < EDIT_TARGET_LOCI`, guide is valid ACGT.
fn draw_edits(
    n: usize,
    roster_len: usize,
    space: &SearchSpace,
    edit_draw: impl Fn(u64) -> u64,
) -> Vec<EditGene> {
    let budget = space.edit_budget as u64;
    if budget == 0 {
        return Vec::new(); // hash-neutral: NO edit field is drawn.
    }
    // edit count in [0, budget] from the dedicated COUNT word (`4 + 2*N`).
    let count_field = 4 + 2 * n as u64;
    let m = in_range_u64(edit_draw(count_field), 0, budget) as usize;
    let roster_len = roster_len.max(1) as u64;
    let mut edits = Vec::with_capacity(m);
    for k in 0..m {
        let base = fi_edit_gene(n, k);
        let gen = in_range_u64(edit_draw(base), 0, u64::from(EDIT_GEN_Q16_DEN - 1)) as u32;
        let species_index = (edit_draw(base + 1) % roster_len) as u16;
        let target = (edit_draw(base + 2) % u64::from(EDIT_TARGET_LOCI)) as u32;
        let guide = draw_guide(edit_draw(base + 3));
        edits.push(EditGene {
            gen,
            species_index,
            target,
            guide,
        });
    }
    edits
}

/// The per-species count when PRESENT, drawn uniformly from `[count_lo, count_hi]`.
#[inline]
fn axis_count(r: u64, axis: &SpeciesAxis) -> u32 {
    in_range_u64(r, u64::from(axis.count_lo), u64::from(axis.count_hi)) as u32
}

/// Whether species `axis` is PRESENT given its presence-draw word `r`: `r mod SCALE < include_bp`. `include_bp ==
/// SCALE` (or anything `>= SCALE`) → always present; `0` → never. Pure integer, deterministic.
#[inline]
fn axis_present(r: u64, axis: &SpeciesAxis) -> bool {
    let p = (r % SCALE) as u16;
    p < axis.include_bp
}

/// Map a roster (which carries ABSENT species as count `0`) so it is never trivially empty: if EVERY entry is
/// `0`, force the first axis (the autotroph anchor) to its `count_lo` (≥1 by construction). Keeps the roster the
/// same length + order (the engine drops the remaining zero-count entries) — the "never an empty roster" rule.
fn ensure_autotroph(roster: &mut [(String, u32)], space: &SearchSpace) {
    if roster.iter().all(|(_, c)| *c == 0) {
        if let (Some(slot), Some(axis)) = (roster.first_mut(), space.species.first()) {
            slot.1 = axis.count_lo.max(1);
        }
    }
}

/// DETERMINISTIC proposal: draw a [`SearchConfig`] from `space` for `(search_seed, trial)`. Same `(search_seed,
/// trial)` → byte-identical config; different `trial`s generally differ. Each field draws from its own
/// `(.., field_index)` stream, so adding a field never perturbs the earlier ones. Each species is first tested
/// for PRESENCE (its `include_bp` draw) and, if absent, contributes count `0` — so proposed rosters differ in
/// the species MIX (D2b diverse communities), not just counts. An all-absent draw falls back to the autotroph
/// (never an empty roster). NO RNG crate — pure splitmix.
#[must_use]
pub fn propose(search_seed: u64, trial: u64, space: &SearchSpace) -> SearchConfig {
    let n = space.species.len();

    // The run's master seed is itself a deterministic draw (full 64-bit word — every run gets its own seed).
    let master_seed = draw(search_seed, trial, 0);

    let mut roster: Vec<(String, u32)> = Vec::with_capacity(n);
    for (i, axis) in space.species.iter().enumerate() {
        let present = axis_present(draw(search_seed, trial, fi_presence(i)), axis);
        let count = if present {
            axis_count(draw(search_seed, trial, fi_count(i)), axis)
        } else {
            0
        };
        roster.push((axis.key.clone(), count));
    }
    ensure_autotroph(&mut roster, space);

    let containment_level = in_range_u64(
        draw(search_seed, trial, fi_containment(n)),
        u64::from(space.containment_lo),
        u64::from(space.containment_hi),
    ) as u8;
    let temp_q = in_range_u64(
        draw(search_seed, trial, fi_temp(n)),
        u64::from(space.temp_lo),
        u64::from(space.temp_hi),
    ) as u16;
    let season = in_range_u64(
        draw(search_seed, trial, fi_season(n)),
        u64::from(space.season_lo),
        u64::from(space.season_hi),
    ) as u8;

    // Mid-run edit schedule (Variant Lab D) — drawn on the EDIT_SALT stream (op_salt = 0 for `propose`), so it
    // never perturbs the count/presence/env draws above. EMPTY when `edit_budget == 0` (the default).
    let edits = draw_edits(n, roster.len(), space, |field| {
        op_draw(EDIT_SALT, search_seed, trial, field)
    });

    SearchConfig {
        master_seed,
        roster,
        containment_level,
        temp_q,
        season,
        edits,
    }
}

// ── Evolutionary operators (D2b) ────────────────────────────────────────────────────────────────────────────
//
// `mutate` perturbs ONE parent; `crossover` recombines TWO. Both are pure functions of `(search_seed, step,
// field)` via the same splitmix `draw`, so re-running with the same `(seed, step)` reproduces the child
// byte-for-byte (inv #3). They allocate a DISTINCT field-base from `propose` (an XOR salt) so a mutate at step
// `s` never collides with a propose at trial `s`. Every produced child is VALID: counts are clamped to the
// axis range, presence is 0/the count, env knobs stay in `[lo, hi]`, and the autotroph fallback holds. The
// child's `master_seed` is freshly DRAWN (a perturbed config is a NEW run — it must not silently inherit the
// parent's recorded hash).

/// Operator stream salts — XORed into `search_seed` so the operators draw from streams disjoint from `propose`
/// and from each other (a mutate at step `s` ≠ a crossover at step `s` ≠ a propose at trial `s`).
const MUTATE_SALT: u64 = 0x4D75_7461_7465_0001;
const CROSS_SALT: u64 = 0x4372_6F73_7300_0002;
const EVOLVE_SALT: u64 = 0x4576_6F6C_7665_0003;

/// A draw for an operator: salts `search_seed`, then reuses the same three-coordinate `draw` so the field-index
/// vocabulary (`fi_count`/`fi_presence`/…) is shared with `propose`.
#[inline]
fn op_draw(salt: u64, search_seed: u64, step: u64, field: u64) -> u64 {
    draw(search_seed ^ salt, step, field)
}

/// Perturb an integer `v` by a bounded signed delta of at most `±max_delta`, clamped to `[lo, hi]`. The delta
/// magnitude + sign come from the scrambled word `r` (deterministic). `max_delta == 0` or a degenerate range →
/// `v` clamped. Used for count + env-knob jitter.
#[inline]
fn perturb(v: u64, r: u64, max_delta: u64, lo: u64, hi: u64) -> u64 {
    let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
    let v = v.clamp(lo, hi);
    if max_delta == 0 || lo == hi {
        return v;
    }
    // delta in [0, max_delta], sign from a fresh bit of the SAME word (high bit, independent of the magnitude).
    let span = max_delta + 1; // inclusive 0..=max_delta
    let mag = ((u128::from(r) * u128::from(span)) >> 64) as u64;
    let down = (r & 1) == 0;
    let out = if down {
        v.saturating_sub(mag)
    } else {
        v.saturating_add(mag)
    };
    out.clamp(lo, hi)
}

/// `MUTATE`: produce a child by perturbing every field of `parent` within `space`. Each count gets a bounded
/// `±delta` (delta = `MUT_COUNT_FRAC` of the axis span), a species occasionally FLIPS present↔absent (prob
/// `MUT_FLIP_BP`), and containment / temp / season are jittered within range. Deterministic over
/// `(search_seed, step)`; every child is in-bounds + non-empty (autotroph fallback). The child's `master_seed`
/// is freshly drawn (a mutated config is a new run).
#[must_use]
pub fn mutate(
    parent: &SearchConfig,
    search_seed: u64,
    step: u64,
    space: &SearchSpace,
) -> SearchConfig {
    /// Count perturbation magnitude as a fraction of the axis span, in bp (10% of the range).
    const MUT_COUNT_FRAC_BP: u64 = 1_000;
    /// Probability (bp) that a species flips present↔absent in a mutation.
    const MUT_FLIP_BP: u64 = 2_000;
    /// Containment / season step magnitude (ordinal ±1).
    const MUT_ORD_DELTA: u64 = 1;
    /// Temp jitter magnitude (q16 permille).
    const MUT_TEMP_DELTA: u64 = 80;

    let n = space.species.len();
    let master_seed = op_draw(MUTATE_SALT, search_seed, step, 0);

    let mut roster: Vec<(String, u32)> = Vec::with_capacity(n);
    for (i, axis) in space.species.iter().enumerate() {
        // The parent's count for this axis (0 if the parent's roster is shorter / key mismatched — operators are
        // robust to a parent built under a different space; we align by index then by key).
        let parent_count = parent
            .roster
            .get(i)
            .filter(|(k, _)| k == &axis.key)
            .map(|(_, c)| u64::from(*c))
            .or_else(|| {
                parent
                    .roster
                    .iter()
                    .find(|(k, _)| k == &axis.key)
                    .map(|(_, c)| u64::from(*c))
            })
            .unwrap_or(0);
        let present_now = parent_count > 0;

        // Flip presence with prob MUT_FLIP_BP (forced present if include_bp == 0 would never allow it — but a
        // flip TO present always uses count_lo.. so it stays valid).
        let flip = (op_draw(MUTATE_SALT, search_seed, step, fi_presence(i)) % SCALE) < MUT_FLIP_BP;
        let present = present_now ^ flip;

        let count = if present {
            let base = if present_now {
                parent_count // perturb the existing count
            } else {
                // a fresh "turn on" lands mid-range so it is a meaningful introduction, not a 1-cell blip.
                u64::from(axis.count_lo) + (u64::from(axis.count_hi) - u64::from(axis.count_lo)) / 2
            };
            let span = u64::from(axis.count_hi).saturating_sub(u64::from(axis.count_lo));
            let max_delta = span * MUT_COUNT_FRAC_BP / SCALE;
            perturb(
                base,
                op_draw(MUTATE_SALT, search_seed, step, fi_count(i)),
                max_delta,
                u64::from(axis.count_lo),
                u64::from(axis.count_hi),
            ) as u32
        } else {
            0
        };
        roster.push((axis.key.clone(), count));
    }
    ensure_autotroph(&mut roster, space);

    let containment_level = perturb(
        u64::from(parent.containment_level),
        op_draw(MUTATE_SALT, search_seed, step, fi_containment(n)),
        MUT_ORD_DELTA,
        u64::from(space.containment_lo),
        u64::from(space.containment_hi),
    ) as u8;
    let temp_q = perturb(
        u64::from(parent.temp_q),
        op_draw(MUTATE_SALT, search_seed, step, fi_temp(n)),
        MUT_TEMP_DELTA,
        u64::from(space.temp_lo),
        u64::from(space.temp_hi),
    ) as u16;
    let season = perturb(
        u64::from(parent.season),
        op_draw(MUTATE_SALT, search_seed, step, fi_season(n)),
        MUT_ORD_DELTA,
        u64::from(space.season_lo),
        u64::from(space.season_hi),
    ) as u8;

    // Mid-run edit schedule (Variant Lab D) — drawn on the `MUTATE_SALT ^ EDIT_SALT` stream so it is disjoint
    // from both mutate's main draws AND propose's edit draws. Span-independent; EMPTY when `edit_budget == 0`.
    // (The child draws a FRESH schedule rather than perturbing the parent's — like its freshly-drawn master_seed,
    // a mutated config is a new run.)
    let edits = draw_edits(n, roster.len(), space, |field| {
        op_draw(MUTATE_SALT ^ EDIT_SALT, search_seed, step, field)
    });

    SearchConfig {
        master_seed,
        roster,
        containment_level,
        temp_q,
        season,
        edits,
    }
}

/// `CROSSOVER`: recombine two parents into a child. For EACH species (by the union of both rosters' keys, in
/// `a`'s order then any `b`-only keys) the count+presence is taken WHOLE from parent `a` or `b` (a per-species
/// coin), and the three env knobs are each picked from one parent. Deterministic over `(search_seed, step)`.
/// `crossover(a, a, ..)` reproduces `a`'s roster (every gene is `a`'s) with a freshly-drawn `master_seed`.
/// The child is in-bounds (parents are assumed valid) + non-empty (autotroph fallback).
#[must_use]
pub fn crossover(a: &SearchConfig, b: &SearchConfig, search_seed: u64, step: u64) -> SearchConfig {
    let master_seed = op_draw(CROSS_SALT, search_seed, step, 0);

    // Build the ordered union of keys: a's order first, then b-only keys (deterministic, no HashMap).
    let mut keys: Vec<&str> = Vec::with_capacity(a.roster.len() + b.roster.len());
    for (k, _) in &a.roster {
        if !keys.contains(&k.as_str()) {
            keys.push(k.as_str());
        }
    }
    for (k, _) in &b.roster {
        if !keys.contains(&k.as_str()) {
            keys.push(k.as_str());
        }
    }

    let count_of = |cfg: &SearchConfig, key: &str| -> u32 {
        cfg.roster
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, c)| *c)
            .unwrap_or(0)
    };

    let mut roster: Vec<(String, u32)> = Vec::with_capacity(keys.len());
    for (i, key) in keys.iter().enumerate() {
        // per-species coin: even word → parent a, odd → parent b.
        let pick_a = (op_draw(CROSS_SALT, search_seed, step, fi_presence(i)) & 1) == 0;
        let count = if pick_a {
            count_of(a, key)
        } else {
            count_of(b, key)
        };
        roster.push(((*key).to_string(), count));
    }
    // env knobs: each from one parent (separate coins; the union length is the field base).
    let n = keys.len();
    let containment_level = if op_draw(CROSS_SALT, search_seed, step, fi_containment(n)) & 1 == 0 {
        a.containment_level
    } else {
        b.containment_level
    };
    let temp_q = if op_draw(CROSS_SALT, search_seed, step, fi_temp(n)) & 1 == 0 {
        a.temp_q
    } else {
        b.temp_q
    };
    let season = if op_draw(CROSS_SALT, search_seed, step, fi_season(n)) & 1 == 0 {
        a.season
    } else {
        b.season
    };

    // Autotroph fallback against the DEFAULT space's anchor key (crossover has no space arg; the first parent's
    // first key is the roster anchor). If all counts are 0, restore the first roster entry to 1.
    if roster.iter().all(|(_, c)| *c == 0) {
        if let Some(slot) = roster.first_mut() {
            slot.1 = 1;
        }
    }

    SearchConfig {
        master_seed,
        roster,
        containment_level,
        temp_q,
        season,
        // Edit recombination is a later slice; a crossover child carries no mid-run edits (hash-neutral — an
        // empty schedule serializes to no `edits` key). Edits ride only via `propose`/`mutate`.
        edits: Vec::new(),
    }
}

/// `PROPOSE_EVOLVED`: deterministically pick an operator over a parent pool. With `0` parents → fall back to a
/// fresh [`propose`] (cold start). With `1` parent → [`mutate`] it. With `≥2` parents → either [`mutate`] one
/// parent or [`crossover`] two, chosen by a `(search_seed, step)` coin; the parents are picked deterministically
/// from the pool by index. The result is a valid in-bounds, non-empty config. NO RNG crate.
#[must_use]
pub fn propose_evolved(
    parents: &[SearchConfig],
    search_seed: u64,
    step: u64,
    space: &SearchSpace,
) -> SearchConfig {
    match parents.len() {
        0 => propose(search_seed, step, space),
        1 => mutate(&parents[0], search_seed, step, space),
        len => {
            // crossover with prob ~2/3, else mutate (drama favours recombination once a pool exists).
            let mode = op_draw(EVOLVE_SALT, search_seed, step, 0) % 3;
            if mode == 0 {
                let i = (op_draw(EVOLVE_SALT, search_seed, step, 1) % (len as u64)) as usize;
                mutate(&parents[i], search_seed, step, space)
            } else {
                let i = (op_draw(EVOLVE_SALT, search_seed, step, 2) % (len as u64)) as usize;
                // pick a DISTINCT second parent deterministically (offset so it differs from i when len > 1).
                let off =
                    1 + (op_draw(EVOLVE_SALT, search_seed, step, 3) % ((len - 1) as u64)) as usize;
                let j = (i + off) % len;
                crossover(&parents[i], &parents[j], search_seed, step)
            }
        }
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

/// One `(config → ScoreVec)` evaluation record — the raw training row the D3 surrogate trains on. Mirrors
/// [`Gem`] MINUS the novelty/score/caption/build_id/gens fields (those are save-time/keep-time concerns, not
/// part of the raw evaluation). OFF-HASH: built purely from fields [`score_config`] already computes — it
/// draws NO `SimRng`, mutates no sim state, and the pinned literal `0x47a0_3c8f_6701_f240` is untouched (the
/// export is read-only, modelled on `observe_species`/`species_signatures`). Serialized one-per-line as JSONL
/// to `data/runs/evals/<search_seed>.jsonl` (see the harness `--save-evals` flag), in EVALUATION ORDER so the
/// surrogate trains on the sequence of evaluations as they happened.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvalRecord {
    /// The reproducible run description (master_seed + roster + env).
    pub config: SearchConfig,
    /// Gated combined quality `Q ∈ [0, SCORE_SCALE]` (pre-novelty).
    pub quality: u64,
    /// The six metric values `[m1, m2, m3, m4, m5, m6]` (explainability).
    pub breakdown: [u16; 6],
    /// The 12-dim novelty fingerprint (PINNED order).
    pub fingerprint: [u16; FP_DIMS],
    /// The `hash_world` the run produced — the byte-identical-replay contract anchor (inv #3).
    pub recorded_hash: u64,
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
                edits: Vec::new(),
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
                // A species is either ABSENT (count 0) or PRESENT with a count in [count_lo, count_hi].
                assert!(
                    *count == 0 || (*count >= axis.count_lo && *count <= axis.count_hi),
                    "{key} count {count} not 0 nor in [{},{}]",
                    axis.count_lo,
                    axis.count_hi
                );
            }
            // The autotroph anchor (always present, include_bp = SCALE) is NEVER absent — a run is never empty.
            assert!(
                cfg.roster[0].1 > 0,
                "autotroph anchor must always be present"
            );
            assert!(
                cfg.roster.iter().any(|(_, c)| *c > 0),
                "roster must never be all-absent"
            );
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
            edits: Vec::new(),
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
            edits: Vec::new(),
        };
        let c = caption(&scorevec([6000, 6000, 1000, 0, 0, 100], [0; FP_DIMS]), &cfg);
        assert!(c.contains("2 spp"), "got: {c}");
    }

    // ---- D2b: widened space — diverse community mixes ----

    /// The set of PRESENT species keys of a config (its "roster shape"), in roster order (owned, so it can be
    /// stored past the config's lifetime).
    fn roster_shape(cfg: &SearchConfig) -> Vec<String> {
        cfg.roster
            .iter()
            .filter(|(_, c)| *c > 0)
            .map(|(k, _)| k.clone())
            .collect()
    }

    #[test]
    fn widened_space_has_seven_free_living_axes() {
        let space = SearchSpace::default();
        assert_eq!(space.species.len(), 7, "expected ~7 free-living axes");
        let keys: Vec<&str> = space.species.iter().map(|a| a.key.as_str()).collect();
        // free-living set; host-dependent symbionts (carsonella/syn3) are EXCLUDED.
        assert!(keys.contains(&"default"));
        assert!(keys.contains(&"pseudomonas"));
        assert!(keys.contains(&"staph"));
        assert!(keys.contains(&"aspergillus-niger"));
        assert!(
            !keys.contains(&"carsonella"),
            "host-dependent symbiont must be excluded"
        );
        assert!(
            !keys.contains(&"syn3"),
            "host-dependent symbiont must be excluded"
        );
        // the autotroph anchor is always present; everything else can drop out.
        assert_eq!(space.species[0].key, "default");
        assert_eq!(space.species[0].include_bp, SCALE as u16);
        assert!(space.species[1..]
            .iter()
            .all(|a| a.include_bp < SCALE as u16));
    }

    #[test]
    fn widened_propose_yields_diverse_species_mixes() {
        // The KEY D2b fix: proposed configs differ in the species MIX, not just counts of one fixed set. Count
        // the number of DISTINCT rosters (present-key shapes) over a swath of trials — must be many.
        let space = SearchSpace::default();
        let mut shapes: Vec<Vec<String>> = Vec::new();
        for trial in 0..256u64 {
            let cfg = propose(7, trial, &space);
            let shape = roster_shape(&cfg);
            if !shapes.contains(&shape) {
                shapes.push(shape);
            }
        }
        // With 6 optional species there are 2^6 = 64 possible shapes; we should see a large fraction. The OLD
        // narrow space (all 4 species always present, count_lo > 0) produced exactly ONE shape.
        assert!(
            shapes.len() >= 16,
            "widened propose should explore many distinct rosters, got {} distinct shapes",
            shapes.len()
        );
        // every shape includes the autotroph anchor + is non-empty.
        for s in &shapes {
            assert!(!s.is_empty(), "no empty roster");
            assert!(
                s.iter().any(|k| k == "default"),
                "autotroph present in every roster: {s:?}"
            );
        }
    }

    // ---- D2b: evolutionary operators ----

    fn parent_a() -> SearchConfig {
        // a hand-built mid-range parent (present subset, valid env).
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

    fn parent_b() -> SearchConfig {
        SearchConfig {
            master_seed: 0xC0FF_EE00,
            roster: vec![
                ("default".to_string(), 900),
                ("ecoli".to_string(), 0),
                ("bacillus".to_string(), 300),
                ("pseudomonas".to_string(), 0),
                ("staph".to_string(), 250),
                ("aspergillus-niger".to_string(), 80),
                ("bdellovibrio".to_string(), 0),
            ],
            containment_level: 3,
            temp_q: 700,
            season: 2,
            edits: Vec::new(),
        }
    }

    /// Assert a config is in-bounds against `space` (absent → 0; present → in axis range) + non-empty.
    fn assert_valid(cfg: &SearchConfig, space: &SearchSpace) {
        assert!(
            cfg.roster.iter().any(|(_, c)| *c > 0),
            "roster must be non-empty (autotroph fallback)"
        );
        for (k, c) in &cfg.roster {
            if let Some(axis) = space.species.iter().find(|a| &a.key == k) {
                assert!(
                    *c == 0 || (*c >= axis.count_lo && *c <= axis.count_hi),
                    "{k} count {c} not 0 nor in [{},{}]",
                    axis.count_lo,
                    axis.count_hi
                );
            }
        }
        assert!(
            cfg.containment_level >= space.containment_lo
                && cfg.containment_level <= space.containment_hi
        );
        assert!(cfg.temp_q >= space.temp_lo && cfg.temp_q <= space.temp_hi);
        assert!(cfg.season >= space.season_lo && cfg.season <= space.season_hi);
    }

    #[test]
    fn mutate_is_deterministic() {
        let space = SearchSpace::default();
        let p = parent_a();
        let c1 = mutate(&p, 99, 7, &space);
        let c2 = mutate(&p, 99, 7, &space);
        assert_eq!(c1, c2, "same (seed,step) → identical child");
        // a different step generally differs.
        assert_ne!(
            mutate(&p, 99, 8, &space),
            c1,
            "a different step should differ"
        );
    }

    #[test]
    fn mutate_is_valid_and_non_empty_over_many_steps() {
        let space = SearchSpace::default();
        let p = parent_a();
        for step in 0..512u64 {
            assert_valid(&mutate(&p, 3, step, &space), &space);
        }
    }

    #[test]
    fn mutate_changes_the_config_under_some_steps() {
        let space = SearchSpace::default();
        let p = parent_a();
        let mut differ = 0;
        for step in 0..256u64 {
            let child = mutate(&p, 5, step, &space);
            // compare the run-defining parts (master_seed is always freshly drawn, so exclude it).
            let same_body = child.roster == p.roster
                && child.containment_level == p.containment_level
                && child.temp_q == p.temp_q
                && child.season == p.season;
            if !same_body {
                differ += 1;
            }
        }
        assert!(
            differ > 200,
            "mutation should change the config under most steps, got {differ}/256"
        );
    }

    #[test]
    fn mutate_all_absent_parent_keeps_autotroph() {
        let space = SearchSpace::default();
        // a degenerate parent with only the autotroph; flips could turn it off → fallback must restore it.
        let mut p = parent_a();
        for (_, c) in p.roster.iter_mut() {
            *c = 0;
        }
        p.roster[0].1 = 1; // only the autotroph
        for step in 0..512u64 {
            let child = mutate(&p, 11, step, &space);
            assert!(
                child.roster.iter().any(|(_, c)| *c > 0),
                "the autotroph fallback must hold at step {step}"
            );
        }
    }

    #[test]
    fn crossover_is_deterministic() {
        let (a, b) = (parent_a(), parent_b());
        let c1 = crossover(&a, &b, 42, 3);
        let c2 = crossover(&a, &b, 42, 3);
        assert_eq!(c1, c2, "same (seed,step) → identical child");
        assert_ne!(
            crossover(&a, &b, 42, 4),
            c1,
            "a different step should differ"
        );
    }

    #[test]
    fn crossover_is_valid_and_non_empty() {
        let space = SearchSpace::default();
        let (a, b) = (parent_a(), parent_b());
        for step in 0..256u64 {
            assert_valid(&crossover(&a, &b, 7, step), &space);
        }
    }

    #[test]
    fn crossover_of_a_with_a_reproduces_a_roster_and_env() {
        // every gene drawn from a OR a is a's gene → the child's roster + env == a's (only master_seed differs).
        let a = parent_a();
        for step in 0..64u64 {
            let child = crossover(&a, &a, 123, step);
            assert_eq!(child.roster, a.roster, "crossover(a,a) roster must equal a");
            assert_eq!(child.containment_level, a.containment_level);
            assert_eq!(child.temp_q, a.temp_q);
            assert_eq!(child.season, a.season);
        }
    }

    #[test]
    fn crossover_genes_come_from_a_parent() {
        // each child species count must equal a's OR b's count for that key (whole-gene inheritance).
        let (a, b) = (parent_a(), parent_b());
        let count_of = |cfg: &SearchConfig, key: &str| -> u32 {
            cfg.roster
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, c)| *c)
                .unwrap_or(0)
        };
        for step in 0..128u64 {
            let child = crossover(&a, &b, 9, step);
            for (k, c) in &child.roster {
                let ca = count_of(&a, k);
                let cb = count_of(&b, k);
                // allow the autotroph-fallback's forced 1 only when both parents are 0 for that key (won't happen here).
                assert!(
                    *c == ca || *c == cb,
                    "{k} count {c} is neither a({ca}) nor b({cb})"
                );
            }
        }
    }

    #[test]
    fn propose_evolved_dispatches_and_stays_valid() {
        let space = SearchSpace::default();
        let (a, b) = (parent_a(), parent_b());
        let pool = vec![a.clone(), b.clone()];
        for step in 0..256u64 {
            // 0 parents → cold propose
            assert_valid(&propose_evolved(&[], 1, step, &space), &space);
            // 1 parent → mutate
            let m = propose_evolved(std::slice::from_ref(&a), 1, step, &space);
            assert_eq!(
                m,
                mutate(&a, 1, step, &space),
                "single-parent evolve == mutate"
            );
            assert_valid(&m, &space);
            // ≥2 parents → mutate or crossover, always valid
            assert_valid(&propose_evolved(&pool, 1, step, &space), &space);
        }
    }

    #[test]
    fn propose_evolved_is_deterministic() {
        let space = SearchSpace::default();
        let pool = vec![parent_a(), parent_b()];
        let c1 = propose_evolved(&pool, 77, 5, &space);
        let c2 = propose_evolved(&pool, 77, 5, &space);
        assert_eq!(c1, c2, "same (seed,step,pool) → identical child");
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

    // ---- D3-A: EvalRecord serde ----

    fn eval_record(quality: u64, hash: u64, seed: u64) -> EvalRecord {
        EvalRecord {
            config: SearchConfig {
                master_seed: seed,
                roster: vec![("default".to_string(), 100)],
                containment_level: 0,
                temp_q: 500,
                season: 0,
                edits: Vec::new(),
            },
            quality,
            breakdown: [1, 2, 3, 4, 5, 6],
            fingerprint: fp_const(7),
            recorded_hash: hash,
        }
    }

    #[test]
    fn eval_record_round_trips_serde() {
        // JSON round-trip preserves the record exactly (field order is stable in declaration order, so the
        // serialized bytes are byte-stable across runs — the surrogate's training-data contract).
        let rec = eval_record(123_456, 0xDEAD_BEEF, 0x00C0_FFEE);
        let json = serde_json::to_string(&rec).expect("serialize");
        let back: EvalRecord = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(rec, back, "round-trip must preserve all fields");

        // Same record → byte-identical JSON (deterministic field order — inv #3).
        let json2 = serde_json::to_string(&rec).expect("serialize");
        assert_eq!(json, json2, "JSON bytes must be stable across runs");
    }

    #[test]
    fn eval_record_serializes_field_in_declaration_order() {
        // Pin the JSON shape the surrogate trains on (changing the field order invalidates stored logs).
        let rec = eval_record(1, 2, 3);
        let json = serde_json::to_string(&rec).expect("serialize");
        // declaration order: config, quality, breakdown, fingerprint, recorded_hash.
        let expected_prefix = r#"{"config":{"master_seed":3,"roster":[["default",100]],"#;
        assert!(
            json.starts_with(expected_prefix),
            "unexpected JSON shape: {json}"
        );
    }

    // ---- Variant Lab D: mid-run edit axis (edit_budget / EditGene) ----

    /// The default space widened with a mid-run-edit budget (everything else unchanged).
    fn space_with_budget(budget: u8) -> SearchSpace {
        SearchSpace {
            edit_budget: budget,
            ..SearchSpace::default()
        }
    }

    #[test]
    fn default_space_has_zero_edit_budget() {
        // The hash-neutral knob: the default search schedules NO mid-run edits.
        assert_eq!(SearchSpace::default().edit_budget, 0);
    }

    #[test]
    fn default_space_proposes_no_edits_and_omits_the_key() {
        // edit_budget == 0 → propose/mutate draw ZERO edit fields → empty schedule, and an empty schedule
        // serializes with NO `edits` key (so eval-log bytes are byte-identical to pre-Variant-Lab-D).
        let space = SearchSpace::default();
        for trial in 0..64u64 {
            let cfg = propose(42, trial, &space);
            assert!(cfg.edits.is_empty(), "default propose schedules no edits");
            let json = serde_json::to_string(&cfg).expect("serialize");
            assert!(
                !json.contains("edits"),
                "an empty schedule must not serialize an `edits` key: {json}"
            );
        }
        let child = mutate(&parent_a(), 7, 3, &space);
        assert!(child.edits.is_empty(), "default mutate schedules no edits");
    }

    #[test]
    fn raising_budget_does_not_perturb_roster_or_env() {
        // THE byte-identity guarantee: adding the edit axis shifts NO existing draw. For the SAME (seed, trial)
        // the non-edit fields of a budget>0 proposal are byte-identical to the budget==0 default — only `edits`
        // is appended. (The edit words live at NEW field indices on a DISTINCT salt, so the count/presence/env
        // draws never move — every existing propose/mutate test passes unchanged.)
        let base = SearchSpace::default();
        let withb = space_with_budget(5);
        for trial in 0..128u64 {
            let a = propose(99, trial, &base);
            let b = propose(99, trial, &withb);
            assert_eq!(a.master_seed, b.master_seed);
            assert_eq!(a.roster, b.roster, "roster draws must be unperturbed");
            assert_eq!(a.containment_level, b.containment_level);
            assert_eq!(a.temp_q, b.temp_q);
            assert_eq!(a.season, b.season);
            assert!(
                a.edits.is_empty(),
                "the budget==0 config still has no edits"
            );
        }
        let p = parent_a();
        for step in 0..128u64 {
            let a = mutate(&p, 5, step, &base);
            let b = mutate(&p, 5, step, &withb);
            assert_eq!(
                a.roster, b.roster,
                "mutate roster draws must be unperturbed"
            );
            assert_eq!(a.containment_level, b.containment_level);
            assert_eq!(a.temp_q, b.temp_q);
            assert_eq!(a.season, b.season);
        }
    }

    /// Assert one drawn schedule is deterministic-bounds-valid against `space` (`len <= budget`, every gene in
    /// range, guide a fixed-length ACGT string).
    fn assert_edits_in_bounds(cfg: &SearchConfig, space: &SearchSpace) {
        assert!(
            cfg.edits.len() <= space.edit_budget as usize,
            "schedule length {} exceeds budget {}",
            cfg.edits.len(),
            space.edit_budget
        );
        for e in &cfg.edits {
            assert!(e.gen < EDIT_GEN_Q16_DEN, "gen {} not a q16 fraction", e.gen);
            assert!(
                (e.species_index as usize) < cfg.roster.len(),
                "species_index {} out of roster (len {})",
                e.species_index,
                cfg.roster.len()
            );
            assert!(
                e.target < EDIT_TARGET_LOCI,
                "target {} out of locus search range",
                e.target
            );
            assert_eq!(
                e.guide.len(),
                EDIT_GUIDE_LEN,
                "guide is the fixed spacer length"
            );
            assert!(
                e.guide
                    .bytes()
                    .all(|c| matches!(c, b'A' | b'C' | b'G' | b'T')),
                "guide {} is not valid ACGT",
                e.guide
            );
        }
    }

    #[test]
    fn budgeted_propose_is_deterministic_and_in_bounds() {
        let space = space_with_budget(4);
        for trial in 0..256u64 {
            let a = propose(7, trial, &space);
            let b = propose(7, trial, &space);
            assert_eq!(a, b, "same (seed,trial) → byte-identical config + edits");
            assert_edits_in_bounds(&a, &space);
        }
        // The axis is LIVE: a positive budget schedules edits on a good fraction of trials, but not all (the
        // count draws [0, budget], so some proposals still schedule zero).
        let scheduled = (0..256u64)
            .filter(|&t| !propose(7, t, &space).edits.is_empty())
            .count();
        assert!(scheduled > 0, "a positive budget must schedule edits");
        assert!(scheduled < 256, "the count draw should sometimes be zero");
    }

    #[test]
    fn budgeted_mutate_is_deterministic_and_in_bounds() {
        let space = space_with_budget(3);
        let p = parent_a();
        for step in 0..256u64 {
            let a = mutate(&p, 11, step, &space);
            let b = mutate(&p, 11, step, &space);
            assert_eq!(a, b, "same (seed,step) → byte-identical config + edits");
            assert_edits_in_bounds(&a, &space);
        }
        let scheduled = (0..256u64)
            .filter(|&s| !mutate(&p, 11, s, &space).edits.is_empty())
            .count();
        assert!(
            scheduled > 0,
            "a positive budget must schedule edits in mutate"
        );
    }

    #[test]
    fn propose_and_mutate_edit_streams_are_disjoint() {
        // The edit schedules of a propose at trial s and a mutate at step s (same seed) draw from DISTINCT salts,
        // so they do not collide — a sanity check that the EDIT_SALT layering keeps the operators independent.
        let space = space_with_budget(4);
        let mut differ = 0;
        for s in 0..128u64 {
            let pe = propose(3, s, &space).edits;
            let me = mutate(&parent_a(), 3, s, &space).edits;
            if pe != me {
                differ += 1;
            }
        }
        assert!(
            differ > 100,
            "propose vs mutate edit schedules should rarely coincide, got {differ}/128 differing"
        );
    }

    #[test]
    fn search_config_with_edits_round_trips_and_serde_defaults() {
        // (1) a config WITH a schedule round-trips byte-stably and serializes the `edits` key.
        let cfg = SearchConfig {
            master_seed: 9,
            roster: vec![("default".to_string(), 100), ("ecoli".to_string(), 50)],
            containment_level: 1,
            temp_q: 400,
            season: 2,
            edits: vec![
                EditGene {
                    gen: 1234,
                    species_index: 1,
                    target: 3,
                    guide: "ACGTACGTACGTACGTACGT".to_string(),
                },
                EditGene {
                    gen: 60_000,
                    species_index: 0,
                    target: 7,
                    guide: "TTTTAAAACCCCGGGGACGT".to_string(),
                },
            ],
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        assert!(
            json.contains(r#""edits""#),
            "a non-empty schedule serializes the key: {json}"
        );
        let back: SearchConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cfg, back, "round-trip must preserve the schedule");

        // (2) an OLD config written WITHOUT the `edits` field deserializes to an empty schedule (serde default),
        // byte-identically — no migration needed.
        let legacy = r#"{"master_seed":9,"roster":[["default",100]],"containment_level":0,"temp_q":500,"season":0}"#;
        let parsed: SearchConfig = serde_json::from_str(legacy).expect("legacy deserialize");
        assert!(
            parsed.edits.is_empty(),
            "a missing `edits` field → empty schedule"
        );

        // (3) an EMPTY schedule serializes to NO `edits` key (eval-log byte-identity with pre-D).
        let empty = SearchConfig {
            edits: Vec::new(),
            ..cfg.clone()
        };
        let ejson = serde_json::to_string(&empty).expect("serialize");
        assert!(
            !ejson.contains("edits"),
            "an empty schedule must omit the key: {ejson}"
        );
    }
}
