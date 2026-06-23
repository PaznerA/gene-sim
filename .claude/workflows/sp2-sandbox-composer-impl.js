export const meta = {
  name: 'sp2-sandbox-composer-impl',
  description:
    'SP-2 sandbox composer (hash-neutral): compose a multi-species run before starting — pick a ROSTER (multiple species from the 10 baked SpeciesSpecs: plant/E.coli/Bdellovibrio + the 7 contaminants) with per-species starting populations, set the environment (climate/seed) + the ContainmentLevel, optionally queue initial edits → build a Vec<RosterEntry> and start a deterministic run via reset_with_roster. Extends the existing main menu (single-species) into a multi-species composer. The SpeciesSpec JSON is the vehicle. The pinned single-species-plant config is unchanged → hash-neutral.',
  whenToUse:
    'Midnight session item 4. The sandbox creative surface (compose → run → observe). Renderer + a thin multi-species roster boundary; hash-neutral. Autonomous; stops for human commit.',
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
  required: ['roster_boundary', 'composer_ux', 'determinism', 'hash_neutrality', 'slices'],
  properties: {
    roster_boundary: { type: 'string', description: 'the LiveSim/GeneSimEnv multi-species roster boundary: e.g. LiveSim.set_roster(keys, counts) → load each SpeciesSpec JSON (res:// bytes → build_species_from_str), build a Vec<RosterEntry> (key/genome/gp_map/entity_count), reset_with_roster. Reuses the contamination register_contaminant + the existing set_species path' },
    composer_ux: { type: 'string', description: 'the godot composer: extend main_menu.gd from a single-species OptionButton to a multi-species roster (add/remove species rows: species + starting count), + the existing climate/seed + a ContainmentLevel selector; Start → set_roster + reset' },
    determinism: { type: 'string', description: 'the composed run is deterministic from (seed, roster, env, containment) — reset_with_roster is RNG-seeded once; no wall-clock; same config → same hash' },
    hash_neutrality: { type: 'string', description: 'why hash-neutral: the pinned single-species-plant config (run_headless) is a separate path, unchanged; the composer adds a new multi-species menu path; pinned literal 0x47a0 untouched' },
    slices: { type: 'array', items: { type: 'string' } },
  },
}
const LENSES = [
  'boundary & determinism: the multi-species roster boundary must build a Vec<RosterEntry> from baked SpeciesSpec JSONs (res:// bytes, inv #2) and reset_with_roster deterministically (same seed+roster → same hash); the pinned config stays a separate unchanged path',
  'composer UX & game-feel: a clean compose screen — add species rows (the 10 baked specs incl. contaminants) with starting counts, climate/seed, ContainmentLevel — that makes "design your consortium then watch it run/get-contaminated" tangible',
  'reuse: lean on the existing main_menu.gd (species OptionButton + climate/seed), reset_with_roster/RosterEntry (core), and the contamination register_contaminant + set_containment boundary — minimal new surface',
]
const proposals = (await parallel(LENSES.map((lens, i) => () =>
  agent(
    `Design the gene-sim SP-2 SANDBOX COMPOSER through this lens: ${lens}.\n\n` +
    `Context: the core has reset_with_roster(Vec<RosterEntry{key,genome,gp_map,entity_count}>) (crates/sim-core/src/lib.rs:133) + the SpeciesRegistry; 10 baked SpeciesSpecs in data/species (default plant, ecoli, bdellovibrio, + 7 contaminants). The godot main_menu.gd (single-species OptionButton + climate/seed) + LiveSim.set_species/set_containment/register_contaminant_json (the boundary) exist. Task: a MULTI-species roster composer — pick several species with per-species starting counts + env + ContainmentLevel → build a Vec<RosterEntry> → reset_with_roster → a deterministic run. KEEP biology in the core (inv #2): GDScript moves species JSON bytes + counts; the core builds the roster. The pinned single-species-plant config (run_headless) must be UNCHANGED (a separate path) → hash-neutral. READ main_menu.gd, crates/godot-sim/src/lib.rs (set_species/reset), the reset_with_roster path, and the SP-3/contamination boundary first.\n\n` +
    `Return a concrete file-level design. Pinned literal 0x47a0_3c8f_6701_f240 unchanged. Do NOT write code.`,
    { label: `design:lens${i}`, phase: 'Design', schema: DSCHEMA },
  ),
))).filter(Boolean)
const chosen = await agent(
  `Judge & synthesize these ${proposals.length} sandbox-composer designs into ONE plan. Pin the multi-species roster boundary (LiveSim.set_roster) + the composer UX + the determinism + the hash-neutrality argument. Output the final design.\n` +
    proposals.map((p, i) => `\n--- Design ${i} ---\n${JSON.stringify(p, null, 2)}`).join('\n'),
  { label: 'design:judge', phase: 'Design', schema: DSCHEMA },
)

phase('Implement')
const contract = JSON.stringify(chosen, null, 2)
const [rustDone, gdDone] = await parallel([
  () => agent(
    `Implement ONLY the Rust/boundary side of this agreed SP-2 plan (do NOT touch godot/*.gd):\n${contract}\n\n` +
    `Add the multi-species roster boundary (e.g. LiveSim.set_roster(keys: PackedStringArray, counts: PackedInt32Array) on godot-sim + a GeneSimEnv passthrough): load each SpeciesSpec via the existing build_species_from_str / register path, build a Vec<RosterEntry>, and reset_with_roster. Reuse the existing set_species/register_contaminant plumbing. The pinned single-species-plant config (run_headless / set_species default) MUST be byte-identical — the roster path is additive; if the literal would move, STOP and report. Add a test: a composed 2-3 species roster runs deterministically (same seed+roster → same hash). Do NOT commit. Report file:line.`,
    { label: 'impl:rust', phase: 'Implement', agentType: 'implementer' },
  ),
  () => agent(
    `Implement ONLY the godot side of this agreed SP-2 plan (do NOT touch crates/**):\n${contract}\n\n` +
    `Extend main_menu.gd from the single-species OptionButton into a multi-species ROSTER composer: add/remove species rows (each: a species from the 10 baked specs + a starting count), keep the climate/seed controls, add a ContainmentLevel selector; on Start, pass the roster (keys + counts) + env + containment to the new LiveSim.set_roster boundary + reset. Keep ALL biology in the core (inv #2) — GDScript moves species JSON bytes + counts + config. Do NOT commit. Report file:line.`,
    { label: 'impl:gdscript', phase: 'Implement' },
  ),
])

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. determinism GREEN against 0x47a0_3c8f_6701_f240 (the pinned single-species path is unchanged; the composer roster path is additive → hash-neutral); livesim/godot-reader green. Report all gates PASS/FAIL with any exact error. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'inv2_preserved', 'roster_deterministic', 'composer_works', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'pinned literal unchanged; the single-species path is byte-identical; the roster path is additive' },
    inv2_preserved: { type: 'boolean', description: 'GDScript moves species bytes + counts + config; the core builds the roster + biology' },
    roster_deterministic: { type: 'boolean', description: 'a composed multi-species roster runs deterministically (same seed+roster → same hash); a test asserts it' },
    composer_works: { type: 'boolean', description: 'the menu composes a multi-species roster (add/remove species + counts) + env + containment → starts a run' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const verdict = await agent(
  `Adversarially verify the SP-2 sandbox composer. Read \`git diff\`. Try to REFUTE each property; default false if unconfirmable. Confirm the pinned single-species literal is unchanged, the roster path is deterministic, and no biology leaked into GDScript (inv #2).`,
  { label: 'verify', phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
)

return { chosen, rustDone, gdDone, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, verdict }
