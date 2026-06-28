export const meta = {
  name: 'discovery-continue-from-gem-impl',
  description:
    'CONTINUE-FROM-GEM runner — the user "continuation after -X generations if an interesting species is discovered during development" ask. crates/harness::discover: a runner that LOADS a saved gem JSON (data/runs/gems/*.json) -> its SearchConfig, and SEEDS a fresh evolutionary search FROM it (the gem config becomes the gen-0 elite/anchor the mutate/crossover pool branches off, optionally with NEW mid-run edits scheduled via edit_budget) so the search keeps developing the discovered community. crates/harness::main: a --from-gem <path> [--continue-gens N] CLI flag. Every continued gem is round-trip-verified (record_episode -> replay == recorded_hash) before it is written, exactly like discover/discover_evolved. HASH-NEUTRAL: the loader + the search are meta-level (std/serde + the splitmix meta-RNG; the sim runs are pure functions of their configs — no SimRng touch), so the pinned literal 0x47a0_3c8f_6701_f240 is UNMOVED. Then gate + adversarially verify.',
  whenToUse: 'After discovery-scenarios (or any batch that produced gems). Lets the auto-research BRANCH from a discovered gem and keep evolving/editing it — "found an interesting species, continue from there".',
  phases: [{ title: 'Impl' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Impl')
const s1 = await agent(
  `Implement the CONTINUE-FROM-GEM runner for gene-sim (crates/harness; std/serde meta-level — the sim runs stay pure functions of configs, inv #3). READ FIRST: crates/harness/src/discover.rs IN FULL — discover_evolved_in_space (the evolutionary loop: gen 0 = pop_size random propose, later gens = propose_evolved of the CURRENTLY-kept gems' configs + an explore fraction; the GemLibrary; capture_and_consider; verify_and_write_library with the record_episode -> replay == recorded_hash round-trip), env_config_for (SearchConfig -> EnvConfig via the data/species boundary), BUILD_ID, gem_file_name. READ crates/discovery/src/search.rs — Gem (config + score + breakdown + fingerprint + recorded_hash + build_id + caption + gens), SearchConfig (master_seed/roster/containment_level/temp_q/season/edits), propose_evolved/mutate/crossover (the operators that branch a parent), SearchSpace + edit_budget. READ crates/harness/src/main.rs — the --discover arg block + run_discover_evolved (the CLI-facing runner pattern to mirror). CLAUDE.md inv #3 (determinism, splitmix meta-RNG only, no rand crate) + inv #5.\n\n` +
  `  - Add a runner (e.g. discover_from_gem(gem_path, search_seed, pop_size, generations, keep, gens, species_dir, out_dir, evals_path) -> io::Result<GemLibrary>) that: (a) reads + serde-parses the gem JSON at gem_path into a Gem; (b) SEEDS the evolutionary search FROM gem.config — the gem config is injected as the gen-0 ANCHOR/elite so propose_evolved branches off it (mutate/crossover of the gem's roster + env + edits), not a cold random start. Reuse the EXISTING discover_evolved_in_space machinery as much as possible (ideally: pre-seed the GemLibrary or the parent pool with the loaded gem, then run the evolutionary generations). Keep the gem's SearchSpace consistent (default or a named one — accept an optional space; default to SearchSpace::default with the gem's edit_budget if edits present).\n` +
  `  - Each continued/branched gem MUST round-trip (record_episode -> replay == recorded_hash) before being written to out_dir — reuse verify_and_write_library UNCHANGED (the gem reproducibility contract). The loaded source gem itself must re-verify (its config replays to its recorded_hash) or be reported as a stale/incompatible gem (e.g. a build_id mismatch -> log + still allow branching, but note it).\n` +
  `  - crates/harness/src/main.rs: a --from-gem <path> flag (+ reuse --pop-size/--evolve-gens/--keep/--discover-gens/--search-seed/--edit-budget) routing to the new runner; document it in the flag-doc block.\n` +
  `  - Tests: (a) DETERMINISM — discover_from_gem with the same (gem, search_seed, pop, gens) into two temp dirs produces byte-identical saved gems; (b) ROUND-TRIP — every continued gem replays to its recorded_hash; (c) ANCHORING — the gen-0 pool genuinely derives from the loaded gem (e.g. at least one early child shares the gem's roster shape / a branched config is a mutate/crossover of the gem, not an unrelated cold propose). Use a tiny real gem (build a SearchConfig, score it, write it) as the fixture so the test is self-contained (no dependency on data/runs/gems).\n` +
  `  - VERIFY the pinned literal 0x47a0_3c8f_6701_f240 is UNMOVED (the runner is meta-level; sim runs are pure functions of configs; cargo test -p sim-core determinism). Build the workspace. Do NOT commit. Report the runner signature + how it seeds from the gem + the --from-gem flag + confirm the round-trip + determinism tests pass + 0x47a0 unmoved.`,
  { label: 'impl', phase: 'Impl', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim (generous timeout ~15 min). The continue-from-gem slice must be GREEN: fmt, clippy, test (incl. the new determinism + round-trip + anchoring tests), determinism MUST stay 0x47a0_3c8f_6701_f240 (the runner is meta-level — a moved hash is a FAIL), license green, godot-reader + livesim green. Report every gate PASS/FAIL with exact errors. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['seeds_search_from_gem', 'continued_gems_round_trip', 'deterministic_per_seed', 'hash_neutral_meta_level', 'issues'],
  properties: {
    seeds_search_from_gem: { type: 'boolean', description: 'The runner LOADS the gem JSON -> its SearchConfig and genuinely seeds the gen-0 evolutionary anchor/pool FROM it (propose_evolved branches off the gem via mutate/crossover) — not a cold random start; the gem roster/env/edits carry into the branch.' },
    continued_gems_round_trip: { type: 'boolean', description: 'inv #3: every continued/branched gem is round-trip-verified (record_episode -> replay == recorded_hash) via the UNCHANGED verify_and_write_library before being written; a gem that fails is dropped.' },
    deterministic_per_seed: { type: 'boolean', description: 'inv #3: discover_from_gem with the same (gem, search_seed, pop, generations, keep, gens) produces byte-identical saved gems across runs; the proposal/operator RNG is the splitmix meta-RNG (no rand crate, no HashMap iteration), never SimRng.' },
    hash_neutral_meta_level: { type: 'boolean', description: 'inv #3: the pinned literal 0x47a0_3c8f_6701_f240 is UNMOVED — the loader + search are meta-level; the sim runs stay pure functions of their configs; no sim-path change.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify the continue-from-gem runner (gene-sim). Read \`git diff\` (crates/harness/src/discover.rs + main.rs) + CLAUDE.md inv #3/#5. Skeptic #${i} — default each boolean FALSE unless confirmed. Hunt: a MOVED pinned literal 0x47a0_3c8f_6701_f240 or any sim-path change (the runner must be meta-level); a continued gem that is NOT round-trip-verified before write (would let an irreproducible gem onto disk — verify_and_write_library must gate it); a non-deterministic branch (rand crate / HashMap iteration / wall-clock in the loader or pool seeding); a runner that does NOT actually seed from the gem (a cold random start dressed up as continuation); a serde-parse that panics on a malformed/older-build_id gem instead of handling it. Report the structured verdict with file:line. Do NOT edit.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['seeds_search_from_gem', 'continued_gems_round_trip', 'deterministic_per_seed', 'hash_neutral_meta_level']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  impl: typeof s1 === 'string' ? s1.slice(0, 700) : s1,
  gate: typeof gate === 'string' ? gate.slice(0, 500) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — branches the search from a discovered gem; round-trips; deterministic; hash-neutral' : 'NEEDS WORK',
}
