export const meta = {
  name: 'discovery-load-gem-replay-impl',
  description:
    'LOAD-GEM-REPLAY — let the player WATCH a discovered scenario live. The renderer reads a saved gem JSON (data/runs/gems/*.json — the round-trip-verified output of the brute-force search) and configures a live run from it: reset(master_seed) + set_roster (resolve the gem roster keys through res://data/species like Load Starter) + set_environment (temp_q/1000, season) + set_containment (level), then SCHEDULES the gem CRISPR edits via the EXISTING apply_edit #[func] at their generations so the discovered (possibly edited) scenario plays out. Renderer-only (inv #2): GDScript reads the inert gem JSON + drives the EXISTING #[func]s (reset/set_roster/set_environment/set_containment/apply_edit/step) — no new core action, no biology in GDScript. Gems are gitignored generated artifacts, so the loader reads a filesystem path (file picker / a configurable dir), not res://. The pinned literal 0x47a0_3c8f_6701_f240 is untouched (zero Rust). Then gate + adversarially verify.',
  whenToUse: 'After the brute-force search produces gems (discovery-scenarios / continue-from-gem). The player-facing "watch the discovered scenario" surface — the payoff of the auto-research.',
  phases: [{ title: 'Impl' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Impl')
const s1 = await agent(
  `Implement LOAD-GEM-REPLAY for gene-sim — renderer-only (GDScript), hash-neutral by construction (inv #2). READ FIRST: a sample gem JSON in data/runs/gems/*.json (the shape: config{master_seed, roster:[[key,count],...], containment_level, temp_q (q16 permille 0..=1000), season, edits:[{gen, species_index, target, guide},...]}, score, breakdown, caption, recorded_hash, build_id, gens). READ godot/main_menu.gd — _on_load_starter (the Load Starter precedent: reads an inert JSON preset and pre-fills roster keys->counts + env + containment, resolving species stems) — model the gem loader on it. READ godot/main.gd — _on_menu_start (main.gd ~595: set_environment / _apply_roster->set_roster / _apply_menu_containment->set_containment / _do_reset) so the gem replay drives the SAME run-config boundary, and how a CRISPR edit is issued in --live (the intervention panel's apply_edit usage: _live.apply_edit(cas, target, guide, species)). READ crates/harness/src/discover.rs edits_to_actions (the REFERENCE mapping: an EditGene's gen-fraction -> gen_abs, and species_index -> the ordered-roster species id) so the renderer schedules the edits at the SAME generations + resolves species_index to the SAME live species id. The godot-sim #[func]s available: reset(seed) set_roster(jsons:PackedStringArray, counts:PackedInt32Array) set_environment(lat,lon,avg_temp,season) set_containment(...) apply_edit(cas,target,guide,species) step(n). NOTE: gems are gitignored (NOT res:// staged) — read them from a FILESYSTEM path (a file picker or a configurable dir via FileAccess on a globalized/absolute path), NOT res://.\n\n` +
  `  - Add a "Load Gem" affordance (a file picker, or a small list of data/runs/gems/*.json via an absolute/globalized path, or a path LineEdit — your call; keep it reachable from the composer/menu or a small panel). On select: read + JSON.parse the gem, then configure a live run: reset(config.master_seed); set_roster from config.roster (resolve each key -> its res://data/species/<key>.json like Load Starter, in the SAME order, with counts); set_environment(lat=0,lon=0, avg_temp=config.temp_q/1000.0, season=config.season); set_containment(config.containment_level, ...). Then RUN, and at each generation matching an edit's gen (gen_abs computed the SAME way as edits_to_actions from the gen-fraction × config.gens), call _live.apply_edit(cas_default, edit.target, edit.guide, resolved_species_id) where resolved_species_id maps edit.species_index through the gem roster order to the live registry id.\n` +
  `  - Renderer-only (inv #2): the gem JSON is inert; GDScript only moves keys/ints/strings into the EXISTING #[func]s. NO genotype->phenotype in GDScript; the apply_edit gate runs in core. Reuse the Load Starter key-resolution + the intervention-panel apply_edit call; add NO new core action.\n` +
  `  - Guard: a missing/garbled gem, an unresolved roster key, or an out-of-range edit shows a clear non-fatal message and leaves the sim untouched (null/has guards like Load Starter). has_method-guard the #[func] calls so an older cdylib degrades.\n` +
  `  - Build the cdylib, stage data/{species,codex,presets} into godot/data/ per run.sh, run the existing search to produce a real gem (cargo run --release -p harness -- --discover --evolve-gens 2 --pop-size 8 --discover-gens 120 --search-seed 1), then headless-verify: load that gem + confirm the run configures + steps without error (a --check path or a parse-clean headless run). Do NOT commit. Report the Load Gem wiring + the roster/env/containment/edit mapping + the verify result.`,
  { label: 'impl', phase: 'Impl', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim (generous timeout ~15 min). Load-gem-replay is renderer-only — determinism MUST stay byte-identical at the pinned literal 0x47a0_3c8f_6701_f240 (zero Rust changed; a moved hash means something unexpected was touched -> FAIL), fmt/clippy/test green, license green, the godot-reader snapshot + livesim smoke green. Report every gate PASS/FAIL with exact errors. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['no_biology_in_gdscript', 'reuses_existing_funcs_no_new_action', 'hash_neutral_zero_rust', 'replays_gem_config_and_edits', 'issues'],
  properties: {
    no_biology_in_gdscript: { type: 'boolean', description: 'inv #2: the gem loader only reads inert gem JSON + drives EXISTING #[func]s with keys/ints/strings; no genotype->phenotype/biology computed in GDScript.' },
    reuses_existing_funcs_no_new_action: { type: 'boolean', description: 'Replay drives the EXISTING reset/set_roster/set_environment/set_containment/apply_edit/step #[func]s (the apply_edit gate runs in core); NO new mutating core action was added. Roster keys resolve through the same res://data/species path as Load Starter.' },
    hash_neutral_zero_rust: { type: 'boolean', description: 'inv #3: zero sim-core/Rust behaviour change; the pinned literal 0x47a0_3c8f_6701_f240 is byte-identical (determinism gate green).' },
    replays_gem_config_and_edits: { type: 'boolean', description: 'A reachable "Load Gem" reads a gem JSON and configures the run from it (master_seed->reset, roster keys+counts->set_roster, temp_q/season->set_environment, containment_level->set_containment) AND schedules the gem edits via apply_edit at the gen mapped the SAME way as edits_to_actions (gen-fraction × gens; species_index -> live roster id); guarded + degrades on a missing/garbled gem or older cdylib.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify load-gem-replay (gene-sim). Read \`git diff\` (godot/*.gd) + a sample data/runs/gems/*.json + crates/harness/src/discover.rs edits_to_actions (the reference edit-timing/species mapping) + CLAUDE.md inv #2/#3. Skeptic #${i} — default each boolean FALSE unless confirmed. Hunt: any Rust/sim-core change or a moved pinned literal 0x47a0_3c8f_6701_f240 (must be pure renderer); genome/biology logic in GDScript; a NEW core action instead of reusing apply_edit/set_*; an edit applied at the WRONG generation or with a wrong species_index->id mapping (must match edits_to_actions: gen-fraction × gens, ordered roster); a roster key not resolved through res://data/species; a missing/garbled-gem or older-cdylib path that crashes instead of degrading. Report the structured verdict with file:line. Do NOT edit.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['no_biology_in_gdscript', 'reuses_existing_funcs_no_new_action', 'hash_neutral_zero_rust', 'replays_gem_config_and_edits']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  impl: typeof s1 === 'string' ? s1.slice(0, 700) : s1,
  gate: typeof gate === 'string' ? gate.slice(0, 500) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — load + replay a discovered gem live; renderer-only; hash-neutral' : 'NEEDS WORK',
}
