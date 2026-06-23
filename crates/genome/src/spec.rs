//! JSON **species starter** format (ADR-017 / multi-species prep): a serde DTO that defines a species' genome
//! as inert DATA, plus a single VALIDATING builder that is the only `SpeciesSpec → Genome` path. This is the
//! vehicle for the layered ecosystem — the abstract default, a real E. coli, a decomposer — and the save
//! format for a future in-game genome editor.
//!
//! Why a DTO instead of `#[derive(Serialize)]` on the genome types: (a) [`DnaSequence`] wraps a PRIVATE buffer
//! with a validating ACGT constructor — a naive derive would bypass validation; (b) [`ParamValue`] needs a
//! STABLE, editor-readable on-disk tagged repr decoupled from the in-memory enum; (c) keeps the data-model
//! crate's invariants enforced at exactly one place. Out-of-domain values are STRICT-REJECTED at load (the
//! file is authoritative input — not clamped like a runtime edit).

use serde::{Deserialize, Serialize};

use crate::{
    DnaSequence, Genome, GoTermId, Locus, LocusId, OntologyTags, ParamId, ParamValue, Parameter,
    SoTermId,
};

/// A species starter: metadata + a fully-specified genome. The on-disk JSON shape (`data/species/<key>.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeciesSpec {
    /// On-disk schema version of THIS DTO (not the genome's `version`).
    pub format_version: u16,
    /// Stable kebab key (== file stem); roster ordering + lineage id.
    pub key: String,
    /// Human-readable name (shown in the UI).
    pub name: String,
    /// Ecological metadata (all optional).
    #[serde(default)]
    pub niche: Niche,
    /// The species genome.
    pub genome: GenomeSpec,
}

/// Ecological metadata for a species (all optional, defaulting to a neutral niche).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Niche {
    /// Organisms spawned at reset (`0` ⇒ the caller's default).
    #[serde(default)]
    pub entity_count: u32,
    #[serde(default)]
    pub description: String,
    /// Optimal temperature in `[0, 1]`, if the species has one.
    #[serde(default)]
    pub temp_optimum: Option<f64>,
    /// Parent species key — RESERVED for fork/speciation provenance (ADR-017).
    #[serde(default)]
    pub parent_key: Option<String>,
    /// Trophic role override (ADR-013 F4): one of
    /// `"autotroph"`|`"heterotroph"`|`"mixotroph"`|`"decomposer"` (case-insensitive at the boundary).
    /// `None` ⇒ the boundary falls back to `gp::role_for(key)` — byte-neutral for every existing spec
    /// (serde default). E. coli sets `"decomposer"` to close the obligate plant→detritus→microbe loop.
    #[serde(default)]
    pub trophic_role: Option<String>,
    /// The declared HOST species KEY for an obligate symbiont (ADR-019 S5). `None` ⇒ no host (every
    /// non-symbiont; serde default → byte-neutral for every existing spec). An obligate symbiont
    /// (`trophic_role == "symbiont"`) names the species key it draws its sole income from; the sim-core boundary
    /// resolves it to a registry `SpeciesId` at register/reset (this crate has no dependency on sim-core, inv #2).
    #[serde(default)]
    pub host_key: Option<String>,
}

/// A genome as data: a model version + ordered loci.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenomeSpec {
    pub version: u16,
    pub loci: Vec<LocusSpec>,
}

/// A locus as data. `id` MUST equal its index in `loci` (the builder asserts it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocusSpec {
    pub id: u32,
    pub name: String,
    /// ACGT bases (validated by [`DnaSequence::new`] on build).
    pub sequence: String,
    pub parameters: Vec<ParameterSpec>,
    pub tags: OntologyTagsSpec,
}

/// A parameter as data.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ParameterSpec {
    pub id: u32,
    pub value: ParamValueSpec,
}

/// A typed parameter value, internally-tagged for editor readability (`{"kind":"numeric","value":..,..}`).
/// This is the ON-DISK CONTRACT — pinned now.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParamValueSpec {
    Numeric { value: f64, min: f64, max: f64 },
    Enum { value: u16, cardinality: u16 },
    Bool { value: bool },
}

/// Ontology tags as data (bare `u32` term ids).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OntologyTagsSpec {
    pub so_term: u32,
    #[serde(default)]
    pub go_refs: Vec<u32>,
}

/// The validated result of [`SpeciesSpec::build`]: a real [`Genome`] + the species metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct BuiltSpecies {
    pub key: String,
    pub name: String,
    pub entity_count: u32,
    /// Trophic role override carried verbatim from `niche.trophic_role` (ADR-013 F4). `None` ⇒ the roster
    /// boundary uses `gp::role_for(key)`. Inert DATA in this crate — resolved to a `gp::TrophicRole` only at
    /// the sim-core boundary (this crate has no dependency on `sim-core`, inv #2).
    pub trophic_role: Option<String>,
    /// The declared HOST species KEY for an obligate symbiont, carried verbatim from `niche.host_key` (ADR-019
    /// S5). `None` for every non-symbiont. Resolved to a registry `SpeciesId` only at the sim-core boundary.
    pub host_key: Option<String>,
    pub genome: Genome,
}

/// A structured, path-carrying build error (no I/O, no RNG — a pure function of the spec).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecError {
    /// A locus' `id` did not equal its index in `loci`.
    LocusIdMismatch {
        locus: usize,
        expected: u32,
        got: u32,
    },
    /// A locus sequence had a non-ACGT byte at `byte`.
    BadBase { locus: usize, byte: usize },
    /// A parameter value fell outside its declared domain.
    ParamOutOfDomain { locus: usize, param: usize },
    /// The assembled genome failed [`Genome::is_valid`].
    GenomeInvalid,
}

impl std::fmt::Display for SpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpecError::LocusIdMismatch {
                locus,
                expected,
                got,
            } => {
                write!(f, "locus {locus}: id {got} must equal its index {expected}")
            }
            SpecError::BadBase { locus, byte } => {
                write!(f, "locus {locus}: non-ACGT base at byte {byte}")
            }
            SpecError::ParamOutOfDomain { locus, param } => {
                write!(
                    f,
                    "locus {locus} param {param}: value out of its declared domain"
                )
            }
            SpecError::GenomeInvalid => write!(f, "assembled genome is invalid"),
        }
    }
}

impl std::error::Error for SpecError {}

impl SpeciesSpec {
    /// Validate + assemble the spec into a real [`Genome`] (the single `SpeciesSpec → Genome` path). Pure: no
    /// I/O, no RNG, no `HashMap` — Vec-ordered, a deterministic function of the spec bytes.
    ///
    /// # Errors
    /// Returns a structured [`SpecError`] for a locus-id/index mismatch, a non-ACGT base, an out-of-domain
    /// parameter, or an otherwise-invalid genome — surfacing the offending path to the (editor) author.
    pub fn build(&self) -> Result<BuiltSpecies, SpecError> {
        let mut loci = Vec::with_capacity(self.genome.loci.len());
        for (i, ls) in self.genome.loci.iter().enumerate() {
            if ls.id as usize != i {
                return Err(SpecError::LocusIdMismatch {
                    locus: i,
                    expected: i as u32,
                    got: ls.id,
                });
            }
            let sequence = DnaSequence::new(ls.sequence.clone().into_bytes())
                .map_err(|byte| SpecError::BadBase { locus: i, byte })?;
            let mut parameters = Vec::with_capacity(ls.parameters.len());
            for (pi, ps) in ls.parameters.iter().enumerate() {
                let value = match ps.value {
                    ParamValueSpec::Numeric { value, min, max } => {
                        ParamValue::Numeric { value, min, max }
                    }
                    ParamValueSpec::Enum { value, cardinality } => {
                        ParamValue::Enum { value, cardinality }
                    }
                    ParamValueSpec::Bool { value } => ParamValue::Bool(value),
                };
                if !value.is_valid() {
                    return Err(SpecError::ParamOutOfDomain {
                        locus: i,
                        param: pi,
                    });
                }
                parameters.push(Parameter {
                    id: ParamId(ps.id),
                    value,
                });
            }
            loci.push(Locus {
                id: LocusId(ls.id),
                name: ls.name.clone(),
                sequence,
                parameters,
                tags: OntologyTags {
                    so_term: SoTermId(ls.tags.so_term),
                    go_refs: ls.tags.go_refs.iter().map(|&g| GoTermId(g)).collect(),
                },
            });
        }
        let genome = Genome {
            version: self.genome.version,
            loci,
        };
        if !genome.is_valid() {
            return Err(SpecError::GenomeInvalid);
        }
        Ok(BuiltSpecies {
            key: self.key.clone(),
            name: self.name.clone(),
            entity_count: self.niche.entity_count,
            trophic_role: self.niche.trophic_role.clone(),
            host_key: self.niche.host_key.clone(),
            genome,
        })
    }

    /// The inverse of [`build`](Self::build): a [`SpeciesSpec`] from an in-memory [`Genome`] — for the editor's
    /// SAVE and the golden round-trip test. Lossless: `from_genome(g, ..).build()?.genome == g`.
    #[must_use]
    pub fn from_genome(genome: &Genome, key: &str, name: &str) -> Self {
        SpeciesSpec {
            format_version: 1,
            key: key.to_string(),
            name: name.to_string(),
            niche: Niche::default(),
            genome: GenomeSpec {
                version: genome.version,
                loci: genome
                    .loci
                    .iter()
                    .map(|l| LocusSpec {
                        id: l.id.0,
                        name: l.name.clone(),
                        sequence: String::from_utf8(l.sequence.bases().to_vec())
                            .expect("ACGT bases are valid UTF-8"),
                        parameters: l
                            .parameters
                            .iter()
                            .map(|p| ParameterSpec {
                                id: p.id.0,
                                value: match p.value {
                                    ParamValue::Numeric { value, min, max } => {
                                        ParamValueSpec::Numeric { value, min, max }
                                    }
                                    ParamValue::Enum { value, cardinality } => {
                                        ParamValueSpec::Enum { value, cardinality }
                                    }
                                    ParamValue::Bool(value) => ParamValueSpec::Bool { value },
                                },
                            })
                            .collect(),
                        tags: OntologyTagsSpec {
                            so_term: l.tags.so_term.0,
                            go_refs: l.tags.go_refs.iter().map(|g| g.0).collect(),
                        },
                    })
                    .collect(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_genome_build_round_trips_sample() {
        let g = crate::sample_genome();
        let spec = SpeciesSpec::from_genome(&g, "default", "Abstract default");
        assert_eq!(
            spec.build().expect("build").genome,
            g,
            "from_genome → build must reproduce the genome losslessly"
        );
    }

    #[test]
    fn json_round_trips_through_serde() {
        let g = crate::sample_genome();
        let spec = SpeciesSpec::from_genome(&g, "default", "Abstract default");
        let json = serde_json::to_string(&spec).expect("serialize");
        let back: SpeciesSpec = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, spec);
        assert_eq!(back.build().expect("build").genome, g);
    }

    #[test]
    fn niche_trophic_role_serde_default_is_none() {
        // ADR-013 F4: the new `trophic_role` field is serde-default `None`, so every existing spec (no such
        // key in its `niche` block) deserializes byte-neutrally — the override is opt-in DATA.
        let n = Niche::default();
        assert_eq!(n.trophic_role, None);
        // A niche JSON WITHOUT the key still parses (default None) — proves existing specs are unaffected.
        let json = r#"{ "entity_count": 5, "description": "x" }"#;
        let parsed: Niche = serde_json::from_str(json).expect("niche parses without trophic_role");
        assert_eq!(parsed.trophic_role, None);
    }

    #[test]
    fn niche_host_key_serde_default_is_none_and_round_trips_to_built_species() {
        // ADR-019 S5: the new `host_key` field is serde-default `None`, so every existing spec (no such key)
        // deserializes byte-neutrally; an obligate symbiont sets it and it reaches the BuiltSpecies verbatim.
        let n = Niche::default();
        assert_eq!(n.host_key, None);
        // A niche JSON WITHOUT host_key still parses (default None) — proves existing specs are unaffected.
        let json = r#"{ "entity_count": 5, "trophic_role": "decomposer" }"#;
        let parsed: Niche = serde_json::from_str(json).expect("niche parses without host_key");
        assert_eq!(parsed.host_key, None);
        // A symbiont declaring a host_key carries it verbatim into the BuiltSpecies.
        let mut spec =
            SpeciesSpec::from_genome(&crate::sample_genome(), "carsonella", "Carsonella");
        spec.niche.trophic_role = Some("symbiont".to_string());
        spec.niche.host_key = Some("default".to_string());
        let back: SpeciesSpec =
            serde_json::from_str(&serde_json::to_string(&spec).expect("ser")).expect("de");
        assert_eq!(back.niche.host_key.as_deref(), Some("default"));
        let built = back.build().expect("build");
        assert_eq!(built.host_key.as_deref(), Some("default"));
    }

    #[test]
    fn niche_trophic_role_override_round_trips_and_reaches_built_species() {
        // The override is carried verbatim from `niche.trophic_role` into the BuiltSpecies (inert DATA in this
        // crate — resolved to a role only at the sim-core boundary, inv #2).
        let mut spec = SpeciesSpec::from_genome(&crate::sample_genome(), "ecoli-core", "E. coli");
        spec.niche.trophic_role = Some("decomposer".to_string());
        let json = serde_json::to_string(&spec).expect("serialize");
        let back: SpeciesSpec = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.niche.trophic_role.as_deref(), Some("decomposer"));
        let built = back.build().expect("build");
        assert_eq!(built.trophic_role.as_deref(), Some("decomposer"));
    }

    #[test]
    fn build_rejects_bad_base() {
        let mut spec = SpeciesSpec::from_genome(&crate::sample_genome(), "x", "x");
        spec.genome.loci[0].sequence = "ACGTX".to_string();
        assert!(matches!(
            spec.build(),
            Err(SpecError::BadBase { locus: 0, .. })
        ));
    }

    #[test]
    fn build_rejects_locus_id_mismatch() {
        let mut spec = SpeciesSpec::from_genome(&crate::sample_genome(), "x", "x");
        spec.genome.loci[1].id = 99;
        assert!(matches!(
            spec.build(),
            Err(SpecError::LocusIdMismatch { locus: 1, .. })
        ));
    }

    #[test]
    fn build_rejects_out_of_domain_param() {
        let mut spec = SpeciesSpec::from_genome(&crate::sample_genome(), "x", "x");
        spec.genome.loci[0].parameters[0].value = ParamValueSpec::Numeric {
            value: 5.0,
            min: 0.0,
            max: 1.0,
        };
        assert!(matches!(
            spec.build(),
            Err(SpecError::ParamOutOfDomain { .. })
        ));
    }
}
