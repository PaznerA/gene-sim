export const meta = {
  name: 'sbol-model-validator-impl',
  description:
    'SB1 (hash-neutral, the first SBOL slice — per the pinned spec docs/llm/proposals/sbol-biobricks-integration-draft.md): a NEW crates/sbol crate (std+serde ONLY — NO RDF engine, NO network, NO GPL) holding the §5-minimal SBOL3 subset (SbolDocument / Component / Feature / Sequence / Range / Interaction / Participation / Constraint), wrapping the EXISTING genome types (SoTermId, DnaSequence) with interned ORDERED ids (IriId(u32), no HashMap) + a fixed JSON-LD subset (de)serialization via serde_json; PLUS the in-core deterministic CLOSED-WORLD validator behind the inv #5 trait (InCoreValidator default + a SubprocessValidator stub for the SB4 boundary). The validator is a PURE, RNG-free, ordered function of the design: it returns an ordered Vec<SbolViolation> and rejects on unknown SO role, malformed sequence/range, BioBrick-RFC10 grammar violation, or an ungrounded interaction (no SBO type) — the §3 rejection conditions. HASH-NEUTRAL: crates/sbol is NEW + UNWIRED — it is NOT yet called from SpeciesSpec::build / apply_edit (that wiring is SB2, the 🔁 re-pin); sim-core is untouched → the pinned literal 0x47a0_3c8f_6701_f240 is trivially byte-identical. inv #1 (no GPL/heavy/networked dep — the Oxigraph/sbol-cli/pySBOL3 path stays subprocess-only, deferred to SB4) + inv #3 (deterministic, ordered, no HashMap iteration) + inv #5 (validator behind a trait) + inv #7 (a new pinned crate). Read the spec §3 (the gate) + §5 (the structs) first. Then gate + adversarially verify.',
  whenToUse: 'After the SBOL design spec (signed-off direction). The hash-neutral SBOL model + in-core validator scaffold — the foundation the closed-world gate (SB2) + the parts catalog (SB3) build on. SB2 (the genotype->phenotype wiring + re-pin) needs separate sign-off.',
  phases: [{ title: 'Impl' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Impl')
const s1 = await agent(
  'Implement SB1 — the SBOL model + in-core validator (hash-neutral; a NEW crates/sbol crate; the pinned literal 0x47a0_3c8f_6701_f240 stays trivially unmoved because crates/sbol is NEW + UNWIRED). READ FIRST: docs/llm/proposals/sbol-biobricks-integration-draft.md — §5 (the exact SBOL3-subset structs to hand-roll), §3 (the closed-world validator: §3.1 grounding, §3.2 the 5 deterministic rejection conditions, the ordered Vec<SbolViolation>, the InCoreValidator/SubprocessValidator trait), §2.x (the dep stance: std+serde+serde_json only, NO RDF engine/Oxigraph/network, the subprocess path deferred to SB4). Then READ the surface it wraps: crates/genome/src/lib.rs (SoTermId, GoTermId, DnaSequence, Locus, OntologyTags, Genome — the SBOL Component wraps these), crates/genome/Cargo.toml + the workspace Cargo.toml (serde/serde_json are already pinned workspace deps; do NOT add a new external crate). crates/oracle-slim (the subprocess-boundary pattern, for the SubprocessValidator stub shape — do NOT implement the subprocess here, just the trait + a stub). CLAUDE.md inv #1 (no GPL/heavy/networked dep linked) + inv #3 (deterministic — ordered ids, no HashMap iteration, no randf/RNG) + inv #5 (the validator is behind a trait) + inv #7 (pin the new crate; reuse workspace serde).\n\n' +
  '  - NEW crates/sbol (added to the workspace, std+serde+serde_json ONLY): the §5 structs — SbolDocument, Component (role: SoTermId, sequence: Option<DnaSequence>, features, interactions), Feature (+ Range), Sequence, Interaction (sbo type + Participations), Participation, Constraint. Use INTERNED ORDERED ids (IriId(u32), a Vec-backed interner — NOT a hashed string; document order stable, inv #3). serde Serialize/Deserialize as the fixed JSON-LD subset (round-trips via serde_json; NO RDF library). NO HashMap in any structure the validator or serialization iterates.\n' +
  '  - The CLOSED-WORLD VALIDATOR behind a trait (inv #5): trait SbolValidator { fn validate(&self, doc: &SbolDocument) -> Vec<SbolViolation>; }. InCoreValidator (default, pure, RNG-free) implements the §3.2 rejection conditions — unknown/illegal SO role for a Component; malformed Sequence (invalid DnaSequence) or out-of-bounds Range; a BioBrick-RFC10 grammar violation (transcription-unit part ordering); an ungrounded Interaction (missing/illegal SBO type or a Participation referencing a nonexistent Feature). Return an ORDERED Vec<SbolViolation> (deterministic order — iterate the ordered structures, never a HashMap). Add a SubprocessValidator STUB (the trait impl shape for the SB4 boundary tool — a documented TODO/no-op, NOT a real subprocess here).\n' +
  '  - A Locus/Genome -> Component MAPPING (read-only, the SBOL VIEW): build an SbolDocument from a genome::Genome (each Locus -> a Component with role = its SoTermId, sequence = its DnaSequence). This is the projection SB2 will validate; here it is a pure function + round-trips. Do NOT wire it into SpeciesSpec::build / apply_edit (that is SB2) — keep crates/sbol standalone + UNWIRED.\n' +
  '  - TESTS (crates/sbol): the structs serde JSON-LD-subset round-trip byte-stable; a Genome->SbolDocument mapping is deterministic + round-trips; the InCoreValidator ACCEPTS a well-formed doc (empty violations) and REJECTS each of the §3.2 conditions (one test per condition) with the EXPECTED violation; the violation order is deterministic (same doc -> same Vec, and shuffling input ids does not reorder the output beyond the defined order); cargo tree -p sbol shows std+serde+serde_json ONLY (no RDF/network/GPL).\n' +
  '  - HASH-NEUTRALITY: cargo test -p sim-core --features determinism (0x47a0_3c8f_6701_f240 byte-identical — crates/sbol is new + unwired, sim-core untouched) + cargo test -p sbol. Confirm crates/sbol adds NO external dep beyond the workspace serde/serde_json. Do NOT commit. Report: the crate structs (§5 fidelity), the validator trait + the rejection conditions covered, the Genome->SbolDocument mapping, the test results, and confirm 0x47a0 unmoved + unwired + std+serde-only.',
  { label: 'impl', phase: 'Impl', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  'Run bash tools/gate.sh for gene-sim (generous timeout ~15 min). SB1 (the new crates/sbol) must be GREEN: fmt, clippy, test (incl. the new crates/sbol round-trip + validator accept/reject + ordering tests), determinism MUST stay 0x47a0_3c8f_6701_f240 BYTE-IDENTICAL (crates/sbol is NEW + UNWIRED — sim-core untouched; report explicitly), license green (crates/sbol is std+serde+serde_json ONLY — NO RDF/Oxigraph/network/GPL dep linked; check_license.sh + cargo tree -p sbol), godot-reader + livesim green. Report every gate PASS/FAIL with exact errors + EXPLICITLY whether 0x47a0 is unmoved + whether crates/sbol added any external dependency. No fixes, no commit.',
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral_unwired_literal_unmoved', 'crates_sbol_std_serde_no_rdf', 'model_wraps_existing_ontology_ordered', 'validator_deterministic_ordered_closed_world', 'issues'],
  properties: {
    hash_neutral_unwired_literal_unmoved: { type: 'boolean', description: 'crates/sbol is NEW + UNWIRED — it is NOT called from SpeciesSpec::build / apply_edit / any sim path (that is SB2); sim-core is untouched → the pinned literal 0x47a0_3c8f_6701_f240 is byte-identical by construction.' },
    crates_sbol_std_serde_no_rdf: { type: 'boolean', description: 'inv #1/#5: crates/sbol links ONLY std + the workspace serde/serde_json — NO RDF engine (Oxigraph/oxrdf), NO network client, NO GPL/heavy/JVM/Python dep (those stay subprocess-only, deferred to SB4); cargo tree -p sbol confirms it.' },
    model_wraps_existing_ontology_ordered: { type: 'boolean', description: 'The §5 SBOL3-subset structs (SbolDocument/Component/Feature/Sequence/Range/Interaction/Participation/Constraint) wrap the EXISTING genome SoTermId/DnaSequence, use interned ORDERED ids (no HashMap), serde JSON-LD-subset round-trip byte-stable, and a Genome->SbolDocument mapping is deterministic + round-trips.' },
    validator_deterministic_ordered_closed_world: { type: 'boolean', description: 'inv #3/#5: the InCoreValidator (behind the SbolValidator trait, + a SubprocessValidator stub) is a PURE RNG-free ordered function returning an ordered Vec<SbolViolation>; it ACCEPTS a well-formed doc and REJECTS each §3.2 condition (unknown SO role / malformed sequence-range / RFC10 grammar / ungrounded interaction) — proven by tests; no HashMap iteration.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    'Adversarially verify SB1 (the new crates/sbol model + in-core validator — hash-neutral). Read git diff (crates/sbol + Cargo.toml) + docs/llm/proposals/sbol-biobricks-integration-draft.md §3 + §5 + CLAUDE.md inv #1/#3/#5/#7. Skeptic #' + i + ' — default each boolean FALSE unless PROVEN. Hunt: a MOVED pinned literal 0x47a0_3c8f_6701_f240 or crates/sbol being WIRED into the sim path (SpeciesSpec::build/apply_edit/any sim-core call — it must be UNWIRED, that is SB2); a NEW external dependency (RDF engine / Oxigraph / network client / GPL / heavy — must be std+serde+serde_json ONLY; the subprocess path is SB4, not here); HashMap iteration or non-determinism in the structs/validator (must be ordered ids + ordered Vec<SbolViolation>, no RNG); the validator NOT actually rejecting a §3.2 condition (a vacuous/always-accept validator); a struct that does not wrap the existing SoTermId/DnaSequence. Report the structured verdict with file:line + EXPLICITLY whether the literal is unmoved, whether crates/sbol is unwired, and whether any external dep was added. Do NOT edit.',
    { label: 'verify:skeptic' + i, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['hash_neutral_unwired_literal_unmoved', 'crates_sbol_std_serde_no_rdf', 'model_wraps_existing_ontology_ordered', 'validator_deterministic_ordered_closed_world']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  impl: typeof s1 === 'string' ? s1.slice(0, 800) : s1,
  gate: typeof gate === 'string' ? gate.slice(0, 600) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — SBOL model + deterministic in-core validator; std+serde-only, unwired, 0x47a0 byte-identical' : 'NEEDS WORK',
}
