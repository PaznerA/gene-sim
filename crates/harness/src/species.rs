//! Boundary loader for JSON species starters (ADR-017): read a [`genome::spec::SpeciesSpec`] file and build it
//! into a validated [`genome::spec::BuiltSpecies`]. The sim CORE stays filesystem-free (inv #2); this boundary
//! does the file I/O — exactly like [`crate::campaign::load_campaign`]. A built species' `genome` is what the
//! core's `Simulation::reset_with_genome` consumes.

use std::io;
use std::path::Path;

use genome::spec::{BuiltSpecies, SpeciesSpec};

/// Build + validate a species from an already-read JSON STRING — no filesystem I/O. This is the `res://` VFS
/// boundary (ADR-017): GDScript reads the bytes via `FileAccess` and hands the text here, so the core never
/// touches the filesystem (inv #2/#4). Pure: serde parse + [`SpeciesSpec::build`] — draws NO RNG (inv #3).
///
/// # Errors
/// A JSON deserialization error or a [`genome::spec::SpecError`] build/validation error (locus-id/index
/// mismatch, non-ACGT base, out-of-domain parameter) — all surfaced as an [`io::Error`] of kind
/// [`io::ErrorKind::InvalidData`].
pub fn build_species_from_str(json: &str) -> io::Result<BuiltSpecies> {
    let spec: SpeciesSpec =
        serde_json::from_str(json).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    spec.build()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Load + validate a species JSON file (`data/species/<key>.json`) into a [`BuiltSpecies`].
///
/// Delegates to [`build_species_from_str`] after reading the file — the single parse/validate path (DRY), so
/// the file-path callers (harness CLI / campaign) and the `res://`-string boundary stay byte-for-byte in sync.
///
/// # Errors
/// An I/O error reading the file, a JSON deserialization error, or a [`genome::spec::SpecError`] build/validation
/// error (locus-id/index mismatch, non-ACGT base, out-of-domain parameter) — all surfaced as an
/// [`io::Error`] of kind [`io::ErrorKind::InvalidData`].
pub fn load_species_file(path: impl AsRef<Path>) -> io::Result<BuiltSpecies> {
    build_species_from_str(&std::fs::read_to_string(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str_matches_load_file_on_shipped_species() {
        // The `res://`-string boundary (build_species_from_str) and the file-path loader (load_species_file)
        // are the SAME parse/validate path: building from the shipped JSON TEXT must yield a BuiltSpecies
        // identical (key, entity_count, genome, …) to loading the file. This locks the two byte SOURCES in
        // sync so the gate catches any drift between the renderer's res:// path and the harness CLI path.
        for stem in ["default", "ecoli", "bdellovibrio"] {
            let path = format!(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/species/{}.json"),
                stem
            );
            let from_file = load_species_file(&path).expect("file loads");
            let text = std::fs::read_to_string(&path).expect("read text");
            let from_str = build_species_from_str(&text).expect("string builds");
            assert_eq!(
                from_file, from_str,
                "build_species_from_str must equal load_species_file for {stem}.json"
            );
        }
    }

    #[test]
    fn from_str_rejects_malformed_json_as_invalid_data() {
        // Malformed bytes must surface as io::ErrorKind::InvalidData (not a panic, not an I/O error), exactly
        // like a serde failure inside load_species_file — the renderer reads this as a `false` + a clean error.
        let err =
            build_species_from_str("{ not valid json ").expect_err("malformed JSON must fail");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

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
    fn shipped_bdellovibrio_species_loads() {
        // ADR-013 F6: the baked real Bdellovibrio HD100 PREDATOR genome (scripts/bake_bdellovibrio_species.py:
        // curated predation-anchor roster × real NCBI GCF_000196175.1 CDS) must load + build. Data-not-code: the
        // gate catches a broken or incomplete re-bake. The niche declares the predator role via trophic_role, and
        // the two TraitMap anchors (gltA → GrowthRate, GO-4108; the lytic attack machinery → PredationCapacity,
        // GO-8745) must be present so the predation kernel's attack-rate lever resolves.
        use sim_core::gp::{bdellovibrio_trait_map, GenotypePhenotypeMap, OntologyMap, Trait};
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/species/bdellovibrio.json"
        );
        let built = load_species_file(path).expect("data/species/bdellovibrio.json should load");
        assert_eq!(built.key, "bdellovibrio");
        assert_eq!(
            built.trophic_role.as_deref(),
            Some("predator"),
            "the niche declares the predator role (data-driven gp::role_from_override → Predator)"
        );
        assert!(built.genome.is_valid());
        assert!(
            built.genome.loci.iter().all(|l| !l.sequence.is_empty()),
            "every Bdellovibrio locus carries a real CDS"
        );
        // The PredationCapacity attack-rate lever resolves off the baked GO-8745 anchor (wild-type 1.0), and
        // GrowthRate off gltA (GO-4108) — so a `hit`-locus knockdown would drive the attack rate down.
        let pheno = OntologyMap::new(bdellovibrio_trait_map()).express(&built.genome);
        assert_eq!(
            pheno.get(Trait::GrowthRate),
            Some(1.0),
            "GrowthRate expresses off gltA wild-type activity"
        );
        assert_eq!(
            pheno.get(Trait::PredationCapacity),
            Some(1.0),
            "PredationCapacity expresses off the lytic attack-machinery anchor (the hit/attack lever)"
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

    #[test]
    fn ecoli_runs_deterministically_off_gltacitrate() {
        // RUN E. coli (ADR-017): the real 136-gene genome runs via run_headless_with through ecoli_trait_map —
        // deterministic (same inputs → same hash twice), and its GrowthRate comes from gltA (1.0), NOT plant 0.6.
        use sim_core::gp::{trait_map_for, OntologyMap, Trait};
        use sim_core::{run_headless_with, EnvParams, SimConfig, Simulation};
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/species/ecoli.json");
        let built = load_species_file(path).expect("ecoli loads");
        let cfg = SimConfig {
            seed: 7,
            generations: 20,
            entity_count: 300,
        };
        let map = || OntologyMap::new(trait_map_for(&built.key));
        let h1 = run_headless_with(&cfg, built.genome.clone(), map());
        let h2 = run_headless_with(&cfg, built.genome.clone(), map());
        assert_eq!(h1.hash, h2.hash, "E. coli run must be deterministic");
        let mut sim = Simulation::reset_with_genome_and_map(
            &cfg,
            &EnvParams::default(),
            built.genome.clone(),
            map(),
        );
        assert_eq!(
            sim.observe().phenotype.get(Trait::GrowthRate),
            Some(1.0),
            "E. coli GrowthRate comes from gltA wild-type activity"
        );
    }

    #[test]
    fn gltacitrate_knockout_drops_growth_across_all_express_sites() {
        // Edit-consistency: knocking out gltA (GO 4108, activity→0) drops the OBSERVED GrowthRate — proving
        // observe + with_genome_and_rng both use the STORED E. coli map (would FAIL if either stayed on the plant
        // WeightedSumMap, which has no GO-4108 binding).
        use sim_core::gp::{trait_map_for, OntologyMap, Trait};
        use sim_core::{EnvParams, SimConfig, Simulation};
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/species/ecoli.json");
        let built = load_species_file(path).expect("ecoli loads");
        let cfg = SimConfig {
            seed: 7,
            generations: 1,
            entity_count: 100,
        };
        let mut sim = Simulation::reset_with_genome_and_map(
            &cfg,
            &EnvParams::default(),
            built.genome.clone(),
            OntologyMap::new(trait_map_for(&built.key)),
        );
        assert_eq!(sim.observe().phenotype.get(Trait::GrowthRate), Some(1.0));
        sim.with_genome_and_rng(|g, _rng| {
            for l in g.loci.iter_mut() {
                if l.tags.go_refs.contains(&genome::GoTermId(4108)) {
                    if let Some(p) = l.parameters.iter_mut().find(|p| p.id == genome::ParamId(0)) {
                        p.value = genome::ParamValue::Numeric {
                            value: 0.0,
                            min: 0.0,
                            max: 1.0,
                        };
                    }
                }
            }
        });
        assert_eq!(
            sim.observe().phenotype.get(Trait::GrowthRate),
            Some(0.0),
            "gltA knockout must drop GrowthRate through the stored E. coli map"
        );
    }
}
