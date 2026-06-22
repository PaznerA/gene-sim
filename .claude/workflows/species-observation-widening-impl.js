export const meta = {
  name: 'species-observation-widening-impl',
  description:
    'Widen SpeciesObservation with per-species population_size/allele_freq/mean_energy via a read-only id-sorted partition pass in observe_all(); marshal through harness + godot-sim so the per-species panels light up for EVERY species (not just the primary). Hash-neutral (read-only projection, never folded into hash).',
  whenToUse:
    'BATCH 3. Completes Layer B of ui-panels-and-relations-view: the panel "—" placeholders become live numbers for every species. Implementable + gateable autonomously; hash-neutral. Stops for human commit.',
  phases: [
    { title: 'Design' },
    { title: 'Implement' },
    { title: 'Gate' },
    { title: 'Verify' },
  ],
}

phase('Design')
const DSCHEMA = {
  type: 'object',
  required: ['fields', 'projection_pass', 'marshalling', 'hash_neutrality_argument', 'tests', 'slices'],
  properties: {
    fields: { type: 'string', description: 'The exact new SpeciesObservation fields (population_size:u32, allele_freq:f64, mean_energy:f64) + types' },
    projection_pass: { type: 'string', description: 'How observe_all() computes them: ONE SpeciesId-partitioned, OrgId-sorted read-only pass over the existing (OrgId, Species, Genotype, Energy) data; ordered, no HashMap, zero RNG' },
    marshalling: { type: 'string', description: 'How species_observation_to_dict (godot-sim) + GeneSimEnv::observe_all (harness) carry the new fields to GDScript; obs.get(key) in main.gd already reads them' },
    hash_neutrality_argument: { type: 'string', description: 'Why hash-neutral: observe_all is a read-only projection never folded into hash_world; determinism literal unchanged; the existing observe_all_is_read_only_does_not_change_hash test still holds' },
    tests: { type: 'array', items: { type: 'string' }, description: 'per-species population sums to total, allele/energy match a hand-computed fixture, read-only-does-not-change-hash still green' },
    slices: { type: 'array', items: { type: 'string' } },
  },
}
const LENSES = [
  'determinism & invariant #3: the per-species pass must be ordered (SpeciesId then OrgId), integer/clean float, zero RNG; observe_all stays a read-only projection out of hash_world',
  'API/seam fit: extend SpeciesObservation + species_observation_to_dict + GeneSimEnv::observe_all minimally so the GDScript panels (which already read obs.get(key,null)) light up with NO further .gd change',
]
const proposals = (await parallel(LENSES.map((lens, i) => () =>
  agent(
    `Design the gene-sim "SpeciesObservation widening" through this lens: ${lens}.\n\n` +
    `Context: crates/sim-core/src/lib.rs has SpeciesObservation{species_id,name,key,role,phenotype} + observe_all() (a read-only per-species projection in SpeciesId order, with test observe_all_is_read_only_does_not_change_hash). The renderer panels (godot/main.gd) already read per-species stats via obs.get(key,null), showing "—" until the core exposes population_size/allele_freq/mean_energy. Task: compute those three per species in observe_all() by ONE SpeciesId-partitioned, OrgId-sorted read-only pass over the world's organisms (Species tag + Genotype + Energy), marshal through harness GeneSimEnv::observe_all + godot-sim species_observation_to_dict. KEEP it a read-only projection (inv #2/#3): zero SimRng, never folded into hash_world. READ the actual files first.\n\n` +
    `The pinned determinism literal 0xf795_eac4_112f_acd5 MUST stay unchanged. Return a concrete file-level design.`,
    { label: `design:lens${i}`, phase: 'Design', schema: DSCHEMA },
  ),
))).filter(Boolean)
const chosen = await agent(
  `Judge & synthesize these ${proposals.length} designs into ONE plan. Pin the field types, the single ordered projection pass, and the marshalling. Hash-neutral (read-only). Output the final design.\n` +
    proposals.map((p, i) => `\n--- Design ${i} ---\n${JSON.stringify(p, null, 2)}`).join('\n'),
  { label: 'design:judge', phase: 'Design', schema: DSCHEMA },
)

phase('Implement')
const impl = await agent(
  `Implement this agreed SpeciesObservation widening, hash-neutral (read-only projection):\n${JSON.stringify(chosen, null, 2)}\n\n` +
  `Add the fields, compute them in observe_all() via the single ordered pass, marshal through harness + godot-sim, add the tests. The existing observe_all_is_read_only_does_not_change_hash MUST stay green and the pinned literal 0xf795_eac4_112f_acd5 MUST NOT change — if it would, STOP and report. Do NOT commit. Report files + lines changed.`,
  { label: 'impl', phase: 'Implement', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. Report all gates PASS/FAIL, determinism + livesim called out. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'read_only', 'stats_correct', 'refuted', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'pinned literal unchanged AND no SimRng draw added' },
    read_only: { type: 'boolean', description: 'observe_all mutates nothing and is not folded into hash_world' },
    stats_correct: { type: 'boolean', description: 'per-species population sums to total; allele/energy match a fixture' },
    refuted: { type: 'boolean', description: 'true if a real hash-neutrality/correctness violation found' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially REFUTE that the SpeciesObservation widening is hash-neutral, read-only, and correct. Read git diff + the determinism/observe_all tests. Default refuted=true if uncertain. Skeptic #${i}.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA },
  ),
))).filter(Boolean)
const refutations = skeptics.filter((s) => s.refuted).length
return { chosen, impl, gate: typeof gate === 'string' ? gate.slice(0, 300) : gate, skeptics, verdict: refutations >= 2 ? 'REJECTED' : 'CONFIRMED hash-neutral' }
