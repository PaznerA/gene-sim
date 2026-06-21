export const meta = {
  name: 'f3-metabolism-keystone-design',
  description:
    'ADR-013 F3 keystone DESIGN (STOP-THE-LINE): real metabolism (uptake->convert->excrete) + energy-funded birth/death replacing constant-N, ledger closure, multi-ISA CI gate. Produces a human-signoff-ready package + hash-neutral infra ONLY — never merges the re-pin.',
  whenToUse:
    'Run AFTER f2-strategy-substrate-impl. F3 breaks ADR-005 (constant-N) and is a deliberate determinism re-pin needing human sign-off + a multi-ISA CI gate first. This workflow designs & adversarially verifies it and builds only hash-neutral infra; it does NOT merge births/deaths.',
  phases: [
    { title: 'Diverge' },
    { title: 'Judge' },
    { title: 'Adversarial' },
    { title: 'SafeInfra' },
  ],
}

// ── Phase 1: 3 independent architectures (distinct angles) ──
phase('Diverge')
const ASCHEMA = {
  type: 'object',
  required: ['metabolism', 'lifecycle', 'ledger_closure', 'contention', 'ordering', 'repin_risk', 'open_questions'],
  properties: {
    metabolism: { type: 'string', description: 'per-organism resource uptake(light/free_nutrient)->convert->excrete(detritus), integer joules, reading ResourceField' },
    lifecycle: { type: 'string', description: 'energy-funded reproduce (surplus->offspring) + death (starvation/age); how this replaces constant-N Wright-Fisher while keeping per-species pools' },
    ledger_closure: { type: 'string', description: 'conservation contract: sum(pools+chem+energy+biomass)==initial+influx-respired-overflow every tick; the three named taps' },
    contention: { type: 'string', description: 'visibility semantics when many organisms draw the same cell: frozen snapshot vs immediate apply; pinned to stay deterministic' },
    ordering: { type: 'string', description: 'deterministic pass order: pre-sort by (cell_index, SpeciesId, OrgId); RNG draw-count stability when N varies' },
    repin_risk: { type: 'string', description: 'why the determinism hash necessarily changes; what the new ledger comment line should say' },
    open_questions: { type: 'array', items: { type: 'string' } },
  },
}
const ANGLES = [
  'chemostat realism: a real continuous-culture joule economy (Monod-like integer uptake, maintenance drain, overflow metabolism)',
  'determinism minimalism: the SMALLEST change to selection that introduces births/deaths while keeping a stable, multi-ISA-reproducible integer hash',
  'gameplay dynamics: population booms/crashes/extinction that are legible and fun in the 30FPS view without sacrificing determinism',
]
const proposals = (await parallel(ANGLES.map((angle, i) => () =>
  agent(
    `Design gene-sim ADR-013 F3 "real metabolism + energy-funded lifecycle" through this angle: ${angle}.\n\n` +
    `Context: today metabolism (crates/sim-core/src/lib.rs) is a decorative 1% EMA of Energy(i64) feeding nothing; selection is constant-N Wright-Fisher with S independent per-species pools (R3-B). ResourceField{light,free_nutrient,detritus} i64 per 32x32 cell is generated off-stream but UNREAD. Ledger is inserted empty, ledger.closes() never called. fixed.rs apportionment is available. F3 must wire resource uptake into energy, fund births from surplus, kill on starvation, replace ADR-005 constant population, and assert ledger closure every tick. ALL hash-path arithmetic must be i64/fixed-point (no float), ordered (sort by cell,species,org), no HashMap iteration. This is a deliberate determinism RE-PIN and STOP-THE-LINE (#3 multi-ISA gate, #6 population policy). READ the actual core files first.\n\n` +
    `Return a concrete, file-level architecture. Do NOT write code.`,
    { label: `diverge:angle${i}`, phase: 'Diverge', schema: ASCHEMA },
  ),
))).filter(Boolean)

// ── Phase 2: judge → one winning architecture ──
phase('Judge')
const winner = await agent(
  `Judge these ${proposals.length} F3 architectures on {determinism cost, multi-ISA reproducibility, biological soundness, gameplay legibility, re-pin blast radius}. Synthesize ONE winning design, grafting the best ideas from the runners-up. Be explicit about the contention/visibility decision and the ledger taps.\n` +
    proposals.map((p, i) => `\n--- Architecture ${i} ---\n${JSON.stringify(p, null, 2)}`).join('\n'),
  { label: 'judge', phase: 'Judge', schema: ASCHEMA },
)

// ── Phase 3: 3 adversarial determinism skeptics → multi-ISA gate spec ──
phase('Adversarial')
const VSCHEMA = {
  type: 'object',
  required: ['nondeterminism_findings', 'multi_isa_risks', 'ledger_leaks', 'gate_spec', 'verdict'],
  properties: {
    nondeterminism_findings: { type: 'array', items: { type: 'string' }, description: 'float-on-hash-path, HashMap/Query-order iteration, platform-divergent rounding, RNG draw-count instability when N varies, etc.' },
    multi_isa_risks: { type: 'array', items: { type: 'string' }, description: 'where x86_64 and aarch64 could diverge' },
    ledger_leaks: { type: 'array', items: { type: 'string' }, description: 'paths where joules could be silently created/destroyed' },
    gate_spec: { type: 'string', description: 'the EXACT multi-ISA CI gate to add (x86_64+aarch64 reproduce identical hash on the full integer pipeline) BEFORE F3 merges' },
    verdict: { type: 'string', description: 'is the winning design ready for human re-pin sign-off? what must change first?' },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially stress-test this F3 design for non-determinism and ledger leaks. Skeptic #${i}, default to flagging. Design:\n${JSON.stringify(winner, null, 2)}\n\nHunt specifically: any float entering hash_world; any HashMap/Bevy-Query iteration order dependence in birth/death; platform-divergent integer rounding; RNG draw-count instability once population varies (the historical 2N/gen invariant breaks); silent joule creation/destruction at caps. Specify the multi-ISA CI gate that must exist before merge.`,
    { label: `adversarial:skeptic${i}`, phase: 'Adversarial', schema: VSCHEMA },
  ),
))).filter(Boolean)

// ── Phase 4: build ONLY hash-neutral infra + the ADR draft; never merge births/deaths ──
phase('SafeInfra')
const infra = await agent(
  `Build ONLY the hash-neutral infrastructure for gene-sim F3 — do NOT implement births/deaths, do NOT change the pinned determinism literal 0xf795_eac4_112f_acd5:\n` +
  `1. Add the multi-ISA CI gate per this spec: ${skeptics.map((s) => s.gate_spec).filter(Boolean).join(' | ')} — a CI job (and/or tools/ script) running the determinism hash on x86_64 AND aarch64 asserting byte-identical (wire into .github/workflows + tools/gate.sh as a documented step; it may no-op-skip locally like the bench gate when cross-arch isn't available, but the CI matrix must be real).\n` +
  `2. Add a ledger_closes() assertion test harness (crates/sim-core) that, given pools+energy+biomass+taps, asserts sum(...)==initial+influx-respired-overflow — exercised on a synthetic fixture now, ready to assert on the live pipeline at F3 merge.\n` +
  `3. Write docs/llm/proposals/f3-metabolism-keystone-draft.md = the ADR-013 F3 draft: the winning design, the adversarial findings, the exact slice breakdown, and the new pinned-hash ledger line plan.\n\n` +
  `Winning design:\n${JSON.stringify(winner, null, 2)}\n\nAdversarial findings:\n${JSON.stringify(skeptics, null, 2)}\n\n` +
  `Run \`bash tools/gate.sh\` after — it MUST stay green (everything you add is hash-neutral). Do NOT commit. End your report with the explicit line: "STOP-THE-LINE: F3 births/deaths require human re-pin sign-off before implementation."`,
  { label: 'safe-infra', phase: 'SafeInfra', agentType: 'implementer' },
)

return { winner, skeptics, infra }
