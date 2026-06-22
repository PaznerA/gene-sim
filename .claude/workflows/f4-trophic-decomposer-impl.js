export const meta = {
  name: 'f4-trophic-decomposer-impl',
  description:
    'ADR-013 F4 IMPLEMENTATION + deliberate RE-PIN: the obligate plant→detritus→E.coli(decomposer)→free_nutrient loop (deletes F3 free_nutrient influx → nutrient endogenous) + emergent MEASURED FlowMatrix folded into the hash + E. coli re-roled Decomposer via niche.trophic_role. Implements the approved f4 draft, MOVES the pinned literal again, gates green, adversarially verifies, and exposes a read-only FlowMatrix export so the BATCH-2 Relations heatmap lights up.',
  whenToUse:
    'Run AFTER f3-metabolism-keystone-impl has landed. The vision payoff: the first real multi-species ecosystem. This MOVES the determinism hash again (a deliberate, ledgered re-pin). Chemostat constant-tuning (F3.4) comes after F4.',
  phases: [
    { title: 'Implement' },
    { title: 'Repin' },
    { title: 'Gate' },
    { title: 'Verify' },
  ],
}

phase('Implement')
const impl = await agent(
  `Implement gene-sim ADR-013 F4 EXACTLY per docs/llm/proposals/f4-trophic-decomposer-draft.md. READ that draft IN FULL first, then the CURRENT (post-F3) crates/sim-core/src/{lib.rs,ledger.rs,resource.rs,gp.rs,fixed.rs} and crates/genome/src/spec.rs, data/species/ecoli.json. F3 is LANDED (PoolStock i64, solar_influx, metabolism uptake→convert→excrete, reproduce_or_die, ledger asserted every tick, FlowMatrix-less). Build F4 ON TOP of it:\n` +
  `1. New crates/sim-core/src/trophic.rs holding the mineralize step + the FlowMatrix recorder; insert the system into the .chain() AFTER metabolism / around reproduce_or_die per the draft's ordering. Keep lib.rs edits minimal.\n` +
  `2. DELETE the free_nutrient arm of solar_influx (light-INFLUX stays — solar is the only true source); free_nutrient becomes ENDOGENOUS, supplied only by decomposer mineralization.\n` +
  `3. PLANTS deplete free_nutrient (autotroph uptake via Strategy.affinity[1]) + shed detritus on TWO arms: F3's carcass→detritus (already there) PLUS a continuous litterfall fraction of the excrete step → PoolStock[cell].detritus.\n` +
  `4. DECOMPOSER branch keyed on strategy.role==Decomposer: tap PoolStock[cell].detritus via affinity[2] (frozen start-of-tick snapshot, per-cell fixed::apportion contention), split granted J via split_budget — Maintenance/Defense fraction RESPIRED (the decomposer's own metabolism), the mineralization remainder re-deposited into the SAME cell's free_nutrient. The per-org mineralize fraction is a NEW Strategy.mineralize_rate:u16 permille expressed from the AcetateOverflow/pta (GO-8959) anchor.\n` +
  `5. Re-role E. coli: add #[serde(default)] niche.trophic_role: Option<String> to genome::spec::Niche (None → role_for fallback, byte-neutral for every existing spec), set "decomposer" in data/species/ecoli.json; reset_with_roster honours the override.\n` +
  `6. FlowMatrix{s, j: Vec<i64>} (flat row-major S×S, j[i*S+j] = NET J from species j into species i this generation), a Resource reset to ZERO at tick start, accumulated during the trophic transfer, row-sum conserved. FOLD it into hash_world in fixed order (this is part of the re-pin).\n` +
  `7. Add a READ-ONLY export so the renderer's Relations heatmap lights up: Simulation::flow_matrix() (or similar) → (s, flat i64) + GeneSimEnv passthrough + LiveSim::flow_matrix() #[func] returning the flat PackedInt64Array + s, matching the contract godot/relations_heatmap.gd already reads. Read-only, no extra hash impact beyond the FlowMatrix already folded.\n\n` +
  `ALL hash-path arithmetic i64/fixed-point (NO float), every order-dependent pass pre-sorted by (cell_index, SpeciesId, OrgId), no HashMap iteration. The ledger MUST still close every tick (mineralization is a detritus-debit / free_nutrient-credit + respired-tap move — conserves J). Update/expand tests (FlowMatrix row-sum==0, obligate loop: killing the decomposer starves the plants, mineralize_rate gene-driven). Do NOT touch the pinned literal yet (Repin phase). Do NOT commit. Report exactly what you changed, file:line.`,
  { label: 'impl', phase: 'Implement', agentType: 'implementer' },
)

phase('Repin')
const repin = await agent(
  `F4 logic is implemented. Perform the DELIBERATE RE-PIN in crates/sim-core/src/lib.rs::determinism_hash_is_pinned (current literal 0x272a_9b4a_7023_0cf5):\n` +
  `1. Build, obtain the NEW hash run_headless now produces for the pinned cfg.\n` +
  `2. STABILITY CHECK: run the canonical run TWICE (and ideally 3 processes) — confirm byte-identical. If it differs run-to-run, STOP (real non-determinism bug — likely FlowMatrix accumulation order or a float); report, do NOT re-pin.\n` +
  `3. Update 0x272a_9b4a_7023_0cf5 → the new value; append one ledger line: "<new>… after ADR-013 F4 (obligate plant→detritus→decomposer→free_nutrient loop; free_nutrient influx deleted → endogenous; E. coli re-roled Decomposer; emergent FlowMatrix S×S folded into hash; ledger still closes)."\n` +
  `4. State: this is the aarch64/Apple value; x86_64 portability is the multi-ISA CI gate's job on push. Do NOT commit. Report old→new hash + the stability result.`,
  { label: 'repin', phase: 'Repin', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\`. The determinism gate MUST be GREEN against the re-pinned literal; godot-reader/livesim MUST stay green (the new FlowMatrix #[func] must not break the gdext smoke). Report all gates PASS/FAIL. If determinism is RED, report the mismatch. Do NOT weaken anything, do NOT commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['flowmatrix_conserved', 'relations_measured', 'ordered_integer', 'obligate_loop_real', 'ledger_still_closes', 'repin_stable', 'issues'],
  properties: {
    flowmatrix_conserved: { type: 'boolean', description: 'FlowMatrix row-sum==0 (energy conserved per trophic column); reset to zero each tick' },
    relations_measured: { type: 'boolean', description: 'FlowMatrix entries are realized conserved-J transfers, NOT a fabricated cosine/embedding input' },
    ordered_integer: { type: 'boolean', description: 'trophic_transfer + mineralize sort by (cell,SpeciesId,OrgId); i64/fixed-point; no float on hash path; no HashMap iteration' },
    obligate_loop_real: { type: 'boolean', description: 'with free_nutrient influx deleted, killing the decomposer genuinely drains free_nutrient → plants starve (a real test exists)' },
    ledger_still_closes: { type: 'boolean', description: 'ledger.closes() still asserted every tick through the mineralization move' },
    repin_stable: { type: 'boolean', description: 'the new hash is run-to-run stable on this arch' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify the gene-sim F4 re-pin. Read \`git diff\` + trophic.rs + the new hash_world/ledger/mineralize code + the FlowMatrix tests. Skeptic #${i}, default each boolean to false if unconfirmable. Hunt: a FlowMatrix that doesn't row-sum to zero or isn't reset per tick; relations that are fabricated rather than measured; HashMap/order-dependence or a float in the trophic transfer; a ledger that silently leaks J through mineralization; an obligate loop that's actually cosmetic (decomposer death does NOT starve plants); run-to-run hash instability.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA },
  ),
))).filter(Boolean)
const ok = skeptics.filter((s) => s.repin_stable && s.flowmatrix_conserved && s.ledger_still_closes && s.ordered_integer).length
return {
  impl,
  repin,
  gate: typeof gate === 'string' ? gate.slice(0, 400) : gate,
  skeptics,
  verdict: ok >= 2 ? 'F4 RE-PIN CONFIRMED (multi-ISA pending CI; Relations view now live)' : 'NEEDS WORK — F4 re-pin not confirmed',
}
