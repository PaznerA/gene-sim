export const meta = {
  name: 'contamination-s4-spore-dormancy-impl',
  description:
    'ADR-019 S4 — spore/dormancy reservoir (REAL mechanic, conditional RE-PIN): spore-forming species (Bacillus, the Aspergillus/Penicillium molds) survive a cull as a dormant SPORE reservoir that REGERMINATES when conditions return — so the cull tool alone cannot eradicate a spore-former (its emergent counter-play). Real endospore biology (NOT a balance fudge per §0.6 — it is a feature; whether the species persists stays emergent). Deterministic, integer, conserved (spore J accounted). The pinned single-species-plant config has no spore-former → likely hash-neutral; the Repin phase decides.',
  whenToUse:
    'Midnight session item 6. After the contamination core + the cull tool. A real biological mechanic; conditional re-pin; multi-ISA validated by CI on push.',
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
  required: ['spore_state', 'sporulation_trigger', 'germination', 'conservation', 'determinism', 'repin_expectation', 'open_questions'],
  properties: {
    spore_state: { type: 'string', description: 'where the dormant spore reservoir lives (per-cell per-species spore J, like PoolProvenance? or a per-org dormant flag) + which species are spore-formers (Bacillus + the molds; declared via a SpeciesSpec trait/flag or trophic-role-adjacent marker)' },
    sporulation_trigger: { type: 'string', description: 'what makes a vegetative org sporulate into the reservoir: on cull (a fraction survives as spores instead of dying) and/or on starvation (low energy); deterministic fraction, integer' },
    germination: { type: 'string', description: 'how spores REGERMINATE: when local conditions return (nutrient/energy available), the per-cell spore reservoir spawns vegetative orgs deterministically (RNG-free placement, OrgIds from NextOrgId); a regermination pass in the schedule' },
    conservation: { type: 'string', description: 'spore J is conserved: sporulation moves an org\'s J into the spore reservoir (not a kill→detritus), germination moves it back into a new vegetative org; ledger_closes holds (a named tap or a paired move). spores are cull-immune (the cull skips the reservoir)' },
    determinism: { type: 'string', description: 'integer/fixed-point, ordered (cell,SpeciesId,OrgId), no new SimRng draw, no HashMap; the germination/sporulation passes sit at a fixed schedule position' },
    repin_expectation: { type: 'string', description: 'will the pinned single-species-plant hash move? (no spore-former in the pinned config → the mechanic is inert there → likely HASH-NEUTRAL; the Repin phase decides empirically)' },
    open_questions: { type: 'array', items: { type: 'string' } },
  },
}
const LENSES = [
  'biology & game-feel: real endospore/conidia dormancy — Bacillus/Aspergillus/Penicillium survive a cull as spores that regerminate, so the cull tool gets an emergent counter-play (you must change CONDITIONS, not just cull); a feature, NOT a stability fudge (§0.6)',
  'determinism & conservation: the spore reservoir + sporulation + germination must be integer, ordered, RNG-free, and CONSERVE J (sporulation = J→reservoir, germination = reservoir→new org; ledger_closes holds); spores are cull-immune at the environment layer',
  'minimal-surface fit: reuse the existing seams — PoolProvenance-style per-cell-per-species reservoir, the reproduce_or_die spawn path for germination, the RegionCull path for the spore-survival fraction; smallest schedule addition',
]
const proposals = (await parallel(LENSES.map((lens, i) => () =>
  agent(
    `Design gene-sim's ADR-019 S4 spore/dormancy reservoir through this lens: ${lens}.\n\n` +
    `READ docs/llm/proposals/contamination-immigration-draft.md §5.4 + the landed pieces: trophic.rs (PoolProvenance per-cell-per-species attribution, mineralize), lib.rs (reproduce_or_die spawn + carcass→detritus + the cull path region_cull + the ledger), chem.rs, ledger.rs, the spore-former species (data/species/{bacillus,aspergillus-niger,penicillium}.json). The mechanic: spore-formers survive a cull (and/or starvation) as a dormant per-cell spore reservoir that REGERMINATES when conditions return — conserved, deterministic. Per §0.6 this is a REAL feature, NOT a balance fudge: whether the spore-former ultimately persists stays EMERGENT. Conditional RE-PIN (likely hash-neutral — no spore-former in the pinned plant config).\n\n` +
    `Return a concrete file-level design. Do NOT write code.`,
    { label: `design:lens${i}`, phase: 'Design', schema: DSCHEMA },
  ),
))).filter(Boolean)
const chosen = await agent(
  `Judge & synthesize these ${proposals.length} spore/dormancy designs into ONE plan. Pin the spore reservoir, the sporulation trigger, the germination pass, the conservation, and the determinism. A REAL mechanic (not balance-forcing). Output the final design.\n` +
    proposals.map((p, i) => `\n--- Design ${i} ---\n${JSON.stringify(p, null, 2)}`).join('\n'),
  { label: 'design:judge', phase: 'Design', schema: DSCHEMA },
)

phase('Implement')
const impl = await agent(
  `Implement gene-sim's ADR-019 S4 spore/dormancy per this agreed design:\n${JSON.stringify(chosen, null, 2)}\n\n` +
  `Add the spore reservoir + sporulation (cull/starvation survival fraction → reservoir) + germination (reservoir → vegetative orgs when conditions return), all integer/fixed-point, ordered (cell,SpeciesId,OrgId), RNG-free, conserved (ledger_closes holds; spore J is a paired move, not a mint/sink; spores are cull-immune). Do NOT touch the pinned literal yet (Repin phase). Add tests: sporulation+germination conserve J + ledger closes; cull-then-regerminate happens deterministically (a spore-former survives a cull as a reservoir, regerminates when nutrient returns); run-to-run stable. Per §0.6: do NOT tune this so the species persists — the test asserts the MECHANIC (cull leaves a reservoir that can regerminate), not a forced equilibrium. Do NOT commit. Report file:line + whether you expect the pinned hash to move (no spore-former in the pinned config → likely not).`,
  { label: 'impl', phase: 'Implement', agentType: 'implementer' },
)

phase('Repin')
const repin = await agent(
  `Spore/dormancy is implemented. Determine + perform the re-pin (current literal 0x47a0_3c8f_6701_f240): build, get the new run_headless hash for the pinned cfg (single-species plant, no spore-former), confirm run-to-run stable (twice/3 processes). If it EQUALS 0x47a0 → HASH-NEUTRAL (the mechanic is inert with no spore-former); leave unchanged, report HASH-NEUTRAL. If it DIFFERS → RE-PIN: update the literal + ledger line "<new>… after ADR-019 S4 (spore/dormancy reservoir: spore-formers survive a cull + regerminate; conserved)." aarch64; x86_64 is CI's job. Do NOT commit. Report outcome + stability.`,
  { label: 'repin', phase: 'Repin', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(`Run \`bash tools/gate.sh\`. determinism GREEN against the (re-pinned or unchanged) literal; all gates. Report PASS/FAIL with any exact error. No commit.`, { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' })

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['conserved', 'deterministic', 'cull_leaves_reservoir', 'regermination_real', 'not_balance_fudge', 'repin_consistent', 'issues'],
  properties: {
    conserved: { type: 'boolean', description: 'sporulation/germination conserve J (paired moves); ledger_closes holds; spores cull-immune' },
    deterministic: { type: 'boolean', description: 'integer, ordered, no new RNG; run-to-run hash stable' },
    cull_leaves_reservoir: { type: 'boolean', description: 'a cull of a spore-former leaves a dormant reservoir (cull-immune), not full eradication' },
    regermination_real: { type: 'boolean', description: 'the reservoir regerminates into vegetative orgs when conditions return (a test shows it)' },
    not_balance_fudge: { type: 'boolean', description: 'the mechanic is a real feature; the test asserts the mechanic, NOT a forced persistence/equilibrium (§0.6)' },
    repin_consistent: { type: 'boolean', description: 'gate determinism GREEN against the literal; re-pin stable+ledgered if moved, else literal unchanged + justified' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify gene-sim's ADR-019 S4 spore/dormancy. Read git diff + the spore reservoir/sporulation/germination code + the tests. Skeptic #${i}, default each boolean false if unconfirmable. Hunt: a J leak / ledger_closes break; a new RNG draw; HashMap/order-dependence; run-to-run instability; spores that are NOT actually cull-immune; a germination that never fires; AND (§0.6) a test/mechanic that FORCES the spore-former to persist (balance fudge) rather than just providing the reservoir-survives-cull mechanic with emergent outcome.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA },
  ),
))).filter(Boolean)
const ok = skeptics.filter((s) => s.conserved && s.deterministic && s.cull_leaves_reservoir && s.not_balance_fudge && s.repin_consistent).length
return { chosen, impl, repin, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, skeptics, verdict: ok >= 2 ? 'S4 CONFIRMED — spore reservoir live' : 'NEEDS WORK' }
