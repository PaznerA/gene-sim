export const meta = {
  name: 'parallel-s0-s1-impl',
  description:
    'Parallelization S0 + S1 (per parallel-sim-draft.md, signed off). S0: add rayon (pinned workspace dep, inv #7), a persistent global ThreadPool (OnceLock — never per-tick spawn), a PAR_THRESHOLD const, a --no-parallel escape hatch, and the ADR-020 entry in DECISIONS.md + commit the proposal doc — ZERO call sites → hash-neutral. S1: rewrite diffuse_and_decay SCATTER→GATHER, land it SERIAL first and prove the pinned hash 0x47a0_3c8f_6701_f240 is byte-identical (gather == scatter integer-for-integer) BEFORE any threads; then parallelize the gather over dst-cell chunks behind a small-grid guard ONLY if it benches faster at 1024 cells (else leave serial, defer to S5). The hash is the correctness oracle — NOT a re-pin.',
  whenToUse:
    'First parallelization slices after human sign-off. Establishes rayon + the byte-identical diffusion gather. The big metabolism win is the next workflow (S2+S3).',
  phases: [
    { title: 'S0-infra' },
    { title: 'S1-gather' },
    { title: 'Gate' },
    { title: 'Bench' },
    { title: 'Verify' },
  ],
}

phase('S0-infra')
const s0 = await agent(
  `Implement parallelization slice S0 for gene-sim (per docs/llm/proposals/parallel-sim-draft.md §9 S0 — READ it first). Rust/sim-core + Cargo + DECISIONS only. NO call sites yet → the pinned hash 0x47a0_3c8f_6701_f240 MUST stay byte-identical (verify it does).\n\n` +
  `1. Add \`rayon\` as a PINNED workspace dependency (Cargo.toml [workspace.dependencies], a fixed version like other pins) + wire it into crates/sim-core/Cargo.toml. rayon is MIT/Apache-2.0 (inv #1: GPL-only boundary → linking rayon is fine). Update Cargo.lock.\n` +
  `2. Add a persistent global rayon ThreadPool built ONCE (a std::sync::OnceLock<rayon::ThreadPool>, or a World resource) — NEVER spawn/teardown per tick. Provide a helper to run a closure on it (pool.install). Pin the worker count from RAYON_NUM_THREADS / an explicit default for stable benches.\n` +
  `3. Add a PAR_THRESHOLD const (~2000 items, documented as bench-tuned) + a --no-parallel escape hatch (an env var or a SimConfig/World flag) that forces the serial path — for differential debugging.\n` +
  `4. Append ADR-020 to docs/llm/DECISIONS.md: the deterministic-parallelization decision, the rayon pin (inv #7), the inv #1 (rayon non-GPL) + inv #3 (the byte-identity argument: compute-parallel/apply-canonical, the hash + multi-ISA oracle) notes. Move docs/llm/proposals/parallel-sim-draft.md to committed status (it already exists). Update CHANGELOG.\n` +
  `Since there are NO call sites, the build must compile + the pinned hash stays 0x47a0_3c8f_6701_f240 (confirm via the determinism test / run_headless). fmt + clippy clean (a built-but-unused pool must not warn — #[allow] or a trivial use). Do NOT commit. Report file:line + confirm hash 0x47a0 unmoved.`,
  { label: 's0', phase: 'S0-infra', agentType: 'implementer' },
)

phase('S1-gather')
const s1 = await agent(
  `Implement parallelization slice S1 for gene-sim (per parallel-sim-draft.md §4 + §9 S1 — READ §4 carefully, the gather proof + the reflect-term warning). Rust/sim-core (chem.rs) only. The pinned hash 0x47a0_3c8f_6701_f240 MUST stay byte-identical.\n\n` +
  `Rewrite \`diffuse_and_decay\` (chem.rs ~315/339-362) from SCATTER to GATHER, and land it SERIAL FIRST (no rayon yet):\n` +
  `  For each OUTPUT cell d, compute new[d] PURELY by reading the frozen src snapshot (the existing src_buf):\n` +
  `    new[d] = (src[d] - 4*(src[d]>>shift))                                  // kept remainder\n` +
  `           + Σ over in-grid von-Neumann neighbours nb of d ( src[nb]>>shift )  // received quanta\n` +
  `           + (count of d's OWN off-grid edges) * (src[d]>>shift)            // the reflect-to-self term\n` +
  `  CRITICAL (the easy-to-get-wrong spot, §4): the reflect term uses the count of d's OWN off-grid edges × (src[d]>>shift), NOT the neighbours' reflects. Von-Neumann adjacency is symmetric so the gather receives exactly what the scatter pushed — prove it integer-for-integer. The decay tap (chem.rs ~378-385, lost = cell>>DECAY_SHIFT) is already per-cell; keep its decayed sum identical.\n` +
  `  VERIFY the pinned hash is STILL 0x47a0_3c8f_6701_f240 with ZERO threads — this is the byte-identity proof (gather == scatter). If it moves, the gather is wrong (likely the reflect term) → fix until byte-identical.\n\n` +
  `THEN (only after serial gather proves the hash): the gather is embarrassingly parallel by output cell (each d writes only new[d] from read-only src). Parallelize it with rayon (par_chunks_mut over row bands) BEHIND a small-grid guard (1024 cells is tiny — only parallelize if cells >= a threshold; the decay sum becomes per-task partial i64 sums merged in fixed chunk order, integer-add associative → identical). Bench it; if parallel diffusion is NOT faster at 1024 cells, LEAVE IT SERIAL and note S5 defers permanent parallel diffusion. Either way the hash stays 0x47a0. Do NOT commit. Report file:line + confirm hash 0x47a0 after serial gather AND after any parallelization + whether you kept diffusion parallel or serial (with the bench reason).`,
  { label: 's1', phase: 'S1-gather', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. determinism MUST be GREEN against 0x47a0_3c8f_6701_f240 (S0+S1 are byte-identical — a moved hash is a FAIL, not a re-pin). fmt/clippy/test/proptest green; license green (rayon is MIT/Apache — confirm scripts/check_license.sh passes). Report all gates PASS/FAIL with exact errors. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Bench')
const bench = await agent(
  `Build release + run \`cargo bench -p sim-core\` (tick_loop 1k/5k/10k × 50 gens). Report times vs baseline (1k=61.7ms, 5k=295ms, 10k=590ms). S0+S1 should be ~flat (the gather is byte-identical; diffusion parallelism over 1024 cells may be neutral). Confirm no regression. Report the table. No commit.`,
  { label: 'bench', phase: 'Bench', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'gather_byte_identical', 'rayon_pinned_nonprgpl', 'serial_first_proven', 'no_regression', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'pinned literal 0x47a0_3c8f_6701_f240 UNCHANGED; determinism gate green (NOT a re-pin)' },
    gather_byte_identical: { type: 'boolean', description: 'the scatter→gather rewrite is byte-identical (proven serial, hash unmoved); the reflect term is correct (own off-grid edges, not neighbours\')' },
    rayon_pinned_nonprgpl: { type: 'boolean', description: 'rayon is added as a PINNED dep (inv #7, recorded in DECISIONS ADR-020) + is MIT/Apache (inv #1 license gate green); persistent pool, no per-tick spawn' },
    serial_first_proven: { type: 'boolean', description: 'the gather landed SERIAL and proved the hash BEFORE any parallelism (the safety discipline)' },
    no_regression: { type: 'boolean', description: 'the bench shows no regression vs baseline (1k must not regress)' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const verdict = await agent(
  `Adversarially verify parallelization S0+S1. Read \`git diff\`. Try to REFUTE each property; default false if unconfirmable. KEY checks: is the pinned literal 0x47a0 UNCHANGED (byte-identical, NOT re-pinned)? Is the gather rewrite provably byte-identical to the scatter (the reflect term + the symmetric-adjacency proof)? Was it landed serial-first? Is rayon pinned + the license gate green? No 1k regression?`,
  { label: 'verify', phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
)

return { s0, s1, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, bench: typeof bench === 'string' ? bench.slice(0, 500) : bench, verdict }
