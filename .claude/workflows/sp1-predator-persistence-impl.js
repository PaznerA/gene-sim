export const meta = {
  name: 'sp1-predator-persistence-impl',
  description:
    'SP-1 completion: give the Bdellovibrio predator PERSISTENCE so the 3-species system reaches a dynamic limit cycle instead of a single Lotka-Volterra overshoot-and-crash (the predator currently self-extinguishes by gen ~330-450 across all seeds). Adds two biologically-grounded, determinism-safe mechanics: (1) Bdellovibrio DORMANCY / starvation-survival (when local prey is scarce the predator drops to a low-metabolism survival state — reduced maintenance, no reproduction — and re-activates when prey returns, its real host-independent biphasic biology); (2) a PREY REFUGE (a deterministic fraction of prey escapes predation per cell, so prey is never driven fully to zero → a recoverable seed → a bounded predator-prey cycle). Builds on the landed MAINTENANCE_BASE 4000→2000 decomposer fix (literal already at 0x64a3). Conditional RE-PIN (likely hash-neutral — the pinned plant config runs no predation).',
  whenToUse:
    'Completes SP-1 (the decomposer fix alone left the predator going extinct). The goal: a genuine, dynamic 3-species coexistence (plant + E.coli decomposer + Bdellovibrio predator all persist long-run with legible oscillation). Conditional re-pin; multi-ISA validated by CI on push.',
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
  required: ['dormancy', 'refuge', 'stability_argument', 'determinism', 'ordering', 'repin_expectation', 'open_questions'],
  properties: {
    dormancy: { type: 'string', description: 'the predator dormancy/starvation-survival mechanic: a deterministic low-prey trigger (per-cell or per-org frozen prey J below a threshold) → reduced maintenance + suppressed reproduction (survive the trough), re-activate when prey returns; grounded in Bdellovibrio host-independent survival; integer, no new RNG' },
    refuge: { type: 'string', description: 'the prey refuge: a deterministic fraction of a cell\'s prey J is unreachable by predation (a spatial/structural refuge) so prey is never driven to exactly zero → a recoverable seed; how it folds into the predation kernel apportion (integer, ties→lowest)' },
    stability_argument: { type: 'string', description: 'why dormancy + refuge yield a BOUNDED predator-prey limit cycle (predator survives prey troughs via dormancy; prey survives predation peaks via refuge) instead of overshoot→crash→extinction; what amplitude/period to expect' },
    determinism: { type: 'string', description: 'both mechanics are integer/fixed-point, RNG-free (deterministic thresholds/fractions), ordered, conserved (dormancy reduces a maintenance debit; refuge just caps consumption — no J created/destroyed); ledger_closes holds' },
    ordering: { type: 'string', description: 'where the mechanics sit (dormancy in metabolism/maintenance; refuge inside trophic::predation) + the deterministic order' },
    repin_expectation: { type: 'string', description: 'will the pinned single-species-plant hash move? (both mechanics are predator/predation-only → the plant-only pinned run does not exercise them → likely HASH-NEUTRAL relative to 0x64a3; the Repin phase decides empirically)' },
    open_questions: { type: 'array', items: { type: 'string' } },
  },
}
const LENSES = [
  'ecology & stability: the standard cure for a Lotka-Volterra overshoot-crash is a PREY REFUGE (prey never hits zero) and/or PREDATOR self-limitation; plus Bdellovibrio DORMANCY (it really survives starvation host-independently) lets the predator ride out prey troughs → a bounded limit cycle. Design both so the 3-species coexistence is dynamic and persistent across seeds',
  'determinism & conservation: dormancy is a deterministic low-prey threshold lowering the predator maintenance debit (no new RNG); refuge is a deterministic fraction capping per-cell consumption (no J created/destroyed); both integer/ordered; ledger_closes must still hold; conditional re-pin (predation-only → likely hash-neutral)',
  'Bdellovibrio biology & game-feel: the biphasic attack-phase/survival life cycle is the in-universe justification for dormancy; the refuge reads as prey hiding in structure; the result should be a legible, watchable predator-prey oscillation (the "interesting emergent system" the user wants) — and the OVERSIGHT lever (CRISPRi on the predator) still bites',
]
const proposals = (await parallel(LENSES.map((lens, i) => () =>
  agent(
    `Design gene-sim's Bdellovibrio PREDATOR PERSISTENCE through this lens: ${lens}.\n\n` +
    `PROBLEM (measured, real): the predator does a single overshoot-and-crash — booms during the resource-rich transient, grinds the E.coli prey to near-zero, then starves to PERMANENT extinction by gen ~330-450 across ALL seeds; no limit cycle, no recovery. The decomposer fix (MAINTENANCE_BASE 4000→2000, landed, literal 0x64a3) made plant+E.coli genuinely dynamic but did NOT fix the predator. Constants alone don't fix it (predation rate/efficiency/uptake/toxin were swept and rejected) — it needs a MECHANIC.\n\n` +
    `Context (LANDED): trophic::predation (frozen prey census, Monod/Holling-II demand, per-cell apportion, conserved consume, FlowMatrix off-diagonal); reproduce_or_die (maintenance debit + starvation death); the Bdellovibrio spec (TrophicRole::Predator). READ crates/sim-core/src/trophic.rs (predation) + lib.rs (metabolism/maintenance/reproduce_or_die) first. Task: add (1) predator dormancy/starvation-survival and (2) a prey refuge, both deterministic + integer + conserved, so the predator PERSISTS and the 3-species system reaches a bounded dynamic limit cycle.\n\n` +
    `Return a concrete file-level design. Conditional determinism RE-PIN. Do NOT write code.`,
    { label: `design:lens${i}`, phase: 'Design', schema: DSCHEMA },
  ),
))).filter(Boolean)
const chosen = await agent(
  `Judge & synthesize these ${proposals.length} predator-persistence designs into ONE plan. Pin the dormancy trigger + the refuge fraction + how both stay deterministic/conserved, and the expected limit-cycle behaviour. Output the final design.\n` +
    proposals.map((p, i) => `\n--- Design ${i} ---\n${JSON.stringify(p, null, 2)}`).join('\n'),
  { label: 'design:judge', phase: 'Design', schema: DSCHEMA },
)

phase('Implement')
const impl = await agent(
  `Implement gene-sim's Bdellovibrio predator persistence per this agreed design:\n${JSON.stringify(chosen, null, 2)}\n\n` +
  `Add (1) the predator dormancy/starvation-survival (deterministic low-prey trigger → reduced maintenance + suppressed reproduction, re-activate on prey return) and (2) the prey refuge (a deterministic fraction of per-cell prey J unreachable by predation). Both integer/fixed-point, RNG-free, ordered, conserved (ledger_closes holds). Build on the landed MAINTENANCE_BASE=2000 (literal 0x64a3_ed4f_7bb1_2779). Add/extend a FUNCTIONAL test: the 3-species roster (plant+E.coli+Bdellovibrio) — using the SHIPPED data/species entity_counts — has the PREDATOR PERSIST (nonzero) over a long run (≥3000 gens) across MULTIPLE seeds, with a dynamic (oscillating, not flat, not extinct) coexistence; the trophic cascade + OVERSIGHT ripple still pass. Do NOT touch the pinned literal yet (Repin phase). Do NOT commit. Report file:line + whether you expect the pinned hash to move (predation-only → likely not).`,
  { label: 'impl', phase: 'Implement', agentType: 'implementer' },
)

phase('Repin')
const repin = await agent(
  `Predator persistence is implemented. Determine + perform the re-pin (current literal 0x64a3_ed4f_7bb1_2779): build, get the new run_headless hash for the pinned cfg (single-species plant), confirm run-to-run stable (twice/3 processes). If it EQUALS 0x64a3 → HASH-NEUTRAL (predation-only mechanics; leave unchanged, report HASH-NEUTRAL). If it DIFFERS → RE-PIN: update the literal + ledger line "<new>… after SP-1 predator persistence (Bdellovibrio dormancy + prey refuge → bounded predator-prey limit cycle)." aarch64; x86_64 is CI's job. Do NOT commit. Report outcome + stability.`,
  { label: 'repin', phase: 'Repin', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\`. determinism GREEN against the (re-pinned or unchanged) literal; all gates. Report PASS/FAIL with any exact error. No commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['predator_persists', 'dynamic_limit_cycle', 'three_species_coexist', 'conserved_deterministic', 'cascade_still_works', 'issues'],
  properties: {
    predator_persists: { type: 'boolean', description: 'the Bdellovibrio predator does NOT go to permanent zero — it persists (nonzero in the second half) over ≥3000 gens across MULTIPLE seeds, the EXACT failure the SP-1 review caught' },
    dynamic_limit_cycle: { type: 'boolean', description: 'predator + prey oscillate in a bounded limit cycle (not overshoot→crash, not flat)' },
    three_species_coexist: { type: 'boolean', description: 'plant + E.coli + Bdellovibrio all persist long-run — a genuine dynamic 3-species coexistence' },
    conserved_deterministic: { type: 'boolean', description: 'dormancy + refuge conserve J (ledger closes), integer/ordered, no new RNG; run-to-run hash stable' },
    cascade_still_works: { type: 'boolean', description: 'the trophic cascade + OVERSIGHT-KO ripple tests still pass' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify gene-sim's predator persistence. RUN the 3-species shipped roster (plant 1000 + E.coli 800 + Bdellovibrio 180) yourself for ≥3000 gens across MULTIPLE seeds (incl. 13679457532755275413, 42, 7, 999999) and check the PREDATOR is nonzero in the second half of EVERY seed (this is the exact thing the SP-1 review refuted — be rigorous). Skeptic #${i}, default each boolean false if unconfirmable. Also hunt: J leak / ledger break from dormancy or refuge; a new RNG draw; non-integer/HashMap; run-to-run instability; a "refuge" that secretly destroys prey J; dormancy that freezes the predator forever (never re-activates).`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA },
  ),
))).filter(Boolean)
const ok = skeptics.filter((s) => s.predator_persists && s.three_species_coexist && s.conserved_deterministic).length
return { chosen, impl, repin, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, skeptics, verdict: ok >= 2 ? 'PREDATOR PERSISTS — 3-species dynamic coexistence' : 'NEEDS WORK — predator still unstable' }
