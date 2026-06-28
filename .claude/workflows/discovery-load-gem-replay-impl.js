export const meta = {
  name: 'discovery-load-gem-replay-impl',
  description:
    'LOAD-GEM-REPLAY (v2, FIDELITY-FIXED) — let the player WATCH a discovered scenario live, replaying the gem EXACTLY as the search scored it. The first renderer-only attempt was RED: the GDScript resolved the mid-run edits differently from harness edits_to_actions (passed the bare target instead of loci[target % loci.len()].id → 81/147 edits failed UnknownTargetLocus; used gem.gens instead of gens_requested for early-stopped gems → wrong gen). FIX (small + hash-neutral, keeps resolution in CORE): (1) crates/discovery — add Gem.gens_requested (off-hash metadata, #[serde(default)]) set from the search horizon; (2) crates/godot-sim — a READ-ONLY #[func] gem_edit_schedule(gem_json) that runs the SAME edits_to_actions resolution (target % loci.len → real LocusId; gen_abs = edit.gen * gens_requested / 65536; species_index → SpeciesId) and returns the resolved [{gen_abs, cas, target, guide, species}] schedule; (3) the renderer reads a gem, calls gem_edit_schedule, configures the run (reset/roster/env/containment via the Load Starter path), and fires each apply_edit at its gen_abs (matching the harness interleave). HASH-NEUTRAL: gens_requested is off-hash (gitignored data/runs, serde-default → old gems load); gem_edit_schedule is read-only resolution reusing the off-hash edits_to_actions; apply_edit is the EXISTING journaled action; biology/resolution stays in core (inv #2); the pinned literal 0x47a0_3c8f_6701_f240 is UNMOVED. Then gate + adversarially verify.',
  whenToUse: 'After discovery-continue-from-gem. The player-facing "watch the discovered scenario" surface — re-scoped from renderer-only to renderer + a tiny read-only core resolver so the edit replay is byte-faithful to the search.',
  phases: [{ title: 'CoreResolve' }, { title: 'Renderer' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('CoreResolve')
const s1 = await agent(
  `Implement the CORE/BOUNDARY half of LOAD-GEM-REPLAY v2 (the fidelity fix). READ FIRST: crates/harness/src/discover.rs edits_to_actions (THE reference resolution — note exactly: gen_abs = edit.gen * gens_requested / EDIT_GEN_Q16_DEN(65536); target = loci[edit.target % loci.len()].id; species_index → the ordered-roster SpeciesId; cas = the default cas) + how it gets gens_requested (the search horizon \`gens\`, NOT gem.gens). READ crates/discovery/src/search.rs (Gem struct + the score path that builds it; the EvalRecord JSON-prefix test that pins EvalRecord — Gem is gitignored data/runs, so an additive serde-default field is safe). READ crates/godot-sim/src/lib.rs (apply_edit/observe_species/loci #[func] patterns; how a #[func] returns a VarArray of VarDictionary). CLAUDE.md inv #2 (biology/resolution stays in core) + inv #3 (gens_requested off-hash; the resolver read-only).\n\n` +
  `  - crates/discovery: add Gem.gens_requested: u32 as the LAST field with #[serde(default)] (off-hash metadata — Gem lives in gitignored data/runs; old gems without it deserialize to 0). Set it in discover.rs score_config (and discover_from_gem) from the search horizon \`gens\` (the SAME value edits_to_actions uses). When a loaded gem has gens_requested == 0 (old gem), fall back to gem.gens (documented divergence for pre-fix gems).\n` +
  `  - crates/godot-sim: a READ-ONLY #[func] gem_edit_schedule(gem_json: GString) -> VarArray that parses the gem JSON, resolves its edits EXACTLY as edits_to_actions does (reuse edits_to_actions or a shared helper — do NOT reimplement the math divergently), and returns an ordered VarArray of { gen_abs, cas, target, guide, species } (the RESOLVED real LocusId + SpeciesId, the gen_abs from gens_requested). It draws ZERO SimRng + mutates nothing (read-only resolution). Guard a bad/garbled gem → empty array + godot_error (like the other guarded #[func]s).\n` +
  `  - A test that gem_edit_schedule's resolved schedule MATCHES what edits_to_actions/build_journal produce for the same gem (same gen_abs + same resolved target LocusId + same species) for a set of sample gems incl. low-loci species (default=4 loci) and an early-stopped gem (gens_requested < gens... use gem.gens). VERIFY the pinned literal 0x47a0_3c8f_6701_f240 is UNMOVED (off-hash; cargo test -p sim-core + -p discovery). Build the cdylib. Do NOT commit. Report the gens_requested field + the gem_edit_schedule #[func] + the match-edits_to_actions test + 0x47a0 unmoved.`,
  { label: 'core-resolve', phase: 'CoreResolve', agentType: 'implementer' },
)

phase('Renderer')
const s2 = await agent(
  `Implement the RENDERER half of LOAD-GEM-REPLAY v2 on the Stage-1 core resolver:\n${typeof s1 === 'string' ? s1.slice(0, 700) : ''}\n\n` +
  `READ godot/main_menu.gd (the prior "Load Gem" FileDialog WIP — reuse it; gems load from an absolute filesystem path, NOT res://, since data/runs is gitignored) + godot/main.gd (the prior gem loader/scheduler WIP _gem_cfg_from_file/_fire_due_gem_edits — REPLACE its divergent target/gen math with the core gem_edit_schedule resolver) + the Load Starter path (_on_load_starter → set_roster/set_environment/set_containment/reset) + the intervention apply_edit usage. inv #2 (no biology/resolution in GDScript — the resolution now comes from gem_edit_schedule).\n\n` +
  `  - The gem loader: read the gem JSON, configure the run via the EXISTING Load Starter path (master_seed→reset, roster keys→set_roster resolved through res://data/species, temp_q/season→set_environment, containment→set_containment). Get the edit schedule from _live.gem_edit_schedule(gem_json) (the CORE resolver — do NOT compute target/gen in GDScript). Fire each schedule entry via _live.apply_edit(cas, target, guide, species) at its gen_abs, MATCHING the harness interleave (apply at the TOP of gen_abs's step — fix the prior off-by-one: the harness applies the point action at loop gen == gen_abs BEFORE that gen's Advance).\n` +
  `  - Renderer-only now (the resolution is in core): GDScript moves the resolved ints/strings into apply_edit; no genome/loci structure read in GDScript. has_method-guard gem_edit_schedule (older cdylib → degrade with a clear message). Fix the headless --gem smoke to report APPLIED edits (apply_edit's VarDictionary applied==true), not merely dispatched, so the gate can SEE edit fidelity.\n` +
  `  - Build cdylib + stage data; produce a real gem with edits (cargo run --release -p harness -- --discover --evolve-gens 2 --pop-size 8 --discover-gens 120 --search-seed 1 --edit-budget 2); headless-verify the --gem load applies EVERY edit (applied==true) at the right gen. Do NOT commit. Report the loader using gem_edit_schedule + the applied-edit smoke result.`,
  { label: 'renderer', phase: 'Renderer', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim (generous timeout ~15 min). LOAD-GEM-REPLAY v2 must be GREEN: fmt, clippy, test (incl. the gem_edit_schedule == edits_to_actions match test), determinism MUST stay 0x47a0_3c8f_6701_f240 (gens_requested is off-hash + the resolver is read-only — a moved hash is a FAIL), license green, godot-reader + livesim green. Report every gate PASS/FAIL with exact errors. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['edit_replay_matches_edits_to_actions', 'config_replay_correct', 'no_biology_in_gdscript', 'hash_neutral_offhash', 'issues'],
  properties: {
    edit_replay_matches_edits_to_actions: { type: 'boolean', description: 'THE FIX: every gem edit replays via the core gem_edit_schedule resolver with the SAME target (loci[edit.target % loci.len()].id), the SAME gen_abs (edit.gen * gens_requested / 65536), and the SAME species as harness edits_to_actions/build_journal — proven by a match test incl. low-loci species + an early-stopped gem; the renderer fires apply_edit at the right gen and every edit applies==true (no UnknownTargetLocus no-ops).' },
    config_replay_correct: { type: 'boolean', description: 'The run config (master_seed→reset, roster keys→set_roster via res://data/species, temp_q/season→set_environment, containment→set_containment) is reconstructed from the gem via the existing Load Starter path.' },
    no_biology_in_gdscript: { type: 'boolean', description: 'inv #2: the edit RESOLUTION (target/gen/species) is computed in CORE (gem_edit_schedule reusing edits_to_actions); GDScript only moves the resolved ints/strings into the existing apply_edit. No genome/loci structure read in GDScript.' },
    hash_neutral_offhash: { type: 'boolean', description: 'inv #3: Gem.gens_requested is off-hash metadata (#[serde(default)], gitignored data/runs, old gems load); gem_edit_schedule is read-only (zero SimRng, no mutation); apply_edit is the existing journaled action; the pinned literal 0x47a0_3c8f_6701_f240 is UNMOVED (determinism gate green).' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify LOAD-GEM-REPLAY v2 (gene-sim) — the prior attempt was RED for edit-replay divergence; verify the FIX. Read \`git diff\` (crates/discovery + crates/godot-sim + godot/*.gd) + crates/harness/src/discover.rs edits_to_actions (the reference) + CLAUDE.md inv #2/#3. Skeptic #${i} — default each boolean FALSE unless confirmed. Hunt: a target NOT resolved as loci[edit.target % loci.len()].id (the original blocker — must match edits_to_actions); a gen_abs using gem.gens instead of gens_requested (the early-stop blocker); resolution math RE-implemented in GDScript (must come from the core gem_edit_schedule #[func]); a MOVED pinned literal 0x47a0_3c8f_6701_f240 or gens_requested folded into the hash (must be off-hash); an edit firing at the wrong gen (off-by-one vs the harness top-of-gen interleave); the --gem smoke reporting dispatched-not-applied (must assert applied==true). Report the structured verdict with file:line. Do NOT edit.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['edit_replay_matches_edits_to_actions', 'config_replay_correct', 'no_biology_in_gdscript', 'hash_neutral_offhash']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  core: typeof s1 === 'string' ? s1.slice(0, 700) : s1,
  renderer: typeof s2 === 'string' ? s2.slice(0, 700) : s2,
  gate: typeof gate === 'string' ? gate.slice(0, 500) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — gem replay is byte-faithful to the search (edits resolved in core via edits_to_actions); hash-neutral' : 'NEEDS WORK',
}
