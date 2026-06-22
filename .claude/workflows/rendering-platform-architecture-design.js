export const meta = {
  name: 'rendering-platform-architecture-design',
  description:
    'Web-research + architecture proposal: how to evolve from the godot isometric/top-down PoC to (A) a REALISTIC Unreal Engine 5 renderer and (B) a WEB build of the simple iso/top-down version, while preserving the deterministic read-only-renderer invariant (#2/#3), plus a PERFORMANCE roadmap for scaling the core. The unifying principle: ONE portable deterministic Rust core → MANY renderers (godot-iso / UE5-realistic / web). DESIGN/RESEARCH ONLY — no sim code, no re-pin, no cargo/gate (a proposal doc only, safe to run in parallel).',
  whenToUse:
    'Parallel fast-progress design while the predator/SP-1 implementation runs. Produces docs/llm/proposals/rendering-platform-architecture-draft.md (the UE5 + web + perf architecture, migration plan, determinism strategy across platforms, ADR draft + slices + roadmap entries).',
  phases: [
    { title: 'Research' },
    { title: 'Verify' },
    { title: 'Design' },
  ],
}

phase('Research')
const RSCHEMA = {
  type: 'object',
  required: ['patterns', 'determinism_impact', 'realistic_rendering', 'migration', 'sources'],
  properties: {
    patterns: { type: 'string', description: 'concrete integration/architecture patterns for this cluster (APIs, FFI/C-ABI, IPC, export targets), current as of 2026' },
    determinism_impact: { type: 'string', description: 'how it preserves (or threatens) inv #2 (renderer read-only) + inv #3 (the deterministic integer core / reproducible hash); cross-platform reproduction concerns' },
    realistic_rendering: { type: 'string', description: 'what realistic/scalable rendering of a microbial joule-economy ecosystem looks like in this cluster (particle/instanced/volumetric for populations + pool/chem fields + the multi-scale zoom)' },
    migration: { type: 'string', description: 'the path from the existing godot-sim cdylib + GSS4 snapshot + LiveSim #[func] surface to this target; what is reused' },
    sources: { type: 'array', items: { type: 'string' }, description: 'URLs / citations' },
  },
}
const CLUSTERS = [
  'UNREAL ENGINE 5 ↔ a headless deterministic Rust sim core: how a UE5 frontend consumes a Rust library — C-ABI/FFI (a cdylib like godot-sim, UE plugin / third-party module), or a snapshot IPC/shared-memory stream; how UE5 renders a microbial/cellular ECOSYSTEM realistically (Niagara GPU particles for populations & the pool/chem fields, instanced static meshes, volumetric materials, the multi-scale cell→colony→ecosystem zoom); keeping all biology in the Rust core (inv #2). Web-search current (2025/2026) UE5 + Rust integration + Niagara-for-data-viz patterns.',
  'WEB / WASM build of the simple iso/top-down version: (a) the Rust sim-core compiled to wasm32 running DETERMINISTICALLY in-browser (does the i64/fixed-point pipeline reproduce the pinned hash on wasm32? extend the multi-ISA gate to wasm); (b) godot 4 HTML5/web export of the EXISTING iso frontend + godot-rust/gdext wasm support for the godot-sim cdylib (is gdext wasm-exportable in 2026?); (c) a thin web renderer (canvas/WebGL/WebGPU) reading the wasm core snapshots. Web-search godot 4 web export + gdext/godot-rust wasm + wasm32 floating-point/integer determinism.',
  'PERFORMANCE scaling of the deterministic core for richer real-time rendering (more entities, larger worlds): deterministic parallelism (can the tick parallelize with a FIXED reduction order without breaking the hash? rayon-with-ordered-merge vs stay single-threaded), SIMD on the non-hash render/snapshot path, the deferred BTreeMap→sorted-Vec hot-path win, snapshot/IPC throughput for a 60fps realistic renderer, larger grids (32×32 → ?), entity-count ceilings. Web-search deterministic-parallelism-in-simulation + ECS perf patterns; reference the current bench (post-F5: ~62/295/591ms at 1k/5k/10k×50).',
]
const research = (await parallel(CLUSTERS.map((cluster, i) => () =>
  agent(
    `Web-research this cluster for the gene-sim rendering/platform architecture and return CITED findings: ${cluster}.\n\n` +
    `Use web search (find WebSearch/WebFetch via ToolSearch, query "web search fetch"). Be current (2025/2026) + concrete. Context: the core is headless deterministic Rust (crates/sim-core + harness), exposed today via a godot-sim cdylib (LiveSim #[func]s) + a GSS4 binary snapshot + a measured FlowMatrix; inv #2 = renderer is READ-ONLY, biology in the core; inv #3 = bit-reproducible integer pipeline, CI multi-ISA gate (x86_64+aarch64). READ crates/godot-sim/src/lib.rs + crates/sim-core/src/snapshot.rs to ground the boundary. Return structured findings. Do NOT run cargo/gate.`,
    { label: `research:c${i}`, phase: 'Research', schema: RSCHEMA },
  ),
))).filter(Boolean)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['determinism_preserved', 'wasm_reproducibility', 'inv2_holds', 'risks', 'verdict'],
  properties: {
    determinism_preserved: { type: 'boolean', description: 'the proposed boundaries keep the core the single source of determinism; renderers stay read-only consumers of snapshots' },
    wasm_reproducibility: { type: 'string', description: 'the real story on wasm32 reproducing the integer hash (the multi-ISA gate extended to wasm) — confirmed/uncertain, with evidence' },
    inv2_holds: { type: 'boolean', description: 'UE5/web renderers compute NO biology — they read snapshots/exports only' },
    risks: { type: 'array', items: { type: 'string' }, description: 'determinism/portability risks (FFI/IPC non-determinism, parallelism reordering, wasm float behaviour, UE tick coupling)' },
    verdict: { type: 'string', description: 'is the architecture sound + determinism-safe? what must be pinned/proven first?' },
  },
}
const verified = await agent(
  `Adversarially verify the gene-sim rendering/platform research for determinism + invariant safety. The hard questions: does wasm32 actually reproduce the pinned integer hash (or does the multi-ISA gate need a wasm leg + possible fixes)? does any UE5/web/IPC/parallelism boundary leak non-determinism into the core? do the renderers stay strictly read-only (inv #2)? Web-search to confirm the wasm-determinism + gdext-wasm claims. Research:\n${JSON.stringify(research, null, 2)}`,
  { label: 'verify', phase: 'Verify', schema: VSCHEMA },
)

phase('Design')
const proposal = await agent(
  `Write docs/llm/proposals/rendering-platform-architecture-draft.md — the rendering & platform architecture for gene-sim, from the verified research. READ crates/godot-sim/src/lib.rs (the LiveSim boundary), crates/sim-core/src/snapshot.rs (GSS4), and CLAUDE.md (the invariants + the "godot/ consumes snapshots, never computes biology" rule) first.\n\n` +
  `The proposal MUST cover:\n` +
  `1. THE PRINCIPLE — ONE portable deterministic Rust core → MANY read-only renderers (godot-iso PoC / UE5-realistic / web). Formalize the core-as-library boundary: a stable C-ABI + the GSS4 snapshot + the exported FlowMatrix/relations/specimen reads as THE contract every renderer consumes (inv #2). The core never depends on any renderer.\n` +
  `2. UNREAL ENGINE 5 (realistic renderer): the integration (C-ABI cdylib / UE module / snapshot IPC), the realistic rendering design (Niagara for populations + pool/chem fields, instanced meshes, volumetric chem, the multi-scale zoom cell→colony→ecosystem), and how it reuses the existing snapshot/LiveSim contract. Renderer read-only.\n` +
  `3. WEB build (the simple iso/top-down version): the recommended path (godot web export of the existing frontend if gdext-wasm allows, OR sim-core→wasm32 + a thin web renderer), the determinism story on wasm32 (extend the multi-ISA CI gate with a wasm leg; what to prove/fix), and the trade-offs.\n` +
  `4. PERFORMANCE roadmap: the deterministic-parallelism analysis (can the tick parallelize with a fixed reduction order, or stay single-threaded + optimize), the deferred BTreeMap→Vec win, SIMD on the render/snapshot path, snapshot throughput for 60fps, larger worlds/entity ceilings, bench targets (vs the post-F5 baseline).\n` +
  `5. DETERMINISM STRATEGY across platforms — how inv #3 extends to wasm + UE + parallelism (the CI gate matrix grows; the read-only-renderer rule protects the hash); what is hash-neutral vs needs proving.\n` +
  `6. MIGRATION + ADR DRAFT + SLICE PLAN + roadmap entries (a paste-ready TASKS.md block). Phase it so the godot PoC keeps working throughout (the core boundary is the stable seam).\n\n` +
  `Cite the verified facts; flag anything uncertain (esp. wasm determinism + gdext-wasm). Keep biology in the core (inv #2), determinism-first. Do NOT run cargo/gate (this is a proposal doc — keep it parallel-safe). Do NOT commit. End with the paste-ready roadmap entries.\n\n` +
  `Verified:\n${JSON.stringify(verified, null, 2)}\n\nResearch:\n${JSON.stringify(research, null, 2)}`,
  { label: 'proposal', phase: 'Design', agentType: 'implementer' },
)

return { research, verified, proposal }
