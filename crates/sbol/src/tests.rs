//! SB1 acceptance tests: serde JSON-LD-subset byte-stable round-trips, the deterministic `Genome →
//! SbolDocument` projection, the `InCoreValidator` accept/reject matrix (one test per §3.2 condition), and
//! the deterministic + id-shuffle-stable violation ordering.

use super::*;

// --- builders --------------------------------------------------------------------------------

/// A SequenceFeature over `seq` covering `[1, len]`.
fn seq_feature(identity: IriId, role: u32, seq: IriId, len: u32) -> Feature {
    Feature::SequenceFeature {
        identity,
        role: SoRole(genome::SoTermId(role)),
        location: Range {
            sequence: seq,
            start: 1,
            end: len,
        },
    }
}

/// A well-formed document: one component, two opaque GENE features over valid sequences, and one valid
/// biochemical-reaction interaction whose participants resolve to those features.
fn well_formed_doc() -> SbolDocument {
    let mut it = Interner::new();
    let ns = it.intern("ns");
    let enc = it.intern(IUPAC_DNA_ENCODING);
    let comp = it.intern("comp");
    let seq_a = it.intern("seqA");
    let feat_a = it.intern("featA");
    let seq_b = it.intern("seqB");
    let feat_b = it.intern("featB");
    let itx = it.intern("itx");

    let component = Component {
        identity: comp,
        types: Vec::new(),
        roles: Vec::new(),
        has_sequence: vec![seq_a, seq_b],
        features: vec![
            seq_feature(feat_a, so::GENE, seq_a, 8),
            seq_feature(feat_b, so::GENE, seq_b, 8),
        ],
        constraints: Vec::new(),
        interactions: vec![Interaction {
            identity: itx,
            types: vec![SboRole(sbo::BIOCHEMICAL_REACTION)],
            participations: vec![
                Participation {
                    roles: vec![SboRole(sbo::REACTANT)],
                    participant: feat_a,
                },
                Participation {
                    roles: vec![SboRole(sbo::PRODUCT)],
                    participant: feat_b,
                },
            ],
        }],
    };

    SbolDocument {
        context: SBOL3_CONTEXT.to_string(),
        namespace: ns,
        strings: it.into_strings(),
        components: vec![component],
        sequences: vec![
            Sequence {
                identity: seq_a,
                elements: "ACGTACGT".to_string(),
                encoding: enc,
            },
            Sequence {
                identity: seq_b,
                elements: "GGCCTTAA".to_string(),
                encoding: enc,
            },
        ],
    }
}

/// A document carrying exactly four violations (one of each non-grammar kind), assembled with two distinct
/// IriId interning orders so the id *values* differ while the structural `Vec` order is identical.
fn multi_violation_doc(shuffle: bool) -> SbolDocument {
    let mut it = Interner::new();
    let (ns, enc, comp, seq_a, feat_a, seq_b, feat_b, itx) = if shuffle {
        // Decoys + reversed relative interning => every IriId integer is shifted vs the non-shuffled doc.
        it.intern("urn:decoy:0");
        it.intern("urn:decoy:1");
        let itx = it.intern("itx");
        let feat_b = it.intern("featB");
        let seq_b = it.intern("seqB");
        let feat_a = it.intern("featA");
        let seq_a = it.intern("seqA");
        let comp = it.intern("comp");
        let enc = it.intern(IUPAC_DNA_ENCODING);
        let ns = it.intern("ns");
        (ns, enc, comp, seq_a, feat_a, seq_b, feat_b, itx)
    } else {
        let ns = it.intern("ns");
        let enc = it.intern(IUPAC_DNA_ENCODING);
        let comp = it.intern("comp");
        let seq_a = it.intern("seqA");
        let feat_a = it.intern("featA");
        let seq_b = it.intern("seqB");
        let feat_b = it.intern("featB");
        let itx = it.intern("itx");
        (ns, enc, comp, seq_a, feat_a, seq_b, feat_b, itx)
    };

    let component = Component {
        identity: comp,
        types: Vec::new(),
        roles: Vec::new(),
        has_sequence: vec![seq_a, seq_b],
        features: vec![
            // featA: unknown role (9999) — opaque, so grammar is unaffected; range is valid.
            seq_feature(feat_a, 9999, seq_a, 8),
            // featB: valid gene role, but a range that runs off the end of seqB (len 8).
            Feature::SequenceFeature {
                identity: feat_b,
                role: SoRole(genome::SoTermId(so::GENE)),
                location: Range {
                    sequence: seq_b,
                    start: 1,
                    end: 999,
                },
            },
        ],
        constraints: Vec::new(),
        interactions: vec![Interaction {
            identity: itx,
            types: vec![SboRole(9999)], // illegal SBO interaction type
            participations: vec![
                Participation {
                    roles: vec![SboRole(sbo::REACTANT)],
                    participant: feat_a,
                },
                Participation {
                    roles: vec![SboRole(sbo::PRODUCT)],
                    participant: feat_b,
                },
            ],
        }],
    };

    SbolDocument {
        context: SBOL3_CONTEXT.to_string(),
        namespace: ns,
        strings: it.into_strings(),
        components: vec![component],
        sequences: vec![
            Sequence {
                identity: seq_a,
                elements: "ACGTACGT".to_string(),
                encoding: enc,
            },
            Sequence {
                identity: seq_b,
                elements: "GGCCXTAA".to_string(), // malformed: 'X' at index 4
                encoding: enc,
            },
        ],
    }
}

fn tags(violations: &[SbolViolation]) -> Vec<&'static str> {
    violations.iter().map(SbolViolation::tag).collect()
}

// --- serde JSON-LD subset round-trip ---------------------------------------------------------

#[test]
fn json_ld_subset_round_trips_byte_stable() {
    let doc = well_formed_doc();
    let json1 = to_json_ld(&doc).expect("serialize");
    let back = from_json_ld(&json1).expect("deserialize");
    assert_eq!(back, doc, "round-trip must reconstruct the document");
    let json2 = to_json_ld(&back).expect("re-serialize");
    assert_eq!(json1, json2, "JSON-LD subset must round-trip byte-stably");
    // The roles/ids serialize as bare ontology/index integers (the fixed subset, no RDF expansion).
    assert!(
        json1.contains("\"@context\""),
        "JSON-LD carries an @context"
    );
    assert!(
        json1.contains(&format!("{}", so::GENE)),
        "SO role is a bare integer"
    );
}

// --- Genome -> SbolDocument projection -------------------------------------------------------

#[test]
fn genome_projection_is_deterministic_and_round_trips() {
    let g = genome::sample_genome();
    let doc = genome_to_document(&g);
    // Deterministic: building twice yields byte-identical documents.
    assert_eq!(doc, genome_to_document(&g));
    // Structure: one aggregate component; one Sequence + one SequenceFeature per locus.
    assert_eq!(doc.components.len(), 1);
    assert_eq!(doc.components[0].features.len(), g.loci.len());
    assert_eq!(doc.sequences.len(), g.loci.len());
    // Round-trips through the JSON-LD subset.
    let json = to_json_ld(&doc).expect("serialize");
    let back = from_json_ld(&json).expect("deserialize");
    assert_eq!(back, doc);
}

#[test]
fn projected_baked_genome_validates_clean() {
    // §6.2: every baked design (SO:704 genes, valid ACGT, no ungrounded interactions) already validates,
    // so the closed-world gate would reject nothing — the genotype→phenotype path is untouched.
    let doc = genome_to_document(&genome::sample_genome());
    assert!(
        InCoreValidator.validate(&doc).is_empty(),
        "the baked species must ground to a well-formed SBOL document"
    );
}

// --- InCoreValidator: accept --------------------------------------------------------------

#[test]
fn validator_accepts_well_formed_doc() {
    assert!(InCoreValidator.validate(&well_formed_doc()).is_empty());
}

#[test]
fn validator_accepts_legal_transcription_unit() {
    // promoter -> rbs -> cds -> terminator parses against the RFC10 production.
    let mut it = Interner::new();
    let ns = it.intern("ns");
    let enc = it.intern(IUPAC_DNA_ENCODING);
    let comp = it.intern("comp");
    let s = it.intern("seq");
    let (fp, fr, fc, ft) = (
        it.intern("p"),
        it.intern("r"),
        it.intern("c"),
        it.intern("t"),
    );
    let component = Component {
        identity: comp,
        types: Vec::new(),
        roles: Vec::new(),
        has_sequence: vec![s],
        features: vec![
            seq_feature(fp, so::PROMOTER, s, 8),
            seq_feature(fr, so::RBS, s, 8),
            seq_feature(fc, so::CDS, s, 8),
            seq_feature(ft, so::TERMINATOR, s, 8),
        ],
        constraints: Vec::new(),
        interactions: Vec::new(),
    };
    let doc = SbolDocument {
        context: SBOL3_CONTEXT.to_string(),
        namespace: ns,
        strings: it.into_strings(),
        components: vec![component],
        sequences: vec![Sequence {
            identity: s,
            elements: "ACGTACGT".to_string(),
            encoding: enc,
        }],
    };
    assert!(InCoreValidator.validate(&doc).is_empty());
}

// --- InCoreValidator: reject (one test per §3.2 condition) --------------------------------

#[test]
fn rejects_unknown_so_role() {
    let mut doc = well_formed_doc();
    if let Feature::SequenceFeature { role, .. } = &mut doc.components[0].features[0] {
        *role = SoRole(genome::SoTermId(9999)); // not in the SO allow-set
    } else {
        panic!("expected a SequenceFeature");
    }
    let v = InCoreValidator.validate(&doc);
    assert_eq!(v.len(), 1);
    match v[0] {
        SbolViolation::UnknownRole { role, .. } => assert_eq!(role.term(), 9999),
        ref other => panic!("expected UnknownRole, got {other:?}"),
    }
}

#[test]
fn rejects_malformed_sequence() {
    let mut doc = well_formed_doc();
    doc.sequences[0].elements = "ACGTXACG".to_string(); // 'X' at index 4
    let v = InCoreValidator.validate(&doc);
    assert_eq!(v.len(), 1);
    match v[0] {
        SbolViolation::MalformedSequence { bad_base_index, .. } => assert_eq!(bad_base_index, 4),
        ref other => panic!("expected MalformedSequence, got {other:?}"),
    }
}

#[test]
fn rejects_out_of_bounds_range() {
    let mut doc = well_formed_doc();
    if let Feature::SequenceFeature { location, .. } = &mut doc.components[0].features[0] {
        location.end = 999; // seqA has length 8
    } else {
        panic!("expected a SequenceFeature");
    }
    let v = InCoreValidator.validate(&doc);
    assert_eq!(v.len(), 1);
    match v[0] {
        SbolViolation::RangeOutOfBounds { end, length, .. } => {
            assert_eq!(end, 999);
            assert_eq!(length, 8);
        }
        ref other => panic!("expected RangeOutOfBounds, got {other:?}"),
    }
}

#[test]
fn rejects_rfc10_grammar_violation() {
    // Fine-grained parts in an illegal order (cds before promoter) fail the transcription-unit production.
    let mut it = Interner::new();
    let ns = it.intern("ns");
    let enc = it.intern(IUPAC_DNA_ENCODING);
    let comp = it.intern("comp");
    let s = it.intern("seq");
    let (fc, fp) = (it.intern("c"), it.intern("p"));
    let component = Component {
        identity: comp,
        types: Vec::new(),
        roles: Vec::new(),
        has_sequence: vec![s],
        features: vec![
            seq_feature(fc, so::CDS, s, 8),
            seq_feature(fp, so::PROMOTER, s, 8),
        ],
        constraints: Vec::new(),
        interactions: Vec::new(),
    };
    let doc = SbolDocument {
        context: SBOL3_CONTEXT.to_string(),
        namespace: ns,
        strings: it.into_strings(),
        components: vec![component],
        sequences: vec![Sequence {
            identity: s,
            elements: "ACGTACGT".to_string(),
            encoding: enc,
        }],
    };
    let v = InCoreValidator.validate(&doc);
    assert_eq!(v.len(), 1);
    assert!(matches!(v[0], SbolViolation::GrammarViolation { .. }));
}

#[test]
fn rejects_ungrounded_interaction() {
    // (a) illegal SBO interaction type.
    let mut doc = well_formed_doc();
    doc.components[0].interactions[0].types = vec![SboRole(9999)];
    let v = InCoreValidator.validate(&doc);
    assert_eq!(v.len(), 1);
    assert!(matches!(
        v[0],
        SbolViolation::UngroundedInteraction {
            defect: InteractionDefect::MissingOrIllegalType,
            ..
        }
    ));

    // (b) a participation referencing a feature that does not exist in the document.
    let mut doc = well_formed_doc();
    let ghost = IriId(9_999);
    doc.components[0].interactions[0].participations[0].participant = ghost;
    let v = InCoreValidator.validate(&doc);
    assert_eq!(v.len(), 1);
    assert!(matches!(
        v[0],
        SbolViolation::UngroundedInteraction {
            defect: InteractionDefect::ParticipantNotFound,
            ..
        }
    ));
}

// --- deterministic, id-shuffle-stable violation ordering -------------------------------------

#[test]
fn violation_order_is_deterministic() {
    let doc = multi_violation_doc(false);
    let a = InCoreValidator.validate(&doc);
    let b = InCoreValidator.validate(&doc);
    assert_eq!(a, b, "same document must yield byte-identical violations");
    assert_eq!(
        tags(&a),
        [
            "unknown_role",
            "range_out_of_bounds",
            "ungrounded_interaction",
            "malformed_sequence",
        ],
        "violations follow the canonical structural traversal order"
    );
}

#[test]
fn violation_order_is_stable_under_id_shuffle() {
    let plain = multi_violation_doc(false);
    let shuffled = multi_violation_doc(true);
    // The shuffle genuinely reassigns IriId integers (different interning order).
    assert_ne!(
        plain.strings, shuffled.strings,
        "the id assignment must actually differ"
    );
    // ...yet the violation ORDER (and kinds) is unchanged — it depends on structure, not id values.
    let vp = InCoreValidator.validate(&plain);
    let vs = InCoreValidator.validate(&shuffled);
    assert_eq!(
        tags(&vp),
        tags(&vs),
        "shuffling input ids must not reorder the output beyond the defined order"
    );
}

// --- the subprocess boundary stub exists (inv #5 seam) ---------------------------------------

#[test]
fn subprocess_validator_is_a_noop_stub() {
    // SB1 ships only the trait-impl shape; the real subprocess lands at SB4.
    assert!(SubprocessValidator::default()
        .validate(&well_formed_doc())
        .is_empty());
}
