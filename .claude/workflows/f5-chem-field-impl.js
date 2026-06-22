export const meta = {
  name: 'f5-chem-field-impl',
  description:
    'ADR-013 F5 chemical/signal diffusion field + deliberate RE-PIN: a double-buffered ChemField (toxin/kin/alarm, i32 milli-units per cell) with conserved 4-neighbour diffusion (Σ-before==Σ-after) + decay, emit (organisms spend J to signal), and sense couplings (toxin suppresses uptake, kin boosts kin survival, alarm biases dispersal). Folds chem into hash_world + ledger closure; GSS3→GSS4 snapshot bump (decoder same commit). Implements the Pillar-4 design from docs/llm/proposals/ecology-substrate-draft.md on top of the landed F3/F4/F3.4 pipeline.',
  whenToUse:
    'Big sim push. Adds allelopathy / kin-cooperation / chemotaxis as emergent behaviour on the trophic web. Deliberate determinism re-pin (executed); multi-ISA validated by CI on push.',
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
  required: ['channels', 'diffusion', 'emit', 'sense', 'units_ledger', 'ordering', 'gss_bump', 'open_questions'],
  properties: {
    channels: { type: 'string', description: 'ChemField{toxin,kin,alarm: Vec<i32> milli-units} + double buffer; where it lives + reset' },
    diffusion: { type: 'string', description: '4-neighbour stencil share=c>>diffuse_shift, remainder kept in-cell, reflecting boundary returns off-grid share to self → Σ-before==Σ-after asserted; then decay (a named tap)' },
    emit: { type: 'string', description: 'how organisms emit each signal (deterministic, integer): toxin = a producer role/budget spends J→toxin milli-units; kin = a per-org species marker; alarm = on a trigger (low energy / death). Pin the J↔milli-unit accounting so the ledger still closes.' },
    sense: { type: 'string', description: 'sense couplings at the org cell: toxin → suppress uptake + drain J (integer permille, like the soil/edit modifiers); kin → boost kin survival/uptake; alarm → bias the reproduce_or_die dispersal step WITHOUT changing the per-birth draw count (draw-count-neutral chemotaxis)' },
    units_ledger: { type: 'string', description: 'reconcile chem i32 milli-units with the i64-J ledger: is chem J-denominated (part of Σ) or a separate signal with a decay tap? Pin so ledger_closes holds every tick (Σ pools+chem+Energy+Biomass == initial+influx−respired−overflow)' },
    ordering: { type: 'string', description: 'the new pipeline order (e.g. advance→influx→diffuse+decay chem→emit chem→metabolism(sense toxin)→mineralize→reproduce_or_die(sense alarm)→assert_flow→assert_ledger); all passes sorted by (cell,SpeciesId,OrgId), integer only' },
    gss_bump: { type: 'string', description: 'GSS3→GSS4: add the 3 chem channels to the snapshot (off-hash) so they overlay in-game; the godot decoder + check scripts update in the same commit (inv #2)' },
    open_questions: { type: 'array', items: { type: 'string' } },
  },
}
const LENSES = [
  'conservation & determinism: the diffusion MUST assert Σ-before==Σ-after (reflecting boundary, integer share+remainder, ties→lowest), chem folds into hash_world row-major, and ledger_closes must still hold every tick with chem included; i32 milli-units, no float, no transcendental, no new HashMap iteration',
  'chem semantics & gameplay: toxin→allelopathic warfare (poison neighbours, competitive exclusion), kin→cooperation (kin-selection boosts), alarm→chemotaxis (dispersal bias) — each a legible emergent behaviour grounded in the budget/role; emit cost in J keeps it honest',
  'units reconciliation with the ledger: pin exactly how i32 chem milli-units relate to the i64-J ledger (J-denominated and part of Σ, with decay as a named respired/overflow tap) so the conservation identity is exact and provable',
]
const proposals = (await parallel(LENSES.map((lens, i) => () =>
  agent(
    `Design gene-sim ADR-013 F5 "chemical/signal diffusion field" through this lens: ${lens}.\n\n` +
    `READ docs/llm/proposals/ecology-substrate-draft.md (Pillar 4 / the chem field + diffusion + emit/sense + the ledger conservation + the hash fold) IN FULL, then the CURRENT landed pipeline in crates/sim-core/src/lib.rs (schedule: advance_tick→reset_flow→solar_influx→metabolism→mineralize→reproduce_or_die→assert_flow_closes→measure_and_assert_ledger), crates/sim-core/src/{trophic.rs,ledger.rs,fixed.rs,snapshot.rs}, and how the EditModifier/soil/climate integer-permille modulation seams work (the precedent for sense couplings). F3/F4/F3.4 have LANDED. F5 inserts the chem field + diffuse/decay/emit stages + the sense couplings, folds chem into hash_world + ledger closure, and bumps GSS3→GSS4. This is a deliberate determinism RE-PIN. Integer/fixed-point only, ordered by (cell,SpeciesId,OrgId), no HashMap, no new transcendental.\n\n` +
    `Return a concrete file-level design. Do NOT write code.`,
    { label: `design:lens${i}`, phase: 'Design', schema: DSCHEMA },
  ),
))).filter(Boolean)
const chosen = await agent(
  `Judge & synthesize these ${proposals.length} F5 designs into ONE plan. Pin the 3 channels, the conserved diffusion + decay, the emit J-accounting, the 3 sense couplings, the units/ledger reconciliation, the pipeline order, and the GSS4 bump. Integer/ordered/conserved. Output the final design.\n` +
    proposals.map((p, i) => `\n--- Design ${i} ---\n${JSON.stringify(p, null, 2)}`).join('\n'),
  { label: 'design:judge', phase: 'Design', schema: DSCHEMA },
)

phase('Implement')
const impl = await agent(
  `Implement gene-sim ADR-013 F5 per this agreed design, on top of the landed F3/F4/F3.4 pipeline:\n${JSON.stringify(chosen, null, 2)}\n\n` +
  `Build the ChemField (double-buffered, i32 milli-units, toxin/kin/alarm), the conserved diffuse+decay stage (assert Σ-before==Σ-after for diffusion; decay is a named tap), the emit stage (organisms spend J deterministically), and the 3 sense couplings (toxin suppresses uptake via an integer-permille demand factor like the EditModifier; kin boosts; alarm biases dispersal draw-count-neutrally). Fold chem into hash_world (row-major) and into the ledger_closes assertion (Σ now includes chem). Bump GSS3→GSS4 + update the godot decoder (godot/snapshot.gd) + check_godot_snapshot + the livesim smoke test's magic/channel-count IN THE SAME COMMIT (inv #2). ALL integer/fixed-point, ordered, no HashMap, no new RNG draw in the diffusion/emit/sense (alarm must NOT change the per-birth draw count). Add tests: diffusion Σ-conservation, ledger_closes with chem, a toxin allelopathy functional test (a toxin-producer suppresses a neighbour), determinism run-to-run. Do NOT touch the pinned literal yet (Repin phase). Do NOT commit. Report file:line.`,
  { label: 'impl', phase: 'Implement', agentType: 'implementer' },
)

phase('Repin')
const repin = await agent(
  `F5 is implemented. Perform the deliberate RE-PIN in crates/sim-core/src/lib.rs::determinism_hash_is_pinned (current literal 0x4e4d_0520_722a_a069):\n` +
  `1. Build, get the new hash run_headless produces for the pinned cfg.\n` +
  `2. STABILITY: run twice (ideally 3 processes), confirm byte-identical. If it differs run-to-run, STOP (non-determinism bug — likely diffusion order or a float), report, do NOT re-pin.\n` +
  `3. Update the literal → new value + append a ledger line "<new>… after ADR-013 F5 (toxin/kin/alarm chem field: conserved 4-neighbour diffusion + decay, emit costs J, sense couplings suppress-uptake/boost-kin/bias-dispersal; chem folded into hash + ledger)."\n` +
  `4. aarch64/Apple value; x86_64 is the multi-ISA CI gate's job on push. Do NOT commit. Report old→new + stability.`,
  { label: 'repin', phase: 'Repin', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\`. determinism MUST be GREEN against the re-pinned literal; godot-reader/livesim must pass with GSS4. Report all gates PASS/FAIL with any exact error. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['diffusion_conserved', 'ledger_closes_with_chem', 'integer_ordered', 'draw_count_stable', 'allelopathy_real', 'repin_stable', 'issues'],
  properties: {
    diffusion_conserved: { type: 'boolean', description: 'diffusion asserts Σ-before==Σ-after every tick; reflecting boundary; integer share+remainder ties→lowest' },
    ledger_closes_with_chem: { type: 'boolean', description: 'ledger_closes still asserted every tick with chem in Σ; decay is a named tap; no silent leak' },
    integer_ordered: { type: 'boolean', description: 'chem/emit/sense are i32/i64 integer, ordered by (cell,SpeciesId,OrgId), no float on hash path, no HashMap iteration' },
    draw_count_stable: { type: 'boolean', description: 'alarm/dispersal bias does NOT change the per-birth RNG draw count; no new RNG in diffusion/emit/sense' },
    allelopathy_real: { type: 'boolean', description: 'a functional test shows a toxin producer measurably suppresses a neighbour (emergent warfare), not cosmetic' },
    repin_stable: { type: 'boolean', description: 'the new hash is run-to-run stable; the re-pin is ledgered' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify gene-sim F5. Read git diff + the chem diffusion/emit/sense code + the conservation/ledger tests. Skeptic #${i}, default each boolean false if unconfirmable. Hunt: diffusion that doesn't conserve (Σ before≠after) or isn't reflecting at the boundary; a float/transcendental on the hash path; chem breaking ledger_closes; a new RNG draw or a per-birth draw-count change from alarm; HashMap iteration; run-to-run hash instability; a "toxin" that doesn't actually suppress neighbours.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA },
  ),
))).filter(Boolean)
const ok = skeptics.filter((s) => s.diffusion_conserved && s.ledger_closes_with_chem && s.integer_ordered && s.repin_stable).length
return { chosen, impl, repin, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, skeptics, verdict: ok >= 2 ? 'F5 RE-PIN CONFIRMED — chem field live' : 'NEEDS WORK' }
