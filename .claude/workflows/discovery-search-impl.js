export const meta = {
  name: 'discovery-search-impl',
  description:
    'Implement D2a of the emergent-discovery epic — the RANDOM-SEARCH loop that makes the D0 scorer + D1 trace actually PRODUCE gems (docs/llm/proposals/emergent-discovery-harness-draft.md §D2). crates/discovery gains a std+serde `search` module: a SearchConfig (master-seed + per-species start counts over the Primordial roster + containment level), a DETERMINISTIC std-only proposal sampler (a splitmix/hash RNG over a search-seed — NO rand crate, keeps discovery std+serde), a Gem record (config + score + breakdown + fingerprint + caption + recorded_hash + build_id, serde), and top-K + novelty-dedup selection (reusing novelty_l1). crates/harness gains a `discover(search_seed, trials, keep)` runner that builds each config (set_roster/set_environment/set_containment), runs capture_trace, scores via DefaultScorer, keeps the top-K novel, and SAVES each gem to data/runs/gems/<score>-<seed>.json ONLY after a record_episode → replay == recorded_hash round-trip (the reproducibility contract) — plus a CLI subcommand. The SIM hash 0x47a0_3c8f_6701_f240 is untouched (search proposes configs + runs the deterministic core; the proposal RNG is meta-level, not the sim RNG). Then gate + adversarially verify.',
  whenToUse: 'After discovery D0/D1 land (ADR-023), to build the D2a random-search gem loop.',
  phases: [{ title: 'Search-types' }, { title: 'Runner' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Search-types')
const s1 = await agent(
  `Implement D2a STAGE 1 — the std+serde SEARCH module in crates/discovery (the config/proposal/gem TYPES, no engine). READ FIRST: docs/llm/proposals/emergent-discovery-harness-draft.md (§D2), docs/llm/proposals/discovery-scorer-spec.md, the existing crates/discovery/ (D0: trace.rs, ecology.rs DefaultScorer + novelty_l1 + ScoreVec + the 12-dim fingerprint), data/presets/primordial.json (the anchor config — roster default/ecoli/bacillus/bdellovibrio + env seed/temp/season + containment), and CLAUDE.md (inv #1 std+serde only, inv #3 deterministic/integer).\n\n` +
  `ADD crates/discovery/src/search.rs (pub mod search in lib.rs):\n` +
  `  - struct SearchConfig { master_seed: u64, roster: Vec<(String /*species key/stem*/, u32 /*count*/)>, containment_level: u8, temp_q: u16 /*q16 permille*/, season: u8 } — #[derive(Clone, Serialize, Deserialize, PartialEq)]. A config is a DETERMINISTIC description of one run.\n` +
  `  - A std-only DETERMINISTIC sampler: fn propose(search_seed: u64, trial: u64, space: &SearchSpace) -> SearchConfig. Use a small splitmix64 / wyhash-style integer hash of (search_seed, trial, field_index) to draw each field within bounded ranges (NO rand/rand_chacha crate — discovery stays std+serde). SearchSpace pins the ranges: the Primordial species set + per-species count ranges (e.g. plant 200..=1200, ecoli 50..=600, bacillus 30..=400, bdellovibrio 10..=200), containment 0..=3, temp/season ranges. Same (search_seed, trial) → byte-identical SearchConfig.\n` +
  `  - struct Gem { config: SearchConfig, score: u64, quality: u64, novelty: u16, breakdown: [u16;6], fingerprint: [u16; FP_DIMS], recorded_hash: u64, build_id: String, caption: String, gens: u32 } — serde; plus fn caption(&ScoreVec, &SearchConfig) -> String (an auto one-liner from the breakdown: e.g. "limit-cycle · 3 spp · 2 takeovers", purely from the integer signals, no biology).\n` +
  `  - struct GemLibrary { gems: Vec<Gem>, keep: usize } with fn consider(&mut self, candidate: Gem) using novelty_l1 vs the kept fingerprints: reject a near-duplicate (nn < dedup_min), else insert and keep the top-K (the keep count) by final score (deterministic tie-break by recorded_hash then seed). Pure std+serde, deterministic, integer — no f64 beyond the existing q16, no rand, no HashMap-iteration in ordered paths.\n` +
  `  - TESTS: propose is deterministic (same (seed,trial) → same config; different trials → generally different configs); GemLibrary keeps top-K + rejects a duplicate fingerprint + is order-independent of insertion for the final set; caption is stable. Run \`cargo test -p discovery\` + clippy GREEN. Do NOT commit. Report the SearchConfig/Gem/GemLibrary API + confirm discovery is STILL std+serde only (cargo tree -e normal -p discovery shows no new deps) + tests green.`,
  { label: 'search-types', phase: 'Search-types', agentType: 'implementer' },
)

phase('Runner')
const s2 = await agent(
  `Implement D2a STAGE 2 — the SEARCH RUNNER + CLI in crates/harness, on the Stage-1 discovery::search module:\n${typeof s1 === 'string' ? s1.slice(0, 800) : ''}\n\n` +
  `READ crates/harness/src/lib.rs (GeneSimEnv reset/step/observe_all/flow_matrix + set_roster/set_environment/set_containment — find the exact builders), crates/harness/src/capture.rs (capture_trace), crates/harness/src/replay.rs (record_episode + replay + the hash contract), crates/harness/src/main.rs (the CLI arg pattern). discovery is already a harness dep (ADR-023).\n\n` +
  `ADD to crates/harness:\n` +
  `  - fn discover(search_seed: u64, trials: u64, keep: usize, gens: u32, out_dir: &Path) -> discovery::search::GemLibrary: for trial in 0..trials → propose a SearchConfig → build a GeneSimEnv from it (set_roster from the config roster as JSON+counts via the SAME res:// data/species path the menu/CLI uses, set_environment, set_containment) → capture_trace(env, master_seed, gens, &[]) → DefaultScorer.score → final_score vs the library → GemLibrary.consider. For each KEPT gem: rebuild the (seed, EnvConfig, journal) and record_episode → assert replay() == recorded_hash BEFORE writing data/runs/gems/<final_score>-<master_seed>.json (drop a gem that fails the round-trip, logging it). Determinism: the SIM runs are pure functions of the config (inv #3); the proposal sampler is the meta-RNG (discovery::search::propose), NOT the sim RNG — the pinned literal 0x47a0_3c8f_6701_f240 is UNTOUCHED.\n` +
  `  - A CLI subcommand in main.rs: \`--discover --trials N --keep K --search-seed S --discover-gens G\` (defaults: trials 64, keep 8, gens 200) → runs discover() into data/runs/gems/ + prints a ranked summary (score · caption · config) of the saved gems. Add data/runs/ to .gitignore (gems are generated artifacts).\n` +
  `  - TESTS (crates/harness/tests/): a small discover() run (e.g. trials 12, keep 4, gens 60) into a TEMP dir is DETERMINISTIC (same search_seed → identical saved gem files + scores); every saved gem ROUND-TRIPS (replay(gem dir) == gem.recorded_hash); the gem set is novelty-deduped (no two kept gems within dedup_min). Assert the run finds at least one non-degenerate gem (quality > 0) over the Primordial space. CRUCIALLY also assert the pinned single-species determinism literal 0x47a0_3c8f_6701_f240 is still produced by the normal pinned config (the search added no sim-path change).\n\n` +
  `Run \`cargo test -p harness\` + the determinism check GREEN. Do NOT commit. Report the discover() signature + the CLI + a sample ranked gem summary from a real small run + CONFIRM 0x47a0_3c8f_6701_f240 unmoved.`,
  { label: 'runner', phase: 'Runner', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. The new discovery::search + harness discover() runner must be GREEN: fmt, clippy, test (incl. the discovery search tests + the harness discover determinism/round-trip tests), determinism MUST be GREEN against the pinned literal 0x47a0_3c8f_6701_f240 (the search adds NO sim-path change — a moved hash is a FAIL), license GREEN (discovery still std+serde — confirm no rand/engine dep crept in; the harness may use its existing rand_chacha but discovery must NOT). If a build rewrote Cargo.lock to add nothing new, that's fine; flag any genuinely new dependency. Report every gate PASS/FAIL with exact errors. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['discovery_still_std_serde', 'sim_hash_untouched', 'gems_round_trip', 'search_deterministic', 'novelty_dedup_real', 'issues'],
  properties: {
    discovery_still_std_serde: { type: 'boolean', description: 'inv #1: crates/discovery still depends on std + serde ONLY — the proposal sampler is a std-only splitmix/hash (NO rand/rand_chacha crate, no sim-core/harness dep). The engine touch (build/run/replay) lives in the harness.' },
    sim_hash_untouched: { type: 'boolean', description: 'inv #3: the search adds NO change to the sim hash path; the pinned literal 0x47a0_3c8f_6701_f240 is still produced by the pinned config (a test asserts it). The proposal RNG is meta-level, distinct from the sim ChaCha8Rng.' },
    gems_round_trip: { type: 'boolean', description: 'Every saved gem is written ONLY after record_episode → replay == recorded_hash (the reproducibility contract); a test asserts saved gems round-trip and a failing round-trip drops the gem.' },
    search_deterministic: { type: 'boolean', description: 'propose(search_seed, trial) is deterministic and a full discover() run with a fixed search_seed produces a byte-identical gem set (a test asserts it). No wall-clock / no Date / no thread-RNG in the proposal or selection.' },
    novelty_dedup_real: { type: 'boolean', description: 'GemLibrary keeps top-K by final score and rejects near-duplicates via integer novelty_l1 (nn < dedup_min); the test really asserts the kept set is diverse + order-independent, not vacuous.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify the gene-sim discovery D2a random-search loop on branch auto/discovery-search-2026-06-24. Read \`git diff main...HEAD\` (or \`git diff\`), crates/discovery/src/search.rs in full, the harness discover() runner + CLI + tests, and docs/llm/proposals/emergent-discovery-harness-draft.md §D2. Skeptic #${i} — default each boolean FALSE unless confirmed. Hunt: a rand/rand_chacha/engine dep sneaking into crates/discovery (inv #1 — the proposal sampler MUST be std-only splitmix/hash); a moved pinned hash 0x47a0_3c8f_6701_f240 or any sim-path change (inv #3); a gem written WITHOUT the record_episode→replay==hash round-trip (broken reproducibility contract); a non-deterministic search (wall-clock/Date/thread-rng in propose or selection → a discover() run that is NOT byte-reproducible per search_seed); a vacuous novelty/top-K test; and an f64 in the score/selection path beyond the fenced q16. Confirm the determinism test for the pinned literal is real and the round-trip assertion actually runs. Report the structured verdict with file:line in issues. Do NOT edit.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['discovery_still_std_serde', 'sim_hash_untouched', 'gems_round_trip', 'search_deterministic', 'novelty_dedup_real']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  searchTypes: typeof s1 === 'string' ? s1.slice(0, 600) : s1,
  runner: typeof s2 === 'string' ? s2.slice(0, 800) : s2,
  gate: typeof gate === 'string' ? gate.slice(0, 500) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — D2a search loop produces reproducible gems, sim-hash untouched, discovery still std+serde' : 'NEEDS WORK',
}
