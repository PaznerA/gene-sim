export const meta = {
  name: 'f3-4-chemostat-tuning-impl',
  description:
    'F3.4 chemostat tuning + conditional RE-PIN: search the F3/F4 metabolic constants for a bounded NON-ZERO equilibrium so the default ecosystem stops sliding to extinction (~gen 240). If a clean equilibrium is found → apply + re-pin + gate + verify. If not (time-boxed) → revert to leave the tree byte-clean and DEFER to the continuation roadmap, so the merge is never blocked.',
  whenToUse:
    'After F4. Makes the shipped ecosystem demoable rather than dead-on-arrival. Iterative single-agent search (parallelism does not help a feedback-driven search); may move the determinism hash.',
  phases: [
    { title: 'Tune' },
    { title: 'Repin' },
    { title: 'Gate' },
    { title: 'Verify' },
  ],
}

phase('Tune')
const TUNE_SCHEMA = {
  type: 'object',
  required: ['succeeded', 'chosen_constants', 'trajectory', 'rationale'],
  properties: {
    succeeded: { type: 'boolean', description: 'true IFF a bounded NON-ZERO equilibrium was found AND applied to the source (constants left edited); false IFF no clean equilibrium found AND all edits reverted (tree byte-clean)' },
    chosen_constants: { type: 'string', description: 'the constant set applied (name=old→new), or the best-found if not succeeded' },
    trajectory: { type: 'string', description: 'population (+ pools, + per-species coexistence) trajectory over the long run at the chosen set' },
    rationale: { type: 'string', description: 'the dominant imbalance and how the chosen set fixes it' },
  },
}
const tune = await agent(
  `Tune the gene-sim F3/F4 chemostat so the DEFAULT ecosystem reaches a bounded NON-ZERO equilibrium instead of sliding to extinction (~gen 240). READ crates/sim-core/src/lib.rs F3 constants (CELL_J_SCALE, POOL_CAP, SOLAR_PER_CELL, UPTAKE_VMAX/K_HALF, MAINTENANCE_BASE/FLOOR, AGE_MAX, REPRO_THRESHOLD, OFFSPRING_ENDOWMENT/SEED_BIOMASS, EFF_NUM/DEN, MAX_POPULATION), crates/sim-core/src/trophic.rs (mineralize, LITTERFALL), and docs/llm/proposals/f3-metabolism-keystone-draft.md + f4-trophic-decomposer-draft.md.\n\n` +
  `METHOD: measure, don't guess. Use the harness per-gen CSV (\`cargo run -p harness -- ...\`, with --species data/species/ecoli.json for the decomposer roster) and/or a scratch run_headless over ~1000 generations to get the population trajectory + pool levels. Diagnose the dominant imbalance (likely: the seeded pools dwarf the per-tick flows, and/or MAINTENANCE_BASE debit exceeds sustainable uptake, and/or REPRO_THRESHOLD is unreachable). Iteratively adjust the constants — keeping ALL arithmetic integer/fixed-point, NO new RNG, ordering unchanged — until: (a) the default plant config population STABILIZES in a sensible band (strictly >0, well below MAX_POPULATION) over ~1000 gens, AND (b) in the plant+E.coli(decomposer) roster BOTH species persist (coexistence — neither extinct), demonstrating the obligate loop sustains life rather than just delaying death.\n\n` +
  `TIME-BOX — this is a search, not a proof. If after a genuine, documented effort you cannot find a clean bounded-non-zero coexistence equilibrium: set succeeded=false, REVERT every edit (\`git checkout -- crates/sim-core/src/lib.rs crates/sim-core/src/trophic.rs\` and any other file you touched) so the tree is byte-identical to HEAD, and report the best-found set + a concrete recommendation for the continuation roadmap. If you DO find one: LEAVE the constants applied, do NOT touch the pinned literal (the Repin phase owns that), set succeeded=true. Either way do NOT commit. Report via the schema.`,
  { label: 'tune', phase: 'Tune', agentType: 'implementer', schema: TUNE_SCHEMA },
)

if (tune && tune.succeeded) {
  phase('Repin')
  const repin = await agent(
    `The chemostat tuning converged and the new constants are applied. Perform the deliberate RE-PIN in crates/sim-core/src/lib.rs::determinism_hash_is_pinned (current literal 0x42fe_54f2_f6d8_360d):\n` +
    `1. Build, obtain the new hash run_headless produces for the pinned cfg.\n` +
    `2. STABILITY CHECK: run the canonical run TWICE (ideally 3 processes) — confirm byte-identical. If it differs, STOP (non-determinism bug), report, do NOT re-pin.\n` +
    `3. Update 0x42fe_54f2_f6d8_360d → the new value; append one ledger line: "<new>… after ADR-013 F3.4 (chemostat constants tuned for a bounded non-zero coexistence equilibrium: <one line of what changed>)."\n` +
    `4. aarch64/Apple value; x86_64 is the multi-ISA CI gate's job on push. Do NOT commit. Report old→new + stability.`,
    { label: 'repin', phase: 'Repin', agentType: 'implementer' },
  )
  phase('Gate')
  const gate = await agent(
    `Run \`bash tools/gate.sh\`. The determinism gate MUST be GREEN against the re-pinned literal. Report all gates PASS/FAIL. No fixes, no commit.`,
    { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
  )
  phase('Verify')
  const VS = {
    type: 'object',
    required: ['equilibrium_nonzero', 'coexistence', 'deterministic', 'not_degenerate', 'issues'],
    properties: {
      equilibrium_nonzero: { type: 'boolean', description: 'default config population stabilizes >0 over a long run, below MAX_POPULATION' },
      coexistence: { type: 'boolean', description: 'plant + E.coli decomposer both persist (neither extinct) in the roster run' },
      deterministic: { type: 'boolean', description: 'new hash run-to-run stable; integer/ordered; no new RNG' },
      not_degenerate: { type: 'boolean', description: 'the equilibrium is not pinned-at-MAX or wildly oscillating; it is a sensible living band' },
      issues: { type: 'array', items: { type: 'string' } },
    },
  }
  const verify = await agent(
    `Adversarially verify the F3.4 tuning re-pin. Read git diff + run the long trajectory yourself (harness per-gen CSV over ~1000 gens, default AND ecoli roster). Try to REFUTE: that the equilibrium is genuinely bounded-non-zero (NOT just slower extinction), that coexistence holds, that it isn't degenerate (pinned-at-MAX/oscillating), and that determinism is intact. Default each false if unconfirmable.`,
    { label: 'verify', phase: 'Verify', schema: VS, agentType: 'reviewer' },
  )
  return { outcome: 'TUNED+REPINNED', tune, repin, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, verify }
} else {
  log('chemostat tuning did not converge to a clean equilibrium — reverted, tree clean, deferring to continuation')
  return { outcome: 'DEFERRED — no clean equilibrium; constants reverted, hash unchanged, F3.4 goes to continuation item #1', tune }
}
