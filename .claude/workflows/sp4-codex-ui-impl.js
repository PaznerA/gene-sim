export const meta = {
  name: 'sp4-codex-ui-impl',
  description:
    'SP-4 codex UI (hash-neutral, inv #2): surface the phenology/ontology/taxonomy descriptions in-game. Convert docs/llm/proposals/sp4-codex-content-draft.md into a structured godot-readable codex data file (per-entry: id/title/category/body/sources for species, anchor genes + GO/SO, trophic roles, the 4 flows), and build the godot UI: a browsable CODEX panel + an INSPECT panel (click an organism/cell → its species entry) + tooltips (genes/traits/roles). Extensible so the contaminant/symbiont species slot in. Renderer-only — reads the codex data + core observe/snapshot exports; biology stays in the core.',
  whenToUse:
    'Midnight session item 5. The educational/presentation layer. Renderer + content data; the pinned literal 0x47a0 stays unchanged. Autonomous; stops for human commit.',
  phases: [
    { title: 'Implement' },
    { title: 'Gate' },
    { title: 'Verify' },
  ],
}

phase('Implement')
const impl = await agent(
  `Implement the SP-4 codex UI for gene-sim — GDScript / content ONLY (do NOT touch crates/** biology; reading core observe/snapshot exports is fine). READ docs/llm/proposals/sp4-codex-content-draft.md (the codex entries + the UI surface plan it proposes) + the existing godot panels (godot/main.gd inspect/specimen/relations panels, godot/panel.gd PanelChrome) + crates/sim-core/src/gp.rs (the Trait/TrophicRole/GO anchors, for the entry ids/keys) + data/species/*.json (the species keys).\n\n` +
  `Build:\n` +
  `1. A structured, godot-readable CODEX DATA file (e.g. godot/codex/codex.json or a .tres) extracted from the draft: per entry { id, category (species|gene|role|flow), title, body (the engaging evidence-based description), sources }. Cover the current species (default plant / E. coli K-12 / Bdellovibrio + the contaminants by key), the 5 anchor genes (gltA/ptsG/pflB/pta/ldhA with GO), the trophic roles, and the 4 flows. Keep it EXTENSIBLE (a schema future species slot into).\n` +
  `2. A browsable CODEX panel (PanelChrome) — a category/entry list + the selected entry's title/body/sources. Toggleable like the other panels.\n` +
  `3. INSPECT integration — clicking an organism/cell (the existing inspect path) surfaces that species' codex entry; tooltips on genes/traits/roles (e.g. in the CRISPR edit picker + the specimen trait readout) show the short description.\n` +
  `Keep ALL biology in the core (inv #2) — GDScript only renders the codex content + reads exported species keys/phenotypes. The pinned literal 0x47a0_3c8f_6701_f240 MUST stay unchanged (renderer-only). Do NOT commit. Report file:line + how many codex entries you authored.`,
  { label: 'impl', phase: 'Implement' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. determinism GREEN against 0x47a0_3c8f_6701_f240 (renderer-only → hash-neutral); livesim/godot-reader green (the codex data file must not break the headless --check / import). Report all gates PASS/FAIL with any exact error. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'inv2_preserved', 'codex_panel', 'inspect_integration', 'content_faithful', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'pinned literal unchanged; no biology in GDScript' },
    inv2_preserved: { type: 'boolean', description: 'GDScript renders codex content + reads exports only; no genome/phenotype logic' },
    codex_panel: { type: 'boolean', description: 'a browsable codex panel exists with the species/gene/role/flow entries' },
    inspect_integration: { type: 'boolean', description: 'clicking an organism/cell + tooltips surface the relevant codex content' },
    content_faithful: { type: 'boolean', description: 'the codex entries faithfully carry the draft\'s evidence-based content (not placeholder)' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const verdict = await agent(
  `Adversarially verify the SP-4 codex UI. Read \`git diff\` + the codex data file. Try to REFUTE each property; default false if unconfirmable. Confirm no biology leaked into GDScript (inv #2), the pinned literal is unchanged, and the codex content is faithful (not placeholder).`,
  { label: 'verify', phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
)

return { impl, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, verdict }
