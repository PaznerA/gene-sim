export const meta = {
  name: 'predator-bdellovibrio-impl',
  description:
    'ADR-013/ADR-017-S9 — the 3rd species: a Bdellovibrio bacteriovorus PREDATOR that preys on the E. coli decomposer, the first TRUE org-eats-org trophic flow. A deterministic InteractionKernel (predator consumes co-located prey J on a frozen snapshot, apportioned, conserved → predator gains J + efficiency-tax respired; carcass→detritus) writing real FlowMatrix predation off-diagonals (A[pred][prey]>0). Completes the chain plant→E.coli→Bdellovibrio (kill the predator → prey boom → mineralization boom → plant boom = a trophic cascade; an OVERSIGHT E. coli edit now also shifts the predator food supply). Conditional RE-PIN.',
  whenToUse:
    'After F5. The richest sim depth before the gameplay phase: emergent predator-prey dynamics + a trophic cascade on the conserved joule economy. May move the determinism hash (conditional, like F4/S6); multi-ISA validated by CI on push.',
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
  required: ['predator_role', 'interaction_kernel', 'conservation', 'flowmatrix', 'species_data', 'cascade', 'ordering', 'repin_expectation', 'open_questions'],
  properties: {
    predator_role: { type: 'string', description: 'add TrophicRole::Predator (or model Bdellovibrio as Heterotroph + a predation affinity)? how a species declares it preys; which roles are eligible PREY (Heterotroph/Decomposer = bacteria; NOT Autotroph plants — Bdellovibrio eats gram-negatives)' },
    interaction_kernel: { type: 'string', description: 'the predation system: predators in a cell consume prey J from the FROZEN start-of-tick prey population in that cell; demand from the predator Strategy/affinity; apportion among co-located predators via fixed::apportion (ties→lowest); a consumed prey org loses J (dies if J→0)' },
    conservation: { type: 'string', description: 'J is conserved: prey J → predator Energy (minus an efficiency-tax respired to the ledger); a killed prey org’s residual Biomass→detritus (the F3 death path); ledger_closes must still hold every tick; no silent leak' },
    flowmatrix: { type: 'string', description: 'predation writes the MEASURED FlowMatrix off-diagonal A[predator][prey] (net J predator gained from prey), row-sum==0 preserved (diagonal-pairing); the first true org-eats-org edge (vs F4 mineralization)' },
    species_data: { type: 'string', description: 'the Bdellovibrio SpeciesSpec: prefer a REAL NCBI genome bake (B. bacteriovorus HD100) like ecoli.json if feasible+quick, else a curated GO-anchored predator spec; data/species/bdellovibrio.json; the predator role/affinity declared via niche.trophic_role' },
    cascade: { type: 'string', description: 'the functional payoff: a plant+E.coli+Bdellovibrio roster shows predator-prey dynamics AND a trophic cascade (remove/throttle the predator → E. coli rises → mineralization rises → plant rises); an OVERSIGHT E. coli edit shifts the predator food supply' },
    ordering: { type: 'string', description: 'where the kernel sits in the schedule + the deterministic order (sort by cell,SpeciesId,OrgId; frozen snapshot; integer)' },
    repin_expectation: { type: 'string', description: 'will the PINNED single-species-plant config hash move? (the kernel is a no-op with no predator present — likely hash-neutral unless a new field folds into hash even at neutral; the Repin phase decides empirically)' },
    open_questions: { type: 'array', items: { type: 'string' } },
  },
}
const LENSES = [
  'ecology & biology: Bdellovibrio bacteriovorus periplasmic predation of E. coli (gram-negative bacteria, NOT plants); emergent predator-prey oscillation + the plant→E.coli→Bdellovibrio trophic cascade on the conserved joule economy',
  'determinism & conservation: the InteractionKernel must consume prey on a FROZEN snapshot, apportion among co-located predators (ties→lowest), conserve J exactly (prey J→predator − efficiency tax→respired; carcass→detritus), keep ledger_closes + FlowMatrix row-sum==0, integer/ordered, no new RNG; conditional re-pin',
  'species data & roster: the Bdellovibrio SpeciesSpec (real NCBI B. bacteriovorus HD100 genome bake if feasible, else curated GO-anchored predator spec), the Predator role declaration, the predation eligibility (prey = Heterotroph/Decomposer), and how a 3-species roster is wired',
]
const proposals = (await parallel(LENSES.map((lens, i) => () =>
  agent(
    `Design gene-sim's 3rd species — a Bdellovibrio PREDATOR (ADR-013 F4 InteractionKernel / ADR-017 S9) — through this lens: ${lens}.\n\n` +
    `Context (all LANDED): F3 energy-funded births/deaths; F4 the obligate plant→detritus→E.coli(decomposer)→nutrient loop + the MEASURED FlowMatrix (trophic.rs, row-sum==0, diagonal-pairing); F5 the chem field. The EXISTING trophic flow is mineralization (decomposer↔detritus) — there is NO org-eats-org predation yet. TrophicRole = {Autotroph,Heterotroph,Mixotroph,Decomposer} (no Predator). data/species has default.json (plant) + ecoli.json (decomposer, real 136-gene K-12). E. coli is the prey (gram-negative). Task: add a Bdellovibrio predator species + a deterministic predation InteractionKernel that is the FIRST true org-eats-org J flow, writing real FlowMatrix off-diagonals, conserving J, and producing an emergent predator-prey + trophic-cascade dynamic. READ crates/sim-core/src/{trophic.rs,lib.rs,gp.rs}, crates/genome/src/spec.rs, data/species/ecoli.json, scripts/bake_ecoli_species.py first.\n\n` +
    `Return a concrete file-level design. This is a deliberate determinism RE-PIN (conditional). Do NOT write code.`,
    { label: `design:lens${i}`, phase: 'Design', schema: DSCHEMA },
  ),
))).filter(Boolean)
const chosen = await agent(
  `Judge & synthesize these ${proposals.length} predator designs into ONE plan. Pin the predator role + prey eligibility, the InteractionKernel (frozen snapshot, apportionment, conservation), the FlowMatrix predation off-diagonal, the Bdellovibrio species data decision (real bake vs curated), and the schedule order. Integer/ordered/conserved. Output the final design.\n` +
    proposals.map((p, i) => `\n--- Design ${i} ---\n${JSON.stringify(p, null, 2)}`).join('\n'),
  { label: 'design:judge', phase: 'Design', schema: DSCHEMA },
)

phase('Implement')
const impl = await agent(
  `Implement gene-sim's Bdellovibrio predator + predation InteractionKernel per this agreed design, on the landed F3/F4/F3.4/F5 pipeline:\n${JSON.stringify(chosen, null, 2)}\n\n` +
  `Build: the predator role/eligibility (gp.rs), the predation system in trophic.rs (predators consume co-located prey J on a frozen snapshot, apportioned via fixed::apportion ties→lowest, conserved: prey J→predator − efficiency-tax→respired, killed prey Biomass→detritus; writes the FlowMatrix predation off-diagonal preserving row-sum==0), the schedule wiring (ordered, sort by cell,SpeciesId,OrgId, integer, no new RNG), and the Bdellovibrio SpeciesSpec data (data/species/bdellovibrio.json + its bake/curate script + the godot/data mirror). Add tests: predation conserves J + ledger closes; FlowMatrix predator→prey off-diagonal is nonzero + row-sum==0; a FUNCTIONAL test — a plant+E.coli+Bdellovibrio roster shows predator-prey dynamics AND the trophic cascade (throttle the predator → E. coli rises → plant rises), deterministic run-to-run. Do NOT touch the pinned literal yet (Repin phase). Do NOT commit. Report file:line + whether you expect the pinned hash to move.`,
  { label: 'impl', phase: 'Implement', agentType: 'implementer' },
)

phase('Repin')
const repin = await agent(
  `The predator is implemented. Determine + perform the re-pin (current literal 0x47a0_3c8f_6701_f240): build, get the new run_headless hash for the pinned cfg (single-species plant, no predator), confirm run-to-run stable (twice/3 processes). If it EQUALS 0x47a0 → HASH-NEUTRAL (the kernel no-ops with no predator); leave the literal unchanged, report HASH-NEUTRAL. If it DIFFERS → RE-PIN: update the literal + append a ledger line "<new>… after ADR-013 predator (Bdellovibrio predation InteractionKernel: first org-eats-org J flow, FlowMatrix off-diagonals, conserved)." aarch64 value; x86_64 is CI's job. Do NOT commit. Report outcome + stability.`,
  { label: 'repin', phase: 'Repin', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\`. determinism GREEN against the (re-pinned or unchanged) literal; license green (any new bake stays clean); livesim/godot-reader green. Report all gates PASS/FAIL with any exact error. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['predation_conserves_J', 'flowmatrix_offdiagonal', 'cascade_real', 'deterministic', 'integer_ordered', 'repin_consistent', 'issues'],
  properties: {
    predation_conserves_J: { type: 'boolean', description: 'prey J → predator − efficiency tax→respired, carcass→detritus; ledger_closes holds every tick' },
    flowmatrix_offdiagonal: { type: 'boolean', description: 'predation writes a nonzero A[predator][prey] off-diagonal; FlowMatrix row-sum==0 preserved' },
    cascade_real: { type: 'boolean', description: 'a functional test shows predator-prey dynamics AND the trophic cascade (throttle predator → prey up → plant up), not cosmetic' },
    deterministic: { type: 'boolean', description: 'run-to-run hash stable; frozen snapshot; no new RNG; no per-birth draw-count change' },
    integer_ordered: { type: 'boolean', description: 'kernel is integer, ordered (cell,SpeciesId,OrgId), no float on hash path, no HashMap iteration' },
    repin_consistent: { type: 'boolean', description: 'gate determinism GREEN against the literal; if re-pinned the new hash is stable+ledgered; if hash-neutral the literal is unchanged + justified' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify gene-sim's Bdellovibrio predator. Read git diff + the predation kernel + the conservation/FlowMatrix/cascade tests. Skeptic #${i}, default each boolean false if unconfirmable. Hunt: predation that leaks/creates J or breaks ledger_closes; a FlowMatrix off-diagonal that doesn't conserve (row-sum≠0); float/transcendental on the hash path; a new RNG draw or per-birth draw-count change; HashMap iteration; a "cascade" test that doesn't actually show throttle-predator→prey-up→plant-up; run-to-run instability; a predator that eats plants (should only eat bacteria).`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA },
  ),
))).filter(Boolean)
const ok = skeptics.filter((s) => s.predation_conserves_J && s.flowmatrix_offdiagonal && s.cascade_real && s.deterministic && s.repin_consistent).length
return { chosen, impl, repin, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, skeptics, verdict: ok >= 2 ? 'PREDATOR CONFIRMED — trophic cascade live' : 'NEEDS WORK' }
