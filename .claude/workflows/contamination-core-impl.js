export const meta = {
  name: 'contamination-core-impl',
  description:
    'Contamination CORE (ADR-019 S0+S1+S2, hash-neutral): the deterministic immigration MECHANIC in the core/harness — a journaled, RNG-free, conserved RegionInoculate Action that spawns a baked contaminant SpeciesSpec at a region (J from a NEW named `immigration` ledger tap), a ContainmentLevel knob that deterministically expands (off-stream IMMG_STREAM_BASE, zero SimRng draws) into a journaled immigration schedule, and ≥1 real contaminant SpeciesSpec bake. Establish/displace/die-out is NOT scripted — it EMERGES from the conserved ADR-013 joule economy. No renderer/UI yet (that is the later SP-3 panel). Hash-neutral (inert until invoked; pinned literal 0x47a0 unchanged).',
  whenToUse:
    'After the ADR-019 contamination proposal. Core-first: build the immigration mechanic + data; intervention panel + UI follow later. Hash-neutral; stops for human commit.',
  phases: [
    { title: 'Implement' },
    { title: 'Gate' },
    { title: 'Verify' },
  ],
}

phase('Implement')
const [rustDone, dataDone] = await parallel([
  () => agent(
    `Implement the contamination CORE mechanic (ADR-019 S1+S2) for gene-sim, HASH-NEUTRAL, per docs/llm/proposals/contamination-immigration-draft.md (READ §2, §3, §6 first). Then READ crates/harness/src/lib.rs (the Action enum @101 + RegionSpec @179 + journaled replay), crates/sim-core/src/ledger.rs (the taps: influx/respired/overflow/chem_decay + ledger_closes), crates/sim-core/src/lib.rs (reproduce_or_die spawn path + NextOrgId + the off-stream derive_seed families like RESOURCE_STREAM_BASE), and crates/genome/src/spec.rs (SpeciesSpec::build).\n\n` +
    `Build (do NOT touch godot/*.gd or data/species — that's the data agent + the later UI):\n` +
    `S1 — Action::RegionInoculate{species_key:String, region:RegionSpec, count:u32, endow_j:i64} (externally-tagged serde-additive; existing actions.ndjson unchanged). A deterministic region-spawn system: spawn \`count\` orgs of the baked SpeciesSpec (built via SpeciesSpec::build / loaded by key) inside the region disc, RNG-FREE placement (deterministic cell-fill in (cell_index, slot) order), OrgIds from NextOrgId; each org's starting J = endow_j MINTED from a NEW NAMED ledger tap \`immigration\` (add the field to Ledger; extend ledger_closes to Σ(live J) == initial + influx + immigration − respired − overflow − chem_decay). Journaled into replay so a contaminated run replays bit-identically.\n` +
    `S2 — a ContainmentLevel knob (an ISO-14644-ladder enum, default Sealed/OFF) that at run start deterministically expands — off a NEW off-stream IMMG_STREAM_BASE derive_seed family, ZERO SimRng draws (the soil/resource off-stream precedent) — into a sorted Vec of journaled (due_epoch, RegionInoculate) events drawn from a configurable consortium (the menu set of species_keys); applied at their epochs (Tick-clocked, never wall-clock).\n` +
    `CRITICAL: establish/displace/die-out is NOT coded — it EMERGES from metabolism→trophic→reproduce_or_die. ALL integer/fixed-point, ordered (cell,SpeciesId,OrgId), no new SimRng draw. The pinned literal 0x47a0_3c8f_6701_f240 MUST stay unchanged (the pinned plant config issues no RegionInoculate, ContainmentLevel defaults OFF → empty schedule, immigration tap neutral at zero) — if it would move, STOP and report. Expose RegionInoculate + the knob on LiveSim/GeneSimEnv for the later panel. Add tests: an inoculation conserves J + ledger_closes holds + is replay-reproducible; same seed+knob+config → identical schedule; pinned config hash-neutral; AND an open-system test — an inoculated WELL-ADAPTED species establishes while a POORLY-ADAPTED one dies, decided by the ledger (not scripted). Do NOT commit. Report file:line.`,
    { label: 'impl:rust', phase: 'Implement', agentType: 'implementer' },
  ),
  () => agent(
    `Bake the contaminant SpeciesSpec data (ADR-019 S0) for gene-sim, HASH-NEUTRAL, per docs/llm/proposals/contamination-immigration-draft.md §5 (READ it) + the scripts/bake_ecoli_species.py / bake_bdellovibrio_species.py conventions + data/species/ecoli.json / bdellovibrio.json shape + crates/genome/src/spec.rs (SpeciesSpec, niche.trophic_role).\n\n` +
    `Bake REAL contaminant SpeciesSpecs (do NOT touch crates/** or godot/**). START with Mycoplasma genitalium G37 (small ~580 kb / ~470-gene genome, famous filter-passer + minimal-cell model — a great first contaminant), via scripts/bake_mycoplasma_species.py → data/species/mycoplasma.json (real NCBI CDS where feasible, niche.trophic_role declared, validating through SpeciesSpec::build). If clean+quick, ADD Bacillus subtilis 168 (spore-former) and/or Pseudomonas aeruginosa PAO1 (biofilm generalist) the same way; otherwise document them as follow-up bakes in the script. Each must build + round-trip via SpeciesSpec::build (add/extend a harness test like shipped_ecoli_species_loads). Mirror to godot/data/species (the run.sh res:// mirror). Hash-neutral (unused on disk → pinned literal unchanged). Do NOT commit. Report which species you baked + their genome provenance + gene counts (cited).`,
    { label: 'impl:data', phase: 'Implement', agentType: 'implementer' },
  ),
])

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. determinism MUST be GREEN against 0x47a0_3c8f_6701_f240 (contamination is inert in the pinned config → hash-neutral); license green (any new bake stays clean); the replay/Action round-trip + ledger_closes tests pass. Report all gates PASS/FAIL with any exact error. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'conserved', 'deterministic_replayable', 'schedule_offstream', 'emergent_not_scripted', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'pinned literal 0x47a0 unchanged; RegionInoculate inert in pinned config; immigration tap neutral at zero; knob default OFF' },
    conserved: { type: 'boolean', description: 'inoculation J is minted from the named immigration tap; ledger_closes holds every tick through inoculation' },
    deterministic_replayable: { type: 'boolean', description: 'RegionInoculate is RNG-free, region-scoped, ordered, journaled; replay reproduces the exact run hash' },
    schedule_offstream: { type: 'boolean', description: 'the ContainmentLevel schedule expands off IMMG_STREAM_BASE with ZERO SimRng draws (no wall-clock); same seed+knob+config → identical schedule' },
    emergent_not_scripted: { type: 'boolean', description: 'establish/displace/die-out EMERGES from the joule ledger (a well-adapted inoculant establishes, a poorly-adapted one starves) — NOT a scripted outcome' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify the gene-sim contamination CORE. Read git diff + the RegionInoculate system + the immigration ledger tap + the ContainmentLevel schedule + the contaminant bake + the tests. Skeptic #${i}, default each boolean false if unconfirmable. Hunt: a J leak / ledger_closes break from inoculation; a new SimRng draw (the schedule MUST be off-stream); a wall-clock in the schedule; the pinned literal moving (anything not inert in the pinned config); a SCRIPTED establish/die outcome (it must emerge from the ledger); non-integer/HashMap iteration; a contaminant bake that doesn't round-trip via SpeciesSpec::build.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA },
  ),
))).filter(Boolean)
const ok = skeptics.filter((s) => s.hash_neutral && s.conserved && s.deterministic_replayable && s.emergent_not_scripted).length
return { rustDone, dataDone, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, skeptics, verdict: ok >= 2 ? 'CONTAMINATION CORE CONFIRMED — emergent + hash-neutral' : 'NEEDS WORK' }
