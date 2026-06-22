export const meta = {
  name: 'oversight-modifier-activation-impl',
  description:
    'ADR-017 OVERSIGHT loop — Slice B (S6 ACTIVATION): replace the identity (1.0×) commit modifier with the real EcoliEditModifier — map a committed KO growth-ratio + EditKind to a [0.5,1.5] selection factor and apply it to the edited species in selection(), so an earned E. coli edit MEASURABLY ripples (gltA KO → E. coli falls → via the F4 loop nutrient drops → plants respond). CONDITIONAL re-pin: the pinned config is a single-species plant with no committed edit, so this may be hash-neutral; the workflow computes the hash and re-pins ONLY if it moves.',
  whenToUse:
    'After oversight-loop-plumbing-impl (Slice A). Activates the earned-edit payoff. May or may not move the determinism hash (the pinned run exercises no edit); the Repin phase decides empirically. Multi-ISA validated by CI on push.',
  phases: [
    { title: 'Implement' },
    { title: 'Repin' },
    { title: 'Gate' },
    { title: 'Verify' },
  ],
}

phase('Implement')
const impl = await agent(
  `Implement ADR-017 OVERSIGHT Slice B (S6 activation) for gene-sim. READ docs/llm/proposals/ecoli-oversight-gameloop-draft.md (the EcoliEditModifier section), crates/harness/src/{firewall.rs,oversight.rs,lib.rs} (Slice A — the CommitEcoliImpact step arm is currently an empty identity no-op), crates/oracle-fba/src/lib.rs (the quantized KO ratio), and crates/sim-core/src/lib.rs (selection(); the existing soil/climate fitness factors are the precedent for a strictly-positive [0.5,1.5] selection modifier; SpeciesEntry; the EditKind in crates/crispr).\n\n` +
  `Wire the REAL modifier:\n` +
  `1. Map a committed impact → a strictly-positive factor in [0.5, 1.5]: from the KO growth_ratio_q (u16 permille, 1000=WT) + the EditKind {Knockout/Knockdown/Activate}. e.g. Knockout/Knockdown ratio q → factor = 0.5 + 0.5*(q/1000) (q=1000→1.0 neutral, q=0→0.5 strong penalty); Activate edits may lift above 1.0 toward 1.5. Pin the exact mapping; keep it integer/fixed-point (no transcendentals) and clamp to [0.5,1.5].\n` +
  `2. Thread the active per-species edit factor from the harness firewall commit into the Simulation (a per-species edit-modifier the core's selection() reads), the SAME way climate/soil modifiers reach selection. Apply it multiplicatively in the per-organism fitness for the EDITED species only. Default = neutral 1.0 for species with no committed edit.\n` +
  `3. Fill the CommitEcoliImpact step arm (currently the empty identity no-op from Slice A) to set that factor on commit, deterministically, at the due_epoch.\n` +
  `4. FUNCTIONAL TEST (the payoff proof): a committed gltA KO (ratio 0) on a plant+E.coli(decomposer) roster measurably DROPS the E. coli population over N generations vs. a no-edit control, AND the plant population responds via the F4 loop (weakened decomposer → less mineralization → free_nutrient drops → plants decline). Deterministic, run-to-run stable.\n\n` +
  `ALL hash-path arithmetic integer/fixed-point, ordered by (cell/SpeciesId/OrgId), no HashMap iteration, no new RNG, no wall-clock. Do NOT touch the pinned literal yet (the Repin phase owns it). Do NOT commit. Report what you changed + whether you expect the pinned-config hash to move (the pinned run is a single-species PLANT with NO committed edit — if the modifier is a clean multiplicative factor defaulting to 1.0 and is NOT folded into hash_world for the no-edit case, the pinned hash should be UNCHANGED).`,
  { label: 'impl', phase: 'Implement', agentType: 'implementer' },
)

phase('Repin')
const repin = await agent(
  `Slice B is implemented. Determine whether the determinism golden master moved (crates/sim-core/src/lib.rs::determinism_hash_is_pinned, current literal 0x4e4d_0520_722a_a069):\n` +
  `1. Build, run run_headless on the pinned cfg (seed 13_679_457_532_755_275_413, 50 gens, 1000 entities), get the hash.\n` +
  `2. STABILITY: run it twice (ideally 3 processes) — confirm byte-identical.\n` +
  `3a. If the new hash EQUALS 0x4e4d_0520_722a_a069 → the activation is HASH-NEUTRAL for the pinned config (the pinned run commits no E. coli edit). Leave the literal UNCHANGED. Report "HASH-NEUTRAL".\n` +
  `3b. If it DIFFERS → deliberate RE-PIN: update the literal to the new value + append a ledger line "<new>… after ADR-017 S6 (EcoliEditModifier activated; committed KO growth-ratio → [0.5,1.5] selection factor on the edited species)." Report old→new.\n` +
  `Either way the value is the aarch64/Apple hash; x86_64 is the multi-ISA CI gate's job on push. Do NOT commit. Report the outcome + the stability result.`,
  { label: 'repin', phase: 'Repin', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\`. determinism MUST be GREEN against whatever the literal now is (unchanged if hash-neutral, or the new re-pinned value). license/livesim/godot-reader green. Report all gates PASS/FAIL with any exact error. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['modifier_deterministic', 'ripple_is_real', 'strictly_positive_bounded', 'no_wallclock_no_rng', 'repin_consistent', 'issues'],
  properties: {
    modifier_deterministic: { type: 'boolean', description: 'the KO-ratio→factor mapping is integer/fixed-point, ordered, run-to-run stable; no transcendental' },
    ripple_is_real: { type: 'boolean', description: 'a committed gltA KO measurably drops E. coli AND ripples to plants via the F4 loop (functional test exists + passes)' },
    strictly_positive_bounded: { type: 'boolean', description: 'the factor is clamped to [0.5,1.5] (no extinction-by-weight, no negative/zero fitness)' },
    no_wallclock_no_rng: { type: 'boolean', description: 'no Instant/SystemTime/now; no new SimRng draw; commit applied at the Tick-clocked due_epoch' },
    repin_consistent: { type: 'boolean', description: 'gate determinism is GREEN against the literal; if re-pinned, the new hash is run-to-run stable + ledgered; if hash-neutral, the literal is unchanged and justified' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify gene-sim OVERSIGHT Slice B (the EcoliEditModifier activation). Read git diff + the modifier mapping + the functional ripple test + the determinism outcome. Skeptic #${i}, default each boolean false if unconfirmable. Hunt: a non-integer/transcendental in the factor; a factor outside [0.5,1.5]; the modifier folded into hash_world for the no-edit case in a way that silently changed the pinned hash without a re-pin; any wall-clock/RNG; a "ripple test" that doesn't actually show the F4 coupling (E. coli down AND plants responding); run-to-run instability.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA },
  ),
))).filter(Boolean)
const ok = skeptics.filter((s) => s.modifier_deterministic && s.ripple_is_real && s.repin_consistent && s.no_wallclock_no_rng).length
return { impl, repin, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, skeptics, verdict: ok >= 2 ? 'SLICE B CONFIRMED — earned-edit ripple live' : 'NEEDS WORK' }
