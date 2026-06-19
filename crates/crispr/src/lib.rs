//! CRISPR mechanic: Cas-variant table, PAM finding (rust-bio), `Score` traits + in-core default impls,
//! and gated edit application (SPEC §4, §8 Stage 1; TAXONOMY.md §3).
//!
//! Stage 1 starts here with the **Cas-variant table**. Per SPEC §4 the table is *data, not code*: the
//! authoritative rows live in `data/cas_variants.ron` and are loaded into ordered [`CasVariant`] rows.
//! The default table is embedded via `include_str!` so it is hermetic for tests and shippable in the
//! binary, while still being editable as a git-friendly RON file (SPEC §5).
//!
//! Invariants in play: variants are kept in a load-ordered [`Vec`] (determinism, inv. #3); the table is
//! serde-(de)serializable plain data; no GPL dependency (serde + ron are MIT/Apache-2.0, inv. #1).

#![forbid(unsafe_code)]

use bio::alphabets::dna;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Stable, small-integer handle for a [`CasVariant`] (inv. #3 — ids are integers, not hashed strings).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CasVariantId(pub u16);

/// The mechanistic outcome a Cas variant produces at its target (TAXONOMY.md §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EditType {
    /// Double-strand break (classic Cas9/Cas12a nuclease).
    Dsb,
    /// Base editor (e.g. cytosine/adenine deaminase) — edits within an activity window.
    BaseEdit,
    /// Prime editor (nCas9 + reverse transcriptase) — edit window set by the pegRNA.
    Prime,
}

/// A Cas-variant **data row** (TAXONOMY.md §3.1), loaded from `data/cas_variants.ron` (SPEC §4).
///
/// This is plain data — the science (PAM finding, scoring, edit application) lives elsewhere in the
/// crate and consumes these rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CasVariant {
    /// Stable id (equals nothing in particular; just a stable handle for actions/logs).
    pub id: CasVariantId,
    /// Display name, e.g. `"SpCas9"`, `"AsCas12a"`, `"SpRY"`.
    pub name: String,
    /// IUPAC PAM pattern, e.g. `NGG`, `NNGRRT`, `TTTV`, `NG`.
    pub pam: String,
    /// Cut position in bp relative to the PAM (blunt vs PAM-distal/staggered).
    pub cut_offset: i16,
    /// Base-/prime-editor activity window (relative positions); `(0, 0)` for a pure DSB.
    pub edit_window: (i16, i16),
    /// The mechanistic edit type.
    pub edit_type: EditType,
}

/// Error returned when the Cas-variant table cannot be parsed.
#[derive(Debug)]
pub struct LoadError(ron::error::SpannedError);

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to parse cas-variant table: {}", self.0)
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl From<ron::error::SpannedError> for LoadError {
    fn from(e: ron::error::SpannedError) -> Self {
        LoadError(e)
    }
}

/// The embedded default Cas-variant table source (SPEC §4 seed table). Kept as data, embedded so the
/// table ships in the binary and tests are hermetic.
const DEFAULT_TABLE_RON: &str = include_str!("../../../data/cas_variants.ron");

/// Parse a Cas-variant table from a RON string into an ordered [`Vec`] (load order preserved, inv. #3).
///
/// # Errors
/// Returns [`LoadError`] if the RON is malformed or does not match the [`CasVariant`] shape.
pub fn load_cas_variants_from_str(ron: &str) -> Result<Vec<CasVariant>, LoadError> {
    Ok(ron::from_str(ron)?)
}

/// The default, literature-seeded Cas-variant table, parsed from the embedded `data/cas_variants.ron`.
///
/// # Panics
/// Panics only if the *embedded* table is malformed, which is a build-time invariant (covered by tests),
/// never a runtime/user input.
#[must_use]
pub fn default_cas_variants() -> Vec<CasVariant> {
    load_cas_variants_from_str(DEFAULT_TABLE_RON).expect("embedded cas_variants.ron is well-formed")
}

// ---------------------------------------------------------------------------
// PAM finding (slice S1.2) — SPEC §4 step 1, via rust-bio (`bio`, MIT, SPEC §2.2).
// ---------------------------------------------------------------------------

/// Which DNA strand a PAM was found on, relative to the forward sequence handed to the finder.
///
/// `Reverse` matches are detected by searching the reverse complement of the forward sequence
/// (computed with `bio::alphabets::dna::revcomp`); all coordinates are reported back in the
/// **forward sequence frame** so callers never juggle two coordinate systems (inv. #3 determinism).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Strand {
    /// The PAM occurs on the given (forward) sequence as-is.
    Forward,
    /// The PAM occurs on the reverse-complement strand.
    Reverse,
}

/// One PAM occurrence and the cut site it implies for a given [`CasVariant`].
///
/// **Coordinate conventions (all in the forward-sequence frame):**
/// - `position` is the 0-based index of the PAM's **leftmost** base in the forward sequence — i.e.
///   `seq[position .. position + pam_len]` are the bases that matched (after reverse-complementing for
///   a `Reverse` hit). This is uniform across strands so sites sort cleanly by position.
/// - `cut_site` is an **inter-base** coordinate (the nick falls *before* index `cut_site`), derived from
///   the variant's [`CasVariant::cut_offset`], which is measured from the PAM's **5' base** along the
///   protospacer's 5'→3' direction:
///   - `Forward`: 5' base of the PAM is `position`, 5'→3' is increasing index ⇒ `cut_site = position + cut_offset`.
///   - `Reverse`: 5' base of the PAM is the rightmost base `position + pam_len - 1`, and the strand's
///     5'→3' runs toward *decreasing* forward index ⇒ `cut_site = (position + pam_len - 1) - cut_offset`.
///
/// `cut_site` is `i64` because a cut can legitimately fall outside `[0, seq.len()]` (e.g. a PAM near an
/// edge with a PAM-distal offset); callers decide whether an out-of-range cut is usable.
///
/// Worked example — SpCas9 `NGG`, `cut_offset = -3`, blunt cut ~3 bp 5' of the PAM:
/// a forward PAM at `position = 10` cuts at `10 + (-3) = 7`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PamSite {
    /// 0-based index of the PAM's leftmost base in the forward sequence frame.
    pub position: usize,
    /// Strand the PAM was found on.
    pub strand: Strand,
    /// Forward-frame inter-base cut coordinate (see type docs). May be negative or `>= seq.len()`.
    pub cut_site: i64,
}

/// Whether an IUPAC degenerate code `code` matches a concrete DNA base `base`.
///
/// Domain logic layered on top of rust-bio (rust-bio handles alphabets / reverse-complement; IUPAC
/// degeneracy is CRISPR-domain, not a rust-bio reimplementation — SPEC §0.4). Both arguments are
/// compared case-insensitively, though `DnaSequence` is already upper-case ACGT.
///
/// Supports the full IUPAC nucleotide set: `A C G T` (and `U`), plus
/// `R Y S W K M B D H V N` (degenerate). An unrecognized code never matches.
#[must_use]
pub fn iupac_matches(code: u8, base: u8) -> bool {
    let base = base.to_ascii_uppercase();
    // Only concrete bases can be matched against; anything else in `base` is a non-match.
    let base = match base {
        b'A' | b'C' | b'G' => base,
        b'T' | b'U' => b'T',
        _ => return false,
    };
    match code.to_ascii_uppercase() {
        b'A' => base == b'A',
        b'C' => base == b'C',
        b'G' => base == b'G',
        b'T' | b'U' => base == b'T',
        b'R' => matches!(base, b'A' | b'G'),        // puRine
        b'Y' => matches!(base, b'C' | b'T'),        // pYrimidine
        b'S' => matches!(base, b'G' | b'C'),        // Strong
        b'W' => matches!(base, b'A' | b'T'),        // Weak
        b'K' => matches!(base, b'G' | b'T'),        // Keto
        b'M' => matches!(base, b'A' | b'C'),        // aMino
        b'B' => matches!(base, b'C' | b'G' | b'T'), // not A
        b'D' => matches!(base, b'A' | b'G' | b'T'), // not C
        b'H' => matches!(base, b'A' | b'C' | b'T'), // not G
        b'V' => matches!(base, b'A' | b'C' | b'G'), // not T
        b'N' => true,                               // aNy
        _ => false,
    }
}

/// Whether the IUPAC `pam` pattern matches `window` base-for-base (`window.len() == pam.len()`).
fn pam_matches_window(pam: &[u8], window: &[u8]) -> bool {
    pam.len() == window.len()
        && pam
            .iter()
            .zip(window.iter())
            .all(|(&code, &base)| iupac_matches(code, base))
}

/// Find every PAM occurrence for `variant` in `seq`, on **both** strands, with the implied cut site.
///
/// `seq` is the forward strand (upper-case ACGT — e.g. [`genome::DnaSequence::bases`]). The reverse
/// strand is searched via `bio::alphabets::dna::revcomp` and all hits are reported in the forward frame
/// (see [`PamSite`]). The returned [`Vec`] is sorted by `(position, strand)` for determinism (inv. #3):
/// never iterate a `HashMap` to build results.
///
/// An empty PAM pattern, or a PAM longer than `seq`, yields no sites.
#[must_use]
pub fn find_pam_sites(seq: &[u8], variant: &CasVariant) -> Vec<PamSite> {
    let pam = variant.pam.as_bytes();
    let (n, l) = (seq.len(), pam.len());
    if l == 0 || l > n {
        return Vec::new();
    }
    let cut_offset = i64::from(variant.cut_offset);
    let mut sites = Vec::new();

    // Forward strand: scan windows directly.
    for position in 0..=(n - l) {
        if pam_matches_window(pam, &seq[position..position + l]) {
            sites.push(PamSite {
                position,
                strand: Strand::Forward,
                cut_site: position as i64 + cut_offset,
            });
        }
    }

    // Reverse strand: scan the reverse complement, map indices back to the forward frame.
    let rc = dna::revcomp(seq);
    for j in 0..=(n - l) {
        if pam_matches_window(pam, &rc[j..j + l]) {
            // A window [j, j+l) on the revcomp (length n) maps to forward indices [n-l-j, n-1-j];
            // its leftmost forward base is the reported `position`.
            let position = n - l - j;
            // 5' base of the PAM on this strand is the rightmost forward base; 5'→3' decreases index.
            let cut_site = (position + l - 1) as i64 - cut_offset;
            sites.push(PamSite {
                position,
                strand: Strand::Reverse,
                cut_site,
            });
        }
    }

    // Deterministic order: by position, then strand (Forward < Reverse).
    sites.sort_unstable_by(|a, b| a.position.cmp(&b.position).then(a.strand.cmp(&b.strand)));
    sites
}

/// Convenience wrapper accepting a [`genome::DnaSequence`] directly (validated upper-case ACGT).
#[must_use]
pub fn find_pam_sites_in(seq: &genome::DnaSequence, variant: &CasVariant) -> Vec<PamSite> {
    find_pam_sites(seq.bases(), variant)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_links_against_genome() {
        // Confirms the dependency edge compiles; genome data model stays usable from crispr.
        let g = genome::sample_genome();
        assert!(g.is_valid());
    }

    #[test]
    fn default_table_has_at_least_five_variants() {
        let table = default_cas_variants();
        assert!(
            table.len() >= 5,
            "expected >=5 seed variants, got {}",
            table.len()
        );
    }

    #[test]
    fn every_variant_has_nonempty_pam_and_edit_type() {
        for v in default_cas_variants() {
            assert!(!v.pam.is_empty(), "variant {} has an empty PAM", v.name);
            // An EditType is always present (it's a non-optional enum field); assert it is one of the
            // known variants so the field is exercised.
            assert!(
                matches!(
                    v.edit_type,
                    EditType::Dsb | EditType::BaseEdit | EditType::Prime
                ),
                "variant {} has an unexpected edit type",
                v.name
            );
        }
    }

    #[test]
    fn known_pams_match_literature() {
        let table = default_cas_variants();
        let pam_of = |name: &str| {
            table
                .iter()
                .find(|v| v.name == name)
                .unwrap_or_else(|| panic!("seed table is missing {name}"))
                .pam
                .clone()
        };
        assert_eq!(pam_of("SpCas9"), "NGG");
        assert_eq!(pam_of("AsCas12a"), "TTTV");
    }

    #[test]
    fn covers_the_required_edit_types_and_relaxed_pam() {
        let table = default_cas_variants();
        let has_type = |t: EditType| table.iter().any(|v| v.edit_type == t);
        assert!(has_type(EditType::Dsb), "no DSB variant");
        assert!(has_type(EditType::BaseEdit), "no base editor");
        assert!(has_type(EditType::Prime), "no prime editor");

        // A PAM-relaxed variant exists (Cas9-NG "NG" and/or SpRY "NRN").
        assert!(
            table.iter().any(|v| v.pam == "NG" || v.pam == "NRN"),
            "no PAM-relaxed variant (NG / NRN)"
        );

        // The base editor carries a non-zero edit window.
        let be = table
            .iter()
            .find(|v| v.edit_type == EditType::BaseEdit)
            .expect("a base editor must be present");
        assert_ne!(
            be.edit_window,
            (0, 0),
            "base editor {} should have a non-zero edit window",
            be.name
        );
    }

    #[test]
    fn default_table_round_trips() {
        let table = default_cas_variants();
        assert!(!table.is_empty());

        // serialize -> re-parse yields the same data (determinism / stable encoding).
        let serialized = ron::to_string(&table).expect("serialize cas-variant table");
        let reparsed = load_cas_variants_from_str(&serialized).expect("re-parse serialized table");
        assert_eq!(table, reparsed);
    }

    #[test]
    fn malformed_ron_is_a_clean_error() {
        let err = load_cas_variants_from_str("this is not ron");
        assert!(err.is_err());
        // The error renders with context (exercises Display).
        let msg = format!("{}", err.unwrap_err());
        assert!(
            msg.contains("cas-variant table"),
            "unexpected message: {msg}"
        );
    }

    // ---- PAM finding (slice S1.2) ----

    /// Look up a seed variant by name (panics if absent — the seed table is a build invariant).
    fn variant(name: &str) -> CasVariant {
        default_cas_variants()
            .into_iter()
            .find(|v| v.name == name)
            .unwrap_or_else(|| panic!("seed table missing {name}"))
    }

    fn fwd_positions(sites: &[PamSite]) -> Vec<usize> {
        sites
            .iter()
            .filter(|s| s.strand == Strand::Forward)
            .map(|s| s.position)
            .collect()
    }

    fn rev_positions(sites: &[PamSite]) -> Vec<usize> {
        sites
            .iter()
            .filter(|s| s.strand == Strand::Reverse)
            .map(|s| s.position)
            .collect()
    }

    #[test]
    fn iupac_matcher_covers_the_full_set() {
        // Concrete codes match only their own base.
        for (code, base, want) in [
            (b'A', b'A', true),
            (b'A', b'C', false),
            (b'G', b'G', true),
            (b'T', b'T', true),
            // U is treated as T on both sides.
            (b'T', b'U', true),
            (b'U', b'T', true),
        ] {
            assert_eq!(iupac_matches(code, base), want, "{code} vs {base}");
        }
        // Degenerate codes.
        assert!(iupac_matches(b'N', b'A') && iupac_matches(b'N', b'T')); // any
        assert!(
            iupac_matches(b'R', b'A') && iupac_matches(b'R', b'G') && !iupac_matches(b'R', b'C')
        );
        assert!(
            iupac_matches(b'Y', b'C') && iupac_matches(b'Y', b'T') && !iupac_matches(b'Y', b'A')
        );
        assert!(
            iupac_matches(b'S', b'G') && iupac_matches(b'S', b'C') && !iupac_matches(b'S', b'A')
        );
        assert!(
            iupac_matches(b'W', b'A') && iupac_matches(b'W', b'T') && !iupac_matches(b'W', b'G')
        );
        assert!(
            iupac_matches(b'K', b'G') && iupac_matches(b'K', b'T') && !iupac_matches(b'K', b'A')
        );
        assert!(
            iupac_matches(b'M', b'A') && iupac_matches(b'M', b'C') && !iupac_matches(b'M', b'G')
        );
        assert!(iupac_matches(b'B', b'C') && !iupac_matches(b'B', b'A')); // not A
        assert!(iupac_matches(b'D', b'A') && !iupac_matches(b'D', b'C')); // not C
        assert!(iupac_matches(b'H', b'A') && !iupac_matches(b'H', b'G')); // not G
        assert!(iupac_matches(b'V', b'A') && !iupac_matches(b'V', b'T')); // not T
                                                                          // Case-insensitive on both sides.
        assert!(iupac_matches(b'n', b'a') && iupac_matches(b'r', b'g'));
        // Unknown code / non-base never matches.
        assert!(!iupac_matches(b'Z', b'A'));
        assert!(!iupac_matches(b'N', b'X'));
    }

    #[test]
    fn ngg_forward_positions_are_correct() {
        // Forward NGG sites at indices 4 (AGG) and 8 (TGG); no other XGG window.
        //            0123456789
        let seq = b"AAAAAGGTTGGCC";
        let sites = find_pam_sites(seq, &variant("SpCas9"));
        // Forward "NGG" starts wherever seq[i+1]==G && seq[i+2]==G.
        // seq: A A A A A G G T T G G C C
        //      0 1 2 3 4 5 6 7 8 9 ...
        // i=4: A G G  ✓   i=8: T G G ✓  (i=5: G G T no; i=9: G G C no)
        assert_eq!(fwd_positions(&sites), vec![4, 8]);
    }

    #[test]
    fn ngg_reverse_strand_hit_is_found_and_mapped_back() {
        // "CCN" on the forward strand is "NGG" on the reverse strand.
        // Forward: C C A T T T T T T  → revcomp reads as a reverse NGG at the left CC.
        //          0 1 2 ...
        let seq = b"CCATTTTTTT";
        let sites = find_pam_sites(seq, &variant("SpCas9"));
        // No forward NGG here.
        assert!(fwd_positions(&sites).is_empty(), "unexpected forward hit");
        // Reverse hit: the CC at forward [0,1] with the N at index 2 → reported leftmost position 0.
        assert_eq!(rev_positions(&sites), vec![0]);

        // Cut-site math for the reverse hit (SpCas9 cut_offset = -3):
        // PAM bases forward [0,2], 5' base = rightmost = index 2, 5'→3' decreases index:
        //   cut_site = 2 - (-3) = 5.
        let rev = sites.iter().find(|s| s.strand == Strand::Reverse).unwrap();
        assert_eq!(rev.cut_site, 5);
    }

    #[test]
    fn ngg_forward_cut_site_math() {
        // Single clean forward NGG at index 3.
        //           0123456
        let seq = b"AAATGGAAAA";
        let sites = find_pam_sites(seq, &variant("SpCas9"));
        let fwd: Vec<_> = sites
            .iter()
            .filter(|s| s.strand == Strand::Forward)
            .collect();
        assert_eq!(fwd.len(), 1);
        assert_eq!(fwd[0].position, 3);
        // SpCas9 cut_offset = -3 → cut_site = 3 + (-3) = 0.
        assert_eq!(fwd[0].cut_site, 0);
    }

    #[test]
    fn tttv_forward_positions_and_cut_site() {
        // AsCas12a "TTTV" (V = A/C/G). Place one TTTA at index 2 and one TTTG at index 9.
        //           0123456789012
        let seq = b"GGTTTACCGTTTGAA";
        let v = variant("AsCas12a");
        let sites = find_pam_sites(seq, &v);
        // TTTV forward windows: index 2 = TTTA ✓, index 9 = TTTG ✓.
        assert_eq!(fwd_positions(&sites), vec![2, 9]);

        // TTTT would NOT match (T is not in V); confirm a TTTT window is excluded.
        let no_t = find_pam_sites(b"TTTT", &v);
        assert!(no_t
            .iter()
            .all(|s| s.strand == Strand::Reverse || s.position != 0));

        // Cut-site math (AsCas12a cut_offset = 18, PAM-distal): forward site at position 2 →
        // cut_site = 2 + 18 = 20.
        let first = sites
            .iter()
            .find(|s| s.strand == Strand::Forward && s.position == 2)
            .unwrap();
        assert_eq!(first.cut_site, 20);
    }

    #[test]
    fn results_are_sorted_and_deterministic() {
        let seq = b"AGGTGGCCAGGTGG";
        let v = variant("SpCas9");
        let a = find_pam_sites(seq, &v);
        let b = find_pam_sites(seq, &v);
        assert_eq!(a, b, "same input must give identical output (determinism)");
        // Sorted by (position, strand).
        let mut sorted = a.clone();
        sorted.sort_unstable_by(|x, y| x.position.cmp(&y.position).then(x.strand.cmp(&y.strand)));
        assert_eq!(a, sorted);
    }

    #[test]
    fn empty_or_oversized_pam_yields_no_sites() {
        let short = b"AC";
        // PAM longer than sequence → no sites.
        assert!(find_pam_sites(short, &variant("AsCas12a")).is_empty());
        // Empty PAM → no sites.
        let mut v = variant("SpCas9");
        v.pam = String::new();
        assert!(find_pam_sites(b"ACGTACGT", &v).is_empty());
    }

    #[test]
    fn convenience_wrapper_matches_byte_api() {
        let g = genome::sample_genome();
        let v = variant("SpCas9");
        for locus in &g.loci {
            assert_eq!(
                find_pam_sites_in(&locus.sequence, &v),
                find_pam_sites(locus.sequence.bases(), &v),
            );
        }
    }

    #[cfg(feature = "proptest")]
    mod prop {
        use super::*;
        use proptest::prelude::*;

        fn arb_edit_type() -> impl Strategy<Value = EditType> {
            prop_oneof![
                Just(EditType::Dsb),
                Just(EditType::BaseEdit),
                Just(EditType::Prime),
            ]
        }

        fn arb_variant() -> impl Strategy<Value = CasVariant> {
            (
                any::<u16>(),
                "[A-Za-z0-9_-]{1,12}",
                "[ACGTNRYSWKMBDHV]{1,8}",
                any::<i16>(),
                any::<(i16, i16)>(),
                arb_edit_type(),
            )
                .prop_map(|(id, name, pam, cut_offset, edit_window, edit_type)| {
                    CasVariant {
                        id: CasVariantId(id),
                        name,
                        pam,
                        cut_offset,
                        edit_window,
                        edit_type,
                    }
                })
        }

        /// An ACGT sequence plus a Cas variant drawn from the seed table.
        fn arb_seq_and_variant() -> impl Strategy<Value = (Vec<u8>, CasVariant)> {
            let table = default_cas_variants();
            (
                proptest::collection::vec(
                    prop_oneof![Just(b'A'), Just(b'C'), Just(b'G'), Just(b'T')],
                    0..64,
                ),
                proptest::sample::select(table),
            )
        }

        proptest! {
            /// Any well-formed table serializes and parses back identically (encode/decode is lossless).
            #[test]
            fn arbitrary_table_round_trips(table in proptest::collection::vec(arb_variant(), 0..16)) {
                let serialized = ron::to_string(&table).expect("serialize");
                let reparsed = load_cas_variants_from_str(&serialized).expect("re-parse");
                prop_assert_eq!(table, reparsed);
            }

            /// No false positives: every reported site's bases (re-derived in the forward frame, with the
            /// reverse complement re-applied for reverse hits) actually match the variant's IUPAC PAM.
            #[test]
            fn every_reported_site_actually_matches_the_pam((seq, v) in arb_seq_and_variant()) {
                let sites = find_pam_sites(&seq, &v);
                let pam = v.pam.as_bytes();
                let n = seq.len();
                for site in &sites {
                    // Reported positions stay inside the sequence and leave room for the PAM.
                    prop_assert!(site.position + pam.len() <= n);
                    let window = &seq[site.position..site.position + pam.len()];
                    match site.strand {
                        Strand::Forward => {
                            for (&code, &base) in pam.iter().zip(window.iter()) {
                                prop_assert!(iupac_matches(code, base), "fwd {} vs {}", code, base);
                            }
                        }
                        Strand::Reverse => {
                            // On the reverse strand the PAM reads against the reverse complement of the window.
                            let rc = dna::revcomp(window);
                            for (&code, &base) in pam.iter().zip(rc.iter()) {
                                prop_assert!(iupac_matches(code, base), "rev {} vs {}", code, base);
                            }
                        }
                    }
                }

                // Determinism (inv. #3): identical input → identical, sorted output.
                prop_assert_eq!(&sites, &find_pam_sites(&seq, &v));
                let is_sorted = sites
                    .windows(2)
                    .all(|w| (w[0].position, w[0].strand) <= (w[1].position, w[1].strand));
                prop_assert!(is_sorted);
            }
        }
    }
}
