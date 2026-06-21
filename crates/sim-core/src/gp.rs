//! Genotype→phenotype mapping (TAXONOMY §2, SPEC §4) — Parameters → [`Trait`]s.
//!
//! This is the **only** place genotype→phenotype logic lives (invariant #2; it stays in `genome`/`sim-core`,
//! never in `godot/`). The mapping is **pure and deterministic** for a fixed genome (invariant #3) and sits
//! behind the [`GenotypePhenotypeMap`] trait so it is pluggable (invariant #5) — [`WeightedSumMap`] is the
//! Stage-1 default. No `HashMap` is iterated: we walk the genome's ordered `loci`/`parameters` only.

use genome::{Genome, GoTermId, LocusId, ParamId};

/// A heritable trait expressed from the genome. Extensible (TAXONOMY §2); new *biological* kinds arrive as
/// ontology nodes (Stage 5), but the small fixed set the engine reasons about is enumerated here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Trait {
    /// Growth rate — feeds [`crate::Simulation`] selection (the only trait that drives the sim).
    GrowthRate,
    /// Overall height / reach of the plant.
    Stature,
    /// How much the plant branches (architecture density).
    Branchiness,
    /// Leaf size.
    LeafSize,
    /// Leaf colour hue.
    LeafHue,
    /// Surface reflectance (colour + spread).
    Reflectance,
    /// Reproductive output (flowering).
    Fecundity,
    /// Drought tolerance (sturdier taper / narrower leaves).
    DroughtTolerance,
    /// CRISPR kill-switch linkage (a discrete bool trait).
    KillSwitchLinkage,

    // ── Microbe traits (ADR-017 F2-2) — the E. coli observable phenotypes, expressed via the E. coli
    // [`ecoli_trait_map`]. Deliberately NOT in [`Trait::ALL`] (that stays the 9 plant render/CSV order); a
    // microbe species expresses these through its own `TraitMap` instead.
    /// Glucose uptake capacity (PTS system) — microbe.
    GlucoseUptake,
    /// Respiration-mode lean (aerobic ↔ fermentative) — microbe.
    RespirationMode,
    /// Acetate overflow — the Layer-3 detritus/mineralization tap — microbe.
    AcetateOverflow,
    /// Fermentation capacity (lactate / ethanol) — microbe.
    FermentationCapacity,
}

impl Trait {
    /// The traits in canonical (declaration) order — the order a [`Phenotype`] stores them in.
    /// A fixed array (not a `HashMap`) so iteration is deterministic (invariant #3). Each trait is anchored
    /// on its OWN flat genome parameter (see [`WeightedSumMap::weight`]) so they vary INDEPENDENTLY — an edit
    /// to one parameter moves exactly one trait, giving the specimen view many distinct, continuous variants.
    pub const ALL: [Trait; 9] = [
        Trait::GrowthRate,
        Trait::Stature,
        Trait::Branchiness,
        Trait::LeafSize,
        Trait::LeafHue,
        Trait::Reflectance,
        Trait::Fecundity,
        Trait::DroughtTolerance,
        Trait::KillSwitchLinkage,
    ];
}

/// An expressed phenotype: an **ordered** list of `(Trait, value)` pairs, each value clamped to `[0, 1]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Phenotype {
    /// Ordered (canonical `Trait::ALL` order). Iterate this; never a `HashMap` (invariant #3).
    pub values: Vec<(Trait, f64)>,
}

impl Phenotype {
    /// The value of a given trait, if present. Linear scan over the (tiny, ordered) list.
    #[must_use]
    pub fn get(&self, t: Trait) -> Option<f64> {
        self.values.iter().find(|(k, _)| *k == t).map(|(_, v)| *v)
    }
}

/// A pure, deterministic genotype→phenotype map (invariant #2, #3, #5).
pub trait GenotypePhenotypeMap {
    /// Express `genome` into a [`Phenotype`]. Same genome ⇒ identical phenotype.
    fn express(&self, genome: &Genome) -> Phenotype;
}

/// How a [`TraitBinding`] selects the locus carrying its parameter (ADR-017 F2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocusSelector {
    /// The locus with this id — a stable positional layout (the plant's loci).
    ByIndex(LocusId),
    /// The FIRST locus (in genome `loci` Vec order) whose `go_refs` contains this GO term — an ONTOLOGY-driven
    /// binding for species whose layout isn't positional (e.g. E. coli genes keyed by molecular function).
    ByGoAnchor(GoTermId),
}

/// One trait's binding: which locus + which parameter within it expresses the trait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraitBinding {
    /// The expressed trait.
    pub trait_: Trait,
    /// Which locus carries the parameter.
    pub locus: LocusSelector,
    /// The parameter id within that locus.
    pub param: ParamId,
}

/// An ordered, per-species set of trait bindings — the genotype→phenotype "wiring" for one species. An ordered
/// `Vec` (never a `HashMap`, inv #3); the binding order IS the [`Phenotype`] order.
#[derive(Debug, Clone, PartialEq)]
pub struct TraitMap(pub Vec<TraitBinding>);

/// Resolve a [`LocusSelector`] against a genome (ordered, pure, no RNG). `ByGoAnchor` takes the FIRST matching
/// locus in `loci` Vec order, so the result is deterministic.
fn resolve_locus(genome: &Genome, sel: LocusSelector) -> Option<&genome::Locus> {
    match sel {
        LocusSelector::ByIndex(id) => genome.locus(id),
        LocusSelector::ByGoAnchor(go) => genome.loci.iter().find(|l| l.tags.go_refs.contains(&go)),
    }
}

/// The genotype→phenotype map driven by a per-species [`TraitMap`] (ADR-017 F2): each trait reads exactly the
/// locus + parameter its species names, so plant and microbe genomes express their OWN traits from one engine
/// (invariant #5). Pure + ordered; a binding whose locus/param is absent expresses a documented `0.0` (never a
/// panic), so an arbitrary loaded genome can never crash expression.
#[derive(Debug, Clone)]
pub struct OntologyMap {
    map: TraitMap,
}

impl OntologyMap {
    /// Build an `OntologyMap` from a species' [`TraitMap`].
    #[must_use]
    pub fn new(map: TraitMap) -> Self {
        Self { map }
    }
}

impl GenotypePhenotypeMap for OntologyMap {
    fn express(&self, genome: &Genome) -> Phenotype {
        let values = self
            .map
            .0
            .iter()
            .map(|b| {
                let scalar = resolve_locus(genome, b.locus)
                    .and_then(|l| l.parameters.iter().find(|p| p.id == b.param))
                    .map_or(0.0, |p| p.value.as_unit_scalar());
                (b.trait_, scalar.clamp(0.0, 1.0))
            })
            .collect();
        Phenotype { values }
    }
}

/// The default PLANT trait map — the 9 bindings that reproduce the historical flat-index anchoring EXACTLY
/// (`GrowthRate`=L0/P0, `Stature`=L0/P1, `Branchiness`=L0/P2, `LeafSize`=L1/P0, `LeafHue`=L1/P1,
/// `Reflectance`=L1/P2, `Fecundity`=L2/P0, `DroughtTolerance`=L3/P0, `KillSwitchLinkage`=L3/P1). Because each
/// binding reads exactly the parameter its old flat anchor did, [`WeightedSumMap`] expresses byte-identically
/// to before F2 (hash-neutral — proven by the unchanged pinned determinism literal).
#[must_use]
pub fn default_plant_trait_map() -> TraitMap {
    let b = |t, l, p| TraitBinding {
        trait_: t,
        locus: LocusSelector::ByIndex(LocusId(l)),
        param: ParamId(p),
    };
    TraitMap(vec![
        b(Trait::GrowthRate, 0, 0),
        b(Trait::Stature, 0, 1),
        b(Trait::Branchiness, 0, 2),
        b(Trait::LeafSize, 1, 0),
        b(Trait::LeafHue, 1, 1),
        b(Trait::Reflectance, 1, 2),
        b(Trait::Fecundity, 2, 0),
        b(Trait::DroughtTolerance, 3, 0),
        b(Trait::KillSwitchLinkage, 3, 1),
    ])
}

/// The E. coli per-species [`TraitMap`] (ADR-017 B-2): the 5 microbe traits bound by ONTOLOGY (`ByGoAnchor`) to
/// the metabolic anchor genes in `data/species/ecoli.json`, each reading that gene's activity parameter (P0,
/// `1.0`=wild-type). A knockout edit (activity→0) drives the bound trait to 0. `GrowthRate` — the only
/// selection-driving trait — anchors on the TCA backbone gene `gltA`. Ordered (inv #3); the GO ids match the
/// curated `go_refs` baked into ecoli.json by `scripts/bake_ecoli_species.py`.
#[must_use]
pub fn ecoli_trait_map() -> TraitMap {
    let b = |t, go| TraitBinding {
        trait_: t,
        locus: LocusSelector::ByGoAnchor(GoTermId(go)),
        param: ParamId(0),
    };
    TraitMap(vec![
        b(Trait::GrowthRate, 4108), // gltA — citrate synthase (TCA/growth backbone)
        b(Trait::GlucoseUptake, 8982), // ptsG — PTS glucose transporter
        b(Trait::RespirationMode, 8861), // pflB — pyruvate formate-lyase (fermentation marker)
        b(Trait::AcetateOverflow, 8959), // pta  — phosphate acetyltransferase (acetate overflow)
        b(Trait::FermentationCapacity, 8720), // ldhA — D-lactate dehydrogenase
    ])
}

/// Select the per-species [`TraitMap`] by the species `key` (ADR-017 "RUN E. coli"). A pure, ordered `match`
/// (never a `HashMap` — inv #3): `"ecoli-core"` → [`ecoli_trait_map`]; EVERY other key → the default plant map,
/// so an unknown/missing key degrades safely to the historical behaviour.
#[must_use]
pub fn trait_map_for(key: &str) -> TraitMap {
    match key {
        "ecoli-core" => ecoli_trait_map(),
        _ => default_plant_trait_map(),
    }
}

/// The transparent Stage-1 default for the PLANT species: each of the 9 traits reads exactly its own anchored
/// genome parameter ([`genome::ParamValue::as_unit_scalar`], clamped to `[0, 1]`), fully DECOUPLED so an edit to
/// one parameter moves exactly one trait (many independent, continuous specimen variants).
///
/// Since ADR-017 F2 this is a thin wrapper over [`OntologyMap`] carrying [`default_plant_trait_map`] — the same
/// anchoring (`GrowthRate`=L0/P0 … `KillSwitchLinkage`=L3/P1) expressed through the per-species binding engine,
/// so it stays byte-identical (hash-neutral) while E. coli / other species supply their OWN [`TraitMap`].
#[derive(Debug, Clone, Copy, Default)]
pub struct WeightedSumMap;

impl GenotypePhenotypeMap for WeightedSumMap {
    fn express(&self, genome: &Genome) -> Phenotype {
        OntologyMap::new(default_plant_trait_map()).express(genome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn express_is_deterministic_for_fixed_genome() {
        let g = genome::sample_genome();
        // AC1: same genome expressed twice ⇒ identical phenotype.
        assert_eq!(WeightedSumMap.express(&g), WeightedSumMap.express(&g));
    }

    #[test]
    fn all_trait_values_in_unit_range() {
        let g = genome::sample_genome();
        let p = WeightedSumMap.express(&g);
        assert_eq!(p.values.len(), Trait::ALL.len());
        for (t, v) in &p.values {
            assert!((0.0..=1.0).contains(v), "trait {t:?} = {v} out of [0,1]");
        }
    }

    #[test]
    fn get_returns_each_trait() {
        let g = genome::sample_genome();
        let p = WeightedSumMap.express(&g);
        for t in Trait::ALL {
            assert!(p.get(t).is_some(), "missing trait {t:?}");
        }
    }

    #[test]
    fn growth_rate_tracks_first_parameter() {
        // p0 of sample_genome is Numeric{value:0.6,0..1} → unit scalar 0.6; GrowthRate = 1.0 * p0.
        let g = genome::sample_genome();
        let p = WeightedSumMap.express(&g);
        assert!((p.get(Trait::GrowthRate).unwrap() - 0.6).abs() < 1e-9);
    }

    #[test]
    fn f2_default_plant_map_pins_expression() {
        // F2 (ADR-017): the ontology re-key must express sample_genome BYTE-IDENTICALLY to the pre-F2 flat
        // anchoring — pinning every trait value proves the re-key is hash-neutral (allele_freq unchanged).
        let g = genome::sample_genome();
        let p = WeightedSumMap.express(&g);
        let expect = [
            (Trait::GrowthRate, 0.6),
            (Trait::Stature, 0.5),
            (Trait::Branchiness, 0.5),
            (Trait::LeafSize, 0.5),
            (Trait::LeafHue, 0.45),
            (Trait::Reflectance, 0.5),
            (Trait::Fecundity, 0.4),
            (Trait::DroughtTolerance, 0.5),
            (Trait::KillSwitchLinkage, 0.0), // Bool(false) → 0.0
        ];
        assert_eq!(p.values.len(), expect.len());
        for ((t, v), (et, ev)) in p.values.iter().zip(expect.iter()) {
            assert_eq!(t, et, "phenotype must stay in Trait::ALL order");
            assert!((v - ev).abs() < 1e-9, "{t:?} = {v}, expected {ev}");
        }
        // The wrapper is exactly OntologyMap(default_plant_trait_map).
        assert_eq!(p, OntologyMap::new(default_plant_trait_map()).express(&g));
    }

    #[test]
    fn by_go_anchor_resolves_first_matching_locus() {
        // An ontology-driven binding reads the FIRST locus whose go_refs contains the anchor (Vec order):
        // sample_genome's L0 carries GO 40007, so ByGoAnchor(40007)/P0 reads L0/P0 = 0.6.
        let g = genome::sample_genome();
        let map = TraitMap(vec![TraitBinding {
            trait_: Trait::GrowthRate,
            locus: LocusSelector::ByGoAnchor(GoTermId(40007)),
            param: ParamId(0),
        }]);
        let p = OntologyMap::new(map).express(&g);
        assert!((p.get(Trait::GrowthRate).unwrap() - 0.6).abs() < 1e-9);
    }

    #[test]
    fn trait_map_for_selects_by_key() {
        // Ordered match (inv #3): the E. coli key → microbe map; every other key → the default plant map.
        assert_eq!(trait_map_for("ecoli-core"), ecoli_trait_map());
        assert_eq!(trait_map_for("default"), default_plant_trait_map());
        assert_eq!(trait_map_for("unknown-species"), default_plant_trait_map());
    }

    #[test]
    fn missing_binding_expresses_zero_not_panic() {
        // A binding whose locus/param is absent yields a documented 0.0 (so an arbitrary loaded genome is safe).
        let g = genome::sample_genome();
        let map = TraitMap(vec![TraitBinding {
            trait_: Trait::GrowthRate,
            locus: LocusSelector::ByIndex(LocusId(99)),
            param: ParamId(0),
        }]);
        assert_eq!(
            OntologyMap::new(map).express(&g).get(Trait::GrowthRate),
            Some(0.0)
        );
    }
}
