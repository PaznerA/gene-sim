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

        proptest! {
            /// Any well-formed table serializes and parses back identically (encode/decode is lossless).
            #[test]
            fn arbitrary_table_round_trips(table in proptest::collection::vec(arb_variant(), 0..16)) {
                let serialized = ron::to_string(&table).expect("serialize");
                let reparsed = load_cas_variants_from_str(&serialized).expect("re-parse");
                prop_assert_eq!(table, reparsed);
            }
        }
    }
}
