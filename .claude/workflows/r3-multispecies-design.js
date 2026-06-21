export const meta = {
  name: 'r3-multispecies-design',
  description: 'R3 multi-species: understand the single-species core, design N architectures, judge, synthesize an ADR-013 draft + slice plan (DESIGN ONLY — no code, R3 is a stop-the-line invariant gate needing human sign-off)',
  phases: [
    { title: 'Understand', detail: 'parallel readers map the single-species selection/snapshot/genome/edit/UI surface' },
    { title: 'Design', detail: 'three independent multi-species architectures' },
    { title: 'Judge', detail: 'score each on determinism cost, inv #6, snapshot, gameplay, risk' },
    { title: 'Synthesize', detail: 'ADR-013 draft + R3 slice breakdown for human sign-off' },
  ],
}

// ── Shared grounding: invariants + the exact files each agent should read. ──────────────────────────────────
const GROUND = [
  'PROJECT gene-sim: 2D CRISPR ecosystem sim. Headless deterministic Rust core (crates/) + read-only Godot renderer (godot/). Repo root is cwd; READ files, do NOT modify anything — this is a DESIGN workflow that returns proposals only.',
  '',
  'INVARIANTS that constrain any multi-species design:',
  ' #2 ALL genotype->phenotype biology lives in the Rust core (crates/sim-core, crates/genome); the renderer only displays snapshots. No biology in GDScript.',
  ' #3 Determinism: one seeded ChaCha8Rng threaded explicitly; NO HashMap iteration in sim logic (use ordered/indexed collections); NO transcendentals in sim math. There is a PINNED determinism hash (test determinism_hash_is_pinned in crates/sim-core/src/lib.rs, current value 0x9fad_2c9f_d298_f73a). Any structural sim change RE-PINS it deliberately (procedure ledgered at ADR-011 consequences).',
  ' #5 Science pluggable behind traits (EnvironmentModifier @ crates/sim-core/src/soil.rs:174, ClimateModifier @ crates/sim-core/src/climate.rs:99).',
  ' #6 Agents act at SPECIES/REGION granularity, not per-organism. Multi-species must keep edits/agency species- or region-scoped.',
  ' #7 Pinned versions (Godot 4.6, godot-rust 0.5.3, Rust 1.96).',
  '',
  'KEY FILES: crates/sim-core/src/lib.rs (selection() @218, the ECS world+components, species_genome() @542, apply_species_edit @546, region edit @579, hash_world); crates/sim-core/src/snapshot.rs (per-cell channel snapshot); crates/sim-core/src/gp.rs (WeightedSumMap genotype->phenotype); crates/genome/src/lib.rs (Genome model); crates/crispr/src/lib.rs (edit scoring + region eval @591); crates/harness/src/{lib.rs,replay.rs}; crates/godot-sim/src/lib.rs (LiveSim gdext); godot/main.gd (renderer, specimen view) + godot/main_menu.gd; docs/llm/SPEC.md (invariants §2.1); docs/llm/DECISIONS.md (ADR format: ## ADR-NNN — title -> ### Context / ### Decision / ### Consequences; next free number is ADR-013).',
  '',
  'R3 GOAL (from docs/llm/TASKS.md): multiple species competing across regions — rewrites selection (per-species sub-populations), the snapshot (per-species channels), genome wiring (per-species genomes), region/species edits routed to a chosen species. Determinism RE-PIN expected. UI: each species gets its OWN PAGE in the specimen view with a TABLE of its trees (the incremental specimen log, per species).',
].join('\n')

const UNDERSTAND_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['area', 'how_it_works_today', 'single_species_assumptions', 'multispecies_touch_points', 'determinism_notes'],
  properties: {
    area: { type: 'string' },
    how_it_works_today: { type: 'string', description: 'concise map with file:line anchors' },
    single_species_assumptions: { type: 'array', items: { type: 'string' }, description: 'places that assume exactly one species/genome' },
    multispecies_touch_points: { type: 'array', items: { type: 'string' }, description: 'what would have to change for N species' },
    determinism_notes: { type: 'string', description: 'RNG stream / hash_world / ordering implications' },
  },
}

const DESIGN_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['approach_name', 'one_liner', 'data_model', 'selection_changes', 'snapshot_design', 'edit_routing', 'determinism_and_repin', 'ui_specimen_pages', 'pros', 'cons', 'risk'],
  properties: {
    approach_name: { type: 'string' },
    one_liner: { type: 'string' },
    data_model: { type: 'string', description: 'how N species + their genomes + per-organism species tag are represented (ECS components, ordered collections — NO HashMap iteration)' },
    selection_changes: { type: 'string', description: 'how selection() becomes per-species sub-populations' },
    snapshot_design: { type: 'string', description: 'per-species channels; renderer-read-only' },
    edit_routing: { type: 'string', description: 'how a species/region edit targets a chosen species (inv #6)' },
    determinism_and_repin: { type: 'string', description: 'stream impact, hash_world fold order, expected re-pin scope' },
    ui_specimen_pages: { type: 'string', description: 'per-species page + table-of-trees in the specimen view (renderer-only)' },
    pros: { type: 'array', items: { type: 'string' } },
    cons: { type: 'array', items: { type: 'string' } },
    risk: { type: 'string', enum: ['low', 'medium', 'high'] },
  },
}

const JUDGE_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['approach_name', 'scores', 'total', 'verdict'],
  properties: {
    approach_name: { type: 'string' },
    scores: {
      type: 'object', additionalProperties: false,
      required: ['determinism_simplicity', 'invariant6_fit', 'snapshot_clarity', 'gameplay_value', 'impl_risk_inverse'],
      properties: {
        determinism_simplicity: { type: 'integer', description: '1-5, higher = smaller/cleaner re-pin + stream impact' },
        invariant6_fit: { type: 'integer', description: '1-5, species/region granularity preserved' },
        snapshot_clarity: { type: 'integer', description: '1-5' },
        gameplay_value: { type: 'integer', description: '1-5' },
        impl_risk_inverse: { type: 'integer', description: '1-5, higher = lower risk' },
      },
    },
    total: { type: 'integer' },
    verdict: { type: 'string' },
  },
}

const ADR_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['adr_title', 'context', 'decision', 'chosen_approach', 'consequences', 'slices', 'repin_plan', 'invariant_risks', 'open_questions_for_human'],
  properties: {
    adr_title: { type: 'string', description: 'e.g. "ADR-013 — Multi-species ecosystem (roadmap R3)"' },
    context: { type: 'string' },
    decision: { type: 'string' },
    chosen_approach: { type: 'string' },
    consequences: { type: 'string' },
    slices: { type: 'array', items: {
      type: 'object', additionalProperties: false,
      required: ['id', 'goal', 'touches', 'repin', 'acceptance'],
      properties: {
        id: { type: 'string', description: 'e.g. R3-A' },
        goal: { type: 'string' },
        touches: { type: 'array', items: { type: 'string' } },
        repin: { type: 'boolean' },
        acceptance: { type: 'string' },
      },
    } },
    repin_plan: { type: 'string', description: 'which slices re-pin the hash and in what order; one re-pin per slice' },
    invariant_risks: { type: 'array', items: { type: 'string' } },
    open_questions_for_human: { type: 'array', items: { type: 'string' }, description: 'R3 is stop-the-line (inv gate) — what the human must decide before implementation' },
  },
}

phase('Understand')
const AREAS = [
  { key: 'selection-world', focus: 'the ECS world, components, and selection() (Wright-Fisher per-cell). Read crates/sim-core/src/lib.rs around selection() @218 + the world/components + hash_world.' },
  { key: 'snapshot', focus: 'the snapshot format + per-cell channels. Read crates/sim-core/src/snapshot.rs and how godot/main.gd consumes it.' },
  { key: 'genome-gp', focus: 'the Genome model + genotype->phenotype. Read crates/genome/src/lib.rs and crates/sim-core/src/gp.rs (WeightedSumMap) + species_genome() @542.' },
  { key: 'edit-routing', focus: 'species + region CRISPR edits. Read apply_species_edit @546 and region edit @579 in crates/sim-core/src/lib.rs, and crates/crispr/src/lib.rs region eval @591.' },
  { key: 'harness-gdext-ui', focus: 'the run drivers + UI. Read crates/harness/src/lib.rs + replay.rs, crates/godot-sim/src/lib.rs (LiveSim), and the specimen view in godot/main.gd (the incremental specimen log).' },
]
const understanding = await parallel(AREAS.map((a) => () =>
  agent(GROUND + '\n\nDIMENSION: map how "' + a.key + '" works TODAY (single-species) and what multi-species would touch.\nFOCUS: ' + a.focus + '\nReturn a precise, file:line-anchored map. Analysis only — change nothing.',
    { label: 'understand:' + a.key, phase: 'Understand', schema: UNDERSTAND_SCHEMA, effort: 'high' })))
const ctx = JSON.stringify(understanding.filter(Boolean), null, 1)

phase('Design')
const APPROACHES = [
  { key: 'speciesid-component', angle: 'A SHARED world where every organism carries a SpeciesId component (small integer index into an ordered species registry); selection partitions per-cell by species; one genome per species in an indexed Vec. Favor the SMALLEST determinism delta.' },
  { key: 'parallel-subworlds', angle: 'N largely-independent per-species sub-simulations sharing the soil/climate field but with separate populations; competition resolved via a shared carrying-capacity per cell. Favor isolation + clarity.' },
  { key: 'tagged-registry', angle: 'A species registry resource (ordered) holding per-species genome + params; organisms tagged; snapshot emits per-species channel planes; edits routed by species index. Favor gameplay richness + the per-species UI.' },
]
const designs = await parallel(APPROACHES.map((ap) => () =>
  agent(GROUND + '\n\nUNDERSTAND-PHASE MAPS (ground your design in these):\n' + ctx
    + '\n\nDESIGN this multi-species ARCHITECTURE: ' + ap.angle
    + '\nBe concrete about the data model (ordered, never HashMap-iterated), selection() rewrite, per-species snapshot channels, species/region edit routing (inv #6), the determinism + re-pin scope, and the per-species specimen PAGE with a table of trees (renderer-only). Analysis only.',
    { label: 'design:' + ap.key, phase: 'Design', schema: DESIGN_SCHEMA, effort: 'high' })))
const designedJson = JSON.stringify(designs.filter(Boolean), null, 1)

phase('Judge')
const JUDGE_BALLOT_SCHEMA = {
  type: 'object', additionalProperties: false, required: ['verdicts'],
  properties: { verdicts: { type: 'array', items: JUDGE_SCHEMA } },
}
const judging = await parallel(['determinism-purist', 'gameplay-lead', 'pragmatic-impl'].map((lens) => () =>
  agent(GROUND + '\n\nCANDIDATE ARCHITECTURES:\n' + designedJson
    + '\n\nYou are the "' + lens + '" judge. Score EACH approach (1-5 per criterion) on determinism_simplicity, invariant6_fit, snapshot_clarity, gameplay_value, impl_risk_inverse. Be harsh on determinism/re-pin blast radius and any HashMap-iteration or transcendental temptation. Return a `verdicts` array with one verdict object PER approach.',
    { label: 'judge:' + lens, phase: 'Judge', schema: JUDGE_BALLOT_SCHEMA })))
const judgedJson = JSON.stringify(judging.filter(Boolean).flatMap((j) => j.verdicts || []), null, 1)

phase('Synthesize')
const adr = await agent(
  GROUND + '\n\nUNDERSTAND MAPS:\n' + ctx + '\n\nDESIGNS:\n' + designedJson + '\n\nJUDGE SCORES:\n' + judgedJson
  + '\n\nSynthesize an ADR-013 DRAFT for multi-species (R3): pick the winning architecture (graft the best ideas from runners-up), and break R3 into gated slices (R3-A, R3-B, …), each with its touched files, whether it RE-PINS the determinism hash (one re-pin per slice), and an acceptance criterion. Flag invariant risks and the OPEN QUESTIONS the human must sign off before implementation (R3 is a stop-the-line invariant gate — this workflow proposes, it does NOT implement).',
  { label: 'synthesize:adr-013', phase: 'Synthesize', schema: ADR_SCHEMA, effort: 'high' })

return { phase: 'R3 multi-species', adr_draft: adr, approaches: designs.filter(Boolean), understanding: understanding.filter(Boolean) }
