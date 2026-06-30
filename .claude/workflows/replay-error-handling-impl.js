export const meta = {
  name: 'replay-error-handling-impl',
  description:
    'replay-error-handling (hash-neutral, crates/harness): a TYPED ReplayError enum + a corrupted-input PROPTEST. The replay path (read_journal/replay) already returns io::Result gracefully (it never panics — serde errors map to io::ErrorKind::InvalidData, the bad actions.ndjson line is reported, and action_count is sanity-checked), but every corruption flattens to a generic InvalidData io::Error. This slice (1) introduces a typed ReplayError enum with DISTINCT variants for the corruption kinds (missing seed.json/actions.ndjson, malformed seed.json, malformed action at line N, action_count mismatch {expected,found}, persisted-spec build failure) — read_journal/replay return Result<_, ReplayError>, with impl From<ReplayError> for io::Error so existing io::Result callers (record_episode round-trip, promote.rs, discover.rs) keep compiling via ?; and (2) adds a PROPTEST that throws arbitrary/corrupted bytes at read_journal/replay (random bytes, truncated JSON, wrong action_count, malformed action lines / guides) and asserts it ALWAYS returns Err(ReplayError) — NEVER panics or UBs. HASH-NEUTRAL: replay.rs is off-hash — a VALID journal still replays to the SAME hash (the error typing does not touch the sim hash path); the pinned literal 0x47a0_3c8f_6701_f240 is byte-identical; proptest is a PINNED, MIT/Apache, TEST-ONLY dev-dependency (inv #7; no runtime/shipped-binary dep). Read crates/harness/src/replay.rs (read_journal ~:464, replay ~:450, the io::ErrorKind::InvalidData mappings) first. Then gate + adversarially verify.',
  whenToUse: 'A beta-hardening robustness slice: typed replay errors + a proptest PROVING the replay parse path is panic-free on adversarial corrupt input (seed.json/actions.ndjson).',
  phases: [{ title: 'Impl' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Impl')
const s1 = await agent(
  'Implement replay-error-handling (hash-neutral, crates/harness; the pinned literal 0x47a0_3c8f_6701_f240 stays byte-identical — replay.rs is OFF-HASH, a valid journal still replays to the same hash). READ FIRST: crates/harness/src/replay.rs — read_journal (~:464, the parse path: serde_json::from_str(seed.json).map_err(to_io) ~:466, the per-line Action parse with "actions.ndjson line N" ~:475, the action_count vs ndjson-lines sanity check ~:485), replay (~:450, pub fn replay(dir) -> io::Result<u64>), record_episode (~:410), SeedJson::env_config (~:324, the io::ErrorKind::InvalidData on a failed spec rebuild), the to_io helper, and the SEED_FILE/ACTIONS_FILE consts (~:358). Note the EXISTING callers that consume io::Result: record_episode round-trip, the discover path (crates/harness/src/discover.rs), promote.rs (record_episode/replay). crates/harness/Cargo.toml (where a proptest dev-dependency is pinned). CLAUDE.md inv #3 (the happy path is byte-identical — a valid journal replays to the SAME hash; the error typing changes nothing about the sim) + inv #7 (pin the proptest dev-dep).\n\n' +
  '  - ReplayError ENUM: add a typed enum (thiserror if already a dep, else a hand-rolled enum with Display + std::error::Error) covering the corruption kinds the code already detects: MissingFile{which} (seed.json / actions.ndjson absent — wrap the io::Error), MalformedSeedJson(serde msg), MalformedAction{line, msg}, ActionCountMismatch{expected, found}, SpecBuildFailed(msg) (the env_config rebuild failure). Each variant carries enough context to diagnose.\n' +
  '  - WIRE IT: read_journal (+ replay, + env_config if it is on this path) return Result<_, ReplayError> with the matching variant at each failure site (replace the flattened io::ErrorKind::InvalidData constructions). Provide impl From<ReplayError> for io::Error (kind InvalidData, the Display as the message) so EXISTING io::Result callers keep compiling via ? (record_episode round-trip / promote.rs / discover.rs unchanged). Keep the public happy-path behaviour identical (a valid journal → Ok(hash), same hash).\n' +
  '  - PROPTEST (the robustness proof): add a proptest (TEST-ONLY) that, for arbitrary inputs, writes a seed.json + actions.ndjson into a temp dir and asserts read_journal/replay returns Err(ReplayError) WITHOUT PANICKING — cover: random bytes as seed.json; truncated/!valid JSON; a valid seed.json with a WRONG action_count vs the ndjson lines; arbitrary/garbage actions.ndjson lines; a malformed guide in an action. Also a happy-path proptest/case: a round-tripped record_episode journal replays Ok to the recorded hash. The key assertion: NO PANIC / NO UB on ANY corrupt input — always a typed Err. (Use proptest as a pinned, MIT/Apache, [dev-dependencies] entry — test-only, NOT a runtime/shipped dep.)\n' +
  '  - HASH-NEUTRALITY: cargo test -p sim-core --features determinism (0x47a0_3c8f_6701_f240 byte-identical — replay error typing is off-hash) + cargo test -p harness (incl. the new proptest + the existing replay round-trip tests, which must still pass byte-identically). Confirm the proptest dep is DEV-only (cargo tree -p harness -e normal does NOT list proptest). Do NOT commit. Report: the ReplayError variants, the From<ReplayError> for io::Error bridge (so callers compile), the proptest corpus + the no-panic assertion, and confirm 0x47a0 unmoved + the happy path byte-identical + proptest is dev-only.',
  { label: 'impl', phase: 'Impl', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  'Run bash tools/gate.sh for gene-sim (generous timeout ~15 min — the proptest runs many cases). replay-error-handling must be GREEN: fmt, clippy, test (incl. the new ReplayError proptest — it must find NO panic + the existing replay/record round-trip tests still pass), determinism MUST stay 0x47a0_3c8f_6701_f240 BYTE-IDENTICAL (replay error typing is OFF-HASH — report explicitly), license green (the new proptest dev-dep must be MIT/Apache + DEV-only — NOT in the shipped dep tree; check_license.sh), godot-reader + livesim green. Report every gate PASS/FAIL with exact errors + EXPLICITLY whether 0x47a0 is unmoved + whether proptest is a dev-only dependency + whether the happy-path replay tests are byte-identical. No fixes, no commit.',
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral_offhash_replay', 'typed_replay_error_enum', 'proptest_no_panic_on_corrupt_input', 'callers_compile_happy_path_byte_identical', 'issues'],
  properties: {
    hash_neutral_offhash_replay: { type: 'boolean', description: 'inv #3: the pinned literal 0x47a0_3c8f_6701_f240 is BYTE-IDENTICAL (replay error typing is off-hash; a VALID journal still replays to the same hash — the error changes never touch the sim hash path); sim-core untouched.' },
    typed_replay_error_enum: { type: 'boolean', description: 'A typed ReplayError enum with DISTINCT variants (missing file, malformed seed.json, malformed action at line N, action_count mismatch {expected,found}, spec build failure) replaces the flattened io::ErrorKind::InvalidData in read_journal/replay; impl From<ReplayError> for io::Error bridges so existing io::Result callers compile.' },
    proptest_no_panic_on_corrupt_input: { type: 'boolean', description: 'A proptest throws arbitrary/corrupted bytes at read_journal/replay (random bytes, truncated JSON, wrong action_count, malformed action lines/guides) and asserts it ALWAYS returns Err(ReplayError) — NEVER panics/UB; proptest is a PINNED, MIT/Apache, TEST-ONLY dev-dependency (not in the runtime/shipped dep tree, inv #7).' },
    callers_compile_happy_path_byte_identical: { type: 'boolean', description: 'Existing callers (record_episode round-trip, replay, promote.rs, discover.rs) still compile + their tests pass; a VALID journal round-trips to the SAME hash (no happy-path regression — Ok(hash) unchanged).' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    'Adversarially verify replay-error-handling (hash-neutral, crates/harness replay.rs). Read git diff + crates/harness/src/replay.rs + Cargo.toml + CLAUDE.md inv #3/#7. Skeptic #' + i + ' — default each boolean FALSE unless PROVEN. Hunt: a MOVED pinned literal 0x47a0_3c8f_6701_f240 or any sim-path change (replay error typing must be off-hash; a valid journal must still replay to the same hash); a remaining unwrap()/expect()/panic!/array-index/slice that can PANIC on corrupt seed.json or actions.ndjson input (the proptest must actually exercise the parse path + the no-panic assertion must be real, not vacuous); proptest added as a NORMAL (runtime/shipped) dependency instead of [dev-dependencies] (inv #7 — must be test-only + pinned + MIT/Apache); the ReplayError enum NOT actually typed (still flattening to InvalidData) or callers failing to compile (missing From bridge); a happy-path regression (a valid journal no longer replays to the recorded hash). Report the structured verdict with file:line + EXPLICITLY whether the literal is unmoved + whether proptest is dev-only. Do NOT edit.',
    { label: 'verify:skeptic' + i, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['hash_neutral_offhash_replay', 'typed_replay_error_enum', 'proptest_no_panic_on_corrupt_input', 'callers_compile_happy_path_byte_identical']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  impl: typeof s1 === 'string' ? s1.slice(0, 800) : s1,
  gate: typeof gate === 'string' ? gate.slice(0, 600) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — typed ReplayError + panic-free corrupt-input proptest; off-hash, happy path byte-identical, 0x47a0 unmoved' : 'NEEDS WORK',
}
