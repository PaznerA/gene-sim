export const meta = {
  name: 'sp3-intervention-panel-impl',
  description:
    'SP-3 intervention panel: 5 spatial, journaled, deterministic, conserved sandbox tools — CRISPR (integrate existing) + PCR amplify (faithful local clones of a species, J from a named influx tap) + Antibiotic cull (kill species in region → carcass→detritus) + Nutrient feed (inject pool J, named influx tap) + Toxin spike (inject the F5 toxin field). A dedicated godot Intervention panel (tool palette + per-tool params + brush, position matters) + TIMELINE event markers (each journaled Action shows on the timeline; replay reproduces it). Hash-neutral (the pinned config has no actions; the new Actions are inert until invoked).',
  whenToUse:
    'After SP-1 (tuned core). The sandbox interaction surface. All interventions are RNG-free/single-stream region Actions conserved via named ledger taps → a sandbox session is fully replayable from seed + journal. Renderer + harness/core Action work; hash-neutral. Autonomous; stops for human commit.',
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
  required: ['actions', 'pcr_clone', 'conservation', 'determinism', 'panel_ux', 'timeline', 'hash_neutrality', 'slices'],
  properties: {
    actions: { type: 'string', description: 'the 4 NEW region Actions (RegionPcrAmplify / RegionCull / RegionNutrient / RegionToxin) + integrating the existing region CRISPR; their fields (species, region/brush, strength, channel); externally-tagged serde additive (existing actions.ndjson unchanged)' },
    pcr_clone: { type: 'string', description: 'PCR = FAITHFUL clones: amplify the targeted species\' local orgs by cloning (offspring inherit the exact genome + heritable state via the existing reproduce inheritance), deterministic placement (no/known RNG), OrgIds from NextOrgId; the clones\' starting J minted from a named "PCR/intervention influx" ledger tap (conserved)' },
    conservation: { type: 'string', description: 'each tool conserves J via a NAMED ledger tap: PCR/Nutrient = a named INFLUX tap; Cull = carcass→detritus (existing death path); Toxin = a named influx into the chem field (milli==J). ledger_closes must hold every tick through every intervention' },
    determinism: { type: 'string', description: 'every Action is RNG-free (or a documented single-stream draw), region-scoped, ordered (sort by cell/SpeciesId/OrgId), integer; journaled so replay reproduces the exact run; the existing ApplyEditRegion precedent' },
    panel_ux: { type: 'string', description: 'the godot Intervention panel: a tool palette (5 tools), per-tool params (target species, gene for CRISPR, strength, brush size), the brush (drag to paint, POSITION MATTERS), a readout of the effect; reuses the existing brush/region plumbing' },
    timeline: { type: 'string', description: 'each journaled intervention shows as an EVENT MARKER on the timeline at its generation (icon per tool); replay scrubbing reflects them; how the existing timeline UI gains markers' },
    hash_neutrality: { type: 'string', description: 'why hash-neutral: the new Actions are inert until invoked; the pinned single-species-plant config issues no actions → literal unchanged; the conserved taps are new ledger fields, neutral at zero' },
    slices: { type: 'array', items: { type: 'string' } },
  },
}
const LENSES = [
  'determinism & conservation: each intervention is an RNG-free region Action conserved via a NAMED ledger tap (PCR/Nutrient influx, Cull→detritus, Toxin influx); journaled so a sandbox run replays bit-identically; ledger_closes holds; the pinned config (no actions) stays hash-neutral',
  'biology & game-feel: CRISPR (edit) / PCR (faithful clone amplification) / Antibiotic (cull) / Nutrient (feed) / Toxin (allelopathic shock) — five legible, spatial tools where WHERE you brush changes the outcome (fertilize a lit cell, cull the predator where prey is dense → cascade)',
  'UX & timeline: a clean Intervention panel (tool palette + params + brush) reusing the existing brush/region plumbing, and timeline event markers so the player SEES their interventions in the run history + on replay',
]
const proposals = (await parallel(LENSES.map((lens, i) => () =>
  agent(
    `Design the gene-sim SP-3 INTERVENTION PANEL (5 spatial tools) through this lens: ${lens}.\n\n` +
    `LOCKED scope (user-confirmed): CRISPR (integrate the existing region edit) + PCR amplify (FAITHFUL clones of a specific organism/species with its exact genome + heritable variation) + Antibiotic cull + Nutrient feed + Toxin spike. All spatial (brush, POSITION MATTERS), all journaled Actions, all deterministic + conserved (named ledger taps), all visible as TIMELINE event markers. Conjugation/HGT + seed + climate are a deferred 2nd wave.\n\n` +
    `Context: the harness Action enum (crates/harness/src/lib.rs — Advance/ApplyEdit/ApplyEditRegion + the OVERSIGHT Request/CommitEcoliImpact) + journaled replay (replay.rs) is the precedent; ApplyEditRegion is the existing RNG-free region intervention. The core has per-cell PoolStock (light/free_nutrient/detritus), the F5 ChemField (toxin/kin/alarm), per-org Energy/Biomass/Genotype/Species/Position, the conserved Ledger (named taps: influx/respired/overflow/chem_decay), reproduce_or_die (the clone/death paths), NextOrgId. The godot side has a brush + region-edit UI + a timeline (replay scrub). READ the harness Action/replay code, the core ledger/pools/chem/reproduce paths, and the godot brush + timeline first.\n\n` +
    `Return a concrete file-level design. The pinned literal 0x47a0_3c8f_6701_f240 MUST stay unchanged (the new Actions are inert until invoked → hash-neutral). Do NOT write code.`,
    { label: `design:lens${i}`, phase: 'Design', schema: DSCHEMA },
  ),
))).filter(Boolean)
const chosen = await agent(
  `Judge & synthesize these ${proposals.length} intervention-panel designs into ONE plan. Pin the 4 new Actions + their conserved named taps, the faithful-PCR-clone mechanic, the deterministic region application, the panel UX, and the timeline markers. Hash-neutral. Output the final design.\n` +
    proposals.map((p, i) => `\n--- Design ${i} ---\n${JSON.stringify(p, null, 2)}`).join('\n'),
  { label: 'design:judge', phase: 'Design', schema: DSCHEMA },
)

phase('Implement')
const contract = JSON.stringify(chosen, null, 2)
const [coreDone, gdDone] = await parallel([
  () => agent(
    `Implement ONLY the Rust/core+harness side of this agreed plan (do NOT touch godot/*.gd):\n${contract}\n\n` +
    `Add the 4 region Actions (RegionPcrAmplify/RegionCull/RegionNutrient/RegionToxin) + their deterministic, RNG-free, conserved region systems on the core (PCR = faithful clones via the existing inheritance, J from a NAMED influx ledger tap; Cull → carcass→detritus; Nutrient → named pool influx tap; Toxin → named chem influx), journaled into the replay stream (externally-tagged serde additive — existing actions.ndjson unchanged). ledger_closes MUST hold through every intervention. The pinned literal 0x47a0_3c8f_6701_f240 MUST stay unchanged (Actions inert until invoked) — if it would move, STOP and report. Expose them on LiveSim (#[func]) for the panel. Add tests: each intervention conserves J + ledger closes + is journaled/replay-reproducible; the pinned config is hash-neutral. Do NOT commit. Report file:line.`,
    { label: 'impl:core', phase: 'Implement', agentType: 'implementer' },
  ),
  () => agent(
    `Implement ONLY the godot side of this agreed plan (do NOT touch crates/**):\n${contract}\n\n` +
    `Build the Intervention panel: a tool palette (CRISPR / PCR / Antibiotic / Nutrient / Toxin), per-tool params (target species, gene for CRISPR, strength, brush size), the brush (drag to paint — POSITION MATTERS), an effect readout; reuse the existing brush/region plumbing + call the new LiveSim intervention #[func]s. Add TIMELINE event markers: each journaled intervention shows as an icon at its generation on the timeline, and replay scrubbing reflects them. Keep ALL biology in the core (inv #2) — GDScript only issues the Action + renders. Do NOT commit. Report file:line.`,
    { label: 'impl:gdscript', phase: 'Implement' },
  ),
])

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. determinism MUST be GREEN against 0x47a0_3c8f_6701_f240 (interventions are inert in the pinned config → hash-neutral); livesim/godot-reader green; the replay/Action round-trip tests pass. Report all gates PASS/FAIL with any exact error. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'conserved', 'deterministic_replayable', 'pcr_faithful', 'timeline_markers', 'position_matters', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'pinned literal unchanged; the new Actions are inert until invoked' },
    conserved: { type: 'boolean', description: 'each intervention conserves J via a named ledger tap; ledger_closes holds every tick' },
    deterministic_replayable: { type: 'boolean', description: 'interventions are RNG-free/single-stream, journaled, and replay reproduces the exact run hash' },
    pcr_faithful: { type: 'boolean', description: 'PCR clones inherit the exact genome + heritable state (faithful amplification, not pure +N)' },
    timeline_markers: { type: 'boolean', description: 'journaled interventions show as timeline event markers; replay reflects them' },
    position_matters: { type: 'boolean', description: 'the tools are region-scoped via the brush — WHERE you apply changes the outcome' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const verdict = await agent(
  `Adversarially verify the SP-3 intervention panel. Read \`git diff\`. Try to REFUTE each property; default false if unconfirmable. Confirm the pinned literal is unchanged, every intervention conserves J (ledger closes) + is journaled/replay-reproducible, PCR clones are faithful, and the timeline shows the events.`,
  { label: 'verify', phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
)

return { chosen, coreDone, gdDone, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, verdict }
