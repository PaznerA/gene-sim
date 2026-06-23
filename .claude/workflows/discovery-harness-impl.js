export const meta = {
  name: 'discovery-harness-impl',
  description:
    'Implement the emergent-discovery D0 scorer + D1 trace per the PINNED spec docs/llm/proposals/discovery-scorer-spec.md. Stage 1: build crates/discovery (std+serde only — fixed/trace/ecology/lib: the 6 integer metrics M1..M6, the gated combine, the InterestingnessScorer trait + DefaultScorer + ScoreParams + novelty) with the 7-archetype synthetic-fixture test oracle (A/B/C HIGH, D/E/F/G LOW, F STRICTLY below A). Stage 2: add the harness D1 capture (capture a PerGenTrace from a live GeneSimEnv via observe_all()+flow_matrix()+the journal, off-hash) + a real-run hash-neutrality test (run hash with capture == 0x47a0_3c8f_6701_f240). Then gate (determinism MUST stay 0x47a0; the discovery tests pass) + adversarially verify (inv #1 std+serde no-GPL, inv #3 hash-neutral + integer/RNG-free, formula fidelity to the spec, the oracle ordering really asserts A>F).',
  whenToUse: 'After the emergent-scorer-design spec is pinned, to build D0+D1 (Roadmap #6 first slice).',
  phases: [{ title: 'Scorer' }, { title: 'Capture' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Scorer')
const s1 = await agent(
  `Implement STAGE 1 of the gene-sim emergent-discovery harness: the new crate crates/discovery (D0 scorer + the D1 trace TYPES). READ THE PINNED SPEC FIRST: docs/llm/proposals/discovery-scorer-spec.md — it has the EXACT integer formulas for M1..M6, the gated combine, novelty, the trait shape, the ScoreParams, and the 7-archetype test oracle. Also read CLAUDE.md (inv #1/#2/#3/#5), crates/relations-index/ (the std-only boundary-crate precedent — its Cargo.toml + lib structure), and crates/sim-core/src/signature.rs (the flow_to_grid octave-log curve your fixed::octave_log_bp must be parity-tested against).\n\n` +
  `BUILD crates/discovery:\n` +
  `  - Cargo.toml: package name "discovery", edition/rust-version/license .workspace = true, [dependencies] serde = { workspace = true } ONLY (std + serde — NO sim-core, NO harness dep; the scorer takes a plain PerGenTrace so the boundary stays clean, inv #1/#5). A [features] proptest = [] like relations-index. Add "crates/discovery" to the root Cargo.toml [workspace] members list (keep it sorted/grouped sensibly).\n` +
  `  - src/fixed.rs: integer helpers — isqrt(u64)->u64, octave_log_bp(u64)->u64 (the signature.rs flow_to_grid octave curve rescaled to SCALE=10_000; ADD A UNIT TEST asserting parity of the curve shape vs sim-core's flow_to_grid behavior — replicate the curve, don't depend on sim-core), ratio_bp(num,den)->u64 (saturating num*SCALE/den), q16(f64)->u16 (the ONE fenced float touch: clamp(floor(x*1000+0.5),0,1000)). NO f64 anywhere else.\n` +
  `  - src/trace.rs: PerGenTrace + GenRow + SpeciesMeta + InocRec EXACTLY per the spec schema, all integer (allele_q is q16 permille), #[derive(Clone, Serialize, Deserialize, PartialEq)]. Sparse flow = Vec<(u16,u16,i64)> (dest,src,amount>0).\n` +
  `  - src/ecology.rs: DefaultScorer implementing the 6 metrics M1..M6 EXACTLY per the spec (basis points, SCALE=10_000, the stable window W with BURN_IN_BP, persistence, integer Simpson, amp+turns dynamism, FlowMatrix-aggregate trophic, saturating events, the multiplicative M6 survival gate), the gated combine → Q ∈ [0,1_000_000], the 12-dim fingerprint, and a novelty_l1 + final_score per the spec. u128-promote where pop² / flow sums could overflow. NO HashMap iteration in any ordered path; fixed field order.\n` +
  `  - src/lib.rs: pub mod fixed/trace/ecology; the InterestingnessScorer trait (score + id), ScoreVec{quality,breakdown,fingerprint} (#[derive(PartialEq,Eq)]), DefaultScorer (id "ecology-d0", Default = the pinned ScoreParams), final_score free fn, ScoredRun, ScoreParams (every pinned threshold/weight as a field so re-tuning needs no code edit). Everything #[must_use] where it returns a score.\n` +
  `  - TESTS (crates/discovery/src/ or tests/): the 7-archetype ORACLE as SYNTHETIC fixtures (hand-build PerGenTrace values — anti-phase pop series for A, flat monoculture for E, dead-by-gen-5 for D, immigrant-establishes for B, converged-coexistence for F, single-boom for G, cascade-with-rebound for C). ASSERT: A/B/C score HIGH (above the spec gates), D/E/F/G LOW, and CRITICALLY final A.quality STRICTLY > F.quality (a live limit cycle beats frozen coexistence — the open-system memory). Plus: determinism (same trace bytes → byte-identical ScoreVec; ScoreVec is Eq), the octave_log_bp parity test, and novelty_l1 monotonicity (empty gem set → SCALE; a near-duplicate → small nn).\n\n` +
  `Run \`cargo test -p discovery\` and \`cargo clippy -p discovery\` until GREEN. Do NOT commit. Report the file list + the 7-archetype scores (A..G quality values) + confirm A.quality > F.quality + all tests green. Keep it std+serde — a sim-core/harness dep here is a STOP-THE-LINE inv #1 break.`,
  { label: 'scorer', phase: 'Scorer', agentType: 'implementer' },
)

phase('Capture')
const s2 = await agent(
  `Implement STAGE 2 of the gene-sim emergent-discovery harness: the D1 TRACE CAPTURE in crates/harness, building on the crates/discovery crate Stage 1 just created:\n${typeof s1 === 'string' ? s1.slice(0, 800) : ''}\n\n` +
  `READ docs/llm/proposals/discovery-scorer-spec.md (the "D1 trace schema" + "WHERE THE HARNESS EMITS IT" section) and crates/harness/src/lib.rs (GeneSimEnv::reset/step/observe_all/flow_matrix ~lines 536-562; SpeciesObservation fields: species_id/key/role/population_size/allele_freq) and crates/harness/src/replay.rs (record_episode + the journal/actions.ndjson contract) and crates/harness/tests/per_gen_stats.rs (the PROOF that stepping-with-reads is hash-neutral — your capture mirrors it).\n\n` +
  `ADD to crates/harness (it may depend on discovery — add discovery = { path = "../discovery" } to crates/harness/Cargo.toml):\n` +
  `  - A capture fn, e.g. \`pub fn capture_trace(env: &mut GeneSimEnv, seed: u64, gens: u32, actions: &[(u32, Action)]) -> discovery::trace::PerGenTrace\`: reset(seed), then for gen in 1..=gens step the journaled action due at this gen (else Advance/the default step), and AFTER each step push a GenRow from observe_all() (pop = population_size, allele_q = q16(allele_freq)) + flow_matrix() (sparse off-diagonal amount>0 edges). species[] (id/key/role) from observe_all() once. inoculations from the actions list (RegionInoculate → species ordinal). Early-stop when Σpop==0 (set g, break). Fill gens_requested + recorded_hash.\n` +
  `  - A TEST (crates/harness/tests/): run a real multi-species headless config BOTH with and without capture_trace and assert the determinism hash is IDENTICAL (capture is off-hash — observe_all/flow_matrix draw zero SimRng) AND equals the run's expected hash; for the PINNED single-species config the literal 0x47a0_3c8f_6701_f240 MUST be unchanged. Then score the captured trace with discovery::DefaultScorer and assert a sane non-degenerate Q for a living multi-species run (e.g. the bdellovibrio predator/prey roster) and a near-0 Q for a dead/monoculture run — grounding the synthetic oracle on a REAL trace.\n\n` +
  `Run \`cargo test -p harness\` + the determinism check until GREEN. Do NOT commit. Report the capture fn signature + the real-run scores + CONFIRM the pinned hash 0x47a0_3c8f_6701_f240 is unmoved (capture is hash-neutral).`,
  { label: 'capture', phase: 'Capture', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. The new crates/discovery + harness capture must be GREEN: fmt, clippy, test (incl. the discovery 7-archetype oracle + the harness capture hash-neutrality test), determinism MUST be GREEN against the pinned literal 0x47a0_3c8f_6701_f240 (capture is off-hash — a moved hash is a FAIL), license MUST stay GREEN (discovery is std+serde, no GPL). Report every gate PASS/FAIL with exact errors. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['std_serde_no_gpl', 'hash_neutral_capture', 'integer_rng_free', 'formula_fidelity', 'oracle_orders_a_over_f', 'issues'],
  properties: {
    std_serde_no_gpl: { type: 'boolean', description: 'inv #1: crates/discovery depends on std + serde ONLY (no sim-core/harness dep, no GPL); the license gate stays green. The harness (not discovery) owns the capture seam.' },
    hash_neutral_capture: { type: 'boolean', description: 'inv #3: D1 capture only READS observe_all()/flow_matrix() (proven zero-SimRng, off hash_world); a test asserts the run hash with capture == without == the pinned 0x47a0_3c8f_6701_f240 for the pinned config. No sim mutation.' },
    integer_rng_free: { type: 'boolean', description: 'inv #3: the score path is integer/quantized (u64/u128), no f64 except the single fenced q16 capture quantization, no RNG, no HashMap iteration in ordered paths; same trace → byte-identical ScoreVec (Eq-tested).' },
    formula_fidelity: { type: 'boolean', description: 'M1..M6 + the gated combine + novelty match the pinned spec (docs/llm/proposals/discovery-scorer-spec.md): the stable window/burn-in, 80% persistence, integer Simpson, amp+turns dynamism (single-boom capped), FlowMatrix-aggregate edges/roles/flow, saturating events, the multiplicative M6 gate that does NOT penalize end-state extinction.' },
    oracle_orders_a_over_f: { type: 'boolean', description: 'The 7-archetype test ACTUALLY asserts A/B/C HIGH, D/E/F/G LOW, and CRITICALLY A.quality STRICTLY > F.quality (live limit cycle beats frozen coexistence — the open-system memory). The assertions are real (the test fails if violated), not vacuous.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify the gene-sim emergent-discovery D0+D1 implementation on branch auto/discovery-d0-d1-2026-06-23. Read \`git diff main...HEAD\` (or \`git diff\`), the new crates/discovery/ in full, the harness capture + its test, and docs/llm/proposals/discovery-scorer-spec.md (the contract). Skeptic #${i} — default each boolean FALSE unless confirmed. Hunt: a sim-core/harness/GPL dep sneaking into crates/discovery (inv #1 STOP-THE-LINE); a moved pinned hash 0x47a0_3c8f_6701_f240 or a capture that mutates the sim / draws RNG (inv #3); an f64 in the score path beyond the single q16 capture quantization, or a HashMap iterated in an ordered path; a metric formula that DEVIATES from the pinned spec (esp. M3 single-boom-capping, M5 saturation/noise-gates, the M6 multiplicative gate NOT penalizing end-state extinction); and — CRITICAL — a 7-archetype oracle whose assertions are vacuous or that does NOT actually assert A.quality > F.quality (the live-cycle-beats-frozen-coexistence ordering). Confirm the harness capture test truly runs a real headless config both ways and asserts hash identity. Report the structured verdict with file:line in issues. Do NOT edit.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['std_serde_no_gpl', 'hash_neutral_capture', 'integer_rng_free', 'formula_fidelity', 'oracle_orders_a_over_f']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  scorer: typeof s1 === 'string' ? s1.slice(0, 700) : s1,
  capture: typeof s2 === 'string' ? s2.slice(0, 700) : s2,
  gate: typeof gate === 'string' ? gate.slice(0, 500) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — D0+D1 integer/hash-neutral/spec-faithful; oracle orders A>F' : 'NEEDS WORK',
}
