export const meta = {
  name: 'perf-alloc-elimination-impl',
  description:
    'Perf: eliminate per-tick heap allocations in the sim-core hot path (metabolism / reproduce_or_die / mineralize / predation / host_coupling / emit_chem / diffuse_and_decay) by moving the per-tick Vec::new / BTreeMap::new / BTreeSet scratch into REUSED World scratch resources (the proven std::mem::take + clear pattern already used for items/frozen_*/rows). BYTE-IDENTICAL: the determinism hash 0x47a0_3c8f_6701_f240 is the correctness oracle — if it stays pinned, the optimization is correct. Target: ~10-15% tick-throughput win at zero determinism risk. No parallelism (Bevy single-thread + RNG ordering = inv #3).',
  whenToUse:
    'After the perf hot-path map. Single-threaded micro-opt (allocation elimination + BTreeSet→sorted-Vec). Must keep the pinned literal byte-identical; before/after benched. Stops for human commit.',
  phases: [
    { title: 'Implement' },
    { title: 'Gate' },
    { title: 'Bench' },
    { title: 'Verify' },
  ],
}

phase('Implement')
const impl = await agent(
  `Optimize the gene-sim sim-core per-tick HOT PATH by eliminating per-tick heap allocations — Rust ONLY, sim-core. This is a BYTE-IDENTICAL optimization: run_headless for the pinned config MUST keep returning hash 0x47a0_3c8f_6701_f240. THE HASH IS THE CORRECTNESS ORACLE — if it stays pinned, your change is correct; if it moves, your change altered behavior (a bug) → fix or revert that change. Do NOT re-pin. Do NOT change any computed value, rounding, or iteration order.\n\n` +
  `READ first: crates/sim-core/src/lib.rs (the existing scratch-resource pattern — metabolism reuses items/frozen_light/frozen_nutrient/frozen_detritus/frozen_toxin/demand via std::mem::take + clear/copy_from_slice; reproduce_or_die reuses repro_scratch.rows/frozen_toxin/frozen_alarm), crates/sim-core/src/trophic.rs, crates/sim-core/src/chem.rs. Find the Resource structs that already hold these scratch buffers (e.g. a MetabolismScratch / ReproScratch). EXTEND those (or add sibling scratch resources following the SAME pattern) to OWN the buffers currently allocated fresh every tick:\n` +
  `  1. metabolism (lib.rs ~902-962): weights: Vec<u64>, shares: Vec<i64>, rem_scratch: Vec<(u128,usize)>, the per-channel split/split_w/split_rem Vecs, and the by_org + from_pool BTreeMaps. Move them into the metabolism scratch resource; reuse via mem::take + clear() each tick.\n` +
  `  2. trophic::mineralize (trophic.rs ~615): weights/shares/rem_scratch → reusable scratch.\n` +
  `  3. trophic::predation (trophic.rs ~880-917): weights/shares/rem_scratch + prey_debit/pred_credit BTreeMaps + the despawn_set BTreeSet → reusable scratch.\n` +
  `  4. trophic::host_coupling (trophic.rs ~1231): the BTreeMaps + despawn_set → reusable scratch.\n` +
  `  5. reproduce_or_die (lib.rs ~1325-1400): dead: Vec<Entity>, maint_energy BTreeMap, parent_debit BTreeMap, the dead_set/BTreeSet → reusable scratch.\n` +
  `  6. emit_chem (chem.rs ~467): spent BTreeMap → reusable scratch.\n` +
  `  7. diffuse_and_decay (chem.rs ~330-368): replace the per-channel scratch-zeroing loop with slice::fill(0); replace the src clear()+extend_from_slice() with copy_from_slice().\n` +
  `For the BTreeSet despawn_set / dead_set: the values are already produced in canonical order, so reuse a sorted Vec + binary_search (or keep a cleared BTreeSet in the scratch resource) — whichever is byte-identical AND avoids the per-tick allocation. For the from_pool BTreeMap: EITHER reuse a cleared BTreeMap, OR convert to a dense Vec indexed by (cell*S + species) IF AND ONLY IF you confirm it is byte-identical (the apply loop must visit entries in the SAME order — sorted (cell,species) — and granting 0 J is a no-op) AND benches faster; otherwise keep the reused BTreeMap.\n\n` +
  `CRITICAL discipline: after EACH buffer you move, rebuild (cargo build -p sim-core) and run the pinned hash check (the determinism_hash_is_pinned test or run_headless on the pinned cfg) to confirm it is STILL 0x47a0_3c8f_6701_f240. Catch a hash move immediately so you know which change broke it. Keep clippy + fmt clean. Do NOT commit. Report file:line for each change + confirm the final pinned hash is 0x47a0_3c8f_6701_f240 + which opts you kept vs reverted (and why).`,
  { label: 'impl', phase: 'Implement', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. determinism MUST be GREEN against 0x47a0_3c8f_6701_f240 (the optimization is byte-identical — a moved hash = FAIL, not a re-pin). fmt/clippy/test/proptest all green. Report all gates PASS/FAIL with any exact error. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Bench')
const bench = await agent(
  `Measure the gene-sim sim-core tick-throughput AFTER the allocation-elimination optimization. Build release (cargo build --release -p sim-core) then run \`cargo bench -p sim-core\` (the tick_loop bench: entities 1000/5000/10000 × 50 gens). Report the new times + throughput (Kelem/s) for each entity count. Compare to the BEFORE baseline: entities_5000_gens_50 = 294.65 ms, entities_10000_gens_50 = 590.83 ms (~847 Kelem/s). Compute the % speedup. Also confirm via cargo flamegraph-free reasoning that no per-tick Vec::new/BTreeMap::new remains in the hot path (grep the changed functions). Report the before/after table + the net % improvement. No commit.`,
  { label: 'bench', phase: 'Bench', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'faster', 'speedup_pct', 'no_pertick_alloc', 'determinism_safe', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'the pinned literal 0x47a0_3c8f_6701_f240 is UNCHANGED (byte-identical optimization; not a re-pin) and the determinism gate is green' },
    faster: { type: 'boolean', description: 'the tick_loop bench is measurably faster than the baseline (294.65ms@5k / 590.83ms@10k)' },
    speedup_pct: { type: 'string', description: 'the measured net % speedup (e.g. "~11% at 10k entities")' },
    no_pertick_alloc: { type: 'boolean', description: 'the moved buffers no longer allocate every tick (reused via the scratch resource); grep confirms no Vec::new/BTreeMap::new left in the hot functions' },
    determinism_safe: { type: 'boolean', description: 'no change to computed values / rounding / iteration order / RNG draws; integer-only; no new HashMap iteration' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const verdict = await agent(
  `Adversarially verify the gene-sim allocation-elimination perf optimization. Read \`git diff\`. Try to REFUTE each property; default false if unconfirmable. The KEY checks: is the pinned literal 0x47a0_3c8f_6701_f240 UNCHANGED (byte-identical — NOT re-pinned)? Is the bench measurably faster? Did any change alter iteration order / rounding / RNG (would have moved the hash)? Are the buffers genuinely reused (no per-tick Vec::new/BTreeMap::new left in metabolism/reproduce_or_die/mineralize/predation/emit_chem/diffuse)?`,
  { label: 'verify', phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
)

return { impl, gate: typeof gate === 'string' ? gate.slice(0, 500) : gate, bench: typeof bench === 'string' ? bench.slice(0, 800) : bench, verdict }
