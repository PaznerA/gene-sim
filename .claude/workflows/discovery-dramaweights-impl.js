export const meta = {
  name: 'discovery-dramaweights-impl',
  description:
    'D3-B.2 (hash-neutral, crates/discovery): the DRAMA-WEIGHTED steering target D. The first batch showed M1 (coexistence) SATURATES — so steer toward DRAMA (M3 dynamism + M5 events), not raw Q. Add a serialized DramaWeights struct (serde, ADR-pinned like ScoreParams, retune-without-code) + a pure-integer drama_target(breakdown:[u16;6], &DramaWeights) -> u64 computing D = (Σ wᵢMᵢ for i∈1..5)/Σw × M6/SCALE — the SAME combine shape as the Q combine (ecology.rs:70-71) but with DramaWeights [m1=8, m2=4, m3=40, m4=8, m5=32] (sum 92; M3+M5 = 72/92 = 78% of the weight, vs ~46% in Q). M6 stays the unchanged multiplicative instant-death gate. CLEAN SEPARATION (the load-bearing design point): D is the STEERING target the surrogate will predict (D3-B.3/B.4); gems are STILL CURATED by final_score (Q × novelty) — Q + final_score + ScoreParams are UNCHANGED. Encodes memory no-hardcoded-balance-open-system (steer toward living dynamics, not forced stability). HASH-NEUTRAL: off-hash pure-integer scorer math reading the breakdown — no sim-path change, the pinned literal 0x47a0_3c8f_6701_f240 is byte-identical; cargo tree -p discovery stays std+serde only (inv #5). Read docs/llm/proposals/surrogate-model-spec.md (the Target paragraph in D3-B) first. Then gate + adversarially verify.',
  whenToUse: 'After D3-A (eval log) + D3-B.1 (feature encoder), both DONE. The steering-target definition the RidgeInt regressor (D3-B.3) predicts + the steered loop (D3-B.4) selects by. Self-contained; does not yet wire into the search loop.',
  phases: [{ title: 'Impl' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Impl')
const s1 = await agent(
  'Implement D3-B.2 — the DRAMA-WEIGHTED steering target D (hash-neutral, crates/discovery; pure integer; the pinned literal 0x47a0_3c8f_6701_f240 stays byte-identical — this is OFF-HASH scorer math, no sim change). READ FIRST: docs/llm/proposals/surrogate-model-spec.md — the "Target" paragraph in D3-B (D = (Σ wᵢMᵢ for i∈1..5)/Σw × M6/SCALE; DramaWeights with 78% on M3+M5; clean steer/curate separation) + the "Test oracle" line (drama-target strictly monotone in M3 & M5, M6→0 crushes it). Then READ the REAL surface: crates/discovery/src/lib.rs — ScoreVec {quality:u64, breakdown:[u16;6]} (~:63), ScoreParams {w1..w5, wsum() ~:170} (the model for the new struct; current Q weights [14,14,22,18,18] ~:145), SCALE (the bp grid 10_000), final_score / final_score_with (~:216/:227 — Q × novelty, the CURATION path that must stay UNCHANGED). crates/discovery/src/ecology.rs — the Q combine `weighted = (w1*m1+..+w5*m5)/wsum().max(1); q_bp = weighted*m6/SCALE` (~:70-71 — mirror its SHAPE exactly). crates/discovery/src/search.rs — EvalRecord {config, quality, breakdown:[u16;6], ...} (~:932 — the stored breakdown D is computed from at train time). CLAUDE.md inv #3 (determinism — pure integer, no f64, no RNG, no HashMap iteration) + inv #5 (science behind a trait; discovery stays std+serde — no GPL/heavy-ML dep) + inv #7 (the weights are a PINNED tunable, like ScoreParams).\n\n' +
  '  - Add a #[derive(serde Serialize/Deserialize, Clone, ...)] DramaWeights struct (mirror ScoreParams) with fields w1..w5 (u64) + a wsum() method, and a Default = the PINNED drama weights w1=8, w2=4, w3=40, w4=8, w5=32 (sum 92; M3+M5 = 72/92 = 78%). Add a build_id / version anchor field if it helps a future re-pin self-invalidate (your call; keep it serde-stable).\n' +
  '  - Add pub fn drama_target(breakdown: &[u16; 6], w: &DramaWeights) -> u64 computing D = ((w.w1*m1 + w.w2*m2 + w.w3*m3 + w.w4*m4 + w.w5*m5) / w.wsum().max(1)) * m6 / SCALE, reading m1..m6 from breakdown[0..6] as u64. EXACTLY the Q combine shape (ecology.rs:70-71) but with DramaWeights — pure integer, no f64, no RNG. (Optional: a drama_target_from(score: &ScoreVec, w) convenience that forwards score.breakdown.)\n' +
  '  - DO NOT change Q / final_score / final_score_with / ScoreParams / the gem curation. D is a NEW, SEPARATE steering target. Prove the separation: the curation path is byte-identical (its tests still pass unchanged).\n' +
  '  - TESTS (the spec oracle): (a) drama_target is STRICTLY MONOTONE increasing in M3 and in M5 (raising breakdown[2] or breakdown[4], all else equal, strictly raises D until saturation); (b) M6==0 crushes D to 0 (the instant-death gate); (c) deterministic + pure integer (same input → same output, no f64 in the fn); (d) the default weights satisfy w3+w5 == 72 and wsum == 92 and (w3+w5)*100/wsum == 78 (the 78% drama share); (e) DramaWeights serde round-trips byte-stable; (f) D differs from Q on a drama-heavy vs stable-but-even breakdown (D ranks the dynamic/eventful run ABOVE the placid-but-coexisting one where Q would not) — the whole point.\n' +
  '  - HASH-NEUTRALITY: run cargo test -p sim-core --features determinism (the pinned literal 0x47a0_3c8f_6701_f240 MUST stay byte-identical — D touches only the off-hash discovery scorer, not the sim) + cargo test -p discovery. Confirm cargo tree -p discovery is still std+serde only (no new dep). Do NOT commit. Report: the DramaWeights struct + drama_target signature, the proof Q/curation is unchanged, the monotonicity/gate tests, and confirm 0x47a0 unmoved + discovery deps unchanged.',
  { label: 'impl', phase: 'Impl', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  'Run bash tools/gate.sh for gene-sim (generous timeout ~15 min). D3-B.2 drama-weights must be GREEN: fmt, clippy, test (incl. the new drama_target monotonicity + M6-gate + serde + Q-unchanged tests), determinism MUST stay 0x47a0_3c8f_6701_f240 BYTE-IDENTICAL (the drama target is OFF-HASH discovery scorer math — a moved literal is a FAIL; report it explicitly), license green (cargo tree -p discovery std+serde only — inv #5; no GPL/heavy-ML dep added), godot-reader + livesim green. Report every gate PASS/FAIL with exact errors + EXPLICITLY whether 0x47a0 is unmoved + whether discovery gained any dependency. No fixes, no commit.',
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral_offhash_integer', 'drama_target_monotone_m3_m5_gated_m6', 'curation_unchanged_steer_separate', 'drama_weights_serialized_pinned', 'issues'],
  properties: {
    hash_neutral_offhash_integer: { type: 'boolean', description: 'inv #3/#5: the pinned literal 0x47a0_3c8f_6701_f240 is BYTE-IDENTICAL (the drama target is OFF-HASH discovery scorer math — no sim-path change); drama_target is PURE INTEGER (no f64), no RNG, no HashMap iteration; cargo tree -p discovery stays std+serde only (no new/GPL/heavy-ML dep).' },
    drama_target_monotone_m3_m5_gated_m6: { type: 'boolean', description: 'D = (Σ wᵢMᵢ i∈1..5)/Σw × M6/SCALE mirrors the Q combine shape (ecology.rs:70-71) with DramaWeights; a test PROVES D is strictly monotone increasing in M3 & M5 and that M6==0 crushes D to 0 (the instant-death gate); the default weights put M3+M5 at 78% of wsum (w3=40,w5=32 of sum 92).' },
    curation_unchanged_steer_separate: { type: 'boolean', description: 'CLEAN SEPARATION: Q / final_score / final_score_with / ScoreParams / gem curation are UNCHANGED (their tests pass byte-identical) — D is a NEW SEPARATE steering target, not a replacement. A test shows D ranks a drama-heavy run above a placid-but-coexisting run where Q would not.' },
    drama_weights_serialized_pinned: { type: 'boolean', description: 'DramaWeights is a serde Serialize/Deserialize struct (retune-without-code, modelled on ScoreParams) with the PINNED default weights, round-trips byte-stable, and is documented for an ADR pin (inv #7); deterministic integer throughout.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    'Adversarially verify D3-B.2 (the drama-weighted steering target D — hash-neutral, crates/discovery). Read git diff (crates/discovery) + docs/llm/proposals/surrogate-model-spec.md (the Target paragraph) + CLAUDE.md inv #3/#5/#7. Skeptic #' + i + ' — default each boolean FALSE unless PROVEN. Hunt: a MOVED pinned literal 0x47a0_3c8f_6701_f240 or any sim-path change (D must be off-hash scorer math); f64 / float arithmetic in drama_target or its training path (cross-platform non-determinism — must be pure integer); a new dependency on crates/discovery (must stay std+serde — inv #5; no GPL/heavy-ML); D NOT monotone in M3/M5 or M6 not gating to 0; Q / final_score / ScoreParams / curation SILENTLY CHANGED (the separation broken — curation must stay byte-identical); the drama weights not actually 78% on M3+M5; HashMap iteration / RNG in the target. Report the structured verdict with file:line + EXPLICITLY whether the literal is unmoved + whether discovery deps changed. Do NOT edit.',
    { label: 'verify:skeptic' + i, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['hash_neutral_offhash_integer', 'drama_target_monotone_m3_m5_gated_m6', 'curation_unchanged_steer_separate', 'drama_weights_serialized_pinned']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  impl: typeof s1 === 'string' ? s1.slice(0, 800) : s1,
  gate: typeof gate === 'string' ? gate.slice(0, 600) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — drama-weighted target D (M3+M5=78%, M6-gated); off-hash integer, 0x47a0 byte-identical; Q/curation unchanged' : 'NEEDS WORK',
}
