export const meta = {
  name: 'perf-optimize-impl',
  description:
    'Hash-neutral performance optimization of the post-F5 hot path (metabolism / mineralize / diffuse_and_decay / reproduce_or_die / emit+sense). Bench the current pipeline, apply optimizations that PRESERVE the exact integer sequence (allocation reduction, pass fusion, data layout, removing redundant sorts/clones) so the determinism literal does NOT move, re-bench, and refresh the stale DECISIONS.md perf baseline.',
  whenToUse:
    'After F5 + balancing. The bench baseline is pre-F3/F4/F5 (stale). Optimizes the now-much-richer tick hash-neutrally (literal 0x47a0_3c8f_6701_f240 unchanged). Autonomous; stops for human commit.',
  phases: [
    { title: 'Optimize' },
    { title: 'Gate' },
    { title: 'Verify' },
  ],
}

phase('Optimize')
const opt = await agent(
  `Hash-neutral performance optimization of the gene-sim post-F5 hot path. The criterion bench is crates/sim-core/benches/tick.rs (run via \`GATE_BENCH=1 cargo bench -p sim-core\` or \`GATE_BENCH=1 tools/gate.sh\`); the DECISIONS.md "Baseline benchmarks" section is STALE (pre-F3/F4/F5).\n\n` +
  `Steps:\n` +
  `1. Build --release, run the bench to get the CURRENT numbers (1k/5k/10k × 50 gens) on the full F3→F4→F5 pipeline.\n` +
  `2. Profile / read the hot systems in crates/sim-core/src/{lib.rs,trophic.rs,chem.rs} (metabolism, mineralize, diffuse_and_decay, reproduce_or_die, emit/sense, hash_world, snapshot): look for per-tick heap allocations (Vec::new in the loop), redundant sorts/clones, repeated resource lookups, the collect-then-sort idioms, double passes that could fuse.\n` +
  `3. Apply optimizations that PRESERVE THE EXACT INTEGER SEQUENCE — reuse scratch buffers across ticks, hoist allocations, avoid redundant clones, tighten data layout, cache what's recomputed — WITHOUT changing iteration order, accumulation order, or any value. The determinism literal 0x47a0_3c8f_6701_f240 MUST stay BYTE-IDENTICAL (this is the hard constraint: a perf win that moves the hash is OUT OF SCOPE / a separate deliberate re-pin — do NOT take it here). If a tempting optimization would change the hash, document it as a deferred follow-up instead.\n` +
  `4. Re-bench; keep only changes that measurably help (or are neutral + cleaner). Refresh the DECISIONS.md baseline table with the new post-F5 numbers + a one-line note on what changed.\n\n` +
  `Run \`cargo test -p sim-core determinism_hash_is_pinned\` after EACH change to confirm the literal is unchanged. Do NOT commit. Report: the before/after bench numbers, each optimization + why it's hash-neutral, and any deferred (would-re-pin) wins.`,
  { label: 'optimize', phase: 'Optimize', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. determinism MUST be GREEN against 0x47a0_3c8f_6701_f240 (optimization is hash-neutral — the literal must be unchanged). Report all gates PASS/FAIL. Also run \`GATE_BENCH=1 cargo bench -p sim-core\` and report the headline throughput. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_unchanged', 'faster_or_neutral', 'correctness_preserved', 'issues'],
  properties: {
    hash_unchanged: { type: 'boolean', description: 'the pinned literal 0x47a0_3c8f_6701_f240 is byte-identical; no integer iteration/accumulation order changed' },
    faster_or_neutral: { type: 'boolean', description: 'the bench is faster (or at least not slower) than before the slice; the DECISIONS baseline is refreshed' },
    correctness_preserved: { type: 'boolean', description: 'all tests + ledger/conservation asserts still pass; behaviour identical' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const verdict = await agent(
  `Adversarially verify the gene-sim perf optimization is HASH-NEUTRAL. Read \`git diff\`. Try to REFUTE: that the pinned literal is byte-identical (no reordered sort/accumulation, no changed value), that the bench is faster-or-neutral, and that all tests pass. Default each false if unconfirmable.`,
  { label: 'verify', phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
)

return { opt, gate: typeof gate === 'string' ? gate.slice(0, 500) : gate, verdict }
