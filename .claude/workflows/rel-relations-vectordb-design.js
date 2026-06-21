export const meta = {
  name: 'rel-relations-vectordb-design',
  description: 'Phase Rel: research vector-DB options at the process boundary + design an inter-species relations model (synergy/parasitism/predation) → ADR-014 draft + slices + relations-view UI sketch (DESIGN/RESEARCH ONLY — no code)',
  phases: [
    { title: 'Research', detail: 'vector-DB-at-the-boundary + embeddings + ecology relation dynamics, in parallel' },
    { title: 'Design', detail: 'two relation-model + service-boundary designs' },
    { title: 'Judge', detail: 'score on inv #1 boundary cleanliness, determinism, gameplay, risk' },
    { title: 'Synthesize', detail: 'ADR-014 draft + slice plan + relations view' },
  ],
}

const GROUND = [
  'PROJECT gene-sim: 2D CRISPR ecosystem sim. Headless deterministic Rust core + read-only Godot renderer. Repo root is cwd; READ files and (for research agents) the web; modify NOTHING — this is a design/research workflow returning proposals only.',
  '',
  'INVARIANTS that constrain Phase Rel:',
  ' #1 GPL / any external service stays at the PROCESS BOUNDARY — invoked as a separate subprocess/service, NEVER linked into the game binary (like SLiM in crates/oracle-slim, which shells out and has no GPL deps). A vector DB MUST live behind this boundary; the sim core must not link a GPL or heavyweight DB crate.',
  ' #2 Biology lives in the Rust core; renderer read-only. The relations VIEW only displays core/service-computed numbers.',
  ' #3 Determinism: the SIM loop is bit-deterministic (seeded ChaCha8Rng, no HashMap iteration, no transcendentals, pinned hash). A vector DB is approximate/non-deterministic by nature — so relation INDEXING/QUERYING (similarity, neighbourhoods) must be kept OUT of the deterministic sim hash, OR fed back only through a deterministic, ordered, quantized channel. Be explicit about which side of the determinism boundary each piece sits.',
  ' #5 Science pluggable behind traits (EnvironmentModifier @ soil.rs:174, ClimateModifier @ climate.rs:99). Relations (mutualism/parasitism/predation) should couple into selection behind a similar trait seam.',
  ' #6 Species/region granularity.',
  '',
  'CONTEXT: Phase Rel (docs/llm/TASKS.md) = a NEW view for inter-species/inter-lineage RELATIONS — mutualism (synergy), parasitism, predation — backed by a VECTOR DB indexing genome/phenotype embeddings (similarity, lineage neighbourhoods). Likely a process-boundary service. NOTE: Phase Rel depends conceptually on R3 multi-species (ADR-013, in design) — assume multiple species exist; design the relation model abstractly so it slots onto whatever R3 lands.',
  '',
  'KEY FILES: crates/oracle-slim/src/lib.rs (the EXISTING process-boundary subprocess pattern — study it as the template for a vector-DB service); crates/sim-core/src/lib.rs (selection @218, the modifier seams usage), soil.rs/climate.rs (the trait seams), crates/genome/src/lib.rs (genome model to embed), crates/sim-core/src/gp.rs (phenotype); docs/llm/SPEC.md (invariants §2.1, §2.2 reuse>reinvent); docs/llm/DECISIONS.md (ADR format ## ADR-NNN -> Context/Decision/Consequences; next free number after ADR-013 is ADR-014).',
].join('\n')

const RESEARCH_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['topic', 'findings', 'options', 'recommendation', 'boundary_and_determinism_notes', 'sources'],
  properties: {
    topic: { type: 'string' },
    findings: { type: 'string' },
    options: { type: 'array', items: {
      type: 'object', additionalProperties: false,
      required: ['name', 'summary', 'license', 'boundary_fit', 'pros', 'cons'],
      properties: {
        name: { type: 'string' },
        summary: { type: 'string' },
        license: { type: 'string', description: 'and whether it can be a SUBPROCESS/service (inv #1) vs a linked crate' },
        boundary_fit: { type: 'string', description: 'how it stays at the process boundary' },
        pros: { type: 'array', items: { type: 'string' } },
        cons: { type: 'array', items: { type: 'string' } },
      },
    } },
    recommendation: { type: 'string' },
    boundary_and_determinism_notes: { type: 'string' },
    sources: { type: 'array', items: { type: 'string' } },
  },
}

const DESIGN_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['design_name', 'one_liner', 'relation_model', 'selection_coupling', 'vectordb_service', 'embedding', 'determinism_boundary', 'relations_view_ui', 'pros', 'cons', 'risk'],
  properties: {
    design_name: { type: 'string' },
    one_liner: { type: 'string' },
    relation_model: { type: 'string', description: 'how synergy/parasitism/predation are represented between species/lineages (ordered, deterministic where it touches the sim)' },
    selection_coupling: { type: 'string', description: 'the trait seam (a RelationModifier?) by which relations shift fitness (inv #5)' },
    vectordb_service: { type: 'string', description: 'the process-boundary service: protocol, lifecycle, where it sits (inv #1)' },
    embedding: { type: 'string', description: 'what genome/phenotype features become the embedding vector; computed in the core' },
    determinism_boundary: { type: 'string', description: 'exactly what is inside vs outside the pinned sim hash' },
    relations_view_ui: { type: 'string', description: 'the new renderer-only relations view (graph? matrix?)' },
    pros: { type: 'array', items: { type: 'string' } },
    cons: { type: 'array', items: { type: 'string' } },
    risk: { type: 'string', enum: ['low', 'medium', 'high'] },
  },
}

const ADR_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['adr_title', 'context', 'decision', 'consequences', 'slices', 'invariant_risks', 'open_questions_for_human'],
  properties: {
    adr_title: { type: 'string', description: 'e.g. "ADR-014 — Inter-species relations + vector-DB index (roadmap Rel)"' },
    context: { type: 'string' },
    decision: { type: 'string' },
    consequences: { type: 'string' },
    slices: { type: 'array', items: {
      type: 'object', additionalProperties: false,
      required: ['id', 'goal', 'touches', 'repin', 'acceptance'],
      properties: {
        id: { type: 'string' }, goal: { type: 'string' },
        touches: { type: 'array', items: { type: 'string' } },
        repin: { type: 'boolean' }, acceptance: { type: 'string' },
      },
    } },
    invariant_risks: { type: 'array', items: { type: 'string' } },
    open_questions_for_human: { type: 'array', items: { type: 'string' } },
  },
}

phase('Research')
const TOPICS = [
  { key: 'vectordb-boundary', q: 'Survey VECTOR DB options that can run as a SEPARATE PROCESS/SERVICE (inv #1 — not linked into the game binary), suitable for a small desktop sim: e.g. Qdrant, Milvus, LanceDB, sqlite-vec, Chroma, usearch, hnswlib. For EACH: license, can-it-be-a-subprocess-or-embedded-file vs must-link, footprint, Rust client at the boundary. Use the web. Recommend the best 1-2 for a pinned-version desktop PoC.' },
  { key: 'embedding-strategy', q: 'Design what to EMBED: which genome (crates/genome/src/lib.rs) + phenotype (gp.rs) features become a fixed-length vector, computed deterministically IN THE CORE, so similarity/lineage-neighbourhood queries are meaningful. Keep the embedding computation deterministic + ordered; the DB query result is advisory (outside the sim hash).' },
  { key: 'ecology-relations', q: 'Model the ECOLOGY: mutualism/synergy, parasitism, predation between species/lineages — how each shifts fitness/population, classic dynamics (Lotka-Volterra-style but with NO transcendentals in the sim path — use bounded multiply/add/clamp). Read soil.rs/climate.rs modifier seams as the coupling template. Propose a RelationModifier seam.' },
  { key: 'oracle-boundary-pattern', q: 'Study crates/oracle-slim/src/lib.rs as the EXISTING process-boundary subprocess template (how it shells out, stays dependency-free, no GPL link). Extract the reusable pattern a vector-DB service should follow, and how the harness/sim talks to it without breaking determinism or inv #1.' },
]
const research = await parallel(TOPICS.map((t) => () =>
  agent(GROUND + '\n\nRESEARCH TOPIC: ' + t.q + '\nReturn structured options + a recommendation. Research/analysis only — change nothing.',
    { label: 'research:' + t.key, phase: 'Research', schema: RESEARCH_SCHEMA, effort: 'high' })))
const researchJson = JSON.stringify(research.filter(Boolean), null, 1)

phase('Design')
const ANGLES = [
  { key: 'lean-sqlite-vec', angle: 'A LEAN design: an embedded file-based vector index (e.g. sqlite-vec / lancedb) run as a sidecar the harness writes to; relations are a small deterministic in-core matrix, the vector DB only powers the VIEW (similarity/neighbourhoods), fully OUTSIDE the sim hash. Minimize determinism risk.' },
  { key: 'service-rich', angle: 'A RICHER design: a vector-DB SERVICE (e.g. Qdrant subprocess) indexing per-generation embeddings, enabling lineage-neighbourhood queries that feed a quantized, ordered relation signal back into selection via a RelationModifier seam (carefully inside the determinism boundary). Maximize gameplay depth.' },
]
const designs = await parallel(ANGLES.map((a) => () =>
  agent(GROUND + '\n\nRESEARCH:\n' + researchJson + '\n\nDESIGN this relations + vector-DB architecture: ' + a.angle
    + '\nBe explicit about the determinism boundary (what is inside vs outside the pinned hash), the inv #1 process boundary, the RelationModifier seam, the embedding, and the new renderer-only relations view. Analysis only.',
    { label: 'design:' + a.key, phase: 'Design', schema: DESIGN_SCHEMA, effort: 'high' })))
const designsJson = JSON.stringify(designs.filter(Boolean), null, 1)

phase('Judge')
const judging = await parallel(['boundary-and-determinism', 'gameplay-and-feasibility'].map((lens) => () =>
  agent(GROUND + '\n\nCANDIDATE DESIGNS:\n' + designsJson
    + '\n\nAs the "' + lens + '" judge, critique each design. Be ruthless about inv #1 (no linked external DB) and inv #3 (keep non-deterministic vector queries OUT of the sim hash, or quantize+order them). Name the SAFER default and what to graft from the other.',
    { label: 'judge:' + lens, phase: 'Judge' })))
const judgedText = judging.filter(Boolean).join('\n\n---\n\n')

phase('Synthesize')
const adr = await agent(
  GROUND + '\n\nRESEARCH:\n' + researchJson + '\n\nDESIGNS:\n' + designsJson + '\n\nJUDGES:\n' + judgedText
  + '\n\nSynthesize an ADR-014 DRAFT for Phase Rel: choose the relation model + the vector-DB-at-the-boundary approach (default to the lower determinism/inv-#1 risk, graft richer ideas where safe). Break it into gated slices (touched files + repin flag + acceptance), list invariant risks, and the open questions for human sign-off. Propose this AFTER R3 multi-species. Proposal only — no implementation.',
  { label: 'synthesize:adr-014', phase: 'Synthesize', schema: ADR_SCHEMA, effort: 'high' })

return { phase: 'Rel relations + vector DB', adr_draft: adr, designs: designs.filter(Boolean), research: research.filter(Boolean) }
