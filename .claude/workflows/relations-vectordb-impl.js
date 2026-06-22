export const meta = {
  name: 'relations-vectordb-impl',
  description:
    'ADR-014/ADR-017-S8 relations sidecar (HASH-NEUTRAL, view-only): export per-species signature vectors (u16[D] from Strategy/role/FlowMatrix-rows/affinity — a read-only off-hash core projection), a crates/relations-index process-boundary crate (inv #1) that indexes them (sqlite-vec if it installs cleanly + S warrants it, else a deterministic in-Rust k-NN/guild clustering) and answers nearest-species / guild queries, and a Relations-view overlay (guild colouring + nearest-species). The ANN/clustering output NEVER re-enters the determinism hash.',
  whenToUse:
    'Big sim push, after F5. The "vector-DB relations" leg of the vision, re-grounded on the MEASURED FlowMatrix (relations are emergent, not fabricated). View-only / off-hash → hash-neutral. Autonomous; stops for human commit.',
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
  required: ['signature', 'index_backend', 'queries', 'overlay', 'hash_neutrality_argument', 'boundary', 'slices'],
  properties: {
    signature: { type: 'string', description: 'the per-species signature vector u16[D]: which dims (Strategy budget[5], role, FlowMatrix row in/out flows, niche affinity) and how they are exported read-only from the core (an off-hash projection like flow_matrix())' },
    index_backend: { type: 'string', description: 'sqlite-vec sidecar (if it installs cleanly + S is large enough to warrant it) vs a deterministic in-Rust k-NN/guild clustering for the current small-S; pick + scaffold the other as the scale path' },
    queries: { type: 'string', description: 'nearest-species (metabolic/interaction similarity) + guild clustering (single-link over the signatures at a pinned integer threshold) — deterministic' },
    overlay: { type: 'string', description: 'how the existing Relations view (godot/relations_heatmap.gd + main.gd) gains a guild-colouring / nearest-species overlay reading the sidecar output' },
    hash_neutrality_argument: { type: 'string', description: 'why hash-neutral: the signature export is read-only off-hash; the relations-index runs at the process boundary; its ANN/clustering output goes ONLY to the view and NEVER folds into hash_world; pinned literal 0x47a0_3c8f_6701_f240 unchanged' },
    boundary: { type: 'string', description: 'crates/relations-index added to scripts/check_license.sh BOUNDARY_CRATES (inv #1); std-only or clean-licensed deps only; sqlite-vec (Apache-2.0/MIT) stays behind the boundary/sidecar' },
    slices: { type: 'array', items: { type: 'string' } },
  },
}
const LENSES = [
  'determinism & inv #1: the relations-index is a process-boundary read of EXPORTED signatures; its output never re-enters hash_world (view-only); the crate carries only clean-licensed deps and is in BOUNDARY_CRATES',
  'relations semantics: what makes two species "related" — metabolic similarity (Strategy/role), interaction profile (the MEASURED FlowMatrix rows), niche overlap; guild clustering + nearest-species that read clearly in the UI',
  'scaling & tooling: sqlite-vec sidecar earns its keep when there are MANY signatures (future: many E. coli edit-variants / species); for the current small-S a deterministic in-Rust k-NN is simpler + exact — pick pragmatically and scaffold the other',
]
const proposals = (await parallel(LENSES.map((lens, i) => () =>
  agent(
    `Design a gene-sim relations sidecar (the "vector-DB relations" feature, re-grounded on the MEASURED FlowMatrix) through this lens: ${lens}.\n\n` +
    `Context: F4 LANDED the emergent MEASURED FlowMatrix (S×S net-J flows), exported read-only via Simulation::flow_matrix() / LiveSim::flow_matrix(); the Relations heatmap view (godot/relations_heatmap.gd) already renders it. crates/oracle-slim + crates/oracle-fba are the process-boundary-crate templates (inv #1, in scripts/check_license.sh BOUNDARY_CRATES); rel-relations-draft.md / ADR-014 had a (now-inverted) vector-DB design. The task: export per-species signature vectors, build a crates/relations-index boundary crate that indexes them (sqlite-vec if warranted, else deterministic in-Rust) for nearest-species + guild clustering, and add a view overlay. The ANN/clustering is VIEW-ONLY and must NEVER re-enter the determinism hash (the FlowMatrix is the on-hash source; this is a downstream off-hash consumer). READ docs/llm/proposals/rel-relations-vectordb-design.md (if present) + the FlowMatrix export + crates/oracle-slim first.\n\n` +
    `Return a concrete file-level design. The pinned literal 0x47a0_3c8f_6701_f240 MUST stay unchanged (hash-neutral). Do NOT write code.`,
    { label: `design:lens${i}`, phase: 'Design', schema: DSCHEMA },
  ),
))).filter(Boolean)
const chosen = await agent(
  `Judge & synthesize these ${proposals.length} relations-sidecar designs into ONE plan. Pin the signature dims, the index backend choice (+ the scaffolded alternative), the deterministic guild/nearest queries, the view overlay, and the inv #1 boundary. View-only, hash-neutral. Output the final design.\n` +
    proposals.map((p, i) => `\n--- Design ${i} ---\n${JSON.stringify(p, null, 2)}`).join('\n'),
  { label: 'design:judge', phase: 'Design', schema: DSCHEMA },
)

phase('Implement')
const impl = await agent(
  `Implement this agreed gene-sim relations sidecar, HASH-NEUTRAL + view-only:\n${JSON.stringify(chosen, null, 2)}\n\n` +
  `Build: (1) the read-only per-species signature export in the core (an off-hash projection — zero RNG, NOT folded into hash_world, like flow_matrix()); (2) crates/relations-index (a process-boundary crate per inv #1 — clean-licensed deps only, added to scripts/check_license.sh BOUNDARY_CRATES; sqlite-vec stays behind the boundary if used) computing nearest-species + deterministic guild clustering; (3) a Relations-view overlay (godot) reading the sidecar output. The relations output must NEVER re-enter the sim — view-only. The pinned literal 0x47a0_3c8f_6701_f240 MUST stay unchanged — if anything would move it, STOP and report. Add tests (signature export read-only-does-not-change-hash; deterministic clustering; license gate green). Do NOT commit. Report file:line + which index backend you used (sqlite-vec vs in-Rust) and why.`,
  { label: 'impl', phase: 'Implement', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\`. determinism MUST stay GREEN against 0x47a0_3c8f_6701_f240 (relations is hash-neutral); license MUST stay GREEN (relations-index is a clean boundary crate, inv #1); livesim/godot-reader green. Report all gates PASS/FAIL with any exact error. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'view_only', 'boundary_clean', 'deterministic_clustering', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'pinned literal unchanged; signature export is read-only off-hash; relations output never folds into hash_world' },
    view_only: { type: 'boolean', description: 'the ANN/clustering output reaches ONLY the view, never the sim/selection' },
    boundary_clean: { type: 'boolean', description: 'relations-index is a clean process-boundary crate (inv #1), in BOUNDARY_CRATES, license gate green; sqlite-vec (if used) stays behind the boundary' },
    deterministic_clustering: { type: 'boolean', description: 'the guild/nearest queries are deterministic (integer thresholds, ordered), reproducible across runs' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const verdict = await agent(
  `Adversarially verify the gene-sim relations sidecar. Read \`git diff\` + the signature export + crates/relations-index + the overlay. Try to REFUTE each property; default false if unconfirmable. Confirm the relations output never re-enters the hash (view-only) and the pinned literal is unchanged.`,
  { label: 'verify', phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
)

return { chosen, impl, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, verdict }
