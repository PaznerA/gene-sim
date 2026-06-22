export const meta = {
  name: 'ecoli-visibility-impl',
  description:
    'Make E. coli actually visible & playable: res:// species loading fix + a genuine microbe specimen view + per-species observe. Hash-neutral (no determinism re-pin).',
  whenToUse:
    'Run FIRST. Unblocks the user seeing E. coli instead of the plant tree. Fixes the cwd species-not-found bug at the Godot↔Rust boundary. Fully implementable + gateable autonomously; stops for human commit.',
  phases: [
    { title: 'Design' },
    { title: 'Implement' },
    { title: 'Gate' },
    { title: 'Verify' },
  ],
}

// ── Phase 1: design panel (3 lenses) → judge synthesizes the exact contract ──
phase('Design')
const DESIGN_SCHEMA = {
  type: 'object',
  required: ['boundary_signature', 'gdscript_flow', 'res_layout', 'specimen_view', 'slices', 'risks'],
  properties: {
    boundary_signature: { type: 'string', description: 'Exact Rust signature for harness::species::build_species_from_str + the LiveSim #[func] that takes a JSON string' },
    gdscript_flow: { type: 'string', description: 'How GDScript reads species JSON via FileAccess(res://...) and passes the string to LiveSim' },
    res_layout: { type: 'string', description: 'Where species JSON lives under godot/ as a res:// resource; how repo-root data/species stays the single source of truth (build copy/symlink)' },
    specimen_view: { type: 'string', description: 'How main.gd renders a microbe (E. coli) specimen view distinct from the plant L-system tree; the per-species observe() split' },
    slices: { type: 'array', items: { type: 'string' }, description: 'Ordered implementation steps, smallest surface' },
    risks: { type: 'array', items: { type: 'string' }, description: 'Invariant/determinism risks and how each is neutralized' },
  },
}
const LENSES = [
  'determinism & invariant #2 (biology stays in Rust; GDScript only moves bytes)',
  'shipped-build robustness (works in --live dev AND packaged .deb/.exe; release.yml staging)',
  'UX: the user must SEE a clearly different microbe view, not a plant placeholder',
]
const proposals = (await parallel(LENSES.map((lens, i) => () =>
  agent(
    `You are designing the fix for the gene-sim E. coli species-loading bug, through this lens: ${lens}.\n\n` +
    `Context: \`godot --path godot -- --live\` sets cwd to godot/, so LiveSim::set_species's relative path data/species/ecoli.json (crates/godot-sim/src/lib.rs ~line 144, resolve_species_path) misses, and the exe-dir fallback also misses in dev. ecoli.json + default.json live at repo-root data/species/. Recommended fix = a res:// VFS boundary: ship species JSON under godot/ as res://, GDScript reads bytes via FileAccess, passes the JSON string to a NEW Rust harness::species::build_species_from_str(&str). Also: the specimen view (main.gd ~line 1417) renders a plant L-system even for microbes (a documented placeholder warning) — design a genuine microbe specimen view driven by the per-species observe() phenotype. READ the actual files before proposing.\n\n` +
    `Keep biology in Rust (inv #2); GDScript moves only bytes + renders; zero determinism impact (species JSON is inert data, no RNG). Return a concrete file-level design.`,
    { label: `design:lens${i}`, phase: 'Design', schema: DESIGN_SCHEMA },
  ),
))).filter(Boolean)

const chosen = await agent(
  `You are the judge. Here are ${proposals.length} designs for the E. coli species-loading fix + microbe specimen view:\n` +
    proposals.map((p, i) => `\n--- Design ${i} ---\n${JSON.stringify(p, null, 2)}`).join('\n') +
    `\n\nSynthesize ONE implementation plan, grafting the best of each. PIN the exact Rust signature and the exact GDScript↔Rust contract so the Rust and GDScript implementers can work in parallel without conflict. Emphasize: hash-neutral, inv #2 preserved, works in BOTH dev --live and packaged builds.`,
  { label: 'design:judge', phase: 'Design', schema: DESIGN_SCHEMA },
)

// ── Phase 2: implement Rust + GDScript in parallel (non-overlapping files) ──
phase('Implement')
const contract = JSON.stringify(chosen, null, 2)
const [rustDone, gdDone] = await parallel([
  () => agent(
    `Implement ONLY the Rust side of this agreed plan (do NOT touch godot/*.gd):\n${contract}\n\n` +
    `Add harness::species::build_species_from_str(&str) -> io::Result<BuiltSpecies> reusing the existing serde_json + SpeciesSpec::build path in crates/harness/src/species.rs (load_species_file should delegate to it). Add/adjust the LiveSim #[func] in crates/godot-sim/src/lib.rs to accept a JSON string from GDScript and build via this boundary, keeping the existing file-path path as a fallback. Add a unit test that build_species_from_str on the shipped ecoli.json bytes yields the same BuiltSpecies as load_species_file. Do NOT change the pinned determinism literal 0xf795_eac4_112f_acd5. Do NOT commit. Report the files + lines you changed.`,
    { label: 'impl:rust', phase: 'Implement', agentType: 'implementer' },
  ),
  () => agent(
    `Implement ONLY the GDScript + res:// side of this agreed plan (do NOT touch crates/**):\n${contract}\n\n` +
    `Ship the species JSON under godot/ as a res:// resource per the plan, make main_menu.gd / main.gd read the selected species JSON via FileAccess(res://...) and pass the string to the new LiveSim boundary, and add a GENUINE microbe specimen view in main.gd (distinct from the plant L-system) driven by the per-species observe() phenotype. Keep ALL biology in Rust — GDScript only moves bytes + renders (inv #2). Do NOT commit. Report the files + lines you changed.`,
    { label: 'impl:gdscript', phase: 'Implement' },
  ),
])

// ── Phase 3: gate ──
phase('Gate')
const gate = await agent(
  `Run the full gate for gene-sim: \`bash tools/gate.sh\` from the repo root. Report each of the 10 gates PASS/FAIL with the determinism gate called out explicitly. If any gate is red, summarize the exact failure. Do NOT fix anything, do NOT commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

// ── Phase 4: adversarial verify ──
phase('Verify')
const VERDICT_SCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'inv2_preserved', 'works_dev_and_packaged', 'microbe_view_distinct', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'pinned determinism literal unchanged AND no RNG/biology entered GDScript' },
    inv2_preserved: { type: 'boolean', description: 'biology stayed in Rust; GDScript only moved bytes/render' },
    works_dev_and_packaged: { type: 'boolean', description: 'species loads in both `godot --path godot -- --live` (res://) AND a packaged build' },
    microbe_view_distinct: { type: 'boolean', description: 'the microbe specimen view is genuinely distinct, not the plant placeholder' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const verdict = await agent(
  `Adversarially verify the just-implemented E. coli visibility change. Read \`git diff\`. Try to REFUTE each property; default a boolean to false if you cannot confirm it from the code. List concrete issues.`,
  { label: 'verify', phase: 'Verify', schema: VERDICT_SCHEMA, agentType: 'reviewer' },
)

log(`gate: ${typeof gate === 'string' ? gate.slice(0, 200) : ''}`)
return { chosen, rustDone, gdDone, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, verdict }
