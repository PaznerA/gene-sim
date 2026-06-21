export const meta = {
  name: 'f4-trophic-decomposer-design',
  description:
    'ADR-013 F4 DESIGN: the obligate plant->detritus->E.coli(decomposer)->free_nutrient loop = the first real multi-species ecosystem; emergent MEASURED FlowMatrix (relations, not fabricated cosine). Produces the ADR-013 F4 / ADR-014-regrounding package + the decomposer species data spec. Design + hash-neutral data only.',
  whenToUse:
    'Run AFTER f3-metabolism-keystone-design lands. F4 couples the per-species pools through the joule economy and is another deliberate re-pin (human sign-off). This workflow designs it and may bake hash-neutral species data; it does NOT merge the coupling.',
  phases: [
    { title: 'Design' },
    { title: 'Judge' },
    { title: 'DataBake' },
    { title: 'Adversarial' },
  ],
}

// ── Phase 1: design panel (3 lenses) ──
phase('Design')
const FSCHEMA = {
  type: 'object',
  required: ['trophic_coupling', 'decomposer_species', 'flow_matrix', 'selection_modifier', 'vision_fit', 'repin_notes'],
  properties: {
    trophic_coupling: { type: 'string', description: 'plants deplete free_nutrient + shed detritus on death; decomposer mineralizes detritus->free_nutrient; the obligate loop' },
    decomposer_species: { type: 'string', description: 'is the decomposer the existing E. coli (soil microbe closing the cycle, per the vision) or a separate baked species? GO-anchored decomposer genes / trait bindings' },
    flow_matrix: { type: 'string', description: 'FlowMatrix S x S net integer J flows built during trophic_transfer; emergent MEASURED relations (row-sum==0), inverting fabricated-cosine ADR-014' },
    selection_modifier: { type: 'string', description: 'how trophic state feeds a strictly-positive [0.5,1.5] ResourceModifier into selection per organism, deterministically' },
    vision_fit: { type: 'string', description: 'how this realizes the north-star: fast abstract plant/animal + E. coli soil decomposer closing the nutrient cycle = first real ecosystem' },
    repin_notes: { type: 'string', description: 'why this re-pins; ordering (sort by cell,species,org); what the new ledger line says' },
  },
}
const LENSES = [
  'ecology: the obligate decomposer loop must create genuine interdependence (kill the decomposer -> nutrient starves -> plants crash) and emergent niche separation',
  'determinism: trophic_transfer + FlowMatrix must be integer, ordered (cell,species,org), conserved (row-sum==0), multi-ISA stable',
  'game-loop & vision: E. coli as the soil microbe the player edits; a CRISPRi knockdown of a decomposer gene must visibly ripple into the plant ecosystem (the earned-edit payoff)',
]
const proposals = (await parallel(LENSES.map((lens, i) => () =>
  agent(
    `Design gene-sim ADR-013 F4 "trophic web + decomposer 3rd species" through this lens: ${lens}.\n\n` +
    `North-star vision: a fast abstract plant/animal 30FPS sim PLUS a deep real E. coli mode, where E. coli is the SOIL MICROBE that closes the nutrient cycle (plants shed detritus -> E. coli decomposer mineralizes it -> free_nutrient -> plants), and the player earns CRISPR edits whose impact ripples across the ecosystem (computed in the background). Context: R3-B gives S independent per-species Wright-Fisher pools today with ZERO coupling. F3 (assumed landed) adds energy-funded metabolism/lifecycle + ledger. ResourceField{light,free_nutrient,detritus} exists. F4 must couple species through the joule economy and emit an emergent MEASURED FlowMatrix (NOT a fabricated cosine-similarity input — this inverts the old ADR-014 Rel draft). Determinism: integer, ordered by (cell,species,org), FlowMatrix row-sum==0. Deliberate re-pin. READ the actual files first.\n\n` +
    `Return a concrete file-level design. Do NOT write code.`,
    { label: `design:lens${i}`, phase: 'Design', schema: FSCHEMA },
  ),
))).filter(Boolean)

// ── Phase 2: judge & synthesize ──
phase('Judge')
const winner = await agent(
  `Judge & synthesize these ${proposals.length} F4 designs into ONE plan, grafting the best ideas. Decide the decomposer-species question (reuse E. coli vs bake a separate species) with a clear rationale tied to the vision. Pin the FlowMatrix semantics and the selection ResourceModifier.\n` +
    proposals.map((p, i) => `\n--- Design ${i} ---\n${JSON.stringify(p, null, 2)}`).join('\n'),
  { label: 'judge', phase: 'Judge', schema: FSCHEMA },
)

// ── Phase 3: hash-neutral data + the ADR draft (no coupling code, no re-pin) ──
phase('DataBake')
const data = await agent(
  `Based on this F4 plan, produce the HASH-NEUTRAL data + design artifacts only (no core coupling code, no re-pin):\n${JSON.stringify(winner, null, 2)}\n\n` +
  `1. Write docs/llm/proposals/f4-trophic-decomposer-draft.md = ADR-013 F4 + ADR-014 regrounding draft: the design, the FlowMatrix conservation contract, the decomposer decision, the slice breakdown, the new pinned-hash plan.\n` +
  `2. If the plan chooses a SEPARATE decomposer species: draft its SpeciesSpec JSON (GO-anchored decomposer genes, the trophic-role declaration) following the data/species/ schema + scripts/bake_ecoli_species.py conventions — as a spec/skeleton validated by SpeciesSpec::build (add a test) but do NOT wire it into selection. If the plan REUSES E. coli as the decomposer, document the exact trait/role bindings instead.\n` +
  `Run \`bash tools/gate.sh\` — it MUST stay green. Do NOT commit. End with: "STOP-THE-LINE: F4 trophic coupling requires human re-pin sign-off before implementation."`,
  { label: 'data-bake', phase: 'DataBake', agentType: 'implementer' },
)

// ── Phase 4: adversarial verify the design ──
phase('Adversarial')
const VSCHEMA = {
  type: 'object',
  required: ['flowmatrix_conserved', 'relations_are_measured', 'deterministic_ordering', 'obligate_loop_real', 'issues'],
  properties: {
    flowmatrix_conserved: { type: 'boolean', description: 'FlowMatrix row-sum==0 (energy conserved per trophic column)' },
    relations_are_measured: { type: 'boolean', description: 'relations are realized energy deltas, NOT fabricated cosine inputs' },
    deterministic_ordering: { type: 'boolean', description: 'trophic_transfer sorts by (cell,species,org); integer; multi-ISA stable' },
    obligate_loop_real: { type: 'boolean', description: 'killing the decomposer genuinely starves the plants (real interdependence, not cosmetic)' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const verdict = await agent(
  `Adversarially verify the F4 design + draft. Read docs/llm/proposals/f4-trophic-decomposer-draft.md and the winning design. Try to REFUTE each property (default false if unconfirmable). Winning design:\n${JSON.stringify(winner, null, 2)}`,
  { label: 'adversarial', phase: 'Adversarial', schema: VSCHEMA, agentType: 'reviewer' },
)

return { winner, data, verdict }
