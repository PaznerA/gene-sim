export const meta = {
  name: 'core-ui-items-123-impl',
  description:
    'Back-to-the-core UI work, items 1-2-3 (renderer-only, hash-neutral, inv #2). ITEM 1: gen-0 specimen fix — a freshly-set species shows its baseline morphology IMMEDIATELY in the specimen view at generation 0 (no fallback-to-ecosystem until the sim steps). ITEM 2: contamination + intervention UI in-run polish (richer tool tooltips, labeled consortium checkboxes, target-picker validation before dispatch, hide the faded species-name watermark bleeding behind CONTROLS). ITEM 3: sandbox readability — surface emergent dynamics legibly (relations top-N flow summary + advisory-label clarity, title-bar vitals ▲▼ trend glyphs, extinction/boom population alerts). All read already-exported core data; the pinned literal 0x47a0 stays unchanged.',
  whenToUse:
    'After the perf chapter — back to the core/gameplay. Three godot UI items on shared main.gd → sequential. Renderer-only; hash-neutral; --shot-verified. Stops for human commit.',
  phases: [
    { title: 'Item1-specimen' },
    { title: 'Item2-polish' },
    { title: 'Item3-readability' },
    { title: 'Gate' },
    { title: 'Verify' },
  ],
}

phase('Item1-specimen')
const item1 = await agent(
  `ITEM 1 — gen-0 specimen fix for gene-sim. GDScript/renderer ONLY (godot/main.gd; do NOT touch crates/**). The pinned hash 0x47a0_3c8f_6701_f240 is unaffected (renderer-only).\n\n` +
  `THE BUG (reproduce it first): in --live mode, setting a non-default species (e.g. bacillus) and switching to the specimen view (VIEW_SPECIMEN) at GENERATION 0 shows nothing / the default plant / falls back to the ecosystem view; the species' baseline morphology only appears AFTER the sim steps a few ticks (when a genome gets logged). Reproduce via: \`godot --path godot -- --live --species bacillus --view specimen --focus 0 --shot /tmp/genesim_shots/item1_before.png\` (NO --inject = gen 0) and confirm it does NOT show the Bacillus rod+endospore.\n\n` +
  `DIAGNOSE precisely (the recon was uncertain): trace _set_view_mode(VIEW_SPECIMEN) → _refresh_live_specimens (~main.gd:2293/2413) → _render_specimens → _specimen_list (~2428) → _live_species_logs / _live_species_order (populated by _log_live_genome via observe_species at reset in _resync_to_live ~2099-2122). Find why _live_species_order is empty (or the baseline isn't rendered) at gen 0 — likely observe_species returns empty on the very first reset before any step, so _log_live_genome falls back to _log_primary_genome which does NOT populate _live_species_order, so _specimen_list falls back to the file-replay path (empty in --live).\n\n` +
  `FIX (minimal, renderer-only): ensure each registered species' BASELINE specimen is built + rendered IMMEDIATELY at reset / on the view switch, from the registered SpeciesSpec / the observe row — not only from the accumulated step log. The gen-0 specimen view must show the species' baseline morphology glyph + its inspect, with zero steps. Verify with \`--shot /tmp/genesim_shots/item1_after.png\` (same command) that the Bacillus rod+endospore + the SPECIMEN panel now show at gen 0. After writing, run \`bash tools/check_godot_snapshot.sh\` to confirm the GDScript parses (no SP-4-style parse error). Do NOT commit. Report file:line + confirm the before/after shots.`,
  { label: 'item1', phase: 'Item1-specimen' },
)

phase('Item2-polish')
const item2 = await agent(
  `ITEM 2 — contamination + intervention UI in-run polish for gene-sim. GDScript/renderer ONLY (godot/main.gd). Hash-neutral. Build on Item 1's changes (already in the tree).\n\n` +
  `Deliver these high-value polish items (from the recon, file:line approximate — verify against the tree):\n` +
  `  (a) RICHER TOOL TOOLTIPS (~main.gd:824-831): the 6 palette buttons (CRISPR/PCR/Antibiotic/Nutrient/Toxin/Inoculate) have terse 1-line tooltips. Expand each to 2-3 lines explaining WHAT it does + that Nutrient/Toxin target the POOL/CHEM channel (not an organism) while Antibiotic/Cull kill a fraction of resident orgs, PCR amplifies a resident, Inoculate seeds a contaminant. Position matters (the brush).\n` +
  `  (b) LABELED CONSORTIUM (~main.gd:917-924): the CONTAMINATION consortium checkboxes (per contaminant key) have no header/explanation. Add a header label ("Airborne immigrants (arrive on schedule when Containment > Sealed):") + a per-checkbox tooltip one-liner per species (a short role/trait blurb — reuse the codex if a codex lookup is handy, else a static one-liner). Also add the Inoculate-tool note that manual seeds work at Sealed but scheduled immigrants need Containment > Sealed.\n` +
  `  (c) TARGET-PICKER VALIDATION (~main.gd:1310-1323): when PCR/Antibiotic/Cull dispatch with no valid target species selected, it silently no-ops. Validate the picker BEFORE dispatch — if invalid, _flash_status / _record_tool_outcome a clear "✗ no target species selected" and return early (so the player gets feedback).\n` +
  `  (d) HIDE THE FADED WATERMARK: a faded species-name text watermark bleeds through BEHIND the CONTROLS panel in the ecosystem + specimen views (visible in screenshots — a large low-alpha label). Find it (search the specimen/world-space label rendering, e.g. a species-name Label drawn at low alpha) and hide it / gate it so it does not bleed behind the panels.\n` +
  `Keep biology in the core (inv #2) — labels/tooltips/validation only read exported data. Run \`bash tools/check_godot_snapshot.sh\` (parse). Do NOT commit. Report file:line per item.`,
  { label: 'item2', phase: 'Item2-polish' },
)

phase('Item3-readability')
const item3 = await agent(
  `ITEM 3 — sandbox readability / emergent-dynamics legibility for gene-sim. GDScript/renderer ONLY (godot/main.gd + godot/relations_heatmap.gd as needed). Hash-neutral. Build on Items 1+2 (in the tree).\n\n` +
  `Deliver these high-value readability items (from the recon — all read already-exported data: observe_species/observe_all, the FlowMatrix, the snapshot, the sparkline history; NO new core export):\n` +
  `  (a) RELATIONS TOP-N FLOW SUMMARY (~main.gd:2809-2857, the Relations view): above the FlowMatrix heatmap, add a plain-language "who's eating whom" summary line listing the top 3-5 NONZERO flows parsed from the matrix, e.g. "Primary flows: plant → ecoli (+450 J/gen), ecoli → bdellovibrio (−200 J/gen)". Also clarify the advisory nearest-species strip (~2850-2856) with a small "◆ ADVISORY · metabolic similarity, not measured flows" badge so it's not confused with the measured heatmap.\n` +
  `  (b) TITLE-BAR VITALS TREND GLYPHS (~main.gd:291 / _refresh_hud): the title-bar chips (population/fitness/allele) update per-frame but give no sense of direction. Add a small ▲/▼/→ trend glyph (color-coded green/red/neutral) next to each, computed from the sparkline history (~main.gd:296) or the previous frame — so "population crashing" / "fitness diverging" is visible at a glance.\n` +
  `  (c) EXTINCTION / BOOM ALERTS: in --live, when a species' population hits 0 (extinction) or jumps >~10× (boom) vs the previous frame, flash a brief HUD notification ("✗ plant extinct" / "📈 bdellovibrio boom") — poll observe_species() population vs the prior frame (a _prev_pop cache), no new core export. This surfaces the emergent "oh, the predator ate them all" moment.\n` +
  `Keep biology in the core (inv #2). Run \`bash tools/check_godot_snapshot.sh\` (parse). Do NOT commit. Report file:line per item.`,
  { label: 'item3', phase: 'Item3-readability' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. determinism GREEN against 0x47a0_3c8f_6701_f240 (renderer-only → hash-neutral); the godot-reader gate MUST be green (no GDScript parse errors across items 1-2-3); livesim green. Report all gates PASS/FAIL with exact errors. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'parses_clean', 'item1_gen0_specimen', 'item2_polish', 'item3_readability', 'inv2_preserved', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'pinned literal 0x47a0 unchanged; determinism green' },
    parses_clean: { type: 'boolean', description: 'GDScript parses (godot-reader gate green) — no SP-4-style parse error' },
    item1_gen0_specimen: { type: 'boolean', description: 'a freshly-set species shows its baseline morphology + inspect in the specimen view at gen 0 (the --shot proves it; no fallback)' },
    item2_polish: { type: 'boolean', description: 'richer tool tooltips + labeled consortium + target-picker validation + the faded watermark hidden' },
    item3_readability: { type: 'boolean', description: 'relations top-N flow summary + advisory badge + vitals trend glyphs + extinction/boom alerts' },
    inv2_preserved: { type: 'boolean', description: 'GDScript only renders + reads exports; no biology in GDScript' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const verdict = await agent(
  `Adversarially verify the core UI items 1-2-3. Read \`git diff\`. Try to REFUTE each property; default false if unconfirmable. KEY checks: does the gen-0 specimen now show the baseline morphology (item 1 — read the fix + confirm the --shot logic)? Are the item-2 polish + item-3 readability pieces actually wired (not stubs)? Does the GDScript parse (godot-reader green)? Is the pinned literal unchanged + no biology leaked into GDScript?`,
  { label: 'verify', phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
)

return { item1, item2, item3, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, verdict }
