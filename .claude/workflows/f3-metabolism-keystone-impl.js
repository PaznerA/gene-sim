export const meta = {
  name: 'f3-metabolism-keystone-impl',
  description:
    'ADR-013 F3 keystone IMPLEMENTATION + deliberate RE-PIN: real metabolism (uptake→convert→excrete, deletes the 1%-EMA RNG draw) + energy-funded reproduce_or_die replacing constant-N Wright-Fisher + Ledger closure asserted every tick. Implements the approved f3 draft, MOVES the pinned determinism literal to the new computed hash + appends a ledger line, runs the gate green, and adversarially verifies the new pipeline is genuinely deterministic. Single-arch local; multi-ISA portability validated by CI on push.',
  whenToUse:
    'Execute the F3 cut for real — the user wants the re-pin DONE, not staged. Implements docs/llm/proposals/f3-metabolism-keystone-draft.md. This MOVES the determinism hash: a deliberate, ledgered re-pin.',
  phases: [
    { title: 'Implement' },
    { title: 'Repin' },
    { title: 'Gate' },
    { title: 'Verify' },
  ],
}

phase('Implement')
const impl = await agent(
  `Implement gene-sim ADR-013 F3 EXACTLY per the approved design in docs/llm/proposals/f3-metabolism-keystone-draft.md. READ that draft IN FULL first, then the current crates/sim-core/src/{lib.rs,ledger.rs,resource.rs,gp.rs,fixed.rs}.\n\n` +
  `Land the real keystone:\n` +
  `1. A mutable PoolStock{light,free_nutrient,detritus: Vec<i64>} seeded ONCE at reset by quantizing the static ResourceField via fixed::to_unit_u16 * CELL_J_SCALE (the static f32 ResourceField stays the render/cap/seed source).\n` +
  `2. Replace the decorative 1%-EMA metabolism with a pure-integer uptake→convert→excrete pass reading PoolStock + the species' cached Strategy{budget,role,affinity}. DELETE the next_u64 draw in metabolism — metabolism becomes RNG-FREE.\n` +
  `3. Add Biomass(i64) + Age(u32) components.\n` +
  `4. Replace constant-N Wright-Fisher selection() with energy-funded reproduce_or_die: death FIRST (starvation after the maintenance debit + senescence at AGE_MAX; carcass Biomass+residual Energy → PoolStock[cell].detritus; despawn via a collected Vec<Entity>, NEVER mutate-during-query), then birth (Energy ≥ REPRO_THRESHOLD → parent spends OFFSPRING_ENDOWMENT, conserved transfer; child inherits Species off-stream + Genotype/DroughtTol/ThermalTol with the SAME per-birth inheritance draw-shape as today; births are the ONLY SimRng consumer; a skipped/over-cap birth does NOT consume the endowment).\n` +
  `5. u64 OrgId via a monotonic NextOrgId resource (NEVER reuse slot indices).\n` +
  `6. MaxPopulation guard set FAR above any resource-supportable equilibrium, with a test asserting it is NEVER hit in the pinned config (provably non-load-bearing).\n` +
  `7. Wire ledger.closes() to ASSERT conservation EVERY tick: Σ(PoolStock + per-org Energy + per-org Biomass) == initial + influx − respired − overflow; the overflow tap is explicit (no silent saturating destruction).\n\n` +
  `ALL hash-path arithmetic i64/fixed-point (NO float), every order-dependent pass pre-sorted by (cell_index, SpeciesId, OrgId), no HashMap/Query-order iteration. Fold Biomass + PoolStock + ledger taps into hash_world in a fixed order; population is now emergent so the hash must tolerate a varying N. Update/expand the tests (but do NOT touch the pinned literal yet — the Repin phase does that). Do NOT commit. Report exactly what you changed, file:line.`,
  { label: 'impl', phase: 'Implement', agentType: 'implementer' },
)

phase('Repin')
const repin = await agent(
  `The F3 logic is implemented. Perform the DELIBERATE RE-PIN of the determinism golden master in crates/sim-core/src/lib.rs::determinism_hash_is_pinned:\n` +
  `1. Build, then obtain the NEW hash run_headless now produces for the pinned cfg (seed 13_679_457_532_755_275_413, 50 gens, 1000 orgs) — e.g. run the determinism test with --nocapture; the old assert FAILS and reports the actual value, or print it from a scratch check.\n` +
  `2. STABILITY CHECK (critical): run the canonical run TWICE and confirm byte-identical (same_config_same_hash). If it differs run-to-run, STOP — there is a real non-determinism bug (uninit order, HashMap, float). Report it and do NOT re-pin.\n` +
  `3. Update the literal 0xf795_eac4_112f_acd5 → the new value, and APPEND one ledger comment line to the existing chain: "<new>… after ADR-013 F3 (energy-funded births/deaths replace constant-N Wright-Fisher; PoolStock i64 uptake/convert/excrete; ledger closes every tick; metabolism RNG draw deleted → births sole RNG consumer; Biomass+Age folded; OrgId→u64)."\n` +
  `4. State explicitly in your report: this value is the hash on THIS arch (aarch64/Apple M); cross-platform (x86_64) portability is validated by the multi-ISA CI gate on push, NOT locally. Do NOT commit. Report old→new hash + the run-to-run stability result.`,
  { label: 'repin', phase: 'Repin', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. The determinism gate MUST now be GREEN against the re-pinned literal (determinism-multi-isa stays SKIP locally — that's expected). Report all gates PASS/FAIL. If determinism is RED, the re-pin value is wrong or the pipeline is non-deterministic — report the exact mismatch. Do NOT fix by weakening anything, do NOT commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['integer_only_hash_path', 'ordered_iteration', 'draw_schedule_stable', 'ledger_closes_every_tick', 'repin_legitimate', 'issues'],
  properties: {
    integer_only_hash_path: { type: 'boolean', description: 'no float folded into hash_world; uptake/convert/excrete/birth/death are i64/fixed-point' },
    ordered_iteration: { type: 'boolean', description: 'all passes sort by (cell,SpeciesId,OrgId); no HashMap/Query-order dependence in births/deaths' },
    draw_schedule_stable: { type: 'boolean', description: 'metabolism draws zero RNG; births are the sole consumer with a FIXED per-birth draw count; only the NUMBER of births varies' },
    ledger_closes_every_tick: { type: 'boolean', description: 'ledger.closes() asserted each tick; overflow tap explicit; no silent joule create/destroy' },
    repin_legitimate: { type: 'boolean', description: 'the new hash is run-to-run stable on this arch; the re-pin captures a real deliberate change, NOT masking a bug' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify the gene-sim F3 re-pin is LEGITIMATE determinism, not a papered-over bug. Read \`git diff\` + the new hash_world / metabolism / reproduce_or_die / ledger code. Skeptic #${i}, default each boolean to false if you cannot confirm it. Hunt: a float on the hash path, HashMap/Query-order iteration in birth/death, a non-monotonic or reused OrgId, an unstable per-birth draw count (the historical 2N/gen invariant is gone — is the new schedule still deterministic?), a ledger that doesn't actually assert closure, any run-to-run instability.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA },
  ),
))).filter(Boolean)
const ok = skeptics.filter((s) => s.repin_legitimate && s.integer_only_hash_path && s.ledger_closes_every_tick && s.ordered_iteration).length
return {
  impl,
  repin,
  gate: typeof gate === 'string' ? gate.slice(0, 400) : gate,
  skeptics,
  verdict: ok >= 2 ? 'F3 RE-PIN CONFIRMED (multi-ISA portability pending CI on push)' : 'NEEDS WORK — re-pin not confirmed',
}
