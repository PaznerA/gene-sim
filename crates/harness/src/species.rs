//! Boundary loader for JSON species starters (ADR-017): read a [`genome::spec::SpeciesSpec`] file and build it
//! into a validated [`genome::spec::BuiltSpecies`]. The sim CORE stays filesystem-free (inv #2); this boundary
//! does the file I/O — exactly like [`crate::campaign::load_campaign`]. A built species' `genome` is what the
//! core's `Simulation::reset_with_genome` consumes.

use std::io;
use std::path::Path;

use genome::spec::{BuiltSpecies, SpeciesSpec};

/// Load + validate a species JSON file (`data/species/<key>.json`) into a [`BuiltSpecies`].
///
/// # Errors
/// An I/O error reading the file, a JSON deserialization error, or a [`genome::spec::SpecError`] build/validation
/// error (locus-id/index mismatch, non-ACGT base, out-of-domain parameter) — all surfaced as an
/// [`io::Error`] of kind [`io::ErrorKind::InvalidData`], preserving the offending path in its message.
pub fn load_species_file(path: impl AsRef<Path>) -> io::Result<BuiltSpecies> {
    let text = std::fs::read_to_string(path)?;
    let spec: SpeciesSpec =
        serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    spec.build()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_default_species_loads_to_sample_genome() {
        // The committed default species must load + build to exactly the wired sample_genome (data-not-code,
        // caught by the gate). This is the roster default that keeps the world byte-identical (hash-neutral).
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/species/default.json"
        );
        let built = load_species_file(path).expect("data/species/default.json should load");
        assert_eq!(
            built.genome,
            genome::sample_genome(),
            "the default species must equal the core's wired sample_genome"
        );
        assert_eq!(built.key, "default");
        assert_eq!(built.entity_count, 1000);
    }
}
