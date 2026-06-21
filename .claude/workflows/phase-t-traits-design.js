export const meta = {
  name: 'phase-t-traits-design',
  description: 'Phase T: brainstorm candidate heritable traits + their environment couplings behind the existing seams, score by gameplay × determinism cost × seam fit, spec the top picks as an ADR-015 draft + slices (DESIGN ONLY — no code)',
  phases: [
    { title: 'Brainstorm', detail: 'parallel agents each propose distinct heritable traits + couplings' },
    { title: 'Score', detail: 'rank by gameplay value, determinism/re-pin cost, seam fit' },
    { title: 'Synthesize', detail: 'pick top 2-3, spec ADR-015 draft + slices' },
  ],
}

const GROUND = [
  'PROJECT gene-sim: 2D CRISPR ecosystem sim. Headless deterministic Rust core + read-only Godot renderer. Repo root is cwd; READ files, modify NOTHING — design workflow returning proposals only.',
  '',
  'Phase T = MORE traits under selection, beyond the existing Genotype + DroughtTol + ThermalTol. New environment<->phenotype couplings behind the pluggable seams (inv #5). Determinism RE-PIN per shipped trait.',
  '',
  'HARD CONSTRAINTS:',
  ' #2 biology in the Rust core only; renderer read-only.',
  ' #3 Determinism: each new heritable trait is a per-individual draw on the single seeded ChaCha8Rng (template = DroughtTol/ThermalTol — find their spawn-draw + selection + hash_world fold). NO HashMap iteration, NO transcendentals (sin/cos/exp) — only +,-,*,clamp,abs,min/max are bit-stable. Each shipped trait folds into hash_world => ONE deliberate RE-PIN of the pinned hash (test determinism_hash_is_pinned in crates/sim-core/src/lib.rs, current 0x9fad_2c9f_d298_f73a; procedure ledgered at ADR-011 consequences).',
  ' #5 Couple via the existing seams: EnvironmentModifier (crates/sim-core/src/soil.rs:174 — soil moisture/nutrients/pH) and ClimateModifier (crates/sim-core/src/climate.rs:99 — insolation/temperature/day_length). A new trait should plug into one of these (or a sibling seam) without touching sim-core selection wiring beyond adding the component + draw + fold.',
  '',
  'TEMPLATE TO STUDY: how ThermalTol was added in Phase E (ADR-012 E3): a per-individual f64 component, a spawn draw in a fixed order, a TemperatureMatchModifier (climate.rs) that scales fitness with climate EXTREMITY so the temperate default stays selection-NEUTRAL (existing tests/pinned config unchanged), and a single re-pin. Read crates/sim-core/src/lib.rs (selection @218, the spawn draws, the trait components, hash_world) + climate.rs + soil.rs.',
  '',
  'AVAILABLE ENVIRONMENT SIGNALS to couple to: soil moisture, soil nutrients, soil pH (soil.rs); insolation, temperature, day_length (climate.rs). Plus whatever R3/Rel add later (multi-species, relations) — note dependencies but design traits that work TODAY where possible.',
  '',
  'ADR format: docs/llm/DECISIONS.md ## ADR-NNN -> ### Context / ### Decision / ### Consequences. Next free number after ADR-013/014 is ADR-015.',
].join('\n')

const TRAIT_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['traits'],
  properties: {
    traits: { type: 'array', items: {
      type: 'object', additionalProperties: false,
      required: ['name', 'biology', 'env_signal', 'seam', 'modifier_sketch', 'determinism_cost', 'neutral_at_default', 'gameplay_hook', 'transcendental_free'],
      properties: {
        name: { type: 'string', description: 'e.g. PhotoEfficiency, DormancyThreshold, pHTolerance, NutrientUptake, PredationResistance' },
        biology: { type: 'string', description: 'what the trait means and how it affects the organism' },
        env_signal: { type: 'string', description: 'which environment signal it couples to (moisture/nutrients/pH/insolation/temperature/day_length/…)' },
        seam: { type: 'string', enum: ['EnvironmentModifier', 'ClimateModifier', 'new-sibling-seam'] },
        modifier_sketch: { type: 'string', description: 'the bounded fitness factor math — MUST be +,-,*,clamp,abs,min/max only (no transcendentals)' },
        determinism_cost: { type: 'string', description: 'spawn-draw order impact + hash_world fold + re-pin scope' },
        neutral_at_default: { type: 'string', description: 'how it stays selection-neutral at the pinned default so existing tests/config are undisturbed (the ThermalTol extremity trick)' },
        gameplay_hook: { type: 'string', description: 'why a player/agent cares — what CRISPR edit or environment choice it makes interesting' },
        transcendental_free: { type: 'boolean' },
      },
    } },
  },
}

const SCORE_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['rankings'],
  properties: {
    rankings: { type: 'array', items: {
      type: 'object', additionalProperties: false,
      required: ['trait_name', 'gameplay', 'determinism_cheapness', 'seam_fit', 'total', 'note'],
      properties: {
        trait_name: { type: 'string' },
        gameplay: { type: 'integer', description: '1-5' },
        determinism_cheapness: { type: 'integer', description: '1-5, higher = smaller re-pin/stream impact, no transcendentals' },
        seam_fit: { type: 'integer', description: '1-5, plugs cleanly into an existing seam' },
        total: { type: 'integer' },
        note: { type: 'string' },
      },
    } },
  },
}

const ADR_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['adr_title', 'context', 'decision', 'chosen_traits', 'consequences', 'slices', 'repin_plan', 'invariant_risks', 'open_questions_for_human'],
  properties: {
    adr_title: { type: 'string', description: 'e.g. "ADR-015 — More heritable traits + environment couplings (roadmap Phase T)"' },
    context: { type: 'string' },
    decision: { type: 'string' },
    chosen_traits: { type: 'array', items: { type: 'string' } },
    consequences: { type: 'string' },
    slices: { type: 'array', items: {
      type: 'object', additionalProperties: false,
      required: ['id', 'goal', 'touches', 'repin', 'acceptance'],
      properties: {
        id: { type: 'string', description: 'e.g. T-A' }, goal: { type: 'string' },
        touches: { type: 'array', items: { type: 'string' } },
        repin: { type: 'boolean' }, acceptance: { type: 'string' },
      },
    } },
    repin_plan: { type: 'string', description: 'one re-pin per shipped trait, ledgered; order' },
    invariant_risks: { type: 'array', items: { type: 'string' } },
    open_questions_for_human: { type: 'array', items: { type: 'string' } },
  },
}

phase('Brainstorm')
const SEEDS = [
  'CLIMATE-coupled traits: day_length <-> dormancy/flowering timing, insolation <-> photosynthetic efficiency, temperature <-> metabolic rate. Use ClimateModifier.',
  'SOIL-coupled traits: nutrients <-> uptake/growth, pH <-> pH tolerance band, moisture <-> water-use efficiency (distinct from DroughtTol). Use EnvironmentModifier.',
  'TRADE-OFF traits: pairs where boosting one trait costs another (e.g. growth-rate vs stress-tolerance) so CRISPR edits create real dilemmas, not strictly-better buttons.',
  'DEFENSE/INTERACTION traits that anticipate R3/Rel: predation-resistance, toxin production, allelopathy — design the heritable trait + a placeholder coupling that lights up once multi-species/relations land.',
  'PHENOLOGY/LIFECYCLE traits: seed dormancy threshold, germination window keyed to season — bounded, transcendental-free, with a clear specimen-view signal.',
]
const brainstorm = await parallel(SEEDS.map((s, i) => () =>
  agent(GROUND + '\n\nBRAINSTORM SEED #' + (i + 1) + ': ' + s
    + '\nPropose 2-4 concrete heritable traits in this vein. For each: bounded transcendental-free modifier math, determinism cost, how it stays neutral at the pinned default, and the gameplay hook. Analysis only.',
    { label: 'brainstorm:' + (i + 1), phase: 'Brainstorm', schema: TRAIT_SCHEMA, effort: 'high' })))
const allTraits = brainstorm.filter(Boolean).flatMap((b) => b.traits || [])
const traitsJson = JSON.stringify(allTraits, null, 1)
log('brainstormed ' + allTraits.length + ' candidate traits')

phase('Score')
const scoring = await parallel(['gameplay-designer', 'determinism-keeper'].map((lens) => () =>
  agent(GROUND + '\n\nCANDIDATE TRAITS:\n' + traitsJson
    + '\n\nAs the "' + lens + '" judge, score EVERY trait (gameplay, determinism_cheapness, seam_fit; 1-5 each). Penalize any transcendental temptation, fragile neutrality, or trait that needs sim-core surgery beyond add-component+draw+fold+modifier.',
    { label: 'score:' + lens, phase: 'Score', schema: SCORE_SCHEMA })))
const scoresJson = JSON.stringify(scoring.filter(Boolean).flatMap((s) => s.rankings || []), null, 1)

phase('Synthesize')
const adr = await agent(
  GROUND + '\n\nCANDIDATE TRAITS:\n' + traitsJson + '\n\nSCORES:\n' + scoresJson
  + '\n\nSynthesize an ADR-015 DRAFT for Phase T: pick the TOP 2-3 traits (best gameplay per determinism cost, cleanest seam fit, neutral at default). Spec each as a gated slice (touched files + repin flag + acceptance), one deliberate re-pin per shipped trait. List invariant risks + open questions for human sign-off. Proposal only — no implementation.',
  { label: 'synthesize:adr-015', phase: 'Synthesize', schema: ADR_SCHEMA, effort: 'high' })

return { phase: 'T more traits', adr_draft: adr, candidate_traits: allTraits, scores: scoring.filter(Boolean) }
