export const meta = {
  name: 'parallel-s2-s3-impl',
  description:
    'Parallelization S2 + S3 (per parallel-sim-draft.md §9, signed off) — THE BIG WIN. S2: refactor metabolism (Pass-1 demand / Pass-2 apportion / Pass-3 convert) into a PURE per-cell-chunk COMPUTE fn (writing disjoint grant slices + per-task litter/toxin/flow records) + a SEQUENTIAL canonical APPLY phase, STILL SERIAL — prove the pinned hash 0x47a0_3c8f_6701_f240 UNMOVED with zero threads (so any hash move is a refactor bug, not a race). S3: parallelize the compute phase with rayon over disjoint whole-cell-group chunks (per-task scratch via map_init, per-task local FlowMatrix merged canonically, PAR_THRESHOLD serial fallback), apply stays sequential+canonical. Prove hash 0x47a0 + parallel==serial + bench the ~1.7-2.4× win (1k stays serial → no regression). The hash is the oracle — NOT a re-pin.',
  whenToUse:
    'After PAR S0+S1 merged. The metabolism parallelization — the dominant ~45% hotspot, the real FPS multiple. Highest value + highest determinism risk; serial-refactor-first then parallelize, each hash-proven + multi-ISA.',
  phases: [
    { title: 'S2-split' },
    { title: 'S3-parallel' },
    { title: 'Gate' },
    { title: 'Bench' },
    { title: 'Verify' },
  ],
}

phase('S2-split')
const s2 = await agent(
  `Implement parallelization slice S2 for gene-sim (per docs/llm/proposals/parallel-sim-draft.md §2.2/§2.3/§2.5/§9 S2 — READ them). Rust/sim-core (metabolism in lib.rs ~691-1066) only. STILL SERIAL — no rayon yet. The pinned hash 0x47a0_3c8f_6701_f240 MUST stay byte-identical (verify with zero threads — any move is a refactor bug).\n\n` +
  `Refactor metabolism into a clean COMPUTE/APPLY split (the discipline the litterfall/toxin path already half-uses):\n` +
  `  - COMPUTE (pure, will-be-parallel): Pass-1 demand (per-item, lib.rs ~786-892), Pass-2 apportion (per-CELL-GROUP, lib.rs ~894-944), Pass-3 convert (per-org, lib.rs ~968-1029) — produce, per cell-group, the grant records into DISJOINT granted[lo..hi] sub-slices + collected Vecs of (org/channel/share) grants, litterfall records, toxin records, and FlowMatrix withdrawal records. Build the Vec<(cell,lo,hi)> cell-group spans via the existing while-walk over the (cell,species,org)-sorted items.\n` +
  `  - APPLY (sequential, canonical order — VERBATIM the current mutation sequence): the per-(channel,cell) PoolStock decrement, the PoolProvenance/FlowMatrix withdrawals (lib.rs:932), the OrgId-keyed by_org Energy/Biomass mutation, and the litterfall/toxin cap-overflow routing (lib.rs:1037/1056) — IN THE EXACT CURRENT ORDER so the integer-add sequence is byte-for-byte unchanged. The cap-overflow routing is order-sensitive → it MUST stay in canonical (cell,species,org) order.\n` +
  `  Keep the per-cell-group loop SERIAL for now (a plain for-loop calling the pure compute fn, then the apply). VERIFY the pinned hash is STILL 0x47a0_3c8f_6701_f240 with zero parallelism — this is the riskiest correctness step done with NO threads. If it moves, the refactor changed an accumulation order → fix until byte-identical. Do NOT commit. Report file:line + confirm hash 0x47a0 unmoved after the serial split.`,
  { label: 's2', phase: 'S2-split', agentType: 'implementer' },
)

phase('S3-parallel')
const s3 = await agent(
  `Implement parallelization slice S3 for gene-sim (per parallel-sim-draft.md §2.3/§2.4/§2.5/§5/§9 S3 — the BIG WIN). Build on the S2 compute/apply split:\n${typeof s2 === 'string' ? s2.slice(0, 600) : ''}\n\n` +
  `Parallelize the metabolism COMPUTE phase (S2 left it serial) with rayon — Rust/sim-core only. The pinned hash 0x47a0_3c8f_6701_f240 MUST stay byte-identical.\n` +
  `  - Run the per-cell-group compute over DISJOINT whole-cell-group chunks via the persistent pool (crate::par::pool().install(...) + par_iter / par_chunks). NEVER split a cell across chunks (the pool decrement is per-(channel,cell) — the cell is the apportionment atom). Size chunks by sum-of-orgs (orgs cluster). Use map_init so each rayon task allocates its OWN per-task scratch ONCE (apportion weights/shares/rem + convert split buffers + a per-task provenance-withdraw scratch) — the World-owned MetabolismScratch single buffers CANNOT be &mut-shared. demand[]/granted[] sub-slices via split_at_mut (disjoint &mut, borrow-checker-enforced — NO RefCell/unsafe).\n` +
  `  - Per-task local FlowMatrix records collected then merged in Phase B in FIXED canonical chunk order (i64 add associative+commutative → order-independent). The APPLY phase stays SEQUENTIAL + canonical (unchanged from S2).\n` +
  `  - PAR_THRESHOLD fallback: if items.len() < crate::par::PAR_THRESHOLD (~2000) run the S2 serial path verbatim (so the pinned ~1k config takes serial → byte-identity guarantee + no small-N regression). Honor the GENESIM_NO_PARALLEL escape hatch.\n` +
  `  VERIFY: the pinned hash is STILL 0x47a0_3c8f_6701_f240 (1k takes serial). Add a metabolism_parallel_equals_serial test: run a config ABOVE the threshold (e.g. 5000 entities) through BOTH the parallel and the forced-serial path, assert IDENTICAL run hash (this proves parallel==serial regardless of thread count — the inv #3 load-bearing proof; make sure the test's own data does NOT overflow i32/i64 under test-profile overflow checks). Do NOT commit. Report file:line + confirm hash 0x47a0 + that the parallel==serial test passes.`,
  { label: 's3', phase: 'S3-parallel', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. determinism MUST be GREEN against 0x47a0_3c8f_6701_f240 (S2+S3 are byte-identical — a moved hash is a FAIL). The metabolism_parallel_equals_serial test MUST pass. fmt/clippy/test/proptest/license green. Report all gates PASS/FAIL with exact errors. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Bench')
const bench = await agent(
  `Build release + run \`cargo bench -p sim-core\` (tick_loop 1k/5k/10k × 50 gens) with the persistent rayon pool (RAYON_NUM_THREADS unset → default ~10). Report times vs baseline (1k=61.7ms, 5k=295ms, 10k=590ms) + the speedup per workload. EXPECT: 1k ~flat (serial path, MUST NOT regress), 5k ~1.7-1.9×, 10k ~2.0-2.4×. Report the before/after table + the measured speedup. If 1k regresses, FLAG it (the PAR_THRESHOLD should prevent it). No commit.`,
  { label: 'bench', phase: 'Bench', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'parallel_equals_serial', 'apply_order_canonical', 'no_unsafe_sharing', 'speedup', 'no_1k_regression', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'pinned literal 0x47a0_3c8f_6701_f240 UNCHANGED; determinism gate green (NOT a re-pin)' },
    parallel_equals_serial: { type: 'boolean', description: 'a metabolism parallel==serial test runs a >threshold config both ways + asserts identical hash (the inv #3 proof); its test data does not overflow' },
    apply_order_canonical: { type: 'boolean', description: 'the order-sensitive APPLY (pool decrement, FlowMatrix, litterfall/toxin cap-routing, Energy/Biomass) stays SEQUENTIAL in the exact current canonical order — only the compute moved off-thread' },
    no_unsafe_sharing: { type: 'boolean', description: 'per-task scratch via map_init (no shared &mut World scratch across threads); disjoint slices via split_at_mut; NO RefCell/unsafe smuggling sharing' },
    speedup: { type: 'string', description: 'the measured speedup (e.g. "5k 1.8×, 10k 2.2×")' },
    no_1k_regression: { type: 'boolean', description: 'the 1k bench did NOT regress (serial path below PAR_THRESHOLD)' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify parallelization S2+S3 (metabolism). Read \`git diff\`. Skeptic #${i}, default each boolean false if unconfirmable. Hunt: a moved pinned hash (0x47a0 → re-pin = FAIL); a parallel==serial test that does NOT actually run (data overflow / no assert reached) or does NOT cover a >threshold config; an APPLY phase that got parallelized (cap-overflow routing / pool decrement / Energy-Biomass mutate must stay canonical-sequential — accidentally parallelizing it silently moves the hash); a cell split across chunks (double-count); shared &mut World scratch across threads (data race / UB) or RefCell/unsafe smuggling; a 1k regression; a FlowMatrix merge that isn't order-independent. Confirm the measured speedup is real (not a noise claim).`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const ok = skeptics.filter((s) => s.hash_neutral && s.parallel_equals_serial && s.apply_order_canonical && s.no_unsafe_sharing && s.no_1k_regression).length
return { s2, s3, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, bench: typeof bench === 'string' ? bench.slice(0, 600) : bench, skeptics, verdict: ok >= 2 ? 'S2+S3 CONFIRMED — metabolism parallel, byte-identical' : 'NEEDS WORK' }
