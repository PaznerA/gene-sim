export const meta = {
  name: 'ecology-substrate-design',
  description: 'BOLD foundational design recon for the ecology interaction substrate (resource/metabolism + genome→allocation + trophic web + chemical field). Understand → diverge → integrate → adversarially pressure-test → ADR-016 epic draft + how R3/Rel re-ground. DESIGN ONLY, anti-safe, on the edge — no code.',
  phases: [
    { title: 'Understand', detail: 'map the current abstract-WF core + what is HARD vs SOFT to change' },
    { title: 'Diverge', detail: 'radical per-pillar + whole-substrate visions, explicitly anti-safe' },
    { title: 'Integrate', detail: 'three coherent end-to-end substrate architectures' },
    { title: 'Pressure-test', detail: 'adversarially verify determinism/buildability without sanding off the ambition' },
    { title: 'Synthesize', detail: 'ADR-016 epic draft + R3/Rel re-grounding + keystone open questions' },
  ],
}

const GROUND = [
  'PROJECT gene-sim: a 2D CRISPR ecosystem sim. Headless deterministic Rust core (crates/) + read-only Godot renderer. Repo root is cwd; READ files, modify NOTHING — this is a DESIGN workflow returning proposals only.',
  '',
  'MANDATE (read carefully): the user EXPLICITLY rejected the safe, incremental path. Do NOT optimize for the smallest determinism re-pin or the least code. We are laying the FOUNDATION of robust systems that organisms genuinely INTERACT THROUGH. Today selection is an ABSTRACT Wright-Fisher pool that multiplies a "fitness weight" — organisms never interact, they only react independently to STATIC environment fields. That is the shortcut we are replacing. Be ambitious, be on the edge — but stay BUILDABLE and HONEST about cost.',
  '',
  'THE FOUR PILLARS of the substrate to design:',
  ' 1. RESOURCE / METABOLIC substrate — turn the static environment fields into DYNAMIC, depletable, regenerating resource pools; organisms have a metabolism (uptake→convert→excrete); FITNESS BECOMES EMERGENT from an energy/biomass balance, not an abstract weight; density-dependent competition AND extinction emerge.',
  ' 2. GENOME → STRATEGY / ALLOCATION — rewrite genotype→phenotype so the genome expresses an ecological STRATEGY / resource-allocation BUDGET (growth vs defense vs reproduction vs storage) + a trophic role (autotroph/heterotroph/mixotroph/decomposer), with real TRADE-OFFS. Traits become budget allocations, not standalone scalars.',
  ' 3. TROPHIC WEB — predation/grazing/parasitism/mutualism as ENERGY TRANSFERS between organisms/species, governed by trait-match rules (attacker trait vs defender trait → how much energy flows). Relations become EMERGENT measurements of flows.',
  ' 4. CHEMICAL / SIGNAL FIELD — a diffusing chemical layer (toxins, allelochemicals, signals) organisms emit/sense → allelopathy, chemical warfare, kin signaling. Diffusion via integer/fixed-point stencils only.',
  '',
  'HARD INVARIANTS — NEVER violate (a violation is stop-the-line):',
  ' #1 GPL / external services stay at the PROCESS BOUNDARY (subprocess only, never linked; template crates/oracle-slim).',
  ' #2 ALL genotype→phenotype→ecology biology lives in the Rust core (crates/sim-core, crates/genome); the renderer is read-only.',
  ' #3 DETERMINISM: one seeded ChaCha8Rng threaded explicitly; NO HashMap iteration in sim logic (ordered/indexed only); NO transcendentals (sin/cos/exp/pow-frac/sqrt-of-float in the sim path). Therefore resource pools, metabolism, diffusion, and trophic transfers MUST be INTEGER or FIXED-POINT arithmetic, ordered, bit-reproducible. The pinned hash (determinism_hash_is_pinned, crates/sim-core/src/lib.rs:739, currently 0x9fad_2c9f_d298_f73a) is RE-PINNED deliberately per ADR-011 procedure — big/multiple re-pins are ACCEPTABLE here.',
  ' #6 Agency at SPECIES/REGION granularity, never per-organism RL. Organisms are ECS entities; strategy/agency is species/region-level.',
  ' #7 Versions pinned.',
  '',
  'SOFT CONSTRAINTS — these SHOULD be challenged/broken if it yields a more robust foundation (say so explicitly):',
  ' - ADR-005 constant-population / NO-EXTINCTION (the [0.5,1.5] strictly-positive fitness band): extinction is probably now DESIRABLE — challenge this.',
  ' - The abstract Wright-Fisher selection() at lib.rs:218 — replace it with resource-driven dynamics.',
  ' - The current gp.rs 5-scalar WeightedSumMap phenotype (some traits dead) — rewrite to strategy/allocation expression.',
  ' - Static SoilField / ClimateField — make resource channels dynamic.',
  ' - Minimal-re-pin discipline — accept a large structural rewrite.',
  '',
  'KEY FILES: crates/sim-core/src/lib.rs (selection @218, the ECS world+components incl. Energy, hash_world @659, reset_with_env spawn draws @384, the pinned hash @739); crates/sim-core/src/soil.rs (EnvironmentModifier @174, static SoilField); crates/sim-core/src/climate.rs (ClimateModifier @99, ClimateSample); crates/sim-core/src/gp.rs (WeightedSumMap, Trait::ALL); crates/genome/src/lib.rs (Genome/Locus model); crates/sim-core/src/snapshot.rs; crates/crispr/src/lib.rs; docs/llm/SPEC.md (invariants §2.1, reuse>reinvent §2.2); docs/llm/DECISIONS.md (ADR format ## ADR-NNN → ### Context/Decision/Consequences; ADR-005 no-extinction; next free number is ADR-016 since ADR-013/014/015 are draft proposals). Also read docs/llm/proposals/ADR-013-draft.md (R3 multi-species) + ADR-014-draft.md (relations + vector DB) — the substrate must RE-GROUND these, not sit beside them.',
].join('\n')

const UNDERSTAND_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['area', 'current_mechanism', 'why_it_is_a_shortcut', 'hard_to_change', 'soft_to_change', 'fixed_point_notes'],
  properties: {
    area: { type: 'string' },
    current_mechanism: { type: 'string', description: 'how it works today, file:line anchored' },
    why_it_is_a_shortcut: { type: 'string', description: 'where it fakes interaction / uses an abstract weight / static field' },
    hard_to_change: { type: 'array', items: { type: 'string' }, description: 'what a rewrite MUST preserve (the hard invariants here)' },
    soft_to_change: { type: 'array', items: { type: 'string' }, description: 'current behaviors that are safe/good to break for a better foundation' },
    fixed_point_notes: { type: 'string', description: 'where float currently lives and how it would become integer/fixed-point' },
  },
}

const PILLAR_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['pillar', 'radical_vision', 'data_model', 'mechanism', 'fixed_point_determinism', 'breaks_soft_constraints', 'emergence_unlocked', 'risk'],
  properties: {
    pillar: { type: 'string' },
    radical_vision: { type: 'string', description: 'the most robust version you can justify — NOT the minimal one' },
    data_model: { type: 'string', description: 'ECS components / resources, ordered/indexed (never HashMap-iterated)' },
    mechanism: { type: 'string', description: 'the per-generation algorithm; explicit about ordering' },
    fixed_point_determinism: { type: 'string', description: 'the exact integer/fixed-point scheme that keeps it bit-reproducible + transcendental-free' },
    breaks_soft_constraints: { type: 'array', items: { type: 'string' }, description: 'which soft constraints (esp ADR-005 / abstract WF) this deliberately breaks and why' },
    emergence_unlocked: { type: 'string', description: 'the emergent behavior this makes possible that the abstract core cannot' },
    risk: { type: 'string', enum: ['low', 'medium', 'high'] },
  },
}

const INTEGRATED_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['arch_name', 'philosophy', 'end_to_end', 'selection_rewrite', 'fixed_point_contract', 'extinction_policy', 'reground_r3', 'reground_rel', 'epic_phases', 'boldness', 'buildability'],
  properties: {
    arch_name: { type: 'string' },
    philosophy: { type: 'string', description: 'the unifying idea (e.g. single energy currency vs niche-partition vs flow-network)' },
    end_to_end: { type: 'string', description: 'resource pools → metabolism → genome/strategy → reproduction → trophic transfers → signals, as ONE coherent loop' },
    selection_rewrite: { type: 'string', description: 'exactly how selection() stops being abstract WF and becomes resource-driven' },
    fixed_point_contract: { type: 'string', description: 'the project-wide integer/fixed-point arithmetic contract for pools/metabolism/diffusion/transfers' },
    extinction_policy: { type: 'string', description: 'does ADR-005 no-extinction survive, get replaced, or get a floor? justify' },
    reground_r3: { type: 'string', description: 'how multi-species (ADR-013) becomes distinct strategies IN this substrate' },
    reground_rel: { type: 'string', description: 'how relations (ADR-014) emerge as flow measurements' },
    epic_phases: { type: 'array', items: { type: 'string' }, description: 'the phased build (F1, F2, …) at a high level' },
    boldness: { type: 'integer', description: '1-5, higher = more foundational/ambitious' },
    buildability: { type: 'integer', description: '1-5, higher = more clearly implementable + determinism-safe' },
  },
}

const VERDICT_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['arch_name', 'determinism_holds', 'transcendental_free', 'invariant_violations', 'fatal_flaws', 'honest_repin_scope', 'on_edge_but_buildable', 'fixes', 'recommendation'],
  properties: {
    arch_name: { type: 'string' },
    determinism_holds: { type: 'boolean', description: 'does the fixed-point scheme ACTUALLY stay bit-reproducible across platforms?' },
    transcendental_free: { type: 'boolean' },
    invariant_violations: { type: 'array', items: { type: 'string' }, description: 'any HARD invariant (#1/#2/#3/#6/#7) it breaks' },
    fatal_flaws: { type: 'array', items: { type: 'string' } },
    honest_repin_scope: { type: 'string', description: 'the REAL rewrite/re-pin cost, not the optimistic one' },
    on_edge_but_buildable: { type: 'boolean', description: 'ambitious yet implementable (true) vs overreaching into the unbuildable (false)' },
    fixes: { type: 'array', items: { type: 'string' }, description: 'what to change to keep the ambition but make it sound' },
    recommendation: { type: 'string' },
  },
}

const ADR_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['adr_title', 'context', 'decision', 'chosen_architecture', 'the_edge', 'fixed_point_contract', 'extinction_policy', 'consequences', 'epic_phases', 'reground_r3_rel_t', 'soft_constraints_broken', 'invariant_risks', 'open_questions_for_human'],
  properties: {
    adr_title: { type: 'string', description: 'e.g. "ADR-016 — Ecology substrate: metabolic economy + trophic interaction (foundational epic)"' },
    context: { type: 'string' },
    decision: { type: 'string' },
    chosen_architecture: { type: 'string', description: 'the winning integrated substrate, grafting the best from runners-up' },
    the_edge: { type: 'string', description: 'what is genuinely radical here + why it is worth the cost' },
    fixed_point_contract: { type: 'string' },
    extinction_policy: { type: 'string' },
    consequences: { type: 'string' },
    epic_phases: { type: 'array', items: {
      type: 'object', additionalProperties: false,
      required: ['id', 'goal', 'touches', 'repin', 'breaks_soft', 'acceptance'],
      properties: {
        id: { type: 'string', description: 'e.g. F1' }, goal: { type: 'string' },
        touches: { type: 'array', items: { type: 'string' } },
        repin: { type: 'boolean' },
        breaks_soft: { type: 'string', description: 'which soft constraint this phase breaks (e.g. replaces WF, allows extinction)' },
        acceptance: { type: 'string' },
      },
    } },
    reground_r3_rel_t: { type: 'string', description: 'how ADR-013 (R3), ADR-014 (Rel), and the former Phase T re-ground onto this substrate' },
    soft_constraints_broken: { type: 'array', items: { type: 'string' } },
    invariant_risks: { type: 'array', items: { type: 'string' } },
    open_questions_for_human: { type: 'array', items: { type: 'string' }, description: 'the keystone decisions the human must make before any implementation slice' },
  },
}

phase('Understand')
const AREAS = [
  { key: 'selection-energy', focus: 'the abstract Wright-Fisher selection() @ lib.rs:218, the Energy component, ADR-005 no-extinction, hash_world @659, reset_with_env spawn draws @384. How does "fitness" work today and where is it faked?' },
  { key: 'environment-fields', focus: 'the STATIC SoilField (soil.rs) + ClimateField (climate.rs) + their EnvironmentModifier/ClimateModifier seams. Why are they static, and what would make them dynamic depletable pools?' },
  { key: 'genome-phenotype', focus: 'the Genome/Locus model (genome/src/lib.rs) + gp.rs WeightedSumMap + Trait::ALL (incl. dead traits). What would it take to express a strategy/allocation budget instead of 5 scalars?' },
  { key: 'determinism-fixedpoint', focus: 'exactly where f64 is used in the sim path (selection weights, soil/climate sampling, modifiers) and how each becomes integer/fixed-point. The derive_seed stream registry. The re-pin procedure.' },
  { key: 'r3-rel-proposals', focus: 'docs/llm/proposals/ADR-013-draft.md (multi-species) + ADR-014-draft.md (relations + vector DB). What in them ASSUMES the abstract core, and what must re-ground on a resource/trophic substrate?' },
]
const understanding = await parallel(AREAS.map((a) => () =>
  agent(GROUND + '\n\nUNDERSTAND DIMENSION "' + a.key + '": ' + a.focus + '\nBe brutally honest about what is a shortcut vs a hard invariant. Analysis only.',
    { label: 'understand:' + a.key, phase: 'Understand', schema: UNDERSTAND_SCHEMA, effort: 'high' })))
const ctx = JSON.stringify(understanding.filter(Boolean), null, 1)

phase('Diverge')
const VISIONS = [
  { key: 'pillar1-metabolic', kind: 'pillar', brief: 'PILLAR 1 — resource/metabolic substrate. Dynamic depletable+regenerating pools (light/water/nutrients/detritus/CO2), per-organism metabolism, emergent energy-balance fitness. Push it as far as robust allows; fixed-point pools.' },
  { key: 'pillar2-allocation', kind: 'pillar', brief: 'PILLAR 2 — genome→strategy/allocation. Genome expresses a resource-allocation budget (growth/defense/reproduction/storage) + trophic role, with trade-offs. Rewrite gp.rs expression. Fixed-point budget.' },
  { key: 'pillar3-trophic', kind: 'pillar', brief: 'PILLAR 3 — trophic web. Energy transfers (predation/grazing/parasitism/mutualism) governed by trait-match rules; relations emerge from flows. Ordered, fixed-point transfers; species/region granular (inv #6).' },
  { key: 'pillar4-chemical', kind: 'pillar', brief: 'PILLAR 4 — chemical/signal diffusion field. Emit/sense toxins/allelochemicals/signals; integer/fixed-point diffusion stencil. Allelopathy, chemical warfare, kin signaling.' },
  { key: 'vision-energy-currency', kind: 'whole', brief: 'WHOLE-SUBSTRATE VISION A — everything is ONE conserved energy/mass CURRENCY: pools, metabolism, reproduction cost, trophic transfer, signal cost all denominated in the same fixed-point unit; conservation laws make it auditable + deterministic. Integrate all 4 pillars under this philosophy.' },
  { key: 'vision-niche-strategy', kind: 'whole', brief: 'WHOLE-SUBSTRATE VISION B — niche/strategy-centric: the genome-expressed strategy vector defines a niche; competition/relations are strategy-overlap + resource-overlap in fixed-point niche space; emergent guilds. Integrate all 4 pillars under this philosophy.' },
]
const diverged = await parallel(VISIONS.map((v) => () =>
  agent(GROUND + '\n\nUNDERSTAND MAPS:\n' + ctx + '\n\nDESIGN — ' + v.brief
    + '\nThis is the ANTI-SAFE phase: design the most robust version, not the cheapest. Be explicit about the fixed-point determinism scheme and which soft constraints you deliberately break. Analysis only.',
    { label: 'diverge:' + v.key, phase: 'Diverge', schema: PILLAR_SCHEMA, effort: 'high' })))
const divergedJson = JSON.stringify(diverged.filter(Boolean), null, 1)

phase('Integrate')
const INTEGRATIONS = [
  { key: 'int-conserved-currency', angle: 'Integrate into a coherent end-to-end substrate under a CONSERVED ENERGY/MASS CURRENCY philosophy (everything is fixed-point energy; conservation = the determinism + balance backbone). Lean toward boldness but keep it buildable.' },
  { key: 'int-flow-network', angle: 'Integrate into a FLOW-NETWORK substrate (the cell-grid + trophic links form a directed flow graph each generation; selection = who captures flow). Strategy genome routes flows.' },
  { key: 'int-staged-pragmatic', angle: 'Integrate into the MOST AGGRESSIVE-YET-SHIPPABLE substrate: still replaces abstract WF with resource-driven dynamics and allows extinction, but sequences the rewrite so each phase is gate-green and re-pinnable. Bold spine, honest staging.' },
]
const integrated = await parallel(INTEGRATIONS.map((it) => () =>
  agent(GROUND + '\n\nUNDERSTAND MAPS:\n' + ctx + '\n\nPILLAR + VISION DESIGNS:\n' + divergedJson
    + '\n\nINTEGRATE everything into ONE coherent end-to-end substrate architecture: ' + it.angle
    + '\nCover the full loop, the selection rewrite, the fixed-point contract, the extinction policy, and how R3 (ADR-013) + Rel (ADR-014) re-ground on it. Analysis only.',
    { label: 'integrate:' + it.key, phase: 'Integrate', schema: INTEGRATED_SCHEMA, effort: 'high' })))
const archs = integrated.filter(Boolean)
const archsJson = JSON.stringify(archs, null, 1)

phase('Pressure-test')
const verdicts = await parallel(archs.map((a) => () =>
  agent(GROUND + '\n\nARCHITECTURE UNDER TEST:\n' + JSON.stringify(a, null, 1)
    + '\n\nYou are an adversarial reviewer. Attack THIS architecture on the HARD constraints: does the fixed-point scheme ACTUALLY stay bit-reproducible + transcendental-free? Does it keep biology-in-core + species/region granularity + the GPL boundary? Is the rewrite/re-pin scope honestly assessed (not optimistic)? Does it overreach into the unbuildable? Crucially: do NOT sand off the ambition — if it is bold-but-buildable say so, and give FIXES that keep the edge while making it sound. Default determinism_holds=false unless the scheme is concretely bit-stable.',
    { label: 'pressure:' + a.arch_name.slice(0, 24), phase: 'Pressure-test', schema: VERDICT_SCHEMA, effort: 'high' })))
const verdictsJson = JSON.stringify(verdicts.filter(Boolean), null, 1)

phase('Synthesize')
const adr = await agent(
  GROUND + '\n\nUNDERSTAND:\n' + ctx + '\n\nDIVERGENT DESIGNS:\n' + divergedJson + '\n\nINTEGRATED ARCHITECTURES:\n' + archsJson + '\n\nADVERSARIAL VERDICTS:\n' + verdictsJson
  + '\n\nSynthesize the foundational ADR-016 DRAFT: choose the integrated substrate architecture that is the BOLDEST one that survived the pressure-test as buildable (graft the best ideas from the others; apply the verdicts\' fixes). Define the fixed-point determinism contract and the extinction policy explicitly. Break the epic into phased slices (F1, F2, …) each with touched files, repin flag, which soft constraint it breaks, and acceptance. Explain how R3/Rel/former-Phase-T re-ground onto it. List the soft constraints broken, the invariant risks, and the KEYSTONE open questions the human must decide before any implementation. This is a stop-the-line foundational proposal — design only, no code.',
  { label: 'synthesize:adr-016', phase: 'Synthesize', schema: ADR_SCHEMA, effort: 'high' })

return { phase: 'Ecology substrate (foundational)', adr_draft: adr, architectures: archs, verdicts: verdicts.filter(Boolean), diverged: diverged.filter(Boolean), understanding: understanding.filter(Boolean) }
