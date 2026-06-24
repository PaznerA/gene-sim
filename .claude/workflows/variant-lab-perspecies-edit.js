export const meta = {
  name: 'variant-lab-perspecies-edit',
  description:
    'Variant Lab Slice A — make the whole-species CRISPR inject target ANY roster species, not just the active/primary one. CORE: add a `species: u16` field to harness `EditAction` with #[serde(default)] (absent → 0 = primary = today\'s behavior, so old journals + the pinned config are BYTE-IDENTICAL — hash-neutral), and resolve it in the env\'s Action::ApplyEdit via the EXISTING species:u16→SpeciesId boundary the other interventions use (commit_species_edit / the SP-3 tools). BOUNDARY: godot-sim `apply_edit` gains a `species: i64` param (mirroring the existing pcr_amplify/cull signatures). UI: the CRISPR inject panel gains a target-species OptionButton (mirroring _pcr_species / _cull_species), and _on_inject_pressed passes the chosen species id. The pinned literal 0x47a0_3c8f_6701_f240 MUST stay byte-identical (default species 0 + serde default). Then gate + adversarially verify.',
  whenToUse: 'Variant Lab epic, Slice A (per-species CRISPR edit). Foundation for save-variant + reseed + auto-research edits.',
  phases: [{ title: 'Core' }, { title: 'Boundary-UI' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Core')
const s1 = await agent(
  `Implement Variant Lab Slice A — CORE part (per-species CRISPR edit) for gene-sim. READ FIRST: crates/harness/src/lib.rs (the EditAction struct ~line 77-95, Action::ApplyEdit ~106, the env.step ApplyEdit handler ~822, commit_species_edit(species:u16) ~624, and how the SP-3 intervention tools resolve a raw species:u16 → SpeciesId at the boundary ~891/1309), crates/sim-core/src/lib.rs (SpeciesId::from_raw ~333, the per-species edit hook ~2956), and CLAUDE.md inv #3 (the pinned literal 0x47a0_3c8f_6701_f240 is sacred). Also read crates/harness/src/main.rs (the recorded-episode golden demo that uses ApplyEdit ~540).\n\n` +
  `GOAL: make Action::ApplyEdit target a CHOSEN species (default the primary), HASH-NEUTRAL.\n` +
  `  - Add \`species: u16\` to EditAction with #[serde(default)] (serde default = 0 → the primary species → EXACTLY today's behavior; so OLD journals without the field deserialize to 0 and the recorded-episode golden + the R2 round-trip + the pinned config are BYTE-IDENTICAL). Document the field (inv #6 species-granular).\n` +
  `  - In the env's Action::ApplyEdit handler, resolve edit.species → SpeciesId via the SAME species:u16→SpeciesId boundary the other interventions use, and apply the edit to THAT species' genome (the per-species hook already exists; species 0 = the resident primary = unchanged). Keep the RNG threading identical (inv #3 — the edit draws from the single seeded stream exactly as before; targeting a non-primary species must draw the SAME way so a species-0 edit is byte-identical to today).\n` +
  `  - VERIFY the pinned literal 0x47a0_3c8f_6701_f240 is UNMOVED (run the determinism test). Add a test: an ApplyEdit with species:0 (or default) is BYTE-IDENTICAL to the pre-change behavior (same run hash); an ApplyEdit targeting a NON-primary species in a multi-species run edits THAT species (its phenotype changes, the others' don't) and is deterministic/replayable. Confirm the recorded-episode golden + R2 save/load round-trip still pass (serde default makes the old journal parse).\n` +
  `Rust only this phase (no godot/UI). Do NOT commit. Report the EditAction change + confirm hash 0x47a0 unmoved + the per-species + serde-default tests pass.`,
  { label: 'core', phase: 'Core', agentType: 'implementer' },
)

phase('Boundary-UI')
const s2 = await agent(
  `Implement Variant Lab Slice A — BOUNDARY + UI part (the godot-sim apply_edit species param + the CRISPR target-species picker), on the Stage-1 core change:\n${typeof s1 === 'string' ? s1.slice(0, 700) : ''}\n\n` +
  `READ crates/godot-sim/src/lib.rs (the apply_edit #[func] ~515, and the EXISTING pcr_amplify ~698 / cull ~747 which already take \`species: i64\` — mirror that), and godot/main.gd (the CRISPR param panel _build_crispr_params with the Cas/Locus/Guide pickers + the 💉 inject button; the PCR panel _build_pcr_params with _pcr_species OptionButton + _populate_species_pickers; _on_inject_pressed ~1264 which calls _live.apply_edit; _picker_species_id / _has_target_species helpers).\n\n` +
  `  - godot-sim: change \`apply_edit(cas, target, guide)\` → \`apply_edit(cas, target, guide, species: i64)\` (mirror pcr_amplify/cull's species param), building Action::ApplyEdit with the EditAction.species set (clamp like the others). Backward note: GDScript callers must pass the species (default the active/primary).\n` +
  `  - UI (godot/main.gd, renderer-only): add a target-species OptionButton to _build_crispr_params (mirror _pcr_species in _build_pcr_params — populated by _populate_species_pickers from observe_species), e.g. _crispr_species. _on_inject_pressed reads the chosen species id (via _picker_species_id, defaulting to the active/primary when none) and passes it to _live.apply_edit(cas, locus, guide, species). Keep the existing 💉 inject button + Enter hook. The appended specimen variant (_append_edit_variant_for) should attribute to the EDITED species id (it already takes a target_sid).\n` +
  `  - Rebuild the cdylib + do a headless parse check. Do NOT commit. Report the godot-sim signature + the UI picker wiring + confirm it builds + parses.`,
  { label: 'boundary-ui', phase: 'Boundary-UI', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim (generous timeout ~15 min). The Variant Lab Slice A (per-species edit) must be GREEN: fmt, clippy, test, determinism MUST be GREEN against the pinned literal 0x47a0_3c8f_6701_f240 (the EditAction.species field is #[serde(default)]=0=primary → BYTE-IDENTICAL; a moved hash is a FAIL), the recorded-episode golden + R2 round-trip pass, license green, godot-reader + livesim green (the apply_edit signature change must not break the GDExtension smoke). Report every gate PASS/FAIL with exact errors. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'serde_default_journal_compat', 'per_species_edit_works', 'rng_threading_identical', 'ui_picker_correct', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'inv #3: the pinned literal 0x47a0_3c8f_6701_f240 is UNMOVED (a species:0/default edit is byte-identical to pre-change); determinism gate green. NOT a re-pin.' },
    serde_default_journal_compat: { type: 'boolean', description: 'EditAction.species is #[serde(default)] so OLD journals (no field) deserialize to 0=primary → the recorded-episode golden + R2 save/load round-trip still pass byte-identically; the journal/replay contract is preserved.' },
    per_species_edit_works: { type: 'boolean', description: 'A test proves an ApplyEdit targeting a NON-primary species in a multi-species run edits THAT species (its phenotype changes, others unchanged) and is deterministic/replayable; the boundary resolves species:u16→SpeciesId correctly.' },
    rng_threading_identical: { type: 'boolean', description: 'inv #3: targeting a species draws from the single seeded stream EXACTLY as before — a species-0 edit consumes the same RNG words as the pre-change code (no extra/reordered draws), which is why the hash holds.' },
    ui_picker_correct: { type: 'boolean', description: 'The CRISPR panel target-species OptionButton mirrors _pcr_species (populated from observe_species), _on_inject_pressed passes the chosen species id (default active/primary), and the appended specimen variant attributes to the edited species. godot-sim apply_edit gained species:i64 mirroring pcr/cull; the GDExtension smoke is green.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify Variant Lab Slice A (per-species CRISPR edit) on branch auto/variant-lab-perspecies-edit-2026-06-24. Read \`git diff main...HEAD\` (or \`git diff\`), the EditAction + env ApplyEdit change, the godot-sim apply_edit signature, the UI picker, and CLAUDE.md inv #3. Skeptic #${i} — default each boolean FALSE unless confirmed. Hunt: a MOVED pinned hash 0x47a0_3c8f_6701_f240 (a re-pin = FAIL — the species field MUST default to 0=primary and a species-0 edit MUST be byte-identical); a serde change that BREAKS the recorded-episode golden / R2 round-trip (the field MUST be #[serde(default)] so old journals parse to 0); RNG threading that changed for a species-0 edit (extra/reordered SimRng draws → silent hash move); a per-species edit that does NOT actually target the chosen species (or that can panic on an out-of-range species id); and a UI/GDExtension break (the apply_edit signature change must update every GDScript caller). Report the structured verdict with file:line. Do NOT edit.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['hash_neutral', 'serde_default_journal_compat', 'per_species_edit_works', 'rng_threading_identical', 'ui_picker_correct']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  core: typeof s1 === 'string' ? s1.slice(0, 700) : s1,
  ui: typeof s2 === 'string' ? s2.slice(0, 600) : s2,
  gate: typeof gate === 'string' ? gate.slice(0, 500) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — per-species edit, hash-neutral, journal-compatible' : 'NEEDS WORK',
}
