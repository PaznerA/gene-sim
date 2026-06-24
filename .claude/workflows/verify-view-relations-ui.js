export const meta = {
  name: 'verify-view-relations-ui',
  description:
    'Adversarially verify the renderer-only VIEW/RELATIONS UI rework (godot/main.gd): (A) the relations graph + heatmap now render FULL-WINDOW (_relations_full, a full-rect Control like _specimen_root) with a compact floating info/toggle card; (B) an always-on top-right VIEW switcher (segmented Ecosystem/Specimen/Relations) + SCOPE (Field/Patch/Cells) panel, REMOVED from the CONTROLS deck; the per-view top-right panels (INTERVENE/CONTAMINATION/SPECIMEN/RELATIONS) shifted down to clear it. ZERO Rust → pinned hash 0x47a0_3c8f_6701_f240 trivially unmoved. Three skeptics read the diff and hunt: an inv #2 violation (any biology in the UI code), an inv #3 risk (any Rust touched), GDScript correctness (a dangling _view_button reference, the VIEW/SCOPE buttons not syncing with KEY_V cycle / --view flag / _set_view_mode, the relations full-window breaking the graph/heatmap feed or swallowing the card clicks via mouse_filter, panel-overlap regressions), and UX faithfulness to the two asks.',
  whenToUse: 'After the view/relations UI rework + shots + gate GREEN, before merge.',
  phases: [{ title: 'Verify' }],
}

const VSCHEMA = {
  type: 'object',
  required: ['no_biology_in_gdscript', 'hash_neutral_no_rust', 'gdscript_correct', 'view_scope_sync_correct', 'relations_full_window_ok', 'ux_faithful', 'issues'],
  properties: {
    no_biology_in_gdscript: { type: 'boolean', description: 'inv #2: every new/changed line is pure VIEW / CAMERA / LAYOUT state — no genotype→phenotype, no biology decided in GDScript.' },
    hash_neutral_no_rust: { type: 'boolean', description: 'inv #3: the diff touches ZERO Rust (only godot/main.gd). The pinned literal 0x47a0_3c8f_6701_f240 cannot move. Flag any crates/** edit.' },
    gdscript_correct: { type: 'boolean', description: 'No dangling reference to the removed _view_button (it was deleted; _set_view_mode / _build_controls / _sync_controls must not touch it). The removed _on_view_pressed has no remaining callers (KEY_V calls _set_view_mode directly). _build_viewscope_ui is called after _build_controls (which no longer creates _view_button/_scope_buttons) and creates _view_buttons + _scope_buttons. No parse/runtime error.' },
    view_scope_sync_correct: { type: 'boolean', description: 'The segmented VIEW toggles stay in step via _sync_view_buttons() called from _set_view_mode (so KEY_V cycling, the --view shot flag, and a button press all reflect); the SCOPE buttons are synced by _sync_scope_buttons() (kept in _build_viewscope_ui, called on zoom change); KEY_1/2/3 + KEY_V still work; the buttons are top-right and separated from the CONTROLS deck.' },
    relations_full_window_ok: { type: 'boolean', description: 'The relations graph + heatmap are now children of a full-rect _relations_full Control (gated visible by _set_view_mode in VIEW_RELATIONS, like _specimen_root), so they fill the field area; _refresh_relations still feeds both (set_matrix + set_data); the full container + graph/heatmap use MOUSE_FILTER_IGNORE so the floating RELATIONS card still receives the Graph/Matrix toggle clicks; the heatmap/graph degrade gracefully (no crash) at any size.' },
    ux_faithful: { type: 'boolean', description: 'Matches the two asks: (1) the relations view is full-window (not a panel on a black screen, like the specimen view); (2) the VIEW switcher is always top-right with the SCOPE below it, separated from the rest of the controls.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}

phase('Verify')
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify the gene-sim VIEW/RELATIONS UI rework on branch auto/view-relations-ui-2026-06-24. Read \`git diff main...HEAD\` (or \`git diff\`) and the changed regions of godot/main.gd: _build_viewscope_ui, _build_controls (view/scope removed), _build_relations_ui (_relations_full full-window + the floating card), _set_view_mode (_relations_full visibility + _sync_view_buttons), _on_view_selected / _sync_view_buttons, the relocated INTERVENE/CONTAMINATION/SPECIMEN/RELATIONS panel positions, and the KEY_V / KEY_1/2/3 input handlers. Also read CLAUDE.md inv #2/#3.\n\n` +
    `Skeptic #${i} — default each boolean FALSE unless confirmed. Hunt for:\n` +
    `  • inv #2: ANY biology in the UI code (there should be none — it's view/camera/layout only).\n` +
    `  • inv #3: confirm ZERO Rust changed (→ hash 0x47a0_3c8f_6701_f240 cannot move). Flag any crates/** edit.\n` +
    `  • a DANGLING _view_button reference (the var + button were removed; ensure nothing still reads/writes it); _on_view_pressed removed with no orphan caller (KEY_V uses _set_view_mode); _build_viewscope_ui ordering (after _build_controls) so _view_buttons/_scope_buttons are populated exactly once.\n` +
    `  • VIEW/SCOPE sync: does _set_view_mode call _sync_view_buttons() so KEY_V cycle + --view + button press all reflect? are KEY_1/2/3 + KEY_V intact? is the panel top-right + separated from CONTROLS?\n` +
    `  • relations full-window: graph+heatmap are children of the full-rect _relations_full (visible-gated in VIEW_RELATIONS); _refresh_relations still feeds BOTH; mouse_filter IGNORE on the full container + children so the floating card's Graph/Matrix toggle still gets clicks; no panel-overlap regression (the relocated y=160 panels clear the always-on view+scope panel).\n` +
    `  • UX faithfulness to the two asks.\n\n` +
    `Report the structured verdict with file:line in issues. Do NOT edit anything.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['no_biology_in_gdscript', 'hash_neutral_no_rust', 'gdscript_correct', 'view_scope_sync_correct', 'relations_full_window_ok', 'ux_faithful']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  skeptics,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — renderer-only, hash-neutral, full-window relations + top-right view/scope' : 'NEEDS WORK',
}
