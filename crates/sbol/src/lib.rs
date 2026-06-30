//! Hand-rolled **SBOL3 (v3.1.0) JSON-LD subset** + the deterministic **in-core closed-world validator**
//! (ADR-037 draft, slice SB1). `std` + `serde` + `serde_json` ONLY — no RDF engine, no Oxigraph, no
//! network (proposal §2.2 / §5). Full-fidelity conformance, SBOL2 import, Turtle/RDF-XML and SynBioHub
//! round-trips live OUT of process behind the `oracle-slim` boundary pattern ([`SubprocessValidator`],
//! deferred to SB4) — never linked here.
//!
//! Invariants baked in:
//! - **inv #1** — one linked crate (this one), `std`+serde; everything heavy/networked is subprocess-only.
//! - **inv #2** — SBOL lives in the core genome layer; the SO/SBO roles wrap the EXISTING [`genome`] ids
//!   ([`SoRole`] = [`genome::SoTermId`]), so "role is data" is preserved. `godot/` never validates.
//! - **inv #3** — every id is an ORDERED integer newtype ([`IriId`]); nothing here iterates a `HashMap`;
//!   the IRI string table is a `Vec<String>` interned in document order ([`Interner`]); the validator is a
//!   pure, RNG-free, ordered traversal returning a deterministically-ordered `Vec<SbolViolation>`.
//! - **inv #5** — validation sits behind the [`SbolValidator`] trait: [`InCoreValidator`] (default, pure)
//!   and the [`SubprocessValidator`] stub (the SB4 boundary seam).
//! - **inv #7** — SBOL3 v3.1.0, BioBrick RFC10 grammar, and the SO/SBO term numbers are pinned ([`so`]/[`sbo`]).
//!
//! **SB1 is hash-neutral + unwired:** this crate is a standalone, additive projection. It is NOT called by
//! `SpeciesSpec::build`/`apply_edit` (that wiring + the closed-world gate is SB2). The `Genome → SbolDocument`
//! [`mapping`][genome_to_document] here is a pure, round-tripping VIEW only.

#![forbid(unsafe_code)]

use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ===========================================================================================
// Pinned ontology term numbers (inv #7). These are the bare SO/SBO accession integers (the IRI is
// `https://identifiers.org/SO:%07d` / `SBO:%07d`); we REFERENCE them, never bundle the ontology files.
// ===========================================================================================

/// Sequence Ontology term numbers used as `Component`/`SequenceFeature` roles (proposal §2.3).
pub mod so {
    /// SO:0000167 promoter.
    pub const PROMOTER: u32 = 167;
    /// SO:0000139 ribosome entry site (RBS).
    pub const RBS: u32 = 139;
    /// SO:0000316 CDS.
    pub const CDS: u32 = 316;
    /// SO:0000141 terminator.
    pub const TERMINATOR: u32 = 141;
    /// SO:0000704 gene — the coarse, opaque part the baked species use.
    pub const GENE: u32 = 704;

    /// The closed-world allow-set of SO roles usable as `Component.role` / `SequenceFeature.role`
    /// (§3.2 rejection condition 1). A role outside this set is an *unknown role*.
    pub const ROLE_ALLOW_SET: [u32; 5] = [PROMOTER, RBS, CDS, TERMINATOR, GENE];
}

/// Systems Biology Ontology term numbers used as `Interaction`/`Participation` roles (proposal §3.1).
pub mod sbo {
    /// SBO:0000010 reactant.
    pub const REACTANT: u32 = 10;
    /// SBO:0000011 product.
    pub const PRODUCT: u32 = 11;
    /// SBO:0000020 inhibitor.
    pub const INHIBITOR: u32 = 20;
    /// SBO:0000459 stimulator.
    pub const STIMULATOR: u32 = 459;

    /// SBO:0000176 biochemical reaction.
    pub const BIOCHEMICAL_REACTION: u32 = 176;
    /// SBO:0000169 inhibition.
    pub const INHIBITION: u32 = 169;
    /// SBO:0000170 stimulation.
    pub const STIMULATION: u32 = 170;
    /// SBO:0000589 genetic production.
    pub const GENETIC_PRODUCTION: u32 = 589;

    /// Allow-set of SBO interaction *types* for `Interaction.types` (§3.2 rejection condition 4).
    pub const INTERACTION_TYPE_ALLOW_SET: [u32; 4] = [
        BIOCHEMICAL_REACTION,
        INHIBITION,
        STIMULATION,
        GENETIC_PRODUCTION,
    ];
    /// Allow-set of SBO participant roles for `Participation.roles`.
    pub const PARTICIPATION_ROLE_ALLOW_SET: [u32; 4] = [REACTANT, PRODUCT, INHIBITOR, STIMULATOR];
}

/// The JSON-LD `@context` anchor for this fixed subset (a referenced IRI constant, not a bundled file).
pub const SBOL3_CONTEXT: &str = "https://sbolstandard.org/v3";
/// Default document namespace for designs minted in-repo (the SBOL `<namespace>/<displayId>` prefix).
pub const DEFAULT_NAMESPACE: &str = "https://gene-sim.local/sbol";
/// IUPAC nucleic-acid encoding IRI for `Sequence.encoding` (referenced, EDAM format_1207).
pub const IUPAC_DNA_ENCODING: &str = "https://identifiers.org/edam:format_1207";

fn default_context() -> String {
    SBOL3_CONTEXT.to_string()
}

// ===========================================================================================
// Interned, ORDERED ids (inv #3). `IriId(u32)` indexes a document-order `Vec<String>` string table —
// NOT a hashed string — so document order is stable and serialization never iterates a `HashMap`.
// ===========================================================================================

/// An interned handle into the document's IRI string table ([`SbolDocument::strings`]). Serialized
/// transparently as the bare index integer; resolved by position, so document order is stable (inv #3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IriId(pub u32);

/// A document-order string interner for IRIs. Dedupes by linear scan (deterministic; no `HashMap`).
#[derive(Debug, Default, Clone)]
pub struct Interner {
    strings: Vec<String>,
}

impl Interner {
    /// A fresh, empty interner.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern `iri`, returning its stable [`IriId`]. Re-interning an existing string returns the same id;
    /// a new string is appended (document order preserved).
    pub fn intern(&mut self, iri: &str) -> IriId {
        if let Some(pos) = self.strings.iter().position(|s| s == iri) {
            return IriId(pos as u32);
        }
        let id = IriId(self.strings.len() as u32);
        self.strings.push(iri.to_string());
        id
    }

    /// Resolve an [`IriId`] back to its IRI string, if present.
    #[must_use]
    pub fn resolve(&self, id: IriId) -> Option<&str> {
        self.strings.get(id.0 as usize).map(String::as_str)
    }

    /// The interned strings, in document order.
    #[must_use]
    pub fn strings(&self) -> &[String] {
        &self.strings
    }

    /// Consume the interner, yielding the string table for an [`SbolDocument`].
    #[must_use]
    pub fn into_strings(self) -> Vec<String> {
        self.strings
    }
}

// ===========================================================================================
// Typed ontology-role newtypes wrapping the EXISTING genome ids (inv #2 — "role is data"). They
// serialize as the bare ontology accession integer in the JSON-LD subset.
// ===========================================================================================

/// An SO term usable as `Component.role` / `SequenceFeature.role`, wrapping [`genome::SoTermId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoRole(pub genome::SoTermId);

impl SoRole {
    /// The bare SO accession integer (e.g. `704` for SO:0000704).
    #[must_use]
    pub const fn term(self) -> u32 {
        (self.0).0
    }
}

impl Serialize for SoRole {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(self.term())
    }
}

impl<'de> Deserialize<'de> for SoRole {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(SoRole(genome::SoTermId(u32::deserialize(deserializer)?)))
    }
}

/// An SBO term for `Interaction.types` / `Participation.roles`. Serialized as the bare SBO accession.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SboRole(pub u32);

// ===========================================================================================
// The SBOL3 subset structs (proposal §2.2 / §5). Ordered fields, `Vec` not `HashMap`.
// ===========================================================================================

/// SBOL3 `Sequence` — primary structure. `elements` is the raw IUPAC string (SBOL3 models `elements` as
/// `xsd:string`); it is validated as IUPAC-DNA by the GATE ([`InCoreValidator`], §3.2 condition 2) via
/// [`genome::DnaSequence::new`], NOT at deserialization — so the validator (not serde) is the closed-world
/// gate even for imported designs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sequence {
    pub identity: IriId,
    pub elements: String,
    pub encoding: IriId,
}

/// SBOL3 `Range` location into a [`Sequence`] — 1-based inclusive per SBOL3 (`1 <= start <= end <= len`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub sequence: IriId,
    pub start: u32,
    pub end: u32,
}

/// SBOL3 `Feature` subtypes used by the subset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Feature {
    /// A reference to another `Component`.
    SubComponent {
        identity: IriId,
        instance_of: IriId,
        location: Option<Range>,
    },
    /// A bare sequence feature carrying an SO role (the projection of a [`genome::Locus`]).
    SequenceFeature {
        identity: IriId,
        role: SoRole,
        location: Range,
    },
    /// An externally-defined molecule (e.g. a ChEBI/UniProt IRI).
    ExternallyDefined { identity: IriId, definition: IriId },
}

impl Feature {
    /// The feature's identity IRI (present on every subtype).
    #[must_use]
    pub fn identity(&self) -> IriId {
        match *self {
            Feature::SubComponent { identity, .. }
            | Feature::SequenceFeature { identity, .. }
            | Feature::ExternallyDefined { identity, .. } => identity,
        }
    }
}

/// SBOL3 `Constraint` restriction kinds (the closed set honored in-core; others are rejected on import).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintKind {
    Precedes,
    Contains,
    Meets,
}

/// SBOL3 `Constraint` — a sequential/topological composition relation between two features.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constraint {
    pub identity: IriId,
    pub restriction: ConstraintKind,
    pub subject: IriId,
    pub object: IriId,
}

/// SBOL3 `Participation` — an SBO-typed role over a feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Participation {
    pub roles: Vec<SboRole>,
    /// The participating feature's identity (must resolve to a [`Feature`] in the document).
    pub participant: IriId,
}

/// SBOL3 `Interaction` — SBO-typed; the trophic/regulatory/metabolic edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interaction {
    pub identity: IriId,
    pub types: Vec<SboRole>,
    pub participations: Vec<Participation>,
}

/// SBOL3 `Component` — the central recursive design entity. `types` = SBO/SO type IRIs; `roles` = SO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Component {
    pub identity: IriId,
    pub types: Vec<IriId>,
    pub roles: Vec<SoRole>,
    pub has_sequence: Vec<IriId>,
    pub features: Vec<Feature>,
    pub constraints: Vec<Constraint>,
    pub interactions: Vec<Interaction>,
}

/// A whole SBOL3 document: the top-levels in stable order + the IRI string table. Serializes as the fixed
/// JSON-LD subset (round-trips byte-stably via [`to_json_ld`]/[`from_json_ld`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SbolDocument {
    #[serde(rename = "@context", default = "default_context")]
    pub context: String,
    pub namespace: IriId,
    /// The IRI string table — every [`IriId`] in this document indexes here (inv #3, document order).
    pub strings: Vec<String>,
    pub components: Vec<Component>,
    pub sequences: Vec<Sequence>,
}

/// Write the document as the fixed JSON-LD subset (pretty, deterministic — stable field + element order).
///
/// # Errors
/// Propagates a [`serde_json::Error`] only if serialization fails (the subset has no failing types).
pub fn to_json_ld(doc: &SbolDocument) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(doc)
}

/// Read a document back from the JSON-LD subset. Does NOT validate biology — that is the GATE's job
/// ([`InCoreValidator`]); this only parses the structure.
///
/// # Errors
/// Propagates a [`serde_json::Error`] on malformed JSON or a shape mismatch.
pub fn from_json_ld(s: &str) -> Result<SbolDocument, serde_json::Error> {
    serde_json::from_str(s)
}

// ===========================================================================================
// Genome -> SbolDocument projection (the read-only SBOL VIEW). Pure + deterministic; UNWIRED (SB2 wires
// the closed-world gate into SpeciesSpec::build / apply_edit — not here).
// ===========================================================================================

/// Project a [`genome::Genome`] into an [`SbolDocument`] using [`DEFAULT_NAMESPACE`].
///
/// Each [`genome::Locus`] becomes (a) a [`Sequence`] from its `DnaSequence` and (b) a
/// [`Feature::SequenceFeature`] whose `role` is the locus's `so_term` and whose 1-based inclusive [`Range`]
/// covers the whole sequence. All loci are gathered, in `loci` order, under one top-level [`Component`].
/// Deterministic: ordered iteration + `format!`-built IRIs, no `HashMap`.
#[must_use]
pub fn genome_to_document(genome: &genome::Genome) -> SbolDocument {
    genome_to_document_ns(genome, DEFAULT_NAMESPACE)
}

/// Like [`genome_to_document`], with an explicit namespace prefix.
#[must_use]
pub fn genome_to_document_ns(genome: &genome::Genome, namespace: &str) -> SbolDocument {
    let mut interner = Interner::new();
    let ns = interner.intern(namespace);
    let encoding = interner.intern(IUPAC_DNA_ENCODING);
    let component_id = interner.intern(&format!("{namespace}/genome"));

    let mut sequences = Vec::with_capacity(genome.loci.len());
    let mut features = Vec::with_capacity(genome.loci.len());
    let mut has_sequence = Vec::with_capacity(genome.loci.len());

    for locus in &genome.loci {
        let seq_id = interner.intern(&format!("{namespace}/seq_{}", locus.id.0));
        let feat_id = interner.intern(&format!("{namespace}/feat_{}", locus.id.0));
        let elements = String::from_utf8(locus.sequence.bases().to_vec())
            .expect("DnaSequence bases are ACGT ASCII");
        let len = elements.len() as u32;
        sequences.push(Sequence {
            identity: seq_id,
            elements,
            encoding,
        });
        features.push(Feature::SequenceFeature {
            identity: feat_id,
            role: SoRole(locus.tags.so_term),
            location: Range {
                sequence: seq_id,
                start: 1,
                end: len,
            },
        });
        has_sequence.push(seq_id);
    }

    let component = Component {
        identity: component_id,
        types: Vec::new(),
        roles: Vec::new(),
        has_sequence,
        features,
        constraints: Vec::new(),
        interactions: Vec::new(),
    };

    SbolDocument {
        context: default_context(),
        namespace: ns,
        strings: interner.into_strings(),
        components: vec![component],
        sequences,
    }
}

// ===========================================================================================
// The closed-world validator behind a trait (inv #5). Pure, RNG-free, ordered traversal.
// ===========================================================================================

/// A deterministic, ordered well-formedness violation (proposal §3.2). On rejection the gate returns an
/// ORDERED `Vec<SbolViolation>` (mirrors `crispr::EditFailure`'s explicit-failure discipline — a rejected
/// design is never a silent success).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SbolViolation {
    /// §3.2(1) — an SO `role` not in [`so::ROLE_ALLOW_SET`] (not usable as a `Component.role`). `owner` is
    /// the `Component` or `SequenceFeature` identity carrying it.
    UnknownRole { owner: IriId, role: SoRole },
    /// §3.2(2a) — a `Sequence` whose `elements` are not valid IUPAC-DNA (`bad_base_index` = first offender).
    MalformedSequence {
        sequence: IriId,
        bad_base_index: usize,
    },
    /// §3.2(2b) — a [`Range`] outside its referenced [`Sequence`] (or referencing a missing sequence).
    RangeOutOfBounds {
        feature: IriId,
        sequence: IriId,
        start: u32,
        end: u32,
        length: u32,
    },
    /// §3.2(3) — the ordered fine-grained part roles of a `Component` do not parse against the RFC10
    /// transcription-unit production (`(promoter rbs cds+ terminator)+`).
    GrammarViolation { component: IriId },
    /// §3.2(4) — an `Interaction` with no/illegal SBO grounding (see [`InteractionDefect`]).
    UngroundedInteraction {
        interaction: IriId,
        defect: InteractionDefect,
    },
}

impl SbolViolation {
    /// A stable variant tag — used to assert that violation *order* is structural (independent of the
    /// concrete [`IriId`] values), per the determinism acceptance criterion.
    #[must_use]
    pub fn tag(&self) -> &'static str {
        match self {
            SbolViolation::UnknownRole { .. } => "unknown_role",
            SbolViolation::MalformedSequence { .. } => "malformed_sequence",
            SbolViolation::RangeOutOfBounds { .. } => "range_out_of_bounds",
            SbolViolation::GrammarViolation { .. } => "grammar_violation",
            SbolViolation::UngroundedInteraction { .. } => "ungrounded_interaction",
        }
    }
}

/// Why an [`Interaction`] is ungrounded (§3.2 condition 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionDefect {
    /// The interaction has no type, or a type outside [`sbo::INTERACTION_TYPE_ALLOW_SET`].
    MissingOrIllegalType,
    /// A `Participation` role outside [`sbo::PARTICIPATION_ROLE_ALLOW_SET`].
    IllegalParticipationRole,
    /// A `Participation.participant` referencing no [`Feature`] in the document.
    ParticipantNotFound,
}

/// The closed-world validator seam (inv #5): pure in-core default + a subprocess boundary impl.
pub trait SbolValidator {
    /// Validate `doc`, returning an ORDERED `Vec<SbolViolation>` (empty ⇒ the design is well-formed and may
    /// enter the sim). Implementations MUST be deterministic.
    fn validate(&self, doc: &SbolDocument) -> Vec<SbolViolation>;
}

/// The default, pure, RNG-free in-core validator (§3.2). Traversal order is fixed (and documented on
/// [`InCoreValidator::validate`]) so the returned `Vec` depends only on document structure — never on
/// [`IriId`] numeric values or any `HashMap` iteration (inv #3).
#[derive(Debug, Default, Clone, Copy)]
pub struct InCoreValidator;

impl SbolValidator for InCoreValidator {
    /// Canonical violation order:
    /// 1. for each `Component` in document order:
    ///    a. each `Component.roles`,
    ///    b. each `Feature` in order — its role, then its range,
    ///    c. the component's RFC10 grammar,
    ///    d. each `Interaction` in order — its type, then each `Participation` (role, then participant);
    /// 2. for each `Sequence` in document order — IUPAC-DNA validity.
    fn validate(&self, doc: &SbolDocument) -> Vec<SbolViolation> {
        let mut out = Vec::new();

        // Ordered set of all feature identities (for participant grounding). A sorted Vec, never a HashMap.
        let mut feature_ids: Vec<u32> = Vec::new();
        for c in &doc.components {
            for f in &c.features {
                feature_ids.push(f.identity().0);
            }
        }
        feature_ids.sort_unstable();

        for c in &doc.components {
            // (1a) Component roles.
            for role in &c.roles {
                if !is_known_so_role(*role) {
                    out.push(SbolViolation::UnknownRole {
                        owner: c.identity,
                        role: *role,
                    });
                }
            }

            // (1b) Features: role + range; collect part classes for the grammar.
            let mut parts: Vec<PartClass> = Vec::new();
            for f in &c.features {
                match f {
                    Feature::SequenceFeature {
                        identity,
                        role,
                        location,
                    } => {
                        if !is_known_so_role(*role) {
                            out.push(SbolViolation::UnknownRole {
                                owner: *identity,
                                role: *role,
                            });
                        }
                        check_range(doc, *identity, location, &mut out);
                        parts.push(PartClass::from_role(*role));
                    }
                    Feature::SubComponent {
                        identity, location, ..
                    } => {
                        if let Some(loc) = location {
                            check_range(doc, *identity, loc, &mut out);
                        }
                    }
                    Feature::ExternallyDefined { .. } => {}
                }
            }

            // (1c) RFC10 grammar over the fine-grained parts only (opaque genes don't constrain ordering).
            let fine: Vec<PartClass> = parts
                .into_iter()
                .filter(PartClass::is_fine_grained)
                .collect();
            if !parse_transcription_units(&fine) {
                out.push(SbolViolation::GrammarViolation {
                    component: c.identity,
                });
            }

            // (1d) Interactions.
            for itx in &c.interactions {
                if itx.types.is_empty() || itx.types.iter().any(|t| !is_known_sbo_interaction(*t)) {
                    out.push(SbolViolation::UngroundedInteraction {
                        interaction: itx.identity,
                        defect: InteractionDefect::MissingOrIllegalType,
                    });
                }
                for p in &itx.participations {
                    if p.roles.iter().any(|r| !is_known_sbo_participation(*r)) {
                        out.push(SbolViolation::UngroundedInteraction {
                            interaction: itx.identity,
                            defect: InteractionDefect::IllegalParticipationRole,
                        });
                    }
                    if feature_ids.binary_search(&p.participant.0).is_err() {
                        out.push(SbolViolation::UngroundedInteraction {
                            interaction: itx.identity,
                            defect: InteractionDefect::ParticipantNotFound,
                        });
                    }
                }
            }
        }

        // (2) Sequence IUPAC-DNA validity — the gate re-checks even imported designs.
        for s in &doc.sequences {
            if let Err(i) = genome::DnaSequence::new(s.elements.as_bytes()) {
                out.push(SbolViolation::MalformedSequence {
                    sequence: s.identity,
                    bad_base_index: i,
                });
            }
        }

        out
    }
}

/// **SB4 boundary stub.** The trait-impl shape for the out-of-process reference validator
/// (`sbol-cli`/pySBOL3 via the `oracle-slim` subprocess pattern). It is a documented NO-OP here: SB1 does
/// NOT spawn a subprocess — see proposal §2.2/§5 and ADR-037. Constructing it asserts the trait seam exists.
#[derive(Debug, Default, Clone)]
pub struct SubprocessValidator {
    // TODO(SB4): the resolved `sbol-cli`/pySBOL3 binary path + args (the oracle-slim `resolve_*_bin` pattern).
    _private: (),
}

impl SbolValidator for SubprocessValidator {
    /// TODO(SB4): shell out to the reference validator, parse its JSON-LD report into `SbolViolation`s.
    /// Deliberately a no-op in SB1 (unwired) — returns no violations so it never masks the in-core gate.
    fn validate(&self, _doc: &SbolDocument) -> Vec<SbolViolation> {
        Vec::new()
    }
}

// --- validator helpers -------------------------------------------------------------------------

fn is_known_so_role(role: SoRole) -> bool {
    so::ROLE_ALLOW_SET.contains(&role.term())
}

fn is_known_sbo_interaction(t: SboRole) -> bool {
    sbo::INTERACTION_TYPE_ALLOW_SET.contains(&t.0)
}

fn is_known_sbo_participation(r: SboRole) -> bool {
    sbo::PARTICIPATION_ROLE_ALLOW_SET.contains(&r.0)
}

fn check_range(doc: &SbolDocument, owner: IriId, r: &Range, out: &mut Vec<SbolViolation>) {
    let len = doc
        .sequences
        .iter()
        .find(|s| s.identity == r.sequence)
        .map(|s| s.elements.len() as u32);
    let in_bounds = match len {
        Some(l) => r.start >= 1 && r.end >= r.start && r.end <= l,
        None => false, // an unresolved sequence reference cannot ground the range
    };
    if !in_bounds {
        out.push(SbolViolation::RangeOutOfBounds {
            feature: owner,
            sequence: r.sequence,
            start: r.start,
            end: r.end,
            length: len.unwrap_or(0),
        });
    }
}

/// BioBrick RFC10 part class derived from an SO role (proposal §2.3). `Opaque` = a gene or any role that
/// is not a fine-grained transcription-unit part; opaque parts do not constrain composition order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PartClass {
    Promoter,
    Rbs,
    Cds,
    Terminator,
    Opaque,
}

impl PartClass {
    fn from_role(role: SoRole) -> Self {
        match role.term() {
            so::PROMOTER => PartClass::Promoter,
            so::RBS => PartClass::Rbs,
            so::CDS => PartClass::Cds,
            so::TERMINATOR => PartClass::Terminator,
            _ => PartClass::Opaque,
        }
    }

    fn is_fine_grained(&self) -> bool {
        !matches!(self, PartClass::Opaque)
    }
}

/// Parse `parts` (already filtered to fine-grained classes) against the RFC10 production
/// `(promoter rbs cds+ terminator)+`. An EMPTY list is valid (a composition of opaque genes declares no
/// transcription unit). Deterministic recursive-descent over an ordered slice.
fn parse_transcription_units(parts: &[PartClass]) -> bool {
    if parts.is_empty() {
        return true;
    }
    let mut i = 0;
    while i < parts.len() {
        if parts[i] != PartClass::Promoter {
            return false;
        }
        i += 1;
        if parts.get(i) != Some(&PartClass::Rbs) {
            return false;
        }
        i += 1;
        if parts.get(i) != Some(&PartClass::Cds) {
            return false;
        }
        while parts.get(i) == Some(&PartClass::Cds) {
            i += 1;
        }
        if parts.get(i) != Some(&PartClass::Terminator) {
            return false;
        }
        i += 1;
    }
    true
}

#[cfg(test)]
mod tests;
