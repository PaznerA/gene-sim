export const meta = {
  name: 'starter-map-library-impl',
  description:
    'STARTER-MAP LIBRARY — promote curated discovery gems into 5-10 named, committed starter maps (the capstone of the auto-research → playable-content loop). Two kinds: GEN-1 (a fresh config — roster keys+counts + env + containment, primordial.json-style + metadata) and GEN-N CHECKPOINT (the gem replayed to gen N via the record/replay journal so the scheduled CRISPR edits are RECORDED in the timeline as journaled ApplyEdit actions — a developed state the player can scrub BACK through). crates/harness: a promote tool (--promote-gem <path> --starter-name <slug> [--checkpoint-gen N]) that writes either the gen-1 starter JSON or the gen-N session (seed + actions.ndjson journal, via the EXISTING record_episode/save machinery) under data/presets/starters/, each carrying its source recorded_hash (reproducible). godot: a ROLLER-COASTER-TYCOON-style scenario selector — a left panel LIST of starters + a big right panel with the description + an animation preview (the scenario-gif-preview GIF if present, else a live replay) scrubbable by a THICK timeline slider — that loads a pick (gen-1 via the Load Starter path; gen-N via load_session) with the recorded interventions on the scrubbable timeline. HASH-NEUTRAL: the promote tool is meta-level (replay = pure function of configs, no SimRng touch); the committed starter data is inert; the gallery is renderer-only (inv #2). The pinned literal 0x47a0_3c8f_6701_f240 is UNMOVED. Then gate + adversarially verify.',
  whenToUse: 'After discovery-continue-from-gem (gen-N replay) + discovery-load-gem-replay (load). The capstone: turn the curated gems into a committed, playable starter-map library with scrub-back timelines.',
  phases: [{ title: 'Promote' }, { title: 'Gallery' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Promote')
const s1 = await agent(
  `Implement the STARTER-MAP PROMOTE tool for gene-sim (crates/harness; meta-level — the sim runs stay pure functions of configs, inv #3). READ FIRST: crates/harness/src/discover.rs (env_config_for: SearchConfig -> EnvConfig; the gem JSON shape: config{master_seed, roster:[[key,count]], containment_level, temp_q, season, edits:[{gen,species_index,target,guide}]}, score, caption, recorded_hash, build_id, gens; BUILD_ID; the round-trip record_episode -> replay == recorded_hash). READ crates/harness/src/replay.rs (EnvConfig, record_episode — how a (seed, EnvConfig, journal) is recorded to a dir as seed.json + actions.ndjson; the journal/session format save_session/load_session use). READ crates/harness/src/discover.rs edits_to_actions (the gem edits -> Action::ApplyEdit at gen_abs mapping) + capture.rs (the per-gen interleave). READ data/presets/primordial.json (the GEN-1 starter format to mirror) + run.sh / tools/check_godot_snapshot.sh (the data/presets res:// staging + byte-gate). CLAUDE.md inv #3 (determinism) + inv #5.\n\n` +
  `  - Add a promote tool (a CLI subcommand, e.g. --promote-gem <gem_path> --starter-name <slug> [--checkpoint-gen N]) writing into data/presets/starters/:\n` +
  `      * GEN-1 (no --checkpoint-gen): write data/presets/starters/<slug>.json = a starter doc { name, caption, dynamics, source_hash (hex of gem.recorded_hash), source_seed, config: { roster:[[key,count]], containment_level, temp_q, season } } — the fresh-config starter (primordial.json-shaped + metadata). NO edits applied (gen-1 is pristine).\n` +
  `      * GEN-N CHECKPOINT (--checkpoint-gen N): build the gem's EnvConfig (env_config_for) + the journal that interleaves the gem's scheduled edits (edits_to_actions) with Advance up to gen N, then record_episode into data/presets/starters/<slug>/ (seed.json + actions.ndjson — the SAME session format load_session reads). The journal RECORDS the edits at their generations (the scrub-back timeline). Write a sibling <slug>/starter.json metadata { name, caption, dynamics, checkpoint_gen: N, source_hash, source_seed }. The recorded session MUST replay to a stable hash (assert record -> replay equal), so the checkpoint is reproducible.\n` +
  `  - A small starters INDEX (data/presets/starters/index.json: a list of { slug, name, kind: "gen1"|"checkpoint", caption, dynamics, source_hash }) so the renderer gallery can enumerate the library without scanning.\n` +
  `  - Ensure data/presets/starters is staged into res:// + byte-gated (run.sh + tools/check_godot_snapshot.sh — the SAME discipline as data/presets/primordial.json; gen-1 JSON + the index are res://-readable; the gen-N sessions may live under user:// at load — your call, but the index + gen-1 starters must be res:// staged).\n` +
  `  - Promote the curated candidates the human selected (the workflow caller passes the gem paths/hashes + slugs/names + per-candidate gen-1-or-checkpoint choice; if none passed, promote a sensible default set from the top gems in data/runs/gems covering distinct dynamics types). Each committed starter carries its source recorded_hash so it is reproducible + traceable.\n` +
  `  - Tests: a promoted gen-1 starter's config rebuilds the SAME EnvConfig + replays to source_hash; a promoted gen-N session record -> replay is hash-stable AND its actions.ndjson contains the gem's edits at the right generations. VERIFY the pinned literal 0x47a0_3c8f_6701_f240 is UNMOVED (meta-level; cargo test -p sim-core determinism). Do NOT commit. Report the promote tool + the gen-1/gen-N formats + the index + the staging + the reproducibility tests.`,
  { label: 'promote', phase: 'Promote', agentType: 'implementer' },
)

phase('Gallery')
const s2 = await agent(
  `Implement the renderer "Starters" GALLERY for gene-sim — renderer-only (GDScript), hash-neutral (inv #2), on the Stage-1 starter library:\n${typeof s1 === 'string' ? s1.slice(0, 700) : ''}\n\n` +
  `READ godot/main_menu.gd (_on_load_starter — the Load Starter precedent that reads a preset + pre-fills roster/env/containment) + godot/main.gd (_on_menu_start -> set_*/_do_reset; load_session usage; the timeline markers + how a journaled edit shows as a marker; the gem-replay loader if discovery-load-gem-replay landed). The starter library: data/presets/starters/index.json + <slug>.json (gen-1) + <slug>/ session dirs (gen-N). Renderer-only: GDScript moves inert JSON + drives EXISTING #[func]s (set_roster/set_environment/set_containment/reset for gen-1; load_session for gen-N).\n\n` +
  `  - Add a "Starters" SCENARIO SELECTOR modelled on Roller Coaster Tycoon's scenario picker: a LEFT panel with a scrollable LIST/ItemList of starters (name + caption + dynamics + a gen-1/checkpoint badge + sustainability/predator flags) and a BIG RIGHT panel showing the selected starter's DESCRIPTION (roster, dynamics, env, the recorded interventions) + an ANIMATION PREVIEW area with a THICK timeline SLIDER under it. Read res://data/presets/starters/index.json to populate the list. The animation preview shows the scenario-gif-preview GIF for that starter IF present (data/presets/starters/<slug>.gif), else a live scrubbable replay; the thick slider scrubs the preview/timeline (reuse the existing timeline/journal scrub plumbing). A "Play"/"Load" button -> LOAD it:\n` +
  `      * gen-1: read <slug>.json config -> the EXISTING Load Starter path (resolve roster keys -> set_roster, temp_q/season -> set_environment, containment -> set_containment, reset) -> a fresh run.\n` +
  `      * gen-N checkpoint: load the <slug>/ session via the EXISTING load_session #[func] -> the run is restored to gen N WITH its journal; the timeline shows the RECORDED edit markers and is scrubbable BACK through them (reuse the existing journal/marker plumbing).\n` +
  `  - Renderer-only: no biology in GDScript; reuse Load Starter + load_session + the timeline markers; add NO new core action. Guard missing index / missing session / older cdylib (degrade with a clear message, has_method guards).\n` +
  `  - Build the cdylib, stage data/{species,codex,presets} into godot/data/ per run.sh, run the Stage-1 promote tool to produce 1-2 sample starters (one gen-1, one gen-N), then headless-verify: the gallery lists them + loads each without error (a --check path / parse-clean run; a gen-N load shows its timeline markers). Do NOT commit. Report the gallery wiring + the gen-1/gen-N load paths + the scrub-back/marker reuse + the verify result.`,
  { label: 'gallery', phase: 'Gallery', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim (generous timeout ~15 min). The starter-map-library slice must be GREEN: fmt, clippy, test (incl. the promote reproducibility tests), determinism MUST stay 0x47a0_3c8f_6701_f240 (the promote tool is meta-level + the gallery is renderer-only — a moved hash is a FAIL), license green, the new data/presets/starters byte-equality mirror green, godot-reader + livesim green. Report every gate PASS/FAIL with exact errors. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['starters_reproducible', 'gen_n_timeline_records_edits', 'no_biology_in_gdscript', 'hash_neutral', 'gallery_loads_both_kinds', 'issues'],
  properties: {
    starters_reproducible: { type: 'boolean', description: 'inv #3: each committed gen-1 starter config rebuilds the same EnvConfig + replays to its source recorded_hash; each gen-N session record->replay is hash-stable. The library is reproducible + traceable to its source gem.' },
    gen_n_timeline_records_edits: { type: 'boolean', description: 'A gen-N checkpoint session journal (actions.ndjson) CONTAINS the gem scheduled edits at the right generations (the recorded interventions), so the renderer timeline shows them + is scrubbable back — the EXISTING journal/marker plumbing, no new mechanic.' },
    no_biology_in_gdscript: { type: 'boolean', description: 'inv #2: the Starters gallery only reads inert starter JSON + drives EXISTING #[func]s (Load Starter set_*/reset for gen-1; load_session for gen-N); no genotype->phenotype in GDScript.' },
    hash_neutral: { type: 'boolean', description: 'inv #3: the promote tool is meta-level (replay = pure function of configs, no SimRng) + the committed data is inert + the gallery is renderer-only; the pinned literal 0x47a0_3c8f_6701_f240 is UNMOVED (determinism gate green).' },
    gallery_loads_both_kinds: { type: 'boolean', description: 'The gallery enumerates data/presets/starters/index.json and loads BOTH a gen-1 starter (fresh config via the Load Starter path) and a gen-N checkpoint (via load_session, restored to gen N with its timeline); data/presets/starters is res:// staged + byte-gated; degrades on a missing index / older cdylib.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify the starter-map-library slice (gene-sim). Read \`git diff\` (crates/harness + godot/*.gd + run.sh/gate staging + data/presets/starters) + CLAUDE.md inv #2/#3. Skeptic #${i} — default each boolean FALSE unless confirmed. Hunt: a MOVED pinned literal 0x47a0_3c8f_6701_f240 or a sim-path change (the promote tool must be meta-level); a committed starter that does NOT replay to its source recorded_hash (irreproducible — defeats the point); a gen-N session whose journal does NOT contain the edits at the right gens (the "recorded interventions in the timeline" claim fails); biology computed in GDScript or a NEW core action instead of reusing Load Starter/load_session/apply_edit; a data/presets/starters that is NOT res:// staged + byte-gated (breaks the exported PCK); a missing-index/older-cdylib path that crashes instead of degrading. Report the structured verdict with file:line. Do NOT edit.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['starters_reproducible', 'gen_n_timeline_records_edits', 'no_biology_in_gdscript', 'hash_neutral', 'gallery_loads_both_kinds']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  promote: typeof s1 === 'string' ? s1.slice(0, 700) : s1,
  gallery: typeof s2 === 'string' ? s2.slice(0, 700) : s2,
  gate: typeof gate === 'string' ? gate.slice(0, 500) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — committed reproducible starter library (gen-1 + gen-N checkpoints with recorded-intervention timelines); hash-neutral' : 'NEEDS WORK',
}
