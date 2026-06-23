export const meta = {
  name: 'contamination-s5-symbionts-impl',
  description:
    'ADR-019 S5 — Mode B obligate symbionts / minimal genomes (REAL mechanic, conditional RE-PIN): a new ObligateSymbiont TrophicRole that REQUIRES a co-located host — it exchanges J with the host via a measured FlowMatrix edge, can only be inoculated where a compatible host exists, and is cull-immune at the environment layer (it lives inside the host). If the host dies, the symbiont dies — emergent, not scripted. Bake the minimal-genome symbionts Carsonella ruddii + JCVI-Syn3.0. The pinned single-species-plant config has no symbiont → likely hash-neutral; the Repin phase decides.',
  whenToUse:
    'Midnight session item 7. The reduced-genome / endosymbiosis mode. A real biological mechanic; conditional re-pin; multi-ISA validated by CI on push.',
  phases: [
    { title: 'Design' },
    { title: 'Implement' },
    { title: 'Repin' },
    { title: 'Gate' },
    { title: 'Verify' },
  ],
}

phase('Design')
const DSCHEMA = {
  type: 'object',
  required: ['role', 'host_coupling', 'inoculation_gating', 'cull_immunity', 'data', 'conservation', 'determinism', 'repin_expectation', 'open_questions'],
  properties: {
    role: { type: 'string', description: 'the new TrophicRole::ObligateSymbiont (fieldless, like the others) + how a species declares it (niche.trophic_role "symbiont" via role_from_str, gene-anchored or data-declared); taps NO abiotic channel — income is the host coupling only' },
    host_coupling: { type: 'string', description: 'the host↔symbiont J exchange: a deterministic per-cell pass where a symbiont draws J from / provisions a co-located compatible HOST org (which host species? declared via an affinity), writing a measured FlowMatrix edge (host↔symbiont), integer, conserved' },
    inoculation_gating: { type: 'string', description: 'a symbiont RegionInoculate only establishes where a compatible host is co-located (else a clean no-op — it cannot free-live, the verified biology); reuses the region_inoculate path with a host-presence check' },
    cull_immunity: { type: 'string', description: 'a symbiont inside a host is cull-immune at the environment layer (region_cull skips ObligateSymbiont orgs, structural — like spores) — you must remove the host to clear it' },
    data: { type: 'string', description: 'bake Carsonella ruddii (~160 kb, ~182 genes) + JCVI-Syn3.0 (~531 kb, 473 genes) SpeciesSpecs (real NCBI for Carsonella; Syn3.0 from the JCVI/Mycoplasma minimal set), niche.trophic_role "symbiont", build+round-trip tests' },
    conservation: { type: 'string', description: 'the host↔symbiont J flux is a paired move (conserved); ledger_closes holds; no new tap (or a documented one); the symbiont death (host lost) → carcass→detritus' },
    determinism: { type: 'string', description: 'integer/fixed-point, ordered (cell,SpeciesId,OrgId), no new SimRng draw, no HashMap; the coupling pass at a fixed schedule slot' },
    repin_expectation: { type: 'string', description: 'will the pinned plant config hash move? (no symbiont in the pinned config → inert → likely HASH-NEUTRAL; the Repin phase decides)' },
    open_questions: { type: 'array', items: { type: 'string' } },
  },
}
const LENSES = [
  'biology: obligate endosymbiosis (Carsonella/Hodgkinia/Syn3.0) — so genome-reduced they cannot free-live; require a host, exchange metabolites, blur the cell↔organelle line; a host-required, cull-immune, host-coupled species, NOT an airborne contaminant',
  'determinism & conservation: the host↔symbiont J exchange must be integer, ordered, RNG-free, conserved (a paired move / a measured FlowMatrix edge); host-required inoculation + environment-layer cull-immunity are structural; conditional re-pin',
  'minimal-surface fit: reuse the seams — a 5th... (6th) TrophicRole, the FlowMatrix for the host↔symbiont edge (like predation/mineralization), the region_inoculate path for host-gated establishment, region_cull for cull-immunity; the bake convention for Carsonella/Syn3.0',
]
const proposals = (await parallel(LENSES.map((lens, i) => () =>
  agent(
    `Design gene-sim's ADR-019 S5 obligate-symbiont mode through this lens: ${lens}.\n\n` +
    `READ docs/llm/proposals/contamination-immigration-draft.md §5.5 + Mode B, and the landed pieces: gp.rs (TrophicRole enum + role_from_str/role_from_override + is_prey + Strategy), trophic.rs (FlowMatrix + predation/mineralize — the org-coupling precedents), lib.rs (region_inoculate the host-gated-spawn precedent + region_cull + reproduce_or_die), the bake scripts (bake_mycoplasma/bacillus), data/species. The mechanic: a TrophicRole::ObligateSymbiont that REQUIRES a co-located compatible host — host↔symbiont J exchange via a FlowMatrix edge, host-required inoculation, environment-layer cull-immunity; if the host dies the symbiont dies (emergent). Bake Carsonella ruddii + JCVI-Syn3.0. Per §0.6 a REAL mechanic, emergent outcomes. Conditional RE-PIN (likely hash-neutral — no symbiont in the pinned plant config).\n\n` +
    `Return a concrete file-level design. Do NOT write code.`,
    { label: `design:lens${i}`, phase: 'Design', schema: DSCHEMA },
  ),
))).filter(Boolean)
const chosen = await agent(
  `Judge & synthesize these ${proposals.length} obligate-symbiont designs into ONE plan. Pin the ObligateSymbiont role, the host↔symbiont coupling + FlowMatrix edge, the host-gated inoculation, the cull-immunity, the conservation, and the Carsonella/Syn3.0 bakes. A REAL mechanic; emergent outcomes. Output the final design.\n` +
    proposals.map((p, i) => `\n--- Design ${i} ---\n${JSON.stringify(p, null, 2)}`).join('\n'),
  { label: 'design:judge', phase: 'Design', schema: DSCHEMA },
)

phase('Implement')
const impl = await agent(
  `Implement gene-sim's ADR-019 S5 obligate-symbiont mode per this agreed design:\n${JSON.stringify(chosen, null, 2)}\n\n` +
  `Add TrophicRole::ObligateSymbiont + the host↔symbiont J coupling pass (writes a FlowMatrix edge, conserved, integer, ordered, RNG-free) + host-required inoculation gating + environment-layer cull-immunity, and bake Carsonella ruddii + JCVI-Syn3.0 SpeciesSpecs (real provenance, niche.trophic_role "symbiont", build+round-trip tests). All integer/fixed-point, ordered (cell,SpeciesId,OrgId), no new SimRng draw; ledger_closes holds (host↔symbiont flux is a paired move). Do NOT touch the pinned literal yet (Repin phase). Add tests: a symbiont establishes ONLY with a compatible host present (host-gated), is cull-immune at the environment layer, dies when its host dies, and the host↔symbiont J flux is conserved + appears in the FlowMatrix; run-to-run stable. Per §0.6: assert the MECHANIC (host-dependence), NOT a forced equilibrium. Do NOT commit. Report file:line + whether you expect the pinned hash to move (no symbiont in the pinned config → likely not).`,
  { label: 'impl', phase: 'Implement', agentType: 'implementer' },
)

phase('Repin')
const repin = await agent(
  `Obligate symbionts are implemented. Determine + perform the re-pin (current literal 0x47a0_3c8f_6701_f240): build, get the new run_headless hash for the pinned cfg (single-species plant, no symbiont), confirm run-to-run stable (twice/3 processes). If it EQUALS 0x47a0 → HASH-NEUTRAL (inert with no symbiont); leave unchanged, report HASH-NEUTRAL. If it DIFFERS → RE-PIN: update the literal + ledger line "<new>… after ADR-019 S5 (obligate symbionts: host-coupled, host-gated inoculation, cull-immune; Carsonella/Syn3.0)." aarch64; x86_64 is CI's job. Do NOT commit. Report outcome + stability.`,
  { label: 'repin', phase: 'Repin', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(`Run \`bash tools/gate.sh\`. determinism GREEN against the (re-pinned or unchanged) literal; license green (new bakes clean); all gates. Report PASS/FAIL with any exact error. No commit.`, { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' })

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['conserved', 'deterministic', 'host_required', 'cull_immune', 'dies_with_host', 'repin_consistent', 'issues'],
  properties: {
    conserved: { type: 'boolean', description: 'host↔symbiont flux is a paired move; ledger_closes holds; FlowMatrix edge conserved' },
    deterministic: { type: 'boolean', description: 'integer, ordered, no new RNG; run-to-run hash stable' },
    host_required: { type: 'boolean', description: 'a symbiont establishes/survives ONLY with a compatible host (host-gated inoculation; cannot free-live)' },
    cull_immune: { type: 'boolean', description: 'a symbiont is cull-immune at the environment layer (region_cull skips it)' },
    dies_with_host: { type: 'boolean', description: 'when its host dies the symbiont dies (emergent, a test shows it) — NOT a forced persistence (§0.6)' },
    repin_consistent: { type: 'boolean', description: 'gate determinism GREEN against the literal; re-pin stable+ledgered if moved, else literal unchanged + justified' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify gene-sim's ADR-019 S5 obligate symbionts. Read git diff + the role/coupling/inoculation-gating/cull-immunity code + the Carsonella/Syn3.0 bakes + the tests. Skeptic #${i}, default each boolean false if unconfirmable. Hunt: a J leak / ledger_closes break in the host coupling; a FlowMatrix edge that doesn't conserve; a new RNG draw; HashMap/order-dependence; run-to-run instability; a symbiont that can free-live (NOT host-gated); a symbiont that is NOT cull-immune; AND (§0.6) a mechanic that FORCES the symbiont to persist rather than the host-dependence with emergent outcome.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA },
  ),
))).filter(Boolean)
const ok = skeptics.filter((s) => s.conserved && s.deterministic && s.host_required && s.repin_consistent).length
return { chosen, impl, repin, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, skeptics, verdict: ok >= 2 ? 'S5 CONFIRMED — obligate symbionts live' : 'NEEDS WORK' }
