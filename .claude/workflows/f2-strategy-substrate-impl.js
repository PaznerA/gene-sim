export const meta = {
  name: 'f2-strategy-substrate-impl',
  description:
    'ADR-013 F2 BudgetAllocationMap: express genome -> Strategy{budget[5] permille simplex, TrophicRole, affinity} per species, cached UNWIRED in SpeciesEntry. Hash-neutral — the gateway substrate F3/F4 need.',
  whenToUse:
    'Run AFTER ecoli-visibility-impl. Fills the core gap: the Strategy type that the fixed.rs apportionment scaffolding was built for but never invoked. Hash-neutral because it is unread by selection; stops for human commit.',
  phases: [
    { title: 'Design' },
    { title: 'Implement' },
    { title: 'Verify' },
  ],
}

// ── Phase 1: design panel (3 lenses) → judge ──
phase('Design')
const DSCHEMA = {
  type: 'object',
  required: ['strategy_type', 'expression', 'trophic_role', 'storage', 'hash_neutrality_argument', 'tests', 'slices'],
  properties: {
    strategy_type: { type: 'string', description: 'The Rust Strategy type: budget [u16; 5] summing to EXACTLY 1000 permille, role: TrophicRole, an affinity vector' },
    expression: { type: 'string', description: 'How the genome expresses a Strategy via OntologyMap budget bindings + fixed::apportion (largest-remainder, ties->lowest). Reuse crates/sim-core/src/fixed.rs.' },
    trophic_role: { type: 'string', description: 'TrophicRole enum {Autotroph, Heterotroph, Mixotroph, Decomposer}; how a species declares it (SpeciesSpec field or derived)' },
    storage: { type: 'string', description: 'Where the expressed Strategy is cached (SpeciesEntry.strategy), computed once in reset_with_roster, NEVER read by selection yet' },
    hash_neutrality_argument: { type: 'string', description: 'Why hash-neutral: zero SimRng draws, not folded into hash_world, not read by selection; pinned literal unchanged' },
    tests: { type: 'array', items: { type: 'string' }, description: 'simplex==1000, ties->lowest, per-species expression pinned, determinism literal unchanged' },
    slices: { type: 'array', items: { type: 'string' } },
  },
}
const LENSES = [
  'determinism & fixed-point: the [u16;5] simplex must sum to 1000 via fixed::apportion with ties->lowest; zero RNG; prove hash-neutral',
  'biology: budget channels (growth/maintenance/defense/storage/reproduction) + TrophicRole must map to real chemostat-J semantics F3/F4 will consume',
  'API/seam fit: how Strategy slots into SpeciesEntry & the OntologyMap without disturbing the existing 9-trait plant / 5-trait E. coli expression',
]
const proposals = (await parallel(LENSES.map((lens, i) => () =>
  agent(
    `Design the gene-sim ADR-013 F2 "BudgetAllocationMap" substrate through this lens: ${lens}.\n\n` +
    `Context: genome->phenotype lives in crates/sim-core/src/gp.rs (OntologyMap, TraitMap, LocusSelector ByIndex/ByGoAnchor). crates/sim-core/src/fixed.rs has apportion/normalize_permille/to_unit_u16 LANDED but UNUSED. SpeciesRegistry/SpeciesEntry (crates/sim-core/src/lib.rs) caches per-species base_growth at reset_with_roster. Task: express each genome into a Strategy{budget:[u16;5] summing to 1000, role:TrophicRole, affinity}, cache it in SpeciesEntry, but DO NOT wire it into selection (keep it unread) so it is fully hash-neutral. READ the actual files first.\n\n` +
    `The pinned determinism literal MUST stay 0xf795_eac4_112f_acd5. Return a concrete file-level design.`,
    { label: `design:lens${i}`, phase: 'Design', schema: DSCHEMA },
  ),
))).filter(Boolean)

const chosen = await agent(
  `Judge & synthesize these ${proposals.length} F2 Strategy-substrate designs into ONE plan:\n` +
    proposals.map((p, i) => `\n--- Design ${i} ---\n${JSON.stringify(p, null, 2)}`).join('\n') +
    `\n\nPick the budget channel semantics, the TrophicRole declaration, the apportion-based expression, and the SpeciesEntry storage. The plan MUST be hash-neutral (unwired). Output the final design.`,
  { label: 'design:judge', phase: 'Design', schema: DSCHEMA },
)

// ── Phase 2: implement (single implementer, one crate) ──
phase('Implement')
const impl = await agent(
  `Implement this agreed gene-sim ADR-013 F2 Strategy substrate, hash-neutral and UNWIRED:\n${JSON.stringify(chosen, null, 2)}\n\n` +
  `Add the Strategy type + TrophicRole, express it per-species via fixed::apportion in reset_with_roster, cache in SpeciesEntry, add the tests (simplex==1000, ties->lowest, per-species expression pinned). Selection must NOT read Strategy. The pinned determinism literal MUST remain 0xf795_eac4_112f_acd5 — if it would change, STOP and report (do NOT re-pin). Do NOT commit. Report files + lines changed.`,
  { label: 'impl', phase: 'Implement', agentType: 'implementer' },
)

// ── Phase 3: gate + 3 adversarial determinism skeptics (majority refute kills it) ──
phase('Verify')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. Report all 10 gates PASS/FAIL, determinism called out. No fixes, no commit.`,
  { label: 'gate', phase: 'Verify', agentType: 'gatekeeper' },
)
const RSCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'unwired', 'simplex_conserved', 'refuted', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'pinned literal unchanged AND no SimRng draw added' },
    unwired: { type: 'boolean', description: 'selection() does NOT read Strategy' },
    simplex_conserved: { type: 'boolean', description: 'budget sums to exactly 1000 with ties->lowest' },
    refuted: { type: 'boolean', description: 'true if you found a REAL determinism/hash-neutrality violation' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially REFUTE that the just-landed F2 Strategy substrate is hash-neutral & unwired. Read \`git diff\` + the determinism test. Hunt for: a sneaked SimRng draw, Strategy folded into hash_world, selection() reading Strategy, a moved pinned literal, a simplex that can miss 1000 or break ties->lowest. Default refuted=true if uncertain. Skeptic #${i}.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: RSCHEMA },
  ),
))).filter(Boolean)
const refutations = skeptics.filter((s) => s.refuted).length

return {
  chosen,
  impl,
  gate: typeof gate === 'string' ? gate.slice(0, 300) : gate,
  skeptics,
  verdict: refutations >= 2 ? 'REJECTED (not hash-neutral)' : 'CONFIRMED hash-neutral',
}
