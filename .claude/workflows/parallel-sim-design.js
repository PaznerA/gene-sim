export const meta = {
  name: 'parallel-sim-design',
  description:
    'DESIGN ONLY (no code): a deterministic, BYTE-IDENTICAL parallelization of the gene-sim sim-core hot path. Parallelize the no-RNG, cell-independent passes (metabolism ~40-50%, diffuse_and_decay ~12-15%, mineralize) with rayon INSIDE each system (Bevy schedule stays single-threaded → system order deterministic); reproduce_or_die (per-birth RNG) + emit_chem + the asserts stay sequential. Compute-in-parallel-per-cell-chunk, collect grants per thread, apply in canonical order → the pinned hash 0x47a0_3c8f_6701_f240 must stay UNCHANGED (no re-pin; multi-ISA gate is the safety net). Produces an ADR draft + an incremental slice plan + a realistic Amdahl speedup estimate for human sign-off.',
  whenToUse:
    'The user chose to pursue parallelization (the big FPS lever). Design + ADR before any code; an inv #3 architecture change → human signs off the ADR before implementation.',
  phases: [
    { title: 'Design' },
    { title: 'ADR' },
  ],
}

phase('Design')
const DSCHEMA = {
  type: 'object',
  required: ['which_passes', 'partition', 'per_thread_scratch', 'thread_safe_writes', 'diffusion_rewrite', 'flowmatrix_reduction', 'rng_handling', 'determinism_proof', 'rayon_integration', 'speedup_estimate', 'slice_plan', 'risks'],
  properties: {
    which_passes: { type: 'string', description: 'which per-tick systems parallelize (the no-RNG cell-independent ones: metabolism, diffuse_and_decay, mineralize — maybe predation/host_coupling) and which STAY sequential (reproduce_or_die per-birth RNG, emit_chem, the asserts, hash_world)' },
    partition: { type: 'string', description: 'the partition unit + strategy: rayon over contiguous CELL chunks (the metabolism `items` are already (cell,species,org)-sorted → contiguous cell ranges = chunks); chunk sizing; the sequential-threshold for small N/cell counts to avoid overhead' },
    per_thread_scratch: { type: 'string', description: 'how each thread gets its OWN scratch buffers (the current single World-owned MetabolismScratch/ReproScratch cannot be shared across threads): thread-local scratch, rayon fold with per-task scratch, or a pre-sized pool of scratch buffers indexed by chunk' },
    thread_safe_writes: { type: 'string', description: 'how parallel writes stay safe + deterministic: COMPUTE the demand/apportion/grants per-cell in parallel (the expensive Monod + largest-remainder math), COLLECT per-thread grant lists, then APPLY them to the ECS orgs + pools SEQUENTIALLY in canonical (cell,species,org) order (cheap) → byte-identical. Disjoint-cell pool writes can be direct; org Energy/Biomass writes go through the canonical apply' },
    diffusion_rewrite: { type: 'string', description: 'the diffuse_and_decay GATHER rewrite: each cell reads its 4 neighbours from the frozen src + computes its own new value (no scatter → no write conflict → parallelizable by cell), proven byte-identical to the current scatter formulation (same integer result, same conservation)' },
    flowmatrix_reduction: { type: 'string', description: 'the FlowMatrix accumulation under parallelism: per-thread local S×S matrices summed in a fixed order (integer add is associative+commutative → order-independent → byte-identical), or a canonical merge' },
    rng_handling: { type: 'string', description: 'why the parallelized passes draw NO SimRng (metabolism/diffusion/mineralize are RNG-free) so parallelism cannot perturb the RNG stream; reproduce_or_die keeps its sequential 4-word-per-birth draw untouched → the hash is preserved' },
    determinism_proof: { type: 'string', description: 'the argument that the parallel version is BYTE-IDENTICAL: each cell-chunk computation is independent + RNG-free; grants applied in canonical order; integer reductions order-independent; no HashMap iteration; the pinned literal 0x47a0 stays + the multi-ISA gate (x86==aarch64) is the cross-platform safety net' },
    rayon_integration: { type: 'string', description: 'rayon (MIT/Apache, inv #1 OK; new pinned dep, inv #7 → recorded) used INSIDE the systems via a persistent global pool (no per-tick spawn); the Bevy schedule stays single-threaded .chain() (system order deterministic). Why NOT Bevy par_iter (shared-resource pool writes conflict)' },
    speedup_estimate: { type: 'string', description: 'a REALISTIC Amdahl estimate: if metabolism+diffusion+mineralize ≈ X% of the tick parallelize over ~P cores and reproduce_or_die+asserts (~Y%) stay sequential, max speedup = 1/((1-X) + X/P). Give the honest expected wall-clock multiple at 1k/5k/10k orgs on a 12-core M4 Max — likely ~2-2.5×, NOT 4× (sequential reproduce_or_die caps it)' },
    slice_plan: { type: 'array', items: { type: 'string' }, description: 'the incremental implementation slices, each BYTE-IDENTICAL + benched + multi-ISA: e.g. S1 diffusion gather+parallel (self-contained), S2 metabolism compute/apply split + parallel, S3 mineralize, S4 predation/host_coupling. Smallest-first, each provable against the hash' },
    risks: { type: 'array', items: { type: 'string' }, description: 'the determinism risks + how the hash oracle + multi-ISA gate catch them; false sharing; overhead at low N; rayon pool nondeterminism (must not affect results); the apply-order discipline' },
  },
}
const LENSES = [
  'DETERMINISM & correctness: the parallel passes MUST be byte-identical (the pinned hash 0x47a0 is the oracle, the multi-ISA gate the cross-platform net). Pin the compute-parallel / apply-canonical split, the gather-diffusion rewrite, the order-independent integer reductions, and why the RNG stream is untouched (the parallelized passes are RNG-free). This is the load-bearing lens.',
  'PERFORMANCE & scaling: rayon over cell chunks on a 12-core M4 Max, persistent pool, the sequential-threshold for small N, false-sharing avoidance, and an HONEST Amdahl estimate (the sequential reproduce_or_die + asserts cap the speedup — give the real expected multiple, not a best case).',
  'ARCHITECTURE & integration: rayon INSIDE the heavy systems while the bevy_ecs schedule stays single-threaded .chain(); per-thread scratch (the World-owned scratch cannot cross threads); the data restructuring (group orgs by cell, partition the sorted items, disjoint-cell pool writes); the incremental slice plan so each step is independently provable + benchable.',
]
const proposals = (await parallel(LENSES.map((lens, i) => () =>
  agent(
    `Design a deterministic, BYTE-IDENTICAL parallelization of the gene-sim sim-core per-tick hot path through this lens: ${lens}.\n\n` +
    `Context: sim-core uses bevy_ecs with a single-threaded explicitly .chain()-ordered schedule (the determinism backbone, ADR-002/ADR-013). Per-tick hotspots (release): metabolism ~40-50% (per-org Monod uptake + per-cell largest-remainder apportion, RNG-FREE), diffuse_and_decay ~12-15% (4-neighbour stencil over 1024 cells × 3 chem channels, RNG-FREE), mineralize (decomposers, RNG-FREE). reproduce_or_die (~20-25%, draws EXACTLY 4 RNG words per birth — MUST stay sequential) + emit_chem + the asserts. Grid = 32×32 = 1024 cells. The pinned hash 0x47a0_3c8f_6701_f240 (run_headless) MUST stay UNCHANGED — the parallel version is byte-identical, NOT a re-pin. The multi-ISA CI gate (x86_64 hash == aarch64 hash) is the cross-platform safety net. rayon is MIT/Apache (inv #1 fine); it would be a new pinned dep (inv #7). READ crates/sim-core/src/lib.rs (the schedule chain ~1907, metabolism ~691-1066, reproduce_or_die ~1249, hash_world ~3068), trophic.rs (mineralize/predation/FlowMatrix), chem.rs (diffuse_and_decay ~315), and docs/llm/DECISIONS.md (ADR-002 determinism, ADR-013, the perf baseline ~806).\n\n` +
    `Return a concrete, file:line-cited design. Be rigorous about byte-identity — a single reordered integer accumulation moves the hash. Do NOT write code.`,
    { label: `design:lens${i}`, phase: 'Design', schema: DSCHEMA },
  ),
))).filter(Boolean)
const chosen = await agent(
  `Judge & synthesize these ${proposals.length} parallelization designs into ONE coherent plan. Pin: which passes parallelize vs stay sequential, the partition + per-thread-scratch + compute-parallel/apply-canonical split, the gather-diffusion rewrite, the FlowMatrix reduction, the determinism proof, rayon integration, the HONEST Amdahl speedup estimate, and the incremental byte-identical slice plan. Resolve any disagreement between the lenses (especially determinism vs performance tradeoffs). Output the final design.\n` +
    proposals.map((p, i) => `\n--- Design ${i} (lens ${i}) ---\n${JSON.stringify(p, null, 2)}`).join('\n'),
  { label: 'design:judge', phase: 'Design', schema: DSCHEMA },
)

phase('ADR')
const adr = await agent(
  `Write docs/llm/proposals/parallel-sim-draft.md — the ADR draft + slice plan for the deterministic parallelization of gene-sim, from this agreed design:\n${JSON.stringify(chosen, null, 2)}\n\n` +
  `Structure: (1) CONTEXT — the measured single-thread ceiling (~847 Kelem/s; alloc/lto micro-opts exhausted ~0-1%) + why parallelism is the remaining lever. (2) DECISION — parallelize the RNG-free cell-independent passes (metabolism/diffusion/mineralize) with rayon inside the systems, Bevy schedule stays single-threaded; compute-parallel + apply-canonical; reproduce_or_die stays sequential. (3) BYTE-IDENTITY GUARANTEE — the determinism proof + the hash oracle + the multi-ISA gate; NO re-pin. (4) HONEST SPEEDUP — the Amdahl estimate (realistic multiple, the sequential cap). (5) INVARIANTS — inv #3 (the determinism argument), inv #1 (rayon non-GPL), inv #7 (rayon pinned + the build-profile note). (6) SLICE PLAN — the incremental, each-byte-identical-and-benched slices. (7) RISKS + the rollback (if a slice moves the hash, revert it — the hash catches every determinism bug). End with a clear GO/NO-GO recommendation + what needs human sign-off. Cite file:line. Do NOT commit. This is for human review before implementation.`,
  { label: 'adr', phase: 'ADR' },
)

return { chosen, adr }
