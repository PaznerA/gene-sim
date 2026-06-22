export const meta = {
  name: 'sp1-core-tuning-impl',
  description:
    'SP-1 core tuning for EMERGENT RICHNESS: tune the full 3-species (plant + E.coli decomposer + Bdellovibrio predator) + chem + predation system so the sandbox produces VISIBLY INTERESTING emergent dynamics — predator-prey oscillation, a demonstrable trophic cascade, spatial chem/allelopathy patterns, and DYNAMIC (not flat, not collapsed, not capped) long-run coexistence. Measure → tune → conditional RE-PIN (if plant-affecting constants change) → verify the dynamics are interesting + non-degenerate + deterministic.',
  whenToUse:
    'After the predator merge. The first SP (sandbox) pillar — a well-tuned core is the foundation of a satisfying sandbox. May move the determinism hash (conditional); multi-ISA validated by CI on push.',
  phases: [
    { title: 'Tune' },
    { title: 'Repin' },
    { title: 'Gate' },
    { title: 'Verify' },
  ],
}

phase('Tune')
const TSCHEMA = {
  type: 'object',
  required: ['changed', 'diagnosis', 'adjustments', 'emergence_evidence'],
  properties: {
    changed: { type: 'boolean', description: 'true IFF constants were changed (left applied); false IFF the system is already richly emergent and you reverted any probe (tree byte-clean)' },
    diagnosis: { type: 'string', description: 'the current 3-species + chem long-run dynamics: is coexistence DYNAMIC (oscillation/fluctuation) or flat? is there a visible predator-prey cycle? a working trophic cascade? spatial chem/allelopathy patterns? any degeneracy (flat lines, collapse, runaway-to-cap, frozen)?' },
    adjustments: { type: 'string', description: 'the constant changes name=old→new (chem yield/decay/sense, predation efficiency/rate, metabolic) and WHY each makes the emergence more legible/interesting — or "none"' },
    emergence_evidence: { type: 'string', description: 'concrete measured evidence the tuned system is interesting: oscillation amplitude/period, cascade magnitude (throttle one species → measured ripple), spatial pattern, coexistence variability — over the long run' },
  },
}
const tune = await agent(
  `SP-1: tune the gene-sim core for EMERGENT RICHNESS in the sandbox. The full system is LANDED: F3 energy-funded lifecycle, F4 trophic loop, F3.4 tuning, F5 chem field (toxin/kin/alarm), and the Bdellovibrio predator (trophic::predation). The earlier "light balancing" only confirmed the 2-species coexistence was ALIVE — SP-1's bar is higher: the 3-species + chem sandbox should be VISIBLY INTERESTING to watch.\n\n` +
  `READ the constants in crates/sim-core/src/{lib.rs (F3.4 metabolic + EFF), chem.rs (DIFFUSE/DECAY/TOXIN_YIELD/sense), trophic.rs (predation efficiency/rate, mineralize)} + the species specs.\n\n` +
  `MEASURE, don't guess: run the plant+E.coli(decomposer)+Bdellovibrio(predator) roster over ~2000-5000 generations (harness per-gen CSV / scratch run_headless), reading per-species population + chem levels + the FlowMatrix + ledger. Assess INTERESTINGNESS (measurable proxies): (a) DYNAMIC coexistence — populations FLUCTUATE/oscillate, not flat lines (predator-prey cycle with a legible amplitude/period); (b) a demonstrable TROPHIC CASCADE (throttle the predator → E.coli rises → mineralization rises → plant rises, measurably); (c) spatial chem/allelopathy structure (toxin gradients matter); (d) NON-DEGENERATE — no flat lines, no instant collapse, no runaway-to-cap, no frozen state. If it's already richly emergent → changed=false, REVERT any probe (\`git checkout -- ...\`), report the evidence. If it's flat/degenerate/dull → LIGHTLY but purposefully tune the chem/predation/metabolic constants (integer/fixed-point, no new RNG, ordering unchanged) toward legible oscillation + cascade + spatial structure, set changed=true, LEAVE applied, do NOT touch the pinned literal (the Repin phase owns it). Do NOT commit. Report via the schema.`,
  { label: 'tune', phase: 'Tune', agentType: 'implementer', schema: TSCHEMA },
)

if (tune && tune.changed) {
  phase('Repin')
  const repin = await agent(
    `SP-1 changed constants. Determine + perform the re-pin (current literal 0x47a0_3c8f_6701_f240): build, get the new run_headless hash for the pinned cfg, confirm run-to-run stable (twice/3 processes). If it EQUALS 0x47a0 → HASH-NEUTRAL (only predator/multi-species constants changed, which the single-species pinned run doesn't exercise); leave unchanged, report HASH-NEUTRAL. If it DIFFERS → RE-PIN: update the literal + append a ledger line "<new>… after SP-1 core tuning (chem/predation/metabolic constants tuned for emergent richness: <one line>)." aarch64 value; x86_64 is CI's job. Do NOT commit. Report outcome + stability.`,
    { label: 'repin', phase: 'Repin', agentType: 'implementer' },
  )
  phase('Gate')
  const gate = await agent(`Run \`bash tools/gate.sh\`. determinism GREEN against the (re-pinned or unchanged) literal; all gates. Report PASS/FAIL. No commit.`, { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' })
  phase('Verify')
  const VS = {
    type: 'object',
    required: ['dynamic_coexistence', 'cascade_real', 'non_degenerate', 'deterministic', 'issues'],
    properties: {
      dynamic_coexistence: { type: 'boolean', description: 'the 3-species + chem long run shows fluctuating/oscillating coexistence, not flat lines' },
      cascade_real: { type: 'boolean', description: 'a measured trophic cascade exists (throttle one species → ripple through the others)' },
      non_degenerate: { type: 'boolean', description: 'no flat-line / collapse / runaway-to-cap / frozen degeneracy at the shipped defaults' },
      deterministic: { type: 'boolean', description: 'new hash run-to-run stable; integer/ordered; no new RNG' },
      issues: { type: 'array', items: { type: 'string' } },
    },
  }
  const verify = await agent(`Adversarially verify the SP-1 tuning: read git diff + run the 3-species long trajectory yourself (~3000 gens). Confirm the coexistence is genuinely DYNAMIC (not flat), the cascade is real, nothing is degenerate, and determinism holds. Default false if unconfirmable.`, { label: 'verify', phase: 'Verify', schema: VS, agentType: 'reviewer' })
  return { outcome: 'TUNED', tune, repin, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, verify }
} else {
  log('3-species + chem dynamics already richly emergent — no change, hash unchanged')
  return { outcome: 'ALREADY RICH — no change, hash unchanged', tune }
}
