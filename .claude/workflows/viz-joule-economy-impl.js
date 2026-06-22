export const meta = {
  name: 'viz-joule-economy-impl',
  description:
    'Visualise the F3/F4 joule economy in-game (hash-neutral renderer, inv #2): add the live PoolStock planes (light/free_nutrient/detritus) + per-cell energy/biomass to the snapshot (GSS2→GSS3, off-hash) and render them as selectable data-layer overlays; the godot reader + check_godot_snapshot channel count update in the same commit. Makes the new ecology legible and playtestable.',
  whenToUse:
    'Continuation slice #1 (post-CHEMOSTAT-J merge). Pure renderer + off-hash snapshot channels; determinism literal 0x4e4d_0520_722a_a069 stays unchanged. Fully autonomous; stops for human commit.',
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
  required: ['channels', 'gss3_format', 'reader_update', 'overlays', 'hash_neutrality_argument', 'slices'],
  properties: {
    channels: { type: 'string', description: 'The new snapshot channels to add (at least light/free_nutrient/detritus resampled from PoolStock; optionally per-cell mean Energy/Biomass) + the new CHANNEL_COUNT' },
    gss3_format: { type: 'string', description: 'The GSS2→GSS3 header/version bump in snapshot.rs; how old GSS2 readers fail loudly; the cdylib snapshot() + godot-sim passthrough' },
    reader_update: { type: 'string', description: 'tools/check_godot_snapshot.sh asserts channels=6 today → the new count; the godot headless reader scene update' },
    overlays: { type: 'string', description: 'How main.gd / organisms.gd / the data-layer shader render the new planes as selectable overlays via the existing layer picker (_layer_picker); legends; an optional per-org energy/biomass tint' },
    hash_neutrality_argument: { type: 'string', description: 'Why hash-neutral: snapshot is a read-only off-hash projection (never folded into hash_world); zero RNG; pinned literal unchanged' },
    slices: { type: 'array', items: { type: 'string' } },
  },
}
const LENSES = [
  'snapshot format & determinism: the GSS2→GSS3 channel addition must stay a read-only off-hash projection (zero RNG, pinned literal unchanged); old readers must fail loudly, not silently mis-parse',
  'render & UX: the pools (light/free_nutrient/detritus) + energy/biomass must read at a glance as data-layer overlays through the existing layer picker, with legends — so a player can SEE nutrient drain, mineralization, and energy',
  'gate compatibility: tools/check_godot_snapshot.sh + the headless reader scene assert channels=6 today — update them in the SAME commit (inv #2: the decoder moves with the format)',
]
const proposals = (await parallel(LENSES.map((lens, i) => () =>
  agent(
    `Design the gene-sim "visualise the joule economy" renderer slice through this lens: ${lens}.\n\n` +
    `Context: F3/F4 LANDED a real joule economy — crates/sim-core/src/lib.rs has a per-cell PoolStock{light,free_nutrient,detritus: Vec<i64>} + per-org Energy(i64)/Biomass(i64); trophic.rs has the FlowMatrix (already surfaced by the Relations heatmap). The snapshot (crates/sim-core/src/snapshot.rs) currently exports 6 channels (density, allele_freq, fitness, soil_moisture, soil_nutrients, soil_ph) as GSS2; godot reads them via the cdylib snapshot() + a data-layer shader + the layer picker (godot/main.gd _layer_picker, organisms.gd). tools/check_godot_snapshot.sh asserts "channels=6". Task: add the live pool planes (+ optional per-cell energy/biomass) as new snapshot channels (GSS2→GSS3), resampled from PoolStock the same way soil is resampled, and render them as selectable overlays. The snapshot is OFF the determinism hash, so this is hash-neutral. KEEP biology in the core (inv #2): godot only renders the exported numbers. READ snapshot.rs, the godot reader + layer picker, and check_godot_snapshot.sh first.\n\n` +
    `The pinned determinism literal 0x4e4d_0520_722a_a069 MUST stay unchanged. Return a concrete file-level design.`,
    { label: `design:lens${i}`, phase: 'Design', schema: DSCHEMA },
  ),
))).filter(Boolean)
const chosen = await agent(
  `Judge & synthesize these ${proposals.length} designs into ONE plan. Pin the exact channel set + new CHANNEL_COUNT, the GSS3 bump, the reader/gate-script update, and the overlay rendering + legends. Hash-neutral (off-hash snapshot). Output the final design.\n` +
    proposals.map((p, i) => `\n--- Design ${i} ---\n${JSON.stringify(p, null, 2)}`).join('\n'),
  { label: 'design:judge', phase: 'Design', schema: DSCHEMA },
)

phase('Implement')
const contract = JSON.stringify(chosen, null, 2)
const [rustDone, gdDone] = await parallel([
  () => agent(
    `Implement ONLY the Rust side of this agreed plan (snapshot.rs + cdylib + the gate script channel count; do NOT touch godot/*.gd):\n${contract}\n\n` +
    `Add the new channels (resample PoolStock like soil is resampled), bump GSS2→GSS3 + CHANNEL_COUNT, update the godot-sim snapshot passthrough, and update tools/check_godot_snapshot.sh's expected channel count. The snapshot stays a read-only off-hash projection — zero RNG, do NOT fold pools into hash_world, do NOT change the pinned literal 0x4e4d_0520_722a_a069. Add/extend a snapshot test for the new channels. Do NOT commit. Report files+lines.`,
    { label: 'impl:rust', phase: 'Implement', agentType: 'implementer' },
  ),
  () => agent(
    `Implement ONLY the GDScript side of this agreed plan (do NOT touch crates/**):\n${contract}\n\n` +
    `Render the new pool/energy/biomass planes as selectable data-layer overlays through the existing layer picker (main.gd _layer_picker + organisms.gd + the data-layer shader), with legends, so a player can SEE nutrient drain / mineralization / energy. Keep ALL biology in the core (inv #2): GDScript only renders the exported channel floats. Do NOT commit. Report files+lines.`,
    { label: 'impl:gdscript', phase: 'Implement' },
  ),
])

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. The godot-reader gate must pass with the NEW channel count; determinism must stay GREEN against 0x4e4d_0520_722a_a069. Report all gates PASS/FAIL. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'inv2_preserved', 'pools_visible', 'gss3_reader_consistent', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'pinned literal unchanged; pools not folded into hash_world; zero RNG' },
    inv2_preserved: { type: 'boolean', description: 'no biology computed in GDScript; only exported channel floats rendered' },
    pools_visible: { type: 'boolean', description: 'light/free_nutrient/detritus (+ energy/biomass) render as selectable overlays with legends' },
    gss3_reader_consistent: { type: 'boolean', description: 'the GSS3 channel count is consistent across snapshot.rs, the cdylib, the godot reader, and check_godot_snapshot.sh; old GSS2 readers fail loudly' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const verdict = await agent(
  `Adversarially verify the joule-economy visualisation. Read \`git diff\`. Try to REFUTE each property; default false if unconfirmable. Confirm the snapshot stayed off-hash (literal unchanged) and no biology leaked into GDScript.`,
  { label: 'verify', phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
)

log(`gate: ${typeof gate === 'string' ? gate.slice(0, 200) : ''}`)
return { chosen, rustDone, gdDone, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, verdict }
