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
    GrowthRate,
    Reflectance,
    DroughtTolerance,
    Fecundity,
    KillSwitchLinkage,
}

impl Trait {
    /// The traits in canonical (declaration) order — the order a [`Phenotype`] stores them in.
    /// A fixed array (not a `HashMap`) so iteration is deterministic (invariant #3).
    pub const ALL: [Trait; 5] = [
        Trait::GrowthRate,
        Trait::Reflectance,
        Trait::DroughtTolerance,
        Trait::Fecundity,
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
/// The scheme is deliberately simple and transparent: each trait is anchored on one parameter slot with a
/// small spillover from its neighbour, so an edit to one parameter has a legible, bounded effect.
/// * `GrowthRate`        = `1.0 * p0`
/// * `Reflectance`       = `0.5 * p1 + 0.5 * p2`
/// * `DroughtTolerance`  = `1.0 * p2`
/// * `Fecundity`         = `0.7 * p0 + 0.3 * p1`
/// * `KillSwitchLinkage` = `1.0 * p2` (the kill-switch bool slot in `sample_genome`)
///
/// Parameter slots beyond those named contribute weight `0.0`. Because every `as_unit_scalar()` is in
/// `[0, 1]` and the named weights per trait sum to `<= 1.0`, the raw sum is already in `[0, 1]`; the final
/// `clamp` is a belt-and-braces guarantee for arbitrary genomes (property AC3).
#[derive(Debug, Clone, Copy, Default)]
pub struct WeightedSumMap;

impl WeightedSumMap {
    /// Weight of flat parameter index `i` toward trait `t`. Pure; the single source of truth for the scheme
    /// documented on [`WeightedSumMap`]. Unknown slots weigh `0.0`.
    #[must_use]
    fn weight(t: Trait, i: usize) -> f64 {
        match (t, i) {
            (Trait::GrowthRate, 0) => 1.0,
            (Trait::Reflectance, 1) => 0.5,
            (Trait::Reflectance, 2) => 0.5,
            (Trait::DroughtTolerance, 2) => 1.0,
            (Trait::Fecundity, 0) => 0.7,
            (Trait::Fecundity, 1) => 0.3,
            (Trait::KillSwitchLinkage, 2) => 1.0,
            _ => 0.0,
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
