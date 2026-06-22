export const meta = {
  name: 'contamination-bakes-remaining-impl',
  description:
    'ADR-019 S0 (remaining Mode-A contaminant bakes, hash-neutral data): bake the rest of the airborne-contaminant SpeciesSpec JSONs — Pseudomonas aeruginosa PAO1 (biofilm generalist), Staphylococcus epidermidis (skin flora), Cutibacterium acnes (slow anaerobe), Aspergillus niger + Penicillium (mold spores) — following the bake_mycoplasma/bacillus_species.py convention (real NCBI CDS, curated role+trait loci, niche.trophic_role). Each builds + round-trips via SpeciesSpec::build. Completes the default consortium. Hash-neutral (unused on disk until a config references them).',
  whenToUse:
    'After the contamination core (mycoplasma/bacillus already baked). Pure data; the pinned literal 0x47a0 stays unchanged. Autonomous; stops for human commit.',
  phases: [
    { title: 'Bake' },
    { title: 'Gate' },
    { title: 'Verify' },
  ],
}

phase('Bake')
const bake = await agent(
  `Bake the remaining ADR-019 Mode-A contaminant SpeciesSpecs for gene-sim — DATA ONLY (do NOT touch crates/** except the harness test file, do NOT touch godot/**). READ scripts/bake_mycoplasma_species.py + scripts/bake_bacillus_species.py (the convention: a single pinned NCBI source → byte-identical reproducible bake; a curated anchor roster of role+trait loci, since the immigration kernel reads role + trait levers not specific genes), data/species/{mycoplasma,bacillus}.json (the shape), crates/genome/src/spec.rs (SpeciesSpec, niche.trophic_role), docs/llm/proposals/contamination-immigration-draft.md §5 (the species + their roles/traits + verified genome facts), and crates/harness/src/species.rs (the round-trip tests like shipped_bacillus_species_loads).\n\n` +
  `Bake these (real NCBI reference assemblies where feasible; prefer the species/strain the draft cites). For each: a scripts/bake_<key>_species.py + data/species/<key>.json + the godot/data/species mirror (gitignored), declaring niche.trophic_role, validating through SpeciesSpec::build:\n` +
  `- pseudomonas (P. aeruginosa PAO1, biofilm metabolic generalist — Heterotroph/Mixotroph; ~6.3 Mb)\n` +
  `- staph (S. epidermidis, skin-flora biofilm — Heterotroph)\n` +
  `- cutibacterium (C. acnes, slow anaerobe — Heterotroph/Decomposer)\n` +
  `- aspergillus-niger (mold, airborne spores, saprotroph — Decomposer; EUKARYOTE: curate a small representative locus set, the bake is a curated anchor roster not the whole genome)\n` +
  `- penicillium (mold, airborne spores, saprotroph — Decomposer; same curated approach)\n` +
  `If a particular NCBI fetch is flaky/unavailable, bake what is cleanly available and DOCUMENT the rest as follow-ups in the script header (do NOT block). Extend crates/harness/src/species.rs with a round-trip test per baked species (assert build + validity + non-empty CDS + the declared role resolves via gp::role_from_override). These keys complete ConsortiumConfig::default_mode_a (already references pseudomonas/aspergillus-niger). The pinned literal 0x47a0_3c8f_6701_f240 MUST stay unchanged (JSONs unused on disk). Do NOT commit. Report which species you baked + genome provenance + gene counts (cited), and which (if any) you deferred + why.`,
  { label: 'bake', phase: 'Bake', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. determinism GREEN against 0x47a0_3c8f_6701_f240 (data unused on disk → hash-neutral); the new shipped_*_species_loads round-trip tests pass; license green. Report all gates PASS/FAIL with any exact error. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'all_build_roundtrip', 'real_provenance', 'roles_resolve', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'pinned literal unchanged (JSONs unused on disk)' },
    all_build_roundtrip: { type: 'boolean', description: 'each baked species builds + round-trips via SpeciesSpec::build (a test asserts it)' },
    real_provenance: { type: 'boolean', description: 'each bake cites a real pinned NCBI source + is reproducible byte-identical' },
    roles_resolve: { type: 'boolean', description: 'each declared niche.trophic_role resolves via gp::role_from_override' },
    issues: { type: 'array', items: { type: 'string' }, description: 'incl. any deferred species + why' },
  },
}
const verdict = await agent(
  `Adversarially verify the remaining ADR-019 contaminant bakes. Read \`git diff\` + the new bake scripts + JSONs + tests. Try to REFUTE each property; default false if unconfirmable. Confirm the pinned literal is unchanged and each species genuinely builds + round-trips with cited real provenance. List which species were baked vs deferred.`,
  { label: 'verify', phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
)

return { bake, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, verdict }
