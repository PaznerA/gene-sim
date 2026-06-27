export const meta = {
  name: 'codex-browse-panel-impl',
  description:
    'Browsable CODEX panel (SP-4 §2.3 follow-up) — a scrollable in-game species/gene/role/flow browser over res://data/codex/codex.json, reusing the EXISTING godot/codex.gd loader (already staged into res:// + byte-gated by check_godot_snapshot.sh, the fix that un-blocked SP-4). Renderer-only (inv #2): GDScript moves inert codex JSON into a read-only browse UI; no genome logic. A new "Codex" entry in the top-right VIEW switcher (or a full-window codex view like the specimen/relations views) listing the baked species with their gene loci / trophic role / trait descriptions, searchable/scrollable. The pinned literal 0x47a0_3c8f_6701_f240 is untouched (zero Rust). Then gate + adversarially verify.',
  whenToUse: 'Gameplay/sandbox phase. The codex content + loader + res:// staging already landed; this adds the browse PANEL so the player can read the codex in-game.',
  phases: [{ title: 'Impl' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Impl')
const s1 = await agent(
  `Implement the browsable CODEX panel for gene-sim — renderer-only (GDScript), hash-neutral by construction (inv #2). READ FIRST: godot/codex.gd IN FULL (the existing read-only loader — how it reads res://data/codex/codex.json via FileAccess + what shape it returns: species entries, genes/loci, roles, flow/relations, descriptions). READ godot/main.gd — the top-right VIEW+SCOPE switcher (_build_viewscope_ui, _view_buttons, _sync_view_buttons, the per-view panels and how a view is selected), the SPECIMEN full-window view (_build_specimen_ui / the full-rect specimen panel) and the RELATIONS full-window view (_build_relations_ui / _relations_full) as the FULL-WINDOW precedents to model a codex view on, and the panel/scroll/list construction idioms already used (VBox/ScrollContainer/ItemList/RichTextLabel). Confirm run.sh + tools/check_godot_snapshot.sh stage data/codex into res:// (they do — codex.gd reads res://data/codex/codex.json); do NOT regress that staging. data/codex/codex.json is the committed source of truth.\n\n` +
  `  - Add a "Codex" view to the top-right VIEW switcher (alongside Ecosystem / Specimen / Relations), or a clearly-reachable full-window codex panel modelled on the specimen/relations full-window views (PRESET_FULL_RECT with the same top/bottom offsets the other full views use, so it sits under the title bar + above the controls deck).\n` +
  `  - The panel loads the codex via codex.gd (the existing loader — do NOT re-parse JSON inline; reuse codex.gd) and presents a BROWSE UX: a left list of entries (species, and/or genes/roles) + a right detail pane (the selected entry: name, trophic role, gene loci, trait descriptions, any flow/relations text), scrollable. A simple text filter/search box if cheap. Degrade gracefully (a clear "codex unavailable" state) if codex.gd returns empty (older build / missing mirror) — has_method / null guards, like the other views.\n` +
  `  - Renderer-only: NO genotype->phenotype, NO Rust. The codex is inert pre-baked content; GDScript only displays it.\n` +
  `  - Build the cdylib (cargo build --manifest-path crates/godot-sim/Cargo.toml), stage data/{species,codex,presets} into godot/data/ per run.sh, then headless-verify the codex view constructs + renders: godot --path godot -- --live --view codex --shot /tmp/codex.png (add the --view codex arg handling if the CLI view picker needs it) and confirm no parse/load error. Do NOT commit. Report the codex view wiring + the loader reuse + the --shot result.`,
  { label: 'impl', phase: 'Impl', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim (generous timeout ~15 min). The codex-browse panel is renderer-only — determinism MUST stay byte-identical at the pinned literal 0x47a0_3c8f_6701_f240 (zero Rust changed; a moved hash means something unexpected was touched -> FAIL), fmt/clippy/test green, license green, the godot-reader snapshot + the codex byte-equality mirror (check_godot_snapshot.sh stages + diffs data/codex vs godot/data/codex) green, livesim smoke green. Report every gate PASS/FAIL with exact errors. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['no_biology_in_gdscript', 'reuses_codex_loader', 'hash_neutral_zero_rust', 'ui_browses_codex', 'issues'],
  properties: {
    no_biology_in_gdscript: { type: 'boolean', description: 'inv #2: the codex panel only displays inert pre-baked JSON; no genotype->phenotype/biology computed in GDScript.' },
    reuses_codex_loader: { type: 'boolean', description: 'The panel reads the codex via the EXISTING godot/codex.gd loader (res://data/codex/codex.json), not a new inline JSON re-parse; the run.sh + check_godot_snapshot.sh codex res:// staging/byte-gate is intact (not regressed).' },
    hash_neutral_zero_rust: { type: 'boolean', description: 'inv #3: zero sim-core/Rust behaviour change; the pinned literal 0x47a0_3c8f_6701_f240 is byte-identical (determinism gate green).' },
    ui_browses_codex: { type: 'boolean', description: 'A reachable Codex view/panel (top-right switcher or full-window, modelled on specimen/relations) lists species/genes/roles with a detail pane, scrollable, with a graceful empty/unavailable state; --shot renders it without parse/load error.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify the codex-browse panel (gene-sim). Read \`git diff\` (godot/*.gd + any run.sh/gate staging touch) + CLAUDE.md inv #2/#3. Skeptic #${i} — default each boolean FALSE unless confirmed. Hunt: any Rust/sim-core change or a moved pinned literal 0x47a0_3c8f_6701_f240 (this must be pure renderer); genome/biology logic creeping into GDScript; an inline JSON re-parse instead of reusing codex.gd; a regressed codex res:// staging/byte-gate (check_godot_snapshot.sh must still stage + diff data/codex); a codex view that crashes on an older cdylib / missing mirror (must guard + degrade). Report the structured verdict with file:line. Do NOT edit.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['no_biology_in_gdscript', 'reuses_codex_loader', 'hash_neutral_zero_rust', 'ui_browses_codex']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  impl: typeof s1 === 'string' ? s1.slice(0, 700) : s1,
  gate: typeof gate === 'string' ? gate.slice(0, 500) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — codex browse panel, renderer-only, reuses codex.gd, hash-neutral' : 'NEEDS WORK',
}
