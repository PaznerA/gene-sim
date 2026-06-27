export const meta = {
  name: 'variant-lab-save-reseed',
  description:
    'Variant Lab Slices B + C — the player save→name→reseed loop. B (save named variant): a READ-ONLY core/env export of a roster species\' CURRENT (post-edit) genome + niche (role/host/key) as a SpeciesSpec JSON (hash-neutral, like observe_species/species_signatures — via SpeciesSpec::from_genome patched with the species\' niche, or from_built); godot-sim `export_species_json(species_id) -> GString`; a specimen-view "💾 Save variant" action that names + stores {name, json} in a renderer saved-variants registry. C (reseed): a "Saved variants" section (mirroring the contaminant consortium panel) where a saved variant is registered (register_contaminant_json) + inoculated (inoculate) into a brush region — REUSING the existing contaminant/Inoculate machinery. The pinned literal 0x47a0_3c8f_6701_f240 stays byte-identical (the export is read-only; register/inoculate are existing journaled actions). Then gate + adversarially verify.',
  whenToUse: 'Variant Lab epic, after Slice A (per-species edit). The player-facing save+reseed loop.',
  phases: [{ title: 'Export' }, { title: 'UI' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Export')
const s1 = await agent(
  `Implement Variant Lab Slice B — the CORE/BOUNDARY export (save a species\' CURRENT edited genome as a SpeciesSpec JSON). READ FIRST: crates/genome/src/spec.rs (SpeciesSpec::from_built ~242 + from_genome — note from_built carries the niche fields (entity_count/trophic_role/host_key) that from_genome drops, per the R2 round-trip), crates/harness/src/lib.rs (how the env holds per-species state: roster Vec<(BuiltSpecies,u32)> ~332, register_contaminant ~474, and where the LIVE per-species genome lives after edits), crates/sim-core/src/lib.rs (the per-species genome entries[sid].genome + the species registry niche/role/host/key; observe_species/species_signatures as the read-only-export precedents), and crates/godot-sim/src/lib.rs (observe_species #[func] ~358 + register_contaminant_json ~636 as the boundary patterns). CLAUDE.md inv #2 (biology stays in core) + #3 (read-only export = hash-neutral).\n\n` +
  `  - Add a READ-ONLY core/env method that, for a given species id, builds a SpeciesSpec from the species\' CURRENT (post-edit) genome + its niche (key/name/role/host) and returns the SpeciesSpec JSON text (serde_json). Use from_built (or from_genome + patch the niche) so a reseeded variant carries its trophic_role/host_key. It must DRAW ZERO SimRng and not mutate the sim (hash-neutral — model it on observe_species/species_signatures). A round-trip test: export species S → build_species_from_str(json) → a BuiltSpecies whose expressed phenotype matches the live species\' phenotype (the save→reseed contract).\n` +
  `  - godot-sim: a #[func] export_species_json(species_id: i64) -> GString returning that JSON (empty GString + godot_error on a bad id / before reset, like the other guarded #[func]s).\n` +
  `  - VERIFY the pinned literal 0x47a0_3c8f_6701_f240 is UNMOVED (the export is read-only — run the determinism test). Rust + cdylib build. Do NOT commit. Report the export method + #[func] signature + confirm hash 0x47a0 unmoved + the round-trip test passes.`,
  { label: 'export', phase: 'Export', agentType: 'implementer' },
)

phase('UI')
const s2 = await agent(
  `Implement Variant Lab Slices B+C — the UI (save-named-variant + the reseed section), on the Stage-1 export:\n${typeof s1 === 'string' ? s1.slice(0, 700) : ''}\n\n` +
  `READ godot/main.gd: the specimen view (_build_specimen_ui ~3059, _render_specimens, _focus, _specimen_list, _specimen_at, the focused specimen + its group/species_id), the contamination panel (_build_contamination_ui ~983 with the _consortium_checks section — MIRROR this for the saved-variants section), the Inoculate tool (TOOL_INOCULATE, _build_inoculate_params, register_contaminant_json + inoculate usage, _registered_contaminants), and _picker_species_id. Renderer-only (inv #2): GDScript only moves inert JSON + names + ints.\n\n` +
  `  - SLICE B (save): in the SPECIMEN view, add a "💾 Save variant" button (in the specimen UX panel / on the focused specimen) → a name LineEdit/prompt → call _live.export_species_json(focused species_id) → store {name, json, key, species_id, traits} in a renderer-side _saved_variants registry (Array/Dictionary). Show a small list of saved variants (name + a tiny glyph/role). Guard: only enabled in --live with the export #[func] present (has_method probe, like observe_species). A blank/dup name gets a sensible default/suffix.\n` +
  `  - SLICE C (reseed): a "Saved variants" SECTION (mirror the contaminant consortium menu) — each saved variant gets a "🌱 Reseed" affordance that registers it via _live.register_contaminant_json(json) (once, tracked like _registered_contaminants) and arms it as the active Inoculate payload, so the next Inoculate brush stroke (TOOL_INOCULATE) inoculates THAT saved variant into the painted disc — reusing 100% of the existing inoculate machinery. (Manual inoculation works at any containment, per ADR-019.) Keep it renderer-only.\n` +
  `  - Build the cdylib + headless parse check + (if feasible) confirm the specimen "Save variant" + the reseed section construct without error. Do NOT commit. Report the saved-variants store + the save button + the reseed section wiring + confirm it builds + parses.`,
  { label: 'ui', phase: 'UI', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim (generous timeout ~15 min). Variant Lab B+C (save-variant export + reseed UI) must be GREEN: fmt, clippy, test (incl. the export round-trip test), determinism MUST be GREEN against the pinned literal 0x47a0_3c8f_6701_f240 (the export is read-only — a moved hash is a FAIL), license green, godot-reader + livesim green (the new export_species_json #[func] + the UI must not break the GDExtension smoke). Report every gate PASS/FAIL with exact errors. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['export_hash_neutral_readonly', 'export_round_trips', 'no_biology_in_gdscript', 'reseed_reuses_contaminant_path', 'ui_correct', 'issues'],
  properties: {
    export_hash_neutral_readonly: { type: 'boolean', description: 'inv #3: export_species_json is READ-ONLY (zero SimRng, no sim mutation — like observe_species); the pinned literal 0x47a0_3c8f_6701_f240 is UNMOVED; determinism gate green.' },
    export_round_trips: { type: 'boolean', description: 'A test proves export species S → build_species_from_str → a BuiltSpecies whose phenotype matches the live species (the save→reseed contract); the exported SpeciesSpec carries the niche (role/host) so a reseed is faithful.' },
    no_biology_in_gdscript: { type: 'boolean', description: 'inv #2: the save/reseed UI only moves inert JSON + names + ints; no genotype→phenotype in GDScript (the export builds the spec in core; register/inoculate stay in core).' },
    reseed_reuses_contaminant_path: { type: 'boolean', description: 'Reseed registers via register_contaminant_json + inoculates via the EXISTING inoculate/TOOL_INOCULATE machinery (no new core action); manual inoculation works at any containment (ADR-019).' },
    ui_correct: { type: 'boolean', description: 'The specimen-view "Save variant" stores {name, json} from the FOCUSED species; the saved-variants reseed section mirrors the contaminant consortium menu; everything is has_method-guarded for an older cdylib / file-replay; godot-reader + livesim smoke green.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify Variant Lab B+C (save-named-variant + reseed) on branch auto/variant-lab-save-reseed-2026-06-24. Read \`git diff main...HEAD\` (or \`git diff\`), the export method + export_species_json #[func], the specimen-view save UI + the saved-variants reseed section, and CLAUDE.md inv #2/#3. Skeptic #${i} — default each boolean FALSE unless confirmed. Hunt: a MOVED pinned hash 0x47a0_3c8f_6701_f240 or an export that DRAWS SimRng / mutates the sim (it must be read-only like observe_species); an export that DROPS the niche (role/host) so a reseeded variant is a different organism; biology computed in GDScript (the save/reseed UI must only move inert JSON+names; the SpeciesSpec is built in core); a reseed that bypasses the existing register_contaminant_json/inoculate path (a new core action would be wrong); and an un-guarded export call that crashes on an older cdylib / file-replay. Report the structured verdict with file:line. Do NOT edit.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['export_hash_neutral_readonly', 'export_round_trips', 'no_biology_in_gdscript', 'reseed_reuses_contaminant_path', 'ui_correct']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  export: typeof s1 === 'string' ? s1.slice(0, 700) : s1,
  ui: typeof s2 === 'string' ? s2.slice(0, 700) : s2,
  gate: typeof gate === 'string' ? gate.slice(0, 500) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — save-variant export hash-neutral + round-trips, reseed reuses the contaminant path' : 'NEEDS WORK',
}
