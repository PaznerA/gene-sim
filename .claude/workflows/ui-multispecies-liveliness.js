export const meta = {
  name: 'ui-multispecies-liveliness',
  description:
    'Make the sim feel ALIVE: enrich the specimen view so every species shows its OWN correct trait set, and parameterize the ecosystem-view sprites across all zoom scopes by traits (branchiness -> more/bigger branches, stature -> size, leaf size/hue/reflectance -> shape+color). All species visibly distinct. Renderer-only, hash-neutral (inv #2).',
  whenToUse:
    'Run after ecoli-visibility-impl (which establishes the boundary + per-species observe + a first microbe view). This is the liveliness POLISH: all species rendered, trait-driven. GDScript/renderer only — biology stays in the Rust core. Hash-neutral; fully autonomous.',
  phases: [
    { title: 'Design' },
    { title: 'Implement' },
    { title: 'Gate' },
    { title: 'Verify' },
  ],
}

phase('Design')
const DSCHEMA = {
  type: 'object',
  required: ['specimen_view', 'ecosystem_sprites', 'trait_visual_map', 'species_distinction', 'read_only_argument', 'slices'],
  properties: {
    specimen_view: { type: 'string', description: 'How the specimen view shows EVERY species with its correct trait set: plant 9-trait L-system vs microbe (E. coli) 5-trait colony/glyph viz; driven by per-species observe()' },
    ecosystem_sprites: { type: 'string', description: 'How sprites in EACH zoom scope over the ecosystem view are parameterized by traits (branchiness->branch count/size, stature->height/scale, leaf_size, leaf_hue, reflectance->tint); organisms.gd / draw path' },
    trait_visual_map: { type: 'string', description: 'Explicit trait->visual-parameter mapping table per species type, snake_case keys matching the core phenotype export' },
    species_distinction: { type: 'string', description: 'How the player can tell species apart at a glance in both views (silhouette, palette, glyph)' },
    read_only_argument: { type: 'string', description: 'Why this is inv #2-clean: all trait values come from the core observe()/snapshot; GDScript only maps numbers->visuals, computes no biology; hash untouched' },
    slices: { type: 'array', items: { type: 'string' } },
  },
}
const LENSES = [
  'visual liveliness & game-feel: the ecosystem must read as a living, varied field — trait variation visibly drives plant shape/size/color so growth and selection are legible',
  'trait fidelity: every exported trait (plant 9 + microbe 5) maps to a distinct visual channel; no trait silently ignored; microbes look like microbes, plants like plants',
  'determinism & invariant #2: zero biology in GDScript — values come only from the core observe()/snapshot; the determinism hash 0xf795_eac4_112f_acd5 is untouched',
]
const proposals = (await parallel(LENSES.map((lens, i) => () =>
  agent(
    `Design a gene-sim renderer upgrade that makes the multi-species sim feel ALIVE, through this lens: ${lens}.\n\n` +
    `Context: godot/main.gd has a specimen view (mode 1, currently an L-system plant; renders a plant placeholder even for microbes) and an ecosystem view with sprites (see godot/organisms.gd and the snapshot/draw path) and zoom scopes (D key cycles scopes). The Rust core exports per-species traits via LiveSim::observe() (plant: GrowthRate/Stature/Branchiness/LeafSize/LeafHue/Reflectance/Fecundity/DroughtTolerance/KillSwitchLinkage; E. coli: GrowthRate/GlucoseUptake/RespirationMode/AcetateOverflow/FermentationCapacity) and per-cell channels via snapshot(). Multi-species is live (R3-B). Task: (1) specimen view shows EVERY species with its OWN correct trait set; (2) ecosystem sprites across all zoom scopes are parameterized by traits so the field looks varied and alive; all species visually distinct. KEEP all biology in the Rust core (inv #2) — GDScript only maps exported numbers to visuals. READ the actual .gd files first.\n\n` +
    `Return a concrete file-level design. Hash-neutral (renderer only).`,
    { label: `design:lens${i}`, phase: 'Design', schema: DSCHEMA },
  ),
))).filter(Boolean)

const chosen = await agent(
  `Judge & synthesize these ${proposals.length} liveliness designs into ONE plan:\n` +
    proposals.map((p, i) => `\n--- Design ${i} ---\n${JSON.stringify(p, null, 2)}`).join('\n') +
    `\n\nPin the trait->visual mapping table (per species type), the specimen-view-per-species layout, and the ecosystem-sprite parameterization across zoom scopes. Renderer-only, inv #2-clean, hash-neutral. Output the final design.`,
  { label: 'design:judge', phase: 'Design', schema: DSCHEMA },
)

phase('Implement')
const impl = await agent(
  `Implement this agreed gene-sim renderer liveliness upgrade — GDScript / renderer ONLY (do NOT touch crates/** biology; reading the existing observe()/snapshot exports is fine; a hash-neutral additive snapshot channel is acceptable ONLY if unavoidable and it must not touch the determinism hash):\n${JSON.stringify(chosen, null, 2)}\n\n` +
  `Enrich the specimen view so every species shows its correct trait set, and parameterize the ecosystem-view sprites across all zoom scopes by traits so the field looks alive and species are distinct. Keep ALL biology in the Rust core (inv #2). The pinned determinism literal 0xf795_eac4_112f_acd5 MUST stay unchanged. Do NOT commit. Report files + lines changed.`,
  { label: 'impl', phase: 'Implement' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. Report all 10 gates PASS/FAIL, determinism + godot-reader/livesim gates called out. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'inv2_preserved', 'all_species_visible', 'traits_drive_visuals', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'pinned determinism literal unchanged' },
    inv2_preserved: { type: 'boolean', description: 'no biology computed in GDScript; values come only from core exports' },
    all_species_visible: { type: 'boolean', description: 'every species renders distinctly in both specimen and ecosystem views' },
    traits_drive_visuals: { type: 'boolean', description: 'trait variation visibly changes sprite shape/size/color (branchiness->branches, etc.)' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const verdict = await agent(
  `Adversarially verify the liveliness upgrade. Read \`git diff\`. Try to REFUTE each property; default false if unconfirmable from the code. Confirm specifically that no genome/phenotype math leaked into GDScript (inv #2) and the determinism literal is unchanged.`,
  { label: 'verify', phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
)

log(`gate: ${typeof gate === 'string' ? gate.slice(0, 200) : ''}`)
return { chosen, impl, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, verdict }
