export const meta = {
  name: 'contamination-s3-renderer-impl',
  description:
    'ADR-019 S3 renderer (hash-neutral, inv #2): the godot contamination UI — a ContainmentLevel slider (ISO-14644 ladder, drives the deterministic immigration schedule via LiveSim.set_containment), a consortium menu (pick the contaminant species_keys), a seed/inoculate brush (issues RegionInoculate via LiveSim.inoculate — position matters), and immigration event markers on the timeline (reusing the SP-3 marker plumbing; each journaled inoculation shows + replays). Reads the LiveSim inoculate/set_containment/register_contaminant/fire_due exports already in core. GDScript only — biology stays in the Rust core.',
  whenToUse:
    'After SP-3 (reuses its brush/timeline plumbing) + the contamination core (the LiveSim exports). Renderer-only; the pinned literal 0x47a0 stays unchanged. Autonomous; stops for human commit.',
  phases: [
    { title: 'Implement' },
    { title: 'Gate' },
    { title: 'Verify' },
  ],
}

phase('Implement')
const impl = await agent(
  `Implement the ADR-019 S3 contamination RENDERER for gene-sim — GDScript / renderer ONLY (do NOT touch crates/** biology; reading existing LiveSim exports is fine). READ docs/llm/proposals/contamination-immigration-draft.md §3 (the containment knob) + §6 S3, the SP-3 intervention panel just landed in godot/main.gd (the tool palette + brush + timeline event markers — REUSE that plumbing), and the contamination-core LiveSim exports in crates/godot-sim/src/lib.rs (inoculate / set_containment / register_contaminant_json / fire_due_inoculations).\n\n` +
  `Build, in godot/:\n` +
  `1. A ContainmentLevel slider/selector (the ISO-14644 ladder: Sealed/OFF → … → dirty) that calls LiveSim.set_containment — dirtier = more contamination pressure (the deterministic immigration schedule is derived in core).\n` +
  `2. A consortium menu — pick the contaminant species_keys (mycoplasma/bacillus + the others as they bake) for the schedule; register them via register_contaminant_json if needed (read the res:// species JSON, pass the string — the inv #2 boundary).\n` +
  `3. A "seed / inoculate" brush tool added to the intervention palette — drag to paint a region, issues RegionInoculate via LiveSim.inoculate (a baked contaminant at that region) — POSITION MATTERS.\n` +
  `4. IMMIGRATION event markers on the timeline — each journaled inoculation (manual or scheduled, when fire_due_inoculations fires one) shows as a marker (reuse the SP-3 marker plumbing, a distinct contamination icon) and replay reproduces them.\n` +
  `Keep ALL biology in the core (inv #2) — GDScript only issues the Action + renders + reads exports. The pinned literal 0x47a0_3c8f_6701_f240 MUST stay unchanged (renderer-only; if a tiny read-only LiveSim accessor is genuinely needed for the schedule/markers, it must be a pure read that does NOT touch the hash). Do NOT commit. Report file:line.`,
  { label: 'impl', phase: 'Implement' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. determinism MUST be GREEN against 0x47a0_3c8f_6701_f240 (renderer-only → hash-neutral); livesim/godot-reader green. Report all gates PASS/FAIL with any exact error. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'inv2_preserved', 'containment_slider', 'seed_brush_position', 'immigration_markers', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'pinned literal unchanged; no biology in GDScript' },
    inv2_preserved: { type: 'boolean', description: 'GDScript only issues Actions + renders + reads exports; all biology in the core' },
    containment_slider: { type: 'boolean', description: 'the ContainmentLevel slider drives LiveSim.set_containment (the core schedule)' },
    seed_brush_position: { type: 'boolean', description: 'the seed/inoculate brush issues RegionInoculate region-scoped — position matters' },
    immigration_markers: { type: 'boolean', description: 'journaled inoculations show as timeline markers + replay reproduces them' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const verdict = await agent(
  `Adversarially verify the ADR-019 S3 contamination renderer. Read \`git diff\`. Try to REFUTE each property; default false if unconfirmable. Confirm no biology leaked into GDScript (inv #2) and the pinned literal is unchanged.`,
  { label: 'verify', phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
)

return { impl, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, verdict }
