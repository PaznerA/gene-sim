//! Parametric genome data model — the data-model source of truth (SPEC §4, docs/llm/TAXONOMY.md).
//!
//! Invariants baked in here:
//! - **Loci are data, not code.** A locus's "kind" is an ontology tag ([`SoTermId`]), never a Rust enum.
//! - **Determinism (inv. #3).** Everything is ordered ([`Vec`] + integer-newtype ids); nothing in this
//!   crate iterates a `HashMap`. Ids are stable small integers, not hashed strings.
//!
//! Stage 0 implements the genome itself; `CasVariant`/`Edit` (crates/crispr) and the
//! `GenotypePhenotypeMap` (crates/sim-core) land in later stages per TAXONOMY.md.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Stable, ordered handle into a [`Genome`]'s locus list (equals the index in `Genome::loci`).
///
/// Serde-(de)serializable so it can ride in replay logs (`actions.ndjson`, SPEC §5): a trivial `u32`
/// newtype, serialized transparently as the bare integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LocusId(pub u32);

/// Stable handle for a [`Parameter`] within a [`Locus`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParamId(pub u32);

/// Sequence Ontology feature type — the locus "kind" (gene / exon / CDS / promoter / …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SoTermId(pub u32);

/// Gene Ontology function reference (molecular function / biological process / cellular component).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GoTermId(pub u32);

/// A DNA-ish sequence: validated upper-case ACGT bytes. Used by `crates/crispr` for PAM finding / edits.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DnaSequence(Vec<u8>);

impl DnaSequence {
    /// Build a sequence, validating that every base is one of `A`, `C`, `G`, `T` (upper-case).
    ///
    /// Returns the 0-based index of the first offending byte on failure.
    pub fn new(bytes: impl Into<Vec<u8>>) -> Result<Self, usize> {
        let bytes = bytes.into();
        if let Some(i) = bytes
            .iter()
            .position(|b| !matches!(b, b'A' | b'C' | b'G' | b'T'))
        {
            return Err(i);
        }
        Ok(Self(bytes))
    }

    /// The raw ACGT bytes.
    #[must_use]
    pub fn bases(&self) -> &[u8] {
        &self.0
    }

    /// Number of bases.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the sequence is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A typed parameter value that carries its own valid domain.
///
/// **Invariant:** a `ParamValue` is always within its domain (see [`ParamValue::is_valid`]). Edits in
/// later stages must preserve this — an edit never yields an invalid genome (SPEC §10.4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamValue {
    /// Continuous; invariant: `min <= value <= max`.
    Numeric { value: f64, min: f64, max: f64 },
    /// Categorical; invariant: `value < cardinality`.
    Enum { value: u16, cardinality: u16 },
    /// Boolean.
    Bool(bool),
}

impl ParamValue {
    /// Whether the value lies within its declared domain.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        match *self {
            ParamValue::Numeric { value, min, max } => min <= max && value >= min && value <= max,
            ParamValue::Enum { value, cardinality } => cardinality > 0 && value < cardinality,
            ParamValue::Bool(_) => true,
        }
    }

    /// Clamp the value back into its domain (no-op for `Enum`/`Bool`, which can't drift continuously).
    pub fn clamp_into_domain(&mut self) {
        if let ParamValue::Numeric { value, min, max } = self {
            if *min <= *max {
                *value = value.clamp(*min, *max);
            }
        }
    }

    /// A normalized scalar in `[0, 1]` for downstream phenotype math (deterministic; pure).
    #[must_use]
    pub fn as_unit_scalar(&self) -> f64 {
        match *self {
            ParamValue::Numeric { value, min, max } => {
                if max > min {
                    ((value - min) / (max - min)).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            }
            ParamValue::Enum { value, cardinality } => {
                if cardinality > 1 {
                    f64::from(value) / f64::from(cardinality - 1)
                } else {
                    0.0
                }
            }
            ParamValue::Bool(b) => {
                if b {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

/// A typed parameter with a stable id.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parameter {
    pub id: ParamId,
    pub value: ParamValue,
}

/// Ontology references describing a locus's kind and function (data, not Rust types).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OntologyTags {
    /// The feature type — the locus "kind".
    pub so_term: SoTermId,
    /// Function references, in stable order.
    pub go_refs: Vec<GoTermId>,
}

/// A locus: a stable id, a DNA-ish sequence, typed parameters, and ontology tags.
#[derive(Debug, Clone, PartialEq)]
pub struct Locus {
    pub id: LocusId,
    pub name: String,
    pub sequence: DnaSequence,
    pub parameters: Vec<Parameter>,
    pub tags: OntologyTags,
}

/// A genome: an ordered set of [`Locus`]es plus a model/schema version (recorded for replay, SPEC §5/§6).
#[derive(Debug, Clone, PartialEq)]
pub struct Genome {
    pub version: u16,
    /// Loci in a fixed order. Iterate this — never a `HashMap` (inv. #3).
    pub loci: Vec<Locus>,
}

impl Genome {
    /// Whether every parameter in every locus is within its domain.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.loci
            .iter()
            .all(|l| l.parameters.iter().all(|p| p.value.is_valid()))
    }

    /// Total parameter count across all loci.
    #[must_use]
    pub fn parameter_count(&self) -> usize {
        self.loci.iter().map(|l| l.parameters.len()).sum()
    }

    /// Find a locus by id (linear scan over the ordered list; ids equal their index in practice).
    #[must_use]
    pub fn locus(&self, id: LocusId) -> Option<&Locus> {
        self.loci.iter().find(|l| l.id == id)
    }
}

/// A tiny, deterministic built-in genome for Stage 0 wiring, tests, and benches.
///
/// Two loci with numeric/enum/bool parameters and SO/GO tags. Fixed content → reproducible.
#[must_use]
pub fn sample_genome() -> Genome {
    Genome {
        version: 1,
        loci: vec![
            Locus {
                id: LocusId(0),
                name: "growth_locus".to_string(),
                // SO:0000704 "gene"; bases chosen to contain NGG/TTTV PAM material for Stage 1.
                sequence: DnaSequence::new(*b"ACGTGGACGTTTTAGGCCGG")
                    .expect("sample bases are valid ACGT"),
                parameters: vec![
                    Parameter {
                        id: ParamId(0),
                        value: ParamValue::Numeric {
                            value: 0.6,
                            min: 0.0,
                            max: 1.0,
                        },
                    },
                    Parameter {
                        id: ParamId(1),
                        value: ParamValue::Enum {
                            value: 1,
                            cardinality: 4,
                        },
                    },
                ],
                tags: OntologyTags {
                    so_term: SoTermId(704),
                    go_refs: vec![GoTermId(8150)],
                },
            },
            Locus {
                id: LocusId(1),
                name: "killswitch_locus".to_string(),
                sequence: DnaSequence::new(*b"TTTACCGGTTTAGGGCAAAC")
                    .expect("sample bases are valid ACGT"),
                parameters: vec![Parameter {
                    id: ParamId(0),
                    value: ParamValue::Bool(false),
                }],
                tags: OntologyTags {
                    so_term: SoTermId(704),
                    go_refs: vec![GoTermId(3674)],
                },
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dna_validation_accepts_acgt_and_locates_bad_byte() {
        assert!(DnaSequence::new(*b"ACGTACGT").is_ok());
        assert_eq!(DnaSequence::new(*b"ACGXACGT"), Err(3));
        assert!(DnaSequence::new(*b"acgt").is_err()); // lower-case rejected
    }

    #[test]
    fn param_validity_and_clamp() {
        let mut v = ParamValue::Numeric {
            value: 5.0,
            min: 0.0,
            max: 1.0,
        };
        assert!(!v.is_valid());
        v.clamp_into_domain();
        assert!(v.is_valid());
        assert_eq!(
            v,
            ParamValue::Numeric {
                value: 1.0,
                min: 0.0,
                max: 1.0
            }
        );

        assert!(ParamValue::Enum {
            value: 3,
            cardinality: 4
        }
        .is_valid());
        assert!(!ParamValue::Enum {
            value: 4,
            cardinality: 4
        }
        .is_valid());
        assert!(ParamValue::Bool(true).is_valid());
    }

    #[test]
    fn unit_scalar_is_in_range() {
        for v in [
            ParamValue::Numeric {
                value: 0.6,
                min: 0.0,
                max: 1.0,
            },
            ParamValue::Enum {
                value: 1,
                cardinality: 4,
            },
            ParamValue::Bool(true),
        ] {
            let s = v.as_unit_scalar();
            assert!((0.0..=1.0).contains(&s), "scalar {s} out of [0,1]");
        }
    }

    #[test]
    fn locus_id_serde_round_trips_as_bare_u32() {
        // SPEC §5: LocusId rides in replay logs. `#[serde(transparent)]` ⇒ encoded as the bare integer.
        let id = LocusId(7);
        let json = serde_json::to_string(&id).expect("serialize LocusId");
        assert_eq!(json, "7", "LocusId should serialize as a bare u32");
        let back: LocusId = serde_json::from_str(&json).expect("deserialize LocusId");
        assert_eq!(back, id);
    }

    #[test]
    fn sample_genome_is_valid_and_stable() {
        let g = sample_genome();
        assert!(g.is_valid());
        assert_eq!(g.loci.len(), 2);
        assert_eq!(g.parameter_count(), 3);
        // Determinism: constructing twice yields byte-identical genomes.
        assert_eq!(g, sample_genome());
        assert_eq!(g.locus(LocusId(0)).unwrap().name, "growth_locus");
    }
}
