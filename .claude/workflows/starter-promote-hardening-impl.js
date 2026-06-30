export const meta = {
  name: 'starter-promote-hardening-impl',
  description:
    'starter-promote-hardening (hash-neutral, crates/harness tooling — the ADR-031 follow-up trap). promote_gen1 currently writes source_hash = hex16(gem.recorded_hash) but DROPS the gem edits without enforcing the gem is edit-free — correct TODAY only because CRISPR edits are hash-neutral; when edits become hash-active a gen-1 starter promoted from an EDITED gem would silently stop replaying to its source_hash. FIX: make the gen-1 starter self-contained re-verifiable — RECOMPUTE the gen-1 source_hash from an EDIT-FREE replay of the pristine config (so the stored hash always equals what the edit-free config actually produces, whether or not the source gem had edits), OR reject firing-edit gems; PLUS store gens (the source horizon) + an edit flag (did the source gem carry edits) in the gen-1 doc. Backward-compatible: the new fields are #[serde(default)] so the 7 already-committed gen-1 starters still parse + the gallery still loads them (their source gems are gone from disk; do NOT re-promote them). HASH-NEUTRAL: promote.rs is off-hash tooling; the recompute uses the existing run_headless/record-replay contract but never touches the pinned config — the pinned literal 0x47a0_3c8f_6701_f240 is byte-identical; harness deps unchanged. Read docs/llm/DECISIONS.md ADR-031 (the "Known trap" paragraph) + crates/harness/src/promote.rs first. Then gate + adversarially verify.',
  whenToUse: 'A Polish/QoL correctness hardening: close the ADR-031 gen-1 source_hash latent trap so the committed starter library stays self-contained re-verifiable when CRISPR edits become hash-active.',
  phases: [{ title: 'Impl' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Impl')
const s1 = await agent(
  'Implement starter-promote-hardening (hash-neutral, crates/harness tooling; the pinned literal 0x47a0_3c8f_6701_f240 stays byte-identical — promote.rs is OFF-HASH, it never touches the pinned determinism config). READ FIRST: docs/llm/DECISIONS.md ADR-031 (the "Known trap (tracked → starter-promote-hardening)" paragraph ~:1370 — the exact problem + the queued fix) + crates/harness/src/promote.rs: promote_gen1 (~:194, writes source_hash = hex16(gem.recorded_hash) ~:200 + source_seed ~:201), the Gen1Starter struct (~:78, source_hash/source_seed/config) + StarterConfig (~:36, edits: Vec::new() ~:70 — gen-1 is pristine), promote_checkpoint (~:211, which ALREADY does record_episode→replay round-trip verification — the precedent for running the sim at promote time), hex16 (~:142), and the imports (build_journal, edits_to_actions, env_config_for, record_episode, replay ~:29-30). CLAUDE.md inv #3 (the recompute must use the deterministic record/replay contract; no RNG/HashMap leak) + inv #7 (provenance/traceability — the stored hash must be MEANINGFUL).\n\n' +
  '  - THE FIX — make a gen-1 starter SELF-CONTAINED RE-VERIFIABLE. Preferred: RECOMPUTE the gen-1 source_hash from an EDIT-FREE replay of the pristine StarterConfig (build the env via env_config_for from the edit-free config + source_seed, run the deterministic headless episode over the source horizon, take its hash) and store THAT as source_hash — so the stored hash ALWAYS equals what the edit-free gen-1 config actually produces, whether or not the source gem carried edits. (Acceptable alternative per ADR-031: REJECT a firing-edit gem in promote_gen1 with a clear error. If you choose reject, still keep the recompute-or-assert so an edit-free gem\'s stored hash is verified to match its replay.) Reuse the existing record_episode/replay contract (like promote_checkpoint) — do NOT hand-roll a hash path.\n' +
  '  - STORE gens + an EDIT FLAG: add `gens` (the source gem\'s horizon — gens_requested, falling back to gens, mirroring promote_checkpoint ~:215) + a `source_had_edits: bool` (did the source gem carry a non-empty edit schedule) to Gen1Starter, so the gen-1 doc is self-contained re-verifiable (a reader can re-run the config for `gens` and assert the hash). Write them in promote_gen1.\n' +
  '  - BACKWARD COMPATIBLE: the new fields MUST be #[serde(default)] (with sensible defaults) so the 7 ALREADY-COMMITTED gen-1 starters in data/presets/starters/*.json (which lack gens/source_had_edits) still deserialize + the gallery still loads them. Do NOT re-promote or rewrite the committed starters (their source gems are gone from disk; they are edit-free so their existing source_hash is already correct). The gallery gate must stay green.\n' +
  '  - TESTS: (a) promote_gen1 on an EDIT-FREE gem → source_hash == the edit-free replay hash (== gem.recorded_hash for an edit-free gem; unchanged behaviour) + source_had_edits=false; (b) promote_gen1 on a gem WITH a (hash-neutral-today) firing edit → the stored source_hash equals the EDIT-FREE replay of the pristine config (NOT necessarily gem.recorded_hash) and source_had_edits=true [or, if you chose reject: it returns a clear error]; (c) the gen-1 doc round-trips through serde with the new fields; (d) a doc WITHOUT the new fields (an old committed starter shape) still deserializes via the serde defaults; (e) re-running the stored config for `gens` reproduces source_hash (self-contained re-verifiable).\n' +
  '  - HASH-NEUTRALITY: cargo test -p sim-core --features determinism (0x47a0_3c8f_6701_f240 byte-identical — promote.rs is off-hash) + cargo test -p harness. Confirm cargo tree -p harness adds NO dependency. Do NOT commit. Report: the recompute-or-reject choice + why, the new Gen1Starter fields + their serde defaults, the proof the committed starters still load, the tests, and confirm 0x47a0 unmoved + deps unchanged.',
  { label: 'impl', phase: 'Impl', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  'Run bash tools/gate.sh for gene-sim (generous timeout ~15 min). starter-promote-hardening must be GREEN: fmt, clippy, test (incl. the new promote_gen1 recompute/edit-flag + serde-default backward-compat tests), determinism MUST stay 0x47a0_3c8f_6701_f240 BYTE-IDENTICAL (promote.rs is OFF-HASH tooling — report explicitly), the godot snapshot byte gate incl. the GALLERY check (the 7 committed gen-1 starters MUST still load — the serde-default backward-compat is load-bearing), license green, godot-reader + livesim green. Report every gate PASS/FAIL with exact errors + EXPLICITLY whether 0x47a0 is unmoved + whether the committed starters still load + whether any dependency changed. No fixes, no commit.',
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral_offhash_tooling', 'gen1_hash_recomputed_or_firing_edits_rejected', 'gen1_self_contained_verifiable', 'committed_starters_unbroken', 'issues'],
  properties: {
    hash_neutral_offhash_tooling: { type: 'boolean', description: 'inv #3: the pinned literal 0x47a0_3c8f_6701_f240 is BYTE-IDENTICAL (promote.rs is off-hash harness tooling; the recompute uses the existing record/replay contract over the SOURCE config, never the pinned determinism config); cargo tree -p harness adds no dependency.' },
    gen1_hash_recomputed_or_firing_edits_rejected: { type: 'boolean', description: 'promote_gen1 no longer blindly copies gem.recorded_hash for an EDITED gem: it either RECOMPUTES the gen-1 source_hash from an edit-free replay of the pristine config (so the stored hash equals what the edit-free config actually produces, even when edits become hash-active) OR rejects firing-edit gems with a clear error. Proven by a test promoting an edited gem.' },
    gen1_self_contained_verifiable: { type: 'boolean', description: 'The gen-1 doc now stores gens (the source horizon) + an edit flag (source_had_edits), so it is self-contained re-verifiable — re-running the stored config for gens reproduces source_hash (a test asserts it).' },
    committed_starters_unbroken: { type: 'boolean', description: 'Backward-compatible: the new fields are #[serde(default)] so the 7 already-committed gen-1 starters (data/presets/starters/*.json, lacking the new fields) still deserialize + the gallery gate stays green; the committed starters are NOT re-promoted/rewritten.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    'Adversarially verify starter-promote-hardening (hash-neutral, crates/harness promote.rs). Read git diff + docs/llm/DECISIONS.md ADR-031 (the Known-trap paragraph) + CLAUDE.md inv #3/#7. Skeptic #' + i + ' — default each boolean FALSE unless PROVEN. Hunt: a MOVED pinned literal 0x47a0_3c8f_6701_f240 or any sim-path change (promote.rs must be off-hash tooling); promote_gen1 STILL blindly copying gem.recorded_hash for an edited gem (the trap not actually closed — the stored hash must come from an edit-free replay OR firing-edit gems rejected); the new Gen1Starter fields NOT #[serde(default)] → the 7 committed starters fail to deserialize (gallery gate red); the committed starters silently re-promoted/rewritten; a new harness dependency; a hand-rolled hash path instead of the record/replay contract; non-determinism in the recompute. Report the structured verdict with file:line + EXPLICITLY whether the literal is unmoved + whether the committed starters still load. Do NOT edit.',
    { label: 'verify:skeptic' + i, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['hash_neutral_offhash_tooling', 'gen1_hash_recomputed_or_firing_edits_rejected', 'gen1_self_contained_verifiable', 'committed_starters_unbroken']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  impl: typeof s1 === 'string' ? s1.slice(0, 800) : s1,
  gate: typeof gate === 'string' ? gate.slice(0, 600) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — gen-1 hash recomputed/edit-rejected + self-contained (gens+edit flag); committed starters unbroken; off-hash, 0x47a0 byte-identical' : 'NEEDS WORK',
}
