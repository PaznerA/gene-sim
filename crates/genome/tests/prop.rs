//! Property-based invariants for the genome data model (gate §10.4).
//! Runs only under `cargo test --features proptest`.
#![cfg(feature = "proptest")]

use genome::ParamValue;
use proptest::prelude::*;

proptest! {
    /// Clamping any numeric value into its domain always yields a valid value (an edit/perturbation
    /// can never leave a Numeric parameter out of range — the basis of "an edit never yields an
    /// invalid genome", SPEC §10.4).
    #[test]
    fn numeric_clamp_always_valid(value in -1e6f64..1e6, lo in -1e6f64..1e6, span in 0.0f64..1e6) {
        let min = lo;
        let max = lo + span;
        let mut v = ParamValue::Numeric { value, min, max };
        v.clamp_into_domain();
        prop_assert!(v.is_valid(), "clamped value not valid: {v:?}");
    }

    /// The normalized scalar is always within [0, 1] for any in-domain enum value.
    #[test]
    fn enum_unit_scalar_in_range(card in 1u16..1000, raw in 0u16..1000) {
        let value = raw % card;
        let s = ParamValue::Enum { value, cardinality: card }.as_unit_scalar();
        prop_assert!((0.0..=1.0).contains(&s), "scalar {s} out of [0,1]");
    }
}
