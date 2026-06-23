export const meta = {
  name: 'ecosystem-species-viz-impl',
  description:
    'Fix the ecosystem-map species visualization (the map is unusable — every organism renders the same size regardless of species). Add a per-cell dominant_species_id channel to GridSnapshot (HASH-NEUTRAL — the snapshot is off the determinism hash path, draws no RNG) so the renderer knows which species occupies each cell, + a per-species visual table (size/color by key/role, real cell-size scale: plant ≫ rod ≫ predator ≫ symbiont), + organisms.gd sizes/colors each cell by its dominant species, + per-zoom-scope differentiation. The pinned literal 0x47a0 stays unchanged.',
  whenToUse:
    'The active blocker — a multi-species map looks like uniform haze. Snapshot is render-only/off-hash so the new channel is hash-neutral. Touches sim-core snapshot + godot renderer; --shot/gate-verified.',
  phases: [
    { title: 'Implement' },
    { title: 'Gate' },
    { title: 'Verify' },
  ],
}

phase('Implement')
const impl = await agent(
  `Fix the gene-sim ECOSYSTEM-MAP species visualization (per the recon plan below). The map currently sizes every organism from ONE per-cell density-derived radius (organisms.gd ~base_r) → all species look the same size → unusable. READ crates/sim-core/src/snapshot.rs (GridSnapshot + the GSS binary writer + SNAPSHOT_MAGIC), crates/sim-core/src/lib.rs Simulation::snapshot() (~the per-cell aggregation), crates/godot-sim/src/lib.rs (the snapshot export to godot), godot/snapshot.gd (the GSS decoder), godot/organisms.gd (the org _draw + base_r + _organism_color), godot/glyph_factory.gd + godot/codex.gd (the per-species palette/role data), and crates/godot-sim/godot/livesim_smoke.gd (asserts the GSS magic + channel_count — a KNOWN gotcha: a format change breaks it, like GSS2→GSS3→GSS4 did before).\n\n` +
  `PHASE 1 — the SNAPSHOT channel (HASH-NEUTRAL: the snapshot is read-only, off hash_world, draws ZERO SimRng — adding a derived display channel is hash-neutral like light/detritus/toxin already are; the pinned literal 0x47a0_3c8f_6701_f240 MUST stay unchanged):\n` +
  `  - Add a per-cell \`dominant_species_id\` to GridSnapshot: in Simulation::snapshot(), for each cell aggregate the resident organisms' Species/SpeciesId and emit the MOST-POPULOUS species id per cell (tie → lowest SpeciesId, deterministic). Serialize it as an added channel (u16→f32, or u16 — your call), bumping the format (SNAPSHOT_MAGIC GSS4→GSS5 AND/OR the channel_count) so readers parse it. UPDATE EVERY GSS reader to the new format: godot/snapshot.gd (decode the channel), crates/godot-sim (if it re-reads/forwards), crates/sim-core/godot-reader if any, tools/check_godot_snapshot.sh expectations, and especially crates/godot-sim/godot/livesim_smoke.gd (the magic + channel_count assert). Add a sim-core test: a multi-species world snapshot has the correct dominant_species_id per cell; a single-species snapshot is unaffected + the pinned hash is byte-identical.\n` +
  `PHASE 2 — the RENDERER (renderer-only, hash-neutral): a per-species VISUAL table (a new godot/species_visual_map.gd or extend glyph_factory) keyed by species key/role giving a SIZE multiplier + a color, on a real cell-size scale (plant/mold LARGE, E.coli/Bacillus small rods, Bdellovibrio a tiny speck, Carsonella/Syn3 tinier). organisms.gd: accept the per-cell dominant_species_id (set_dominant_species_ids), and in _draw size + color each cell's organisms by its dominant species via the table (replace the single density-derived base_r + the single-species _organism_color). main.gd: wire set_dominant_species_ids(snap.dominant_species_id) after loading the snapshot. Per-zoom-scope: Field = species-colored/sized density; Patch/Cells = the species-sized sprites. Graceful: unknown species_id → a default scale, never crash; empty cell → no draw.\n\n` +
  `KEEP biology in the core (inv #2) — the renderer only maps the exported species id → a visual. The pinned literal 0x47a0_3c8f_6701_f240 MUST stay unchanged (snapshot off-hash). After writing, run \`bash tools/check_godot_snapshot.sh\` (the GSS5 reader + render must be green) + verify a snapshot decodes with the new channel. Do NOT commit. Report file:line + confirm the hash is unmoved + all GSS readers updated to the new format.`,
  { label: 'impl', phase: 'Implement', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. determinism MUST be GREEN against 0x47a0_3c8f_6701_f240 (the snapshot is off the hash path — adding dominant_species_id is hash-neutral; a moved hash is a FAIL). godot-reader + livesim MUST be green (every GSS reader updated to the new format — the livesim_smoke magic/channel assert is the classic break). fmt/clippy/test green. Report all gates PASS/FAIL with exact errors. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'all_gss_readers_updated', 'dominant_species_channel', 'per_species_sizing', 'inv2_preserved', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'pinned literal 0x47a0 unchanged (snapshot off-hash); determinism gate green' },
    all_gss_readers_updated: { type: 'boolean', description: 'the GSS format change is handled by EVERY reader (snapshot.gd, livesim_smoke.gd, check_godot_snapshot, godot-sim) — godot-reader + livesim gates green, no stale-magic break' },
    dominant_species_channel: { type: 'boolean', description: 'the snapshot carries a correct per-cell dominant_species_id (a sim-core test asserts it on a multi-species world); single-species byte-identical' },
    per_species_sizing: { type: 'boolean', description: 'organisms.gd sizes + colors each cell by its dominant species via a real-scale visual table (plant ≫ rod ≫ predator ≫ symbiont) — not one uniform radius' },
    inv2_preserved: { type: 'boolean', description: 'the renderer only maps the exported species id → a visual; no biology in GDScript' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const verdict = await agent(
  `Adversarially verify the ecosystem-map species-viz fix. Read \`git diff\`. Try to REFUTE each property; default false if unconfirmable. KEY checks: is the pinned literal 0x47a0 UNCHANGED (snapshot off-hash, NOT a re-pin)? Is EVERY GSS reader updated to the new format (no stale magic/channel_count assert — the livesim_smoke gotcha)? Does the snapshot carry a correct per-cell dominant_species_id? Does organisms.gd actually size species differently (plant vs rod vs predator vs symbiont) instead of one uniform radius?`,
  { label: 'verify', phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
)

return { impl, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, verdict }
