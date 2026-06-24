export const meta = {
  name: 'discovery-evolve-impl',
  description:
    'Implement D2b of the emergent-discovery epic — WIDEN the search space + add the EVOLUTIONARY proposer so the search escapes the single-cluster D2a behavior (it kept only 1 gem on the narrow Primordial space) and surfaces DIVERSE, dramatic runs. crates/discovery::search: widen SearchSpace::default to ~7 free-living species (default/ecoli/bacillus/bdellovibrio/pseudomonas/staph/aspergillus-niger) with a per-species PRESENT/ABSENT toggle (so configs differ in species MIX, not just counts) + broader count ranges; add deterministic std-only mutate(config) + crossover(a,b) operators (perturb counts / flip a species in-out / tweak containment-temp-season; splitmix over search_seed+step — NO rand crate). crates/harness::discover: an evolutionary discover loop (random gen 0 → keep top-K → each generation propose a new population by mutating/crossing the kept gems + some random exploration → fold into the GemLibrary), plus a CLI flag. Tests: operators deterministic + in-bounds; the evolutionary run byte-reproducible per search_seed + every gem round-trips; DIVERSITY improves (more distinct kept gems than the narrow random). The SIM hash 0x47a0_3c8f_6701_f240 is untouched (meta-level search, no sim-path change). Then gate + adversarially verify.',
  whenToUse: 'After D2a (ADR-024) lands, to widen the space + add evolutionary search (Roadmap #6 D2b).',
  phases: [{ title: 'Widen-evolve' }, { title: 'Runner' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Widen-evolve')
const s1 = await agent(
  `Implement D2b STAGE 1 — WIDEN the search space + add the EVOLUTIONARY operators in crates/discovery::search. READ FIRST: docs/llm/proposals/emergent-discovery-harness-draft.md (§D2 "random → evolutionary"), docs/llm/proposals/discovery-scorer-spec.md, crates/discovery/src/search.rs (the existing SearchConfig / SearchSpace / propose / Gem / GemLibrary / novelty_l1 from D2a, ADR-024), and CLAUDE.md (inv #1 std+serde only, inv #3 deterministic/integer). Note the available baked species (data/species/): default, ecoli, bacillus, bdellovibrio, pseudomonas, staph, aspergillus-niger are good FREE-LIVING choices (carsonella/syn3 are host-dependent symbionts — exclude; mycoplasma/cutibacterium/penicillium optional).\n\n` +
  `In crates/discovery/src/search.rs (still std+serde ONLY — NO rand crate):\n` +
  `  - WIDEN SearchSpace::default: ~7 free-living species axes (default/ecoli/bacillus/bdellovibrio/pseudomonas/staph/aspergillus-niger) with BROADER count ranges, AND a per-species PRESENT/ABSENT mechanism so proposed configs differ in species MIX (e.g. an include-probability or a min_count of 0 with a presence draw) — the key fix so the search explores DIVERSE communities, not just count tweaks of the same 4 species. Keep propose() deterministic over (search_seed, trial). A trivial all-absent roster must fall back to at least the autotroph (never an empty roster).\n` +
  `  - ADD deterministic std-only EVOLUTIONARY operators: \`mutate(parent: &SearchConfig, search_seed: u64, step: u64, space: &SearchSpace) -> SearchConfig\` (perturb each count by a bounded ± delta clamped to the axis range, occasionally flip a species present↔absent, tweak containment/temp_q/season within range; splitmix64 hash of (search_seed, step, field) drives every choice) and \`crossover(a: &SearchConfig, b: &SearchConfig, search_seed: u64, step: u64) -> SearchConfig\` (per-species pick the count/presence from parent a or b deterministically; pick env knobs from one parent). Both produce VALID in-bounds configs. Optionally a \`propose_evolved(parents: &[SearchConfig], search_seed, step, space)\` that deterministically picks mutate (1 parent) or crossover (2 parents).\n` +
  `  - TESTS: mutate + crossover are DETERMINISTIC (same (seed,step) → identical child) and always produce in-bounds, non-empty configs (the autotroph-fallback holds); a child differs from its parent under at least some steps; crossover of (a,a) == a-ish; the widened propose() yields configs with DIFFERENT species mixes (more distinct rosters than the old narrow space). Run \`cargo test -p discovery\` + clippy GREEN. Do NOT commit. Report the widened SearchSpace + the operator signatures + confirm discovery is STILL std+serde only (no rand) + tests green.`,
  { label: 'widen-evolve', phase: 'Widen-evolve', agentType: 'implementer' },
)

phase('Runner')
const s2 = await agent(
  `Implement D2b STAGE 2 — the EVOLUTIONARY discover loop + CLI in crates/harness, on the Stage-1 widened space + operators:\n${typeof s1 === 'string' ? s1.slice(0, 800) : ''}\n\n` +
  `READ crates/harness/src/discover.rs (the D2a discover() runner + gem write + the record_episode→replay round-trip guard), crates/harness/src/main.rs (the --discover CLI). discovery::search now has mutate/crossover/propose_evolved.\n\n` +
  `ADD to crates/harness:\n` +
  `  - An evolutionary loop, e.g. \`discover_evolved(search_seed, pop_size, generations, keep, gens, out_dir) -> GemLibrary\`: GENERATION 0 = propose pop_size RANDOM configs (the D2a propose), build/capture/score each, fold into a GemLibrary(keep). For each subsequent generation: propose pop_size NEW configs by mutate/crossover of the CURRENT kept gems' configs (the parents) + a fraction of fresh random exploration; build/capture/score; fold in. After all generations, write the final top-K gems to data/runs/gems/ — each ONLY after the record_episode→replay==recorded_hash round-trip (unchanged contract). Keep the D2a non-evolutionary discover() too (or make generations=0 reduce to it).\n` +
  `  - A CLI flag on --discover: \`--evolve-gens G\` (+ \`--pop-size P\`, default e.g. 16) → routes to discover_evolved; default G=0 keeps the pure-random D2a behavior. Print a ranked summary + a note of how many DISTINCT gems were kept (the diversity win).\n` +
  `  - TESTS (crates/harness/tests/): an evolutionary discover_evolved run (small: pop 8, gens 3, keep 6, gens-per-trial 60) into a TEMP dir is DETERMINISTIC (same search_seed → identical saved gems); every saved gem ROUND-TRIPS (replay==recorded_hash); on the WIDENED space the run keeps MORE distinct gems (novelty-deduped) than a same-budget pure-random D2a run (the diversity assertion — the whole point of D2b); and the evolutionary best score is >= the random best (evolution does not regress; if the landscape makes this flaky, assert >= with a tolerance or log it). CRUCIALLY assert the pinned literal 0x47a0_3c8f_6701_f240 is still produced by the normal pinned config (the search adds no sim-path change).\n\n` +
  `Run \`cargo test -p harness\` + the determinism check GREEN. Do NOT commit. Report discover_evolved's signature + the CLI + a sample evolutionary ranked summary (distinct-gem count vs random) + CONFIRM 0x47a0_3c8f_6701_f240 unmoved.`,
  { label: 'runner', phase: 'Runner', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim (use a generous timeout — the full suite + determinism re-verify can take ~10-15 min). The D2b widened space + evolutionary discover must be GREEN: fmt, clippy, test (incl. the discovery operator tests + the harness evolutionary determinism/round-trip/diversity tests), determinism MUST be GREEN against the pinned literal 0x47a0_3c8f_6701_f240 (the search adds NO sim-path change), license GREEN (discovery still std+serde — confirm NO rand/engine dep crept in). Report every gate PASS/FAIL with exact errors. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['discovery_still_std_serde', 'sim_hash_untouched', 'operators_deterministic_in_bounds', 'gems_round_trip', 'diversity_improved', 'issues'],
  properties: {
    discovery_still_std_serde: { type: 'boolean', description: 'inv #1: crates/discovery still depends on std + serde ONLY — the widened space + mutate/crossover are std-only splitmix (NO rand/rand_chacha, no sim-core/harness dep).' },
    sim_hash_untouched: { type: 'boolean', description: 'inv #3: the evolutionary search adds NO sim-path change; the pinned literal 0x47a0_3c8f_6701_f240 is still produced by the pinned config (a test asserts it).' },
    operators_deterministic_in_bounds: { type: 'boolean', description: 'mutate/crossover/propose are DETERMINISTIC (same (search_seed, step) → identical child) and ALWAYS produce VALID, in-bounds, non-empty configs (the autotroph fallback holds; counts clamped to axis ranges; presence toggles valid). No wall-clock/Date/thread-rng in the operators.' },
    gems_round_trip: { type: 'boolean', description: 'Every saved gem from discover_evolved is written ONLY after record_episode → replay == recorded_hash; a test asserts saved gems round-trip; the evolutionary run is byte-reproducible per search_seed.' },
    diversity_improved: { type: 'boolean', description: 'A test asserts the WIDENED + evolutionary search keeps MORE distinct (novelty-deduped) gems than a same-budget pure-random D2a run — the D2b goal of escaping the single-cluster behavior. The assertion is real, not vacuous.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify the gene-sim discovery D2b evolutionary search on branch auto/discovery-evolve-2026-06-24. Read \`git diff main...HEAD\` (or \`git diff\`), crates/discovery/src/search.rs (the widened space + mutate/crossover), the harness discover_evolved + CLI + tests, and docs/llm/proposals/emergent-discovery-harness-draft.md §D2. Skeptic #${i} — default each boolean FALSE unless confirmed. Hunt: a rand/engine dep sneaking into crates/discovery (inv #1 — operators MUST be std-only splitmix); a moved pinned hash 0x47a0_3c8f_6701_f240 or any sim-path change (inv #3); a non-deterministic operator (wall-clock/Date/thread-rng) or one that can produce an OUT-OF-BOUNDS / EMPTY roster (must fall back to the autotroph); a gem written WITHOUT the record_episode→replay round-trip; a VACUOUS diversity test that doesn't actually compare distinct-gem counts widened-evolutionary vs narrow-random; and an f64 in the score/selection path beyond the fenced q16. Confirm the pinned-literal determinism test is real. Report the structured verdict with file:line in issues. Do NOT edit.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['discovery_still_std_serde', 'sim_hash_untouched', 'operators_deterministic_in_bounds', 'gems_round_trip', 'diversity_improved']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  widen: typeof s1 === 'string' ? s1.slice(0, 600) : s1,
  runner: typeof s2 === 'string' ? s2.slice(0, 800) : s2,
  gate: typeof gate === 'string' ? gate.slice(0, 500) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — D2b widened+evolutionary search; diverse gems; sim-hash untouched; discovery still std+serde' : 'NEEDS WORK',
}
