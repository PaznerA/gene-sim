export const meta = {
  name: 'sbol-biobricks-integration-design',
  description:
    'RESEARCH + DESIGN ONLY (no production code): the DEEP SBOL (Synthetic Biology Open Language) integration with a CLOSED-WORLD rule — no genetic process executes unless it is defined as an SBOL construct — plus the BioBricks Foundation discipline (standard, characterized, composable, registry-grounded parts under an assembly grammar). Web-research SBOL3 vs SBOL2 (data model, RDF/JSON-LD, tooling maturity, RUST feasibility), the BioBrick assembly standards (RFC10 / Type-IIS / MoClo / 3A) + the iGEM Registry of Standard Biological Parts (BBa_* ids, characterization, data licensing), and the SBOL tool licenses (libSBOL / pySBOL3 / libSBOLj / SynBioHub) vs inv #1 + how Sequence Ontology / GO / SBO map to SBOL roles. KEY GROUNDING (the model is already ontology-first): crates/genome Locus carries tags.so_term (a Sequence Ontology term — SBOL roles ARE SO terms), real NCBI CDS sequences, and crispr edits DnaSequence — so SBOL is a FORMALIZATION + a VALIDATION GATE, not new biology. Adversarially verify the bio/SBOL/licensing claims, then EXPAND the seed docs/llm/proposals/sbol-biobricks-integration-draft.md into a pinned spec + an ADR-draft + the candidate inv #8 proposal (the genetic vocabulary is closed over SBOL) + the DETERMINISM RE-PIN plan for SB2 (re-grounding Genome/Locus could move the hash — a 🔁 STOP-THE-LINE re-pin) + the slice plan SB1..SB6. DESIGN ONLY — doc-only, hash-neutral, NO Cargo/code change. Foundational → produces a sign-off-ready package, never implements.',
  whenToUse: 'On the user go for the SBOL+BioBricks foundational epic. Produces the buildable spec + ADR-draft + inv #8 proposal + the re-pin plan, for human sign-off BEFORE any implementation (touches the genome model + a candidate new invariant + a likely determinism re-pin).',
  phases: [{ title: 'Research' }, { title: 'Synthesize' }, { title: 'Review' }],
}

phase('Research')
const TOPICS = [
  { key: 'sbol-model-and-rust', q: 'SBOL3 vs SBOL2: the data model (Component/Feature/Sequence/Interaction/Participation/Constraint; roles = Sequence Ontology; SBO for interactions), the serialization (SBOL3 RDF/JSON-LD vs SBOL2 XML), tooling maturity (libSBOL/pySBOL3/sbol2/libSBOLj), and whether any usable RUST SBOL library exists or a focused SBOL3 subset must be hand-rolled. What is the MINIMAL SBOL3 subset that can represent a bacterial genome of SO-typed loci + sequences + trophic interactions, and round-trip it?' },
  { key: 'biobricks-and-registry', q: 'The BioBricks Foundation engineering discipline: the assembly standards (classic RFC10 prefix/suffix EcoRI/XbaI/SpeI/PstI; Type-IIS / Golden Gate / MoClo; 3A assembly) and which is the cleanest COMPOSITION GRAMMAR for a game. The iGEM Registry of Standard Biological Parts: BBa_* part ids, part TYPES (promoter/RBS/CDS/terminator), CHARACTERIZATION datasheets, and the DATA LICENSING of the registry (can a non-commercial game ground its parts in real registry parts?).' },
  { key: 'licensing-and-ontologies', q: 'Licenses of the SBOL ecosystem tools (libSBOL, pySBOL2/3, libSBOLj, SynBioHub) — Apache/MIT/BSD vs any copyleft (GPL) — for inv #1 (a GPL tool must stay a subprocess, never linked). How Sequence Ontology (SO), Gene Ontology (GO), and the Systems Biology Ontology (SBO) are used as SBOL roles/types, and where to obtain pinnable ontology/term sets. Any SBOL VALIDATOR usable as a subprocess for conformance.' },
]
const research = await parallel(TOPICS.map((t) => () =>
  agent(
    'RESEARCH (web) for the SBOL+BioBricks deep-integration design. Use web search/fetch for AUTHORITATIVE, CURRENT, CITED sources (sbolstandard.org, the SBOL3 spec, BioBricks Foundation / iGEM parts.igem.org, the SO/GO/SBO ontologies, the tool repos + their LICENSE files). Topic "' + t.key + '": ' + t.q + '\n\nReturn a CITED findings brief (URLs): the concrete facts, the recommended choice for THIS project (a deterministic Rust headless sim, std+serde-preferring, non-commercial, inv #1 GPL-at-the-boundary), and any licensing/maturity RISK. Flag anything you could NOT verify. Do NOT edit any file.',
    { label: 'research:' + t.key, phase: 'Research', agentType: 'general-purpose' },
  ),
))
const researchText = research.filter(Boolean).map((t, i) => '### ' + TOPICS[i].key + '\n' + (typeof t === 'string' ? t : JSON.stringify(t))).join('\n\n')

phase('Synthesize')
const spec = await agent(
  'You are the JUDGE. Using the cited research below + the existing SEED docs/llm/proposals/sbol-biobricks-integration-draft.md (READ IT) + the real model (crates/genome Locus/OntologyTags/SoTermId/DnaSequence; data/species/*.json; crates/crispr edits), EXPAND the seed in place into a PINNED, buildable spec.\n\nRESEARCH:\n' + researchText.slice(0, 9000) + '\n\n' +
  'Edit docs/llm/proposals/sbol-biobricks-integration-draft.md so it contains, with CITATIONS: (1) the pinned choices — SBOL3 vs SBOL2, the in-core Rust SBOL subset (the exact structs) vs subprocess validation split, the BioBrick assembly standard/grammar, the registry-grounding approach, the pinned ontology/term sources + tool versions (inv #7); (2) the CLOSED-WORLD validation gate — precisely how a Locus/edit/Interaction is grounded + validated against SBOL, and exactly when an ungrounded process is REJECTED (deterministic, in front of genotype→phenotype); (3) the candidate inv #8 (genetic vocabulary closed over SBOL) — the exact wording + whether to elevate it or keep it ADR-pinned; (4) the inv #1 LICENSING verdict for every SBOL tool/dataset touched (linked-OK vs subprocess-only vs avoid); (5) the DETERMINISM RE-PIN PLAN for SB2 — how to re-ground Genome/Locus as SBOL without an ACCIDENTAL hash move (preserve meaning; any hash change is an ADR-owned 🔁 re-pin + the multi-ISA gate); (6) the slice plan SB1..SB6 with deps + which are hash-neutral vs the 🔁🛑 SB2; (7) an ADR-draft block (reserve a free ADR number — note ADR-035 is pending on a branch, so use a number beyond the current max; state it). Keep "DESIGN ONLY — sign-off required" at the top. Then RETURN a ~400-word executive summary: the pinned choices, the closed-world gate, the inv #8 recommendation, the licensing verdict, the re-pin risk, and the slice list. WRITE the doc; touch NO production code/Cargo.',
  { label: 'judge-expands-spec', phase: 'Synthesize', agentType: 'general-purpose' },
)

phase('Review')
const RSCHEMA = {
  type: 'object',
  required: ['bio_sbol_claims_accurate_cited', 'closed_world_gate_deterministic', 'licensing_inv1_clean', 'repin_plan_sound', 'issues'],
  properties: {
    bio_sbol_claims_accurate_cited: { type: 'boolean', description: 'The SBOL/BioBricks/registry/ontology claims are accurate + CITED to authoritative sources (sbolstandard.org spec, iGEM/BioBricks, SO/GO/SBO); the SBOL3-vs-SBOL2 + Rust-feasibility call is justified; unverifiable claims are flagged, not asserted.' },
    closed_world_gate_deterministic: { type: 'boolean', description: 'The closed-world rule (no genetic process without an SBOL grounding) is a DETERMINISTIC validation gate in front of genotype→phenotype (a pure function of the design — no RNG/wall-clock), and the spec is precise about what is grounded + when an ungrounded process is rejected.' },
    licensing_inv1_clean: { type: 'boolean', description: 'inv #1: every SBOL tool/dataset has a license verdict — linked crates are non-copyleft (Apache/MIT/BSD); any GPL/copyleft tool is subprocess-only (never linked); registry/ontology data licensing is checked against the non-commercial stance. No GPL crate proposed for linking.' },
    repin_plan_sound: { type: 'boolean', description: 'The SB2 re-grounding has a sound determinism plan: re-expressing Genome/Locus as SBOL preserves meaning; any hash move is an ADR-owned 🔁 re-pin with the multi-ISA gate + sign-off (NOT an accidental drift); SB1/SB3-6 hash-neutrality is argued.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const reviews = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    'Adversarially review the EXPANDED SBOL+BioBricks design in docs/llm/proposals/sbol-biobricks-integration-draft.md (DESIGN-ONLY — verify it is accurate, implementable, and invariant-safe; no code exists yet). Read the doc + crates/genome (the Locus/OntologyTags model) + CLAUDE.md inv #1/#2/#3/#5/#7. Skeptic #' + i + ' — default each boolean FALSE unless the doc convincingly + with citations establishes it. Hunt: an inaccurate/uncited SBOL or BioBricks or registry claim (or a hallucinated Rust SBOL crate / part id); a closed-world gate that is NOT deterministic (RNG/wall-clock/hash-order leaking into validation); a LICENSING miss (a GPL/copyleft SBOL tool proposed for LINKING instead of a subprocess — inv #1; or non-commercial-incompatible registry data); an unsound re-pin plan (an accidental hash move, or SB2 not flagged 🔁🛑/no multi-ISA gate); the inv #8 elevation under- or over-claimed. Report the structured verdict + EXPLICITLY whether the licensing is inv #1-clean and the re-pin plan is sound. Do NOT edit.',
    { label: 'review:skeptic' + i, phase: 'Review', schema: RSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => reviews.filter((s) => s[k]).length
const keys = ['bio_sbol_claims_accurate_cited', 'closed_world_gate_deterministic', 'licensing_inv1_clean', 'repin_plan_sound']
const sound = keys.every((k) => tally(k) >= 2)
return {
  summary: typeof spec === 'string' ? spec.slice(0, 1400) : spec,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: reviews.flatMap((s) => s.issues || []),
  verdict: sound ? 'DESIGN SOUND — SBOL+BioBricks spec ready to present for sign-off (claims cited, gate deterministic, licensing inv #1-clean, re-pin plan sound)' : 'DESIGN NEEDS WORK — gaps flagged before sign-off',
}
