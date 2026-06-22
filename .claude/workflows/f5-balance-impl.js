export const meta = {
  name: 'f5-balance-impl',
  description:
    'Light balancing pass over the combined F3.4 + F5 (chem field) dynamics: measure the long-run multi-species + chem trajectory, diagnose any imbalance the chem field introduced (toxin allelopathy runaway / chem-induced collapse / dead toxin sink), and LIGHTLY tune the chem (yield/decay/sense strengths) + metabolic constants for a healthy LIVING ecosystem with interesting emergence (allelopathic pressure that does not sterilize). Conditional RE-PIN: if constants change → re-pin; if already healthy → leave unchanged (hash-neutral).',
  whenToUse:
    'After F5. Confirms the chem field did not destabilize the F3.4 coexistence, and lightly tunes for gameplay. May move the determinism hash; multi-ISA validated by CI on push.',
  phases: [
    { title: 'Balance' },
    { title: 'Repin' },
    { title: 'Gate' },
    { title: 'Verify' },
  ],
}

phase('Balance')
const BSCHEMA = {
  type: 'object',
  required: ['changed', 'diagnosis', 'adjustments', 'trajectory'],
  properties: {
    changed: { type: 'boolean', description: 'true IFF you applied constant changes (left edited); false IFF the F3.4+F5 system was already healthy and you reverted any probe so the tree is byte-clean' },
    diagnosis: { type: 'string', description: 'the long-run F3.4+F5 dynamics: does plant+decomposer coexistence still hold with chem active? what did toxin/kin/alarm do (allelopathy pressure, kin stabilization, dispersal)? any runaway/collapse?' },
    adjustments: { type: 'string', description: 'the LIGHT constant changes (chem yield/decay shifts/sense strengths and/or metabolic constants) name=old→new, or "none"' },
    trajectory: { type: 'string', description: 'population (+ chem levels + per-species coexistence) over the long run at the final constants' },
  },
}
const balance = await agent(
  `LIGHT balancing of the gene-sim combined F3.4 + F5 (chem) dynamics. F5 (toxin/kin/alarm chem field) just landed on top of the F3.4-tuned coexistence — verify it didn't destabilize the ecosystem, and lightly tune for healthy, interesting emergence (NOT a heavy sweep). READ crates/sim-core/src/chem.rs (the chem constants: DIFFUSE_SHIFT/DECAY_SHIFT/TOXIN_YIELD/sense strengths/CHEM_CAP) + the F3.4 metabolic constants in lib.rs.\n\n` +
  `MEASURE, don't guess: run the plant+E.coli(decomposer) roster over ~1500-3000 generations (harness per-gen CSV / a scratch run_headless), reading per-species population + chem levels + ledger. Diagnose: does coexistence STILL hold with chem active? Does toxin allelopathy cause a runaway (one species poisons the other to extinction) or a dead toxin sink? Is kin/alarm doing anything legible? If the system is healthy + interesting → set changed=false, REVERT any probe edits (\`git checkout -- ...\`) so the tree is byte-clean, report. If it needs a LIGHT nudge (a couple of chem/metabolic constants) for a living, non-sterilizing equilibrium → apply the minimal changes (integer/fixed-point, no new RNG, ordering unchanged), set changed=true, LEAVE them applied, do NOT touch the pinned literal (the Repin phase owns it). Do NOT commit. Report via the schema.`,
  { label: 'balance', phase: 'Balance', agentType: 'implementer', schema: BSCHEMA },
)

if (balance && balance.changed) {
  phase('Repin')
  const repin = await agent(
    `The balancing changed constants. Determine + perform the re-pin (current literal 0x47a0_3c8f_6701_f240): build, get the new run_headless hash for the pinned cfg, confirm run-to-run stable (twice/3 processes). If it differs → update the literal + append a ledger line "<new>… after F5 balancing (chem/metabolic constants tuned for a living allelopathic ecosystem: <one line>)." If it equals (unlikely if constants changed) → leave unchanged + report HASH-NEUTRAL. aarch64 value; x86_64 is CI's job. Do NOT commit. Report old→new + stability.`,
    { label: 'repin', phase: 'Repin', agentType: 'implementer' },
  )
  phase('Gate')
  const gate = await agent(`Run \`bash tools/gate.sh\`. determinism GREEN against the (re-pinned or unchanged) literal; all gates. Report PASS/FAIL. No commit.`, { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' })
  phase('Verify')
  const VS = {
    type: 'object',
    required: ['coexistence_holds', 'chem_emergence_real', 'deterministic', 'not_sterile', 'issues'],
    properties: {
      coexistence_holds: { type: 'boolean', description: 'plant + decomposer both persist over a long run with chem active' },
      chem_emergence_real: { type: 'boolean', description: 'toxin allelopathy / kin / alarm produce a legible effect (not inert, not sterilizing)' },
      deterministic: { type: 'boolean', description: 'new hash run-to-run stable; integer/ordered; no new RNG' },
      not_sterile: { type: 'boolean', description: 'the ecosystem is alive (not collapsed to extinction by chem)' },
      issues: { type: 'array', items: { type: 'string' } },
    },
  }
  const verify = await agent(`Adversarially verify the F5 balancing re-pin: read git diff + run the long trajectory yourself (roster, ~2000 gens). Confirm coexistence holds with chem, the chem effects are real-but-not-sterilizing, and determinism is intact. Default false if unconfirmable.`, { label: 'verify', phase: 'Verify', schema: VS, agentType: 'reviewer' })
  return { outcome: 'BALANCED+REPINNED', balance, repin, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, verify }
} else {
  log('F3.4+F5 dynamics already healthy — no constant change, tree clean, hash unchanged')
  return { outcome: 'ALREADY HEALTHY — no change, hash unchanged', balance }
}
