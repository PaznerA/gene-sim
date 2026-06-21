//! Genotype→phenotype mapping (TAXONOMY §2, SPEC §4) — Parameters → [`Trait`]s.
//!
//! This is the **only** place genotype→phenotype logic lives (invariant #2; it stays in `genome`/`sim-core`,
//! never in `godot/`). The mapping is **pure and deterministic** for a fixed genome (invariant #3) and sits
//! behind the [`GenotypePhenotypeMap`] trait so it is pluggable (invariant #5) — [`WeightedSumMap`] is the
//! Stage-1 default. No `HashMap` is iterated: we walk the genome's ordered `loci`/`parameters` only.

use genome::Genome;

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

/// The transparent Stage-1 default: each trait is a fixed **weighted sum** of the genome's parameter
/// unit-scalars ([`genome::ParamValue::as_unit_scalar`]), clamped to `[0, 1]`.
///
/// ## How the weighting works
/// Parameters are gathered into one ordered vector by walking `genome.loci` then each locus's `parameters`
/// (stable order — invariant #3). For trait `t`, parameter at flat index `i` contributes with weight
/// `weight(t, i)` taken from [`WeightedSumMap::weight`]; the products are summed and clamped to `[0, 1]`.
///
/// ## Documented weights ([`WeightedSumMap::weight`])
/// Each trait is anchored 1:1 on its OWN flat genome parameter (fully DECOUPLED, so an edit to one parameter
/// moves exactly one trait — many independent, continuous specimen variants):
/// * `GrowthRate`=p0 · `Stature`=p1 · `Branchiness`=p2 · `LeafSize`=p3 · `LeafHue`=p4 · `Reflectance`=p5 ·
///   `Fecundity`=p6 · `DroughtTolerance`=p7 · `KillSwitchLinkage`=p8 (the kill-switch bool slot).
///
/// Parameter slots beyond those anchored contribute weight `0.0`. Because each trait reads exactly one
/// `as_unit_scalar()` (in `[0, 1]`), the raw sum is already in `[0, 1]`; the final `clamp` is a belt-and-braces
/// guarantee for arbitrary genomes (property AC3).
#[derive(Debug, Clone, Copy, Default)]
pub struct WeightedSumMap;

impl WeightedSumMap {
    /// Weight of flat parameter index `i` toward trait `t`. Pure; the single source of truth for the scheme
    /// documented on [`WeightedSumMap`]. Unknown slots weigh `0.0`.
    #[must_use]
    fn weight(t: Trait, i: usize) -> f64 {
        // Each trait is anchored on its OWN flat genome parameter (decoupled): trait value == that parameter's
        // unit scalar. So an edit to parameter k moves exactly trait k — independent, continuous variation.
        let anchor = match t {
            Trait::GrowthRate => 0,
            Trait::Stature => 1,
            Trait::Branchiness => 2,
            Trait::LeafSize => 3,
            Trait::LeafHue => 4,
            Trait::Reflectance => 5,
            Trait::Fecundity => 6,
            Trait::DroughtTolerance => 7,
            Trait::KillSwitchLinkage => 8,
        };
        if i == anchor {
            1.0
        } else {
            0.0
        }
    }
}

impl GenotypePhenotypeMap for WeightedSumMap {
    fn express(&self, genome: &Genome) -> Phenotype {
        // Flatten parameters in stable order (loci, then parameters) — no HashMap (invariant #3).
        let scalars: Vec<f64> = genome
            .loci
            .iter()
            .flat_map(|l| l.parameters.iter())
            .map(|p| p.value.as_unit_scalar())
            .collect();

        let values = Trait::ALL
            .iter()
            .map(|&t| {
                let raw: f64 = scalars
                    .iter()
                    .enumerate()
                    .map(|(i, &s)| WeightedSumMap::weight(t, i) * s)
                    .sum();
                (t, raw.clamp(0.0, 1.0))
            })
            .collect();

        Phenotype { values }
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
}
