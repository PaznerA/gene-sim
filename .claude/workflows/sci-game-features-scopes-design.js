export const meta = {
  name: 'sci-game-features-scopes-design',
  description: 'Comprehensive feature proposal for the science-based sim-game + the multi-scale ZOOM-SCOPES architecture (cell → cluster → organ → specimen → ecosystem; future LLM genome-based traits at every scope). DESIGN/RESEARCH ONLY — no code; produces a feature roadmap + scopes spec.',
  phases: [
    { title: 'Understand', detail: 'current scopes/UI + genome model + the joule engine state' },
    { title: 'Design', detail: 'zoom-scopes hierarchy, LLM-genome-at-scopes, the full sci-game feature set, evidence grounding' },
    { title: 'Synthesize', detail: 'feature roadmap proposal + zoom-scopes architecture spec' },
  ],
}

const GROUND = [
  'PROJECT gene-sim: a 2D, evidence-based CRISPR ecosystem SIM-GAME. Headless deterministic Rust core (crates/) + read-only Godot renderer (godot/). Repo root is cwd; READ files + the web (research agents), modify NOTHING — this returns proposals only.',
  '',
  "USER'S STRATEGIC FRAMING (verbatim intent): an EVIDENCE-BASED engine is the priority (\"evidence based engine je hlavní\") — the simulation must stay scientifically grounded, not hand-wavy. We just laid the ecology foundation (ADR-013 CHEMOSTAT-J, a conserved i64-joule economy organisms interact through). The user wants us to STEP BACK and think about the bigger sci-game, especially:",
  ' - ZOOM SCOPES: a multi-scale representation hierarchy. Max zoom = individual CELLS (of an organism); next scope = clusters of cells; next = ORGANS; next = the SPECIMEN placed in a map; out to the ecosystem. In the FUTURE an LLM should cover REAL genome-based traits at ALL levels/scopes (molecular → cellular → tissue/organ → organism → population).',
  ' - A COMPREHENSIVE feature set needed for the sci-based-sim-game (the user knows environment + more species are key — those are in flight; think beyond them).',
  '',
  'HARD INVARIANTS (any feature must respect): #2 ALL biology in the Rust core, renderer read-only (the scopes RENDER core-computed state; an LLM-genome layer computes biology in the core / a process-boundary service, never in GDScript). #3 determinism (seeded, fixed-point, no transcendentals in the sim path). #1 GPL/LLM/external services at the process boundary only (subprocess, never linked — the LLM-genome layer is a boundary service like crates/oracle-slim). #6 species/region agency granularity. #7 pinned versions.',
  '',
  'KEY FILES: godot/main.gd (the current renderer + the existing --zoom scope presets [1 field … 6 cells] + the specimen/L-system view + the ecosystem view); crates/sim-core/src/{lib.rs (the ECS world, ADR-013 substrate), gp.rs (genotype→phenotype), snapshot.rs (per-cell channels the renderer reads), fixed.rs (the new joule apportionment backbone)}; crates/genome/src/lib.rs (the Genome/Locus model + docs/llm/TAXONOMY.md the data-model source of truth); crates/oracle-slim/src/lib.rs (the process-boundary subprocess template — the pattern an LLM-genome service follows); docs/llm/DECISIONS.md (ADR-013 the engine); docs/llm/proposals/*.md (ecology-substrate + r3/rel/phase-t drafts); docs/llm/SPEC.md (invariants + build order: core first, Godot UI last).',
].join('\n')

const UNDERSTAND_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['area', 'current_state', 'gaps_for_sci_game', 'scope_relevance'],
  properties: {
    area: { type: 'string' },
    current_state: { type: 'string', description: 'what exists today, file-anchored' },
    gaps_for_sci_game: { type: 'array', items: { type: 'string' } },
    scope_relevance: { type: 'string', description: 'how this relates to the multi-scale zoom-scopes idea' },
  },
}

const DESIGN_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['topic', 'proposal', 'science_grounding', 'core_vs_boundary', 'determinism_notes', 'features', 'risk'],
  properties: {
    topic: { type: 'string' },
    proposal: { type: 'string', description: 'the concrete design' },
    science_grounding: { type: 'string', description: 'what real science / data it is grounded in (evidence-based mandate)' },
    core_vs_boundary: { type: 'string', description: 'what lives in the Rust core vs a process-boundary service (LLM etc.) vs the read-only renderer' },
    determinism_notes: { type: 'string', description: 'what stays inside vs outside the deterministic sim hash' },
    features: { type: 'array', items: {
      type: 'object', additionalProperties: false,
      required: ['name', 'what', 'depends_on', 'effort'],
      properties: {
        name: { type: 'string' }, what: { type: 'string' },
        depends_on: { type: 'string' }, effort: { type: 'string', enum: ['S', 'M', 'L', 'XL'] },
      },
    } },
    risk: { type: 'string', enum: ['low', 'medium', 'high'] },
  },
}

const ROADMAP_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['vision', 'zoom_scopes', 'llm_genome_layer', 'feature_roadmap', 'evidence_strategy', 'open_questions_for_human'],
  properties: {
    vision: { type: 'string', description: 'the sci-based-sim-game in 2-3 sentences' },
    zoom_scopes: { type: 'array', description: 'the scope hierarchy, max-zoom (cell) to ecosystem', items: {
      type: 'object', additionalProperties: false,
      required: ['scope', 'represents', 'data_shown', 'genome_trait_mapping', 'renderer_or_core'],
      properties: {
        scope: { type: 'string', description: 'e.g. cell / cell-cluster / organ / specimen-in-map / region / ecosystem' },
        represents: { type: 'string' },
        data_shown: { type: 'string' },
        genome_trait_mapping: { type: 'string', description: 'which real genome-based traits surface at this scope (future LLM layer)' },
        renderer_or_core: { type: 'string', description: 'what the core computes vs the renderer draws' },
      },
    } },
    llm_genome_layer: { type: 'string', description: 'how an LLM covers real genome-based traits at all scopes, at the process boundary (inv #1/#2)' },
    feature_roadmap: { type: 'array', items: {
      type: 'object', additionalProperties: false,
      required: ['phase', 'theme', 'features', 'depends_on'],
      properties: {
        phase: { type: 'string' }, theme: { type: 'string' },
        features: { type: 'array', items: { type: 'string' } }, depends_on: { type: 'string' },
      },
    } },
    evidence_strategy: { type: 'string', description: 'how the whole thing stays EVIDENCE-BASED (real data sources, ontologies, validation)' },
    open_questions_for_human: { type: 'array', items: { type: 'string' } },
  },
}

phase('Understand')
const AREAS = [
  { key: 'current-scopes-ui', focus: 'the renderer today: godot/main.gd zoom scopes (--zoom 1 field … 6 cells), the ecosystem view, the specimen/L-system view, what each currently shows. What is the current max-zoom and is anything sub-organism rendered?' },
  { key: 'genome-model', focus: 'crates/genome/src/lib.rs + docs/llm/TAXONOMY.md + gp.rs: the genome/locus/parameter model and how it expresses traits today. What would real genome→trait mapping at molecular/cellular/organ scopes require?' },
  { key: 'engine-substrate', focus: 'the ADR-013 joule engine (DECISIONS.md + ecology-substrate-draft.md + sim-core): what biology the core will compute, what snapshots expose, and where a per-organism internal (cells/organs) model could attach without breaking determinism.' },
]
const understanding = await parallel(AREAS.map((a) => () =>
  agent(GROUND + '\n\nUNDERSTAND "' + a.key + '": ' + a.focus + '\nAnalysis only.',
    { label: 'understand:' + a.key, phase: 'Understand', schema: UNDERSTAND_SCHEMA, effort: 'high' })))
const ctx = JSON.stringify(understanding.filter(Boolean), null, 1)

phase('Design')
const TOPICS = [
  { key: 'zoom-scopes-architecture', brief: 'Design the MULTI-SCALE ZOOM-SCOPES hierarchy: cell → cell-cluster → organ → specimen-in-map → region → ecosystem. For EACH scope: what it represents, what data it shows, what the CORE computes vs the renderer draws (inv #2), and how it stays deterministic. This is the headline step-back.' },
  { key: 'llm-genome-at-scopes', brief: 'Design the future LLM layer that covers REAL genome-based traits at ALL scopes (molecular→cellular→organ→organism→population). It must run at the PROCESS BOUNDARY (inv #1, like oracle-slim) and feed the CORE deterministically (quantized/ordered), never compute biology in the renderer. How do genome features map to traits at each scope?' },
  { key: 'sci-game-feature-set', brief: 'Enumerate the COMPREHENSIVE feature set a science-based sim-game needs beyond environment + multi-species: progression/campaign, scenarios, the CRISPR design loop, data/telemetry, sharing/replay, validation against real data, accessibility, etc. Prioritize and sequence.' },
  { key: 'evidence-based-grounding', brief: 'Design how the engine stays EVIDENCE-BASED ("evidence based engine je hlavní"): real data sources (genomics, ecology, ontologies like SO/GO/ENVO), parameter provenance, validation/calibration against published results, and how the SLiM oracle (crates/oracle-slim) + an LLM layer keep claims defensible. Use the web to name concrete datasets/ontologies.' },
]
const designs = await parallel(TOPICS.map((t) => () =>
  agent(GROUND + '\n\nUNDERSTAND MAPS:\n' + ctx + '\n\nDESIGN: ' + t.brief + '\nBe concrete + science-grounded. Analysis only.',
    { label: 'design:' + t.key, phase: 'Design', schema: DESIGN_SCHEMA, effort: 'high' })))
const designsJson = JSON.stringify(designs.filter(Boolean), null, 1)

phase('Synthesize')
const roadmap = await agent(
  GROUND + '\n\nUNDERSTAND:\n' + ctx + '\n\nDESIGNS:\n' + designsJson
  + '\n\nSynthesize ONE comprehensive proposal for the science-based sim-game: the VISION, the full ZOOM-SCOPES hierarchy (cell→ecosystem, with the genome-trait mapping + core-vs-renderer split per scope), the LLM-genome layer (at the process boundary), a sequenced FEATURE ROADMAP (themes/phases that build on the ADR-013 engine + environment + multi-species), the EVIDENCE-BASED strategy (real data/ontologies/validation), and the open questions for the human. Proposal only — no code. This is the strategic step-back the user asked for.',
  { label: 'synthesize:roadmap', phase: 'Synthesize', schema: ROADMAP_SCHEMA, effort: 'high' })

return { phase: 'sci-game features + zoom scopes', roadmap, designs: designs.filter(Boolean), understanding: understanding.filter(Boolean) }
