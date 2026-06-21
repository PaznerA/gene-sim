export const meta = {
  name: 'ui-panels-and-relations-view',
  description:
    'Expand the UI panels (per-species population/allele/fitness + energy/pools where available) and add a NEW Relations view rendering the emergent FlowMatrix (S x S species-to-species joule flows) as a heatmap. Reads core exports only; degrades gracefully until F4 wires the FlowMatrix. Renderer-only, hash-neutral (inv #2).',
  whenToUse:
    'Run in batch 2 (after f4-trophic-decomposer-design has pinned the FlowMatrix shape in its draft). Builds the relations VIEW + richer panels against that contract, lighting up automatically when F4 coupling lands. GDScript/renderer only; hash-neutral; fully autonomous.',
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
  required: ['panel_expansion', 'relations_view', 'flowmatrix_contract', 'graceful_degradation', 'read_only_argument', 'slices'],
  properties: {
    panel_expansion: { type: 'string', description: 'Per-species panels: population, allele_freq, fitness, and energy/pools when the core exposes them; layout + how species are selected/cycled' },
    relations_view: { type: 'string', description: 'A new view mode rendering the FlowMatrix S x S as a heatmap (rows=source species, cols=sink), with sign/magnitude legible; how it slots into the existing view-mode toggle' },
    flowmatrix_contract: { type: 'string', description: 'The exact shape read from the core (per docs/llm/proposals/f4-trophic-decomposer-draft.md): flat S*S i64, row-major, row-sum==0; the LiveSim export name to consume' },
    graceful_degradation: { type: 'string', description: 'How the view behaves before F4 wires the FlowMatrix (zero/placeholder matrix, clearly labelled "not yet coupled"), lighting up automatically when the export appears' },
    read_only_argument: { type: 'string', description: 'Why inv #2-clean: the FlowMatrix is MEASURED in the core; GDScript only renders the exported integers; hash untouched' },
    slices: { type: 'array', items: { type: 'string' } },
  },
}
const LENSES = [
  'information design: dense per-species stats + an at-a-glance relations heatmap that makes trophic interdependence (who feeds whom) instantly readable',
  'forward-compatibility: the relations view must read the F4 FlowMatrix contract from the proposal draft and degrade gracefully until coupling lands, then light up with no further UI change',
  'determinism & invariant #2: relations are MEASURED in the core (emergent FlowMatrix, not fabricated); GDScript renders exported integers only; hash 0xf795_eac4_112f_acd5 untouched',
]
const proposals = (await parallel(LENSES.map((lens, i) => () =>
  agent(
    `Design a gene-sim UI upgrade — expanded per-species panels + a new Relations view — through this lens: ${lens}.\n\n` +
    `Context: godot/main.gd has a view-mode toggle (ecosystem / specimen) and control/stat panels; LiveSim (crates/godot-sim/src/lib.rs) exports observe()/snapshot()/region_allele(). Multi-species is live (R3-B). The emergent FlowMatrix (S x S species-to-species joule flows, row-major i64, row-sum==0) is specified in docs/llm/proposals/f4-trophic-decomposer-draft.md and will be exported by a LiveSim #[func] once F4 lands. Task: (1) expand panels to show per-species population/allele/fitness (+ energy/pools when exposed); (2) add a new Relations view rendering the FlowMatrix as a heatmap, reading the F4 contract and degrading gracefully (zero/placeholder, labelled) until coupling lands. KEEP all measurement in the Rust core (inv #2). READ docs/llm/proposals/f4-trophic-decomposer-draft.md and the actual .gd + lib.rs export surface first.\n\n` +
    `Return a concrete file-level design. Hash-neutral (renderer only).`,
    { label: `design:lens${i}`, phase: 'Design', schema: DSCHEMA },
  ),
))).filter(Boolean)

const chosen = await agent(
  `Judge & synthesize these ${proposals.length} panel+relations designs into ONE plan:\n` +
    proposals.map((p, i) => `\n--- Design ${i} ---\n${JSON.stringify(p, null, 2)}`).join('\n') +
    `\n\nPin the panel layout, the relations heatmap rendering, the FlowMatrix read contract (from the F4 draft), and the graceful-degradation behavior. Renderer-only, inv #2-clean, hash-neutral. Output the final design.`,
  { label: 'design:judge', phase: 'Design', schema: DSCHEMA },
)

phase('Implement')
const impl = await agent(
  `Implement this agreed gene-sim panels + relations-view upgrade — GDScript / renderer ONLY (reading core exports is fine; if the FlowMatrix export does not exist yet, render a clearly-labelled placeholder and wire the decoder so it lights up when F4 adds the export; a hash-neutral additive LiveSim read-only #[func] that merely surfaces an already-computed core value is acceptable, but it must NOT touch the determinism hash):\n${JSON.stringify(chosen, null, 2)}\n\n` +
  `Keep ALL measurement/biology in the Rust core (inv #2). The pinned determinism literal 0xf795_eac4_112f_acd5 MUST stay unchanged. Do NOT commit. Report files + lines changed.`,
  { label: 'impl', phase: 'Implement' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. Report all 10 gates PASS/FAIL, determinism + godot-reader/livesim called out. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'inv2_preserved', 'panels_per_species', 'relations_view_present', 'degrades_gracefully', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'pinned determinism literal unchanged' },
    inv2_preserved: { type: 'boolean', description: 'no biology/measurement computed in GDScript; values from core exports only' },
    panels_per_species: { type: 'boolean', description: 'panels show per-species stats' },
    relations_view_present: { type: 'boolean', description: 'a new relations heatmap view exists and reads the FlowMatrix contract' },
    degrades_gracefully: { type: 'boolean', description: 'relations view is labelled/placeholder until F4 wires the matrix, with no crash' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const verdict = await agent(
  `Adversarially verify the panels + relations-view upgrade. Read \`git diff\`. Try to REFUTE each property; default false if unconfirmable. Confirm no measurement leaked into GDScript (inv #2) and the determinism literal is unchanged.`,
  { label: 'verify', phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
)

log(`gate: ${typeof gate === 'string' ? gate.slice(0, 200) : ''}`)
return { chosen, impl, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, verdict }
