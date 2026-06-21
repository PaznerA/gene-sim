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

    #[test]
    fn shipped_ecoli_species_loads() {
        // The baked real E. coli K-12 core genome (scripts/bake_ecoli_species.py: BiGG e_coli_core roster ×
        // real NCBI GCF_000005845.2 CDS) must load + build — 136 real genes, each a non-empty ACGT CDS.
        // Data-not-code: the gate catches a broken or incomplete re-bake.
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/species/ecoli.json");
        let built = load_species_file(path).expect("data/species/ecoli.json should load");
        assert_eq!(built.key, "ecoli-core");
        assert_eq!(
            built.genome.loci.len(),
            136,
            "e_coli_core is 136 real genes"
        );
        assert!(built.genome.is_valid());
        assert!(
            built.genome.loci.iter().all(|l| !l.sequence.is_empty()),
            "every E. coli locus carries a real CDS"
        );
    }

    #[test]
    fn ecoli_genome_expresses_microbe_traits() {
        // B-2 end-to-end: the REAL 136-gene E. coli genome, expressed through its ByGoAnchor `ecoli_trait_map`,
        // yields the 5 microbe traits. Wild-type activity (1.0) on every anchor gene → each trait expresses 1.0;
        // this proves the ontology bindings resolve against the baked `go_refs` (ADR-017 F2 + B-2).
        use sim_core::gp::{ecoli_trait_map, GenotypePhenotypeMap, OntologyMap, Trait};
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/species/ecoli.json");
        let built = load_species_file(path).expect("data/species/ecoli.json should load");
        let pheno = OntologyMap::new(ecoli_trait_map()).express(&built.genome);
        for t in [
            Trait::GrowthRate,
            Trait::GlucoseUptake,
            Trait::RespirationMode,
            Trait::AcetateOverflow,
            Trait::FermentationCapacity,
        ] {
            assert_eq!(
                pheno.get(t),
                Some(1.0),
                "{t:?} should express wild-type 1.0"
            );
        }
    }
}
