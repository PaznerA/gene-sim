export const meta = {
  name: 'sandbox-load-starter-impl',
  description:
    'Wire "Load Starter" into the SP-2 sandbox composer — a one-click button that reads res://data/presets/primordial.json (the trophic-realistic producer-heavy starter: plant 800 / E. coli 250 / Bacillus 150 / Bdellovibrio 50, Sealed + a pre-armed consortium) and PRE-FILLS the composer roster + environment (temp/season) + containment, so a new player has an onboarding ramp instead of a blank composer. Renderer-only (inv #2): GDScript reads inert preset JSON and fills UI fields; the actual run still goes through the existing set_roster/set_environment/set_containment boundary. Ensures data/presets is staged into res:// (run.sh + check_godot_snapshot.sh byte-gate, the SAME discipline as the species/codex mirrors) so Load Starter works in the exported build too. The pinned literal 0x47a0_3c8f_6701_f240 is untouched (zero Rust). Then gate + adversarially verify.',
  whenToUse: 'Gameplay/sandbox phase, after SP-2 composer. primordial.json exists; "Load Starter" is referenced in a comment but unwired — this is the onboarding ramp.',
  phases: [{ title: 'Impl' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Impl')
const s1 = await agent(
  `Implement "Load Starter" for the gene-sim SP-2 sandbox composer — renderer-only (GDScript), hash-neutral (inv #2). READ FIRST: data/presets/primordial.json (the exact shape: roster keys+counts, env temp/season, containment level/consortium — match the fields the composer drives). READ godot/main.gd — the SP-2 composer / roster UI (search for the roster composer rows, the species pickers, the per-species count fields, the environment temp/season controls, the containment selector, and how a run is currently launched via set_roster/set_environment/set_containment); the comment near line ~517 that already references "the Load Starter preset"; and how the composer state is held (the roster Array/Dictionary the UI edits before launch). READ run.sh + tools/check_godot_snapshot.sh — confirm whether data/presets is staged into godot/data/presets (the species + codex mirrors are; if presets is NOT staged + byte-gated, ADD it the same way — mkdir + cp + a diff -rq byte-equality assert — so Load Starter works in the exported PCK, not just dev cwd).\n\n` +
  `  - Add a "Load Starter" button to the composer (near the roster composer / the run controls). On press: read res://data/presets/primordial.json via FileAccess (the SAME inert-bytes path codex.gd / _apply_species use), parse it, and PRE-FILL the composer UI state: the roster rows (species key -> starting count), the environment temp + season controls, and the containment level/consortium. The player can then tweak + launch through the EXISTING run path. Do NOT bypass set_roster/set_environment/set_containment — Load Starter only fills the UI fields the composer already feeds to those.\n` +
  `  - Guard: a missing/garbled preset (older build, absent mirror) shows a clear non-fatal message and leaves the composer untouched (no crash) — null/has guards like the other res:// reads. Unknown roster keys in the preset are skipped with a logged note, not a hard error.\n` +
  `  - Renderer-only: NO genome logic, NO Rust. The preset is inert config the UI mirrors into its own fields.\n` +
  `  - Build the cdylib, stage data/{species,codex,presets} into godot/data/ per run.sh, then headless-verify: launch the composer, confirm Load Starter fills the roster/env/containment without error (a --shot of the composer with the starter loaded, or a parse-clean headless run). Do NOT commit. Report the button wiring + the preset->UI field mapping + whether you had to add presets to the res:// staging/gate + the verify result.`,
  { label: 'impl', phase: 'Impl', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim (generous timeout ~15 min). "Load Starter" is renderer + data only — determinism MUST stay byte-identical at the pinned literal 0x47a0_3c8f_6701_f240 (zero Rust; a moved hash -> FAIL), fmt/clippy/test green, license green, the godot-reader snapshot green, and (if you added presets staging) the new data/presets byte-equality mirror green. Report every gate PASS/FAIL with exact errors. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['no_biology_in_gdscript', 'uses_existing_run_boundary', 'hash_neutral_zero_rust', 'prefills_from_preset', 'issues'],
  properties: {
    no_biology_in_gdscript: { type: 'boolean', description: 'inv #2: Load Starter only reads inert preset JSON and fills UI fields; no genotype->phenotype/biology in GDScript.' },
    uses_existing_run_boundary: { type: 'boolean', description: 'The starter pre-fills the composer UI state which still launches through the EXISTING set_roster/set_environment/set_containment boundary — it does NOT add a new launch path or compute the run itself.' },
    hash_neutral_zero_rust: { type: 'boolean', description: 'inv #3: zero sim-core/Rust behaviour change; the pinned literal 0x47a0_3c8f_6701_f240 is byte-identical (determinism gate green).' },
    prefills_from_preset: { type: 'boolean', description: 'A Load Starter button reads res://data/presets/primordial.json (staged + byte-gated into res://) and pre-fills roster + env (temp/season) + containment; a missing/garbled preset is a guarded non-fatal no-op, unknown keys skipped with a note.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify "Load Starter" for the gene-sim sandbox composer. Read \`git diff\` (godot/*.gd + any run.sh / check_godot_snapshot.sh staging touch) + data/presets/primordial.json + CLAUDE.md inv #2/#3. Skeptic #${i} — default each boolean FALSE unless confirmed. Hunt: any Rust/sim-core change or a moved pinned literal 0x47a0_3c8f_6701_f240 (pure renderer + data); genome/biology logic in GDScript; a NEW launch path bypassing set_roster/set_environment/set_containment; a preset read that is NOT res:// staged + byte-gated (would work in dev cwd but break the exported PCK); a missing-preset/garbled-JSON/unknown-key path that crashes instead of degrading. Report the structured verdict with file:line. Do NOT edit.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['no_biology_in_gdscript', 'uses_existing_run_boundary', 'hash_neutral_zero_rust', 'prefills_from_preset']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  impl: typeof s1 === 'string' ? s1.slice(0, 700) : s1,
  gate: typeof gate === 'string' ? gate.slice(0, 500) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — Load Starter pre-fills the composer from primordial.json, res://-staged, renderer-only, hash-neutral' : 'NEEDS WORK',
}
