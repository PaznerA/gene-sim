export const meta = {
  name: 'specimen-view-upgrade-impl',
  description:
    'A proper upgrade of the SPECIMEN view (hash-neutral, inv #2): evidence-based, distinct, trait-driven MORPHOLOGY for all baked species (plant L-system stays; add real bacterial/mold/spore-former/mycoplasma/symbiont glyphs — rods + flagella, mold hyphae + conidiophore chains, visible endospores when sporulating, wall-less pleomorphic mycoplasma, tiny minimal-genome symbionts), a RICH per-specimen inspect (genome loci/genes + traits with values + trophic role + edit history + the SP-4 codex description), and better browse/compare layout. Folds in the deferred SP-4 codex content WITH its res:// staging fix (run.sh + release.yml + the gate) so it ships + surfaces in the specimen inspect. Renderer + content only; biology stays in the core.',
  whenToUse:
    'After the review-fixes merge. The specimen view needs a substantial visual + informational upgrade (the user: "UI specimen potřebuje pořádné vylepšení"). Renderer-only; the pinned literal 0x47a0 stays unchanged. Autonomous; stops for human commit.',
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
  required: ['morphology', 'inspect', 'codex_integration', 'staging', 'layout', 'hash_neutrality', 'slices'],
  properties: {
    morphology: { type: 'string', description: 'the evidence-based, trait-driven glyph/form per species type: plant L-system (exists); bacterial rods/cocci/vibrioid (E.coli flagella, Bdellovibrio comma+flagellum, Pseudomonas/Staph) with biofilm; spore-formers (Bacillus) showing an endospore when sporulating; mold (Aspergillus/Penicillium) hyphae + conidiophore + brlA→abaA→wetA conidia chains; wall-less pleomorphic Mycoplasma; tiny minimal-genome symbionts (Carsonella/Syn3.0). Driven by the exported traits + trophic role + dormancy/spore state.' },
    inspect: { type: 'string', description: 'the rich per-specimen inspect panel: genome (loci/genes), each trait with value + a short codex blurb, the trophic role, the gene anchors (gltA/ptsG/…), the CRISPR edit history / lineage; reads core observe/observe_all/loci exports' },
    codex_integration: { type: 'string', description: 'how the SP-4 codex content (docs/llm/proposals/sp4-codex-content-draft.md) becomes a structured res:// data file surfaced in the specimen inspect + tooltips (species/gene/role/flow entries)' },
    staging: { type: 'string', description: 'the res:// staging fix that blocked the earlier SP-4: stage data/codex into godot/data/codex in run.sh + release.yml (.deb + .zip) + tools/check_godot_snapshot.sh (+ a byte-equality check), mirroring the data/species convention; so the codex ships + the gate exercises the real content path' },
    layout: { type: 'string', description: 'the specimen view layout/interaction: browse/select specimens (the existing per-species log), compare, the trait readout; a clean panel' },
    hash_neutrality: { type: 'string', description: 'why hash-neutral: renderer + content only; reads core exports; the pinned literal 0x47a0 untouched' },
    slices: { type: 'array', items: { type: 'string' } },
  },
}
const LENSES = [
  'visual fidelity & evidence-based morphology: each species reads as ITSELF at a glance — a mold is hyphae+conidia, a spore-former shows a spore, Bdellovibrio is a tiny comma predator, Mycoplasma is a wall-less blob, a symbiont is a minimal speck; trait variation visibly drives size/shape/colour/state',
  'information & the codex: the specimen inspect teaches — genome, traits with real meaning, trophic role, gene anchors, edit history, the SP-4 codex blurbs; this is the educational heart (and the natural home for the deferred SP-4 content)',
  'robustness & determinism: renderer-only (inv #2 — reads core exports, no biology in GDScript), hash-neutral (literal 0x47a0 untouched), and the codex data MUST be res://-staged reproducibly (run.sh/release.yml/gate) so it ships + the gate tests the real path — AND the GDScript must parse clean (the earlier SP-4 died on a parse error: self-check with a headless --check)',
]
const proposals = (await parallel(LENSES.map((lens, i) => () =>
  agent(
    `Design a proper upgrade of the gene-sim SPECIMEN view through this lens: ${lens}.\n\n` +
    `Context: the specimen view (godot/main.gd mode 1, the V toggle) today renders a plant L-system (godot/lsystem.gd) + a microbe rod (godot/microbe.gd) per species, with a trait readout, fed from the per-species observe()/observe_all() exports. 12 species are baked (default plant, ecoli, bdellovibrio, 7 contaminants: mycoplasma/bacillus/pseudomonas/staph/cutibacterium/aspergillus-niger/penicillium, 2 symbionts: carsonella/syn3). The SP-4 codex content draft (docs/llm/proposals/sp4-codex-content-draft.md) is on main but its UI was DEFERRED (gate RED: a GDScript parse error + the codex JSON was never staged into res:// — run.sh/release.yml/check_godot_snapshot mirror only data/species). The core exports: observe/observe_all (per-species phenotype + key + role), loci, snapshot, flow_matrix. KEEP all biology in the core (inv #2). The pinned literal 0x47a0_3c8f_6701_f240 MUST stay unchanged. READ main.gd (specimen view + _build_specimen_ui + the inspect path), lsystem.gd, microbe.gd, the sp4-codex-content-draft, run.sh + tools/check_godot_snapshot.sh + .github/workflows/release.yml (the staging), and crates/sim-core/src/gp.rs (Trait/TrophicRole/GO anchors) first.\n\n` +
    `Return a concrete file-level design. Do NOT write code.`,
    { label: `design:lens${i}`, phase: 'Design', schema: DSCHEMA },
  ),
))).filter(Boolean)
const chosen = await agent(
  `Judge & synthesize these ${proposals.length} specimen-upgrade designs into ONE plan. Pin the per-species morphology mapping, the rich inspect, the codex data + its res:// STAGING (the thing that blocked SP-4 — must be reproducible in run.sh/release.yml/gate), the layout, and the hash-neutrality. Output the final design.\n` +
    proposals.map((p, i) => `\n--- Design ${i} ---\n${JSON.stringify(p, null, 2)}`).join('\n'),
  { label: 'design:judge', phase: 'Design', schema: DSCHEMA },
)

phase('Implement')
const impl = await agent(
  `Implement this agreed gene-sim SPECIMEN view upgrade — GDScript + content + staging ONLY (do NOT touch crates/** biology; reading core exports is fine):\n${JSON.stringify(chosen, null, 2)}\n\n` +
  `Build: (1) the evidence-based per-species morphology (extend/replace microbe.gd + the specimen render path so each species type — bacterial rod/coccus/vibrioid, mold hyphae+conidia, spore-former with endospore, wall-less mycoplasma, tiny symbiont — renders distinctly + trait-driven; plant L-system stays); (2) the rich specimen inspect (genome/traits+codex-blurbs/role/gene-anchors/edit-history from the core exports); (3) the SP-4 codex as a structured res:// data file surfaced in the inspect + tooltips; (4) the res:// STAGING fix — stage data/codex in run.sh + .github/workflows/release.yml (.deb + .zip) + tools/check_godot_snapshot.sh with a byte-equality check (mirror the data/species lines), so the codex ships + the gate exercises the real content path (this is what blocked SP-4 — get it right). KEEP biology in the core (inv #2). The pinned literal 0x47a0_3c8f_6701_f240 MUST stay unchanged. CRITICAL: after writing the GDScript, SELF-CHECK it parses (a headless godot --check / import) BEFORE reporting — the earlier SP-4 attempt died on an uncaught parse error. Do NOT commit. Report file:line + confirm the GDScript parses clean + the codex stages reproducibly.`,
  { label: 'impl', phase: 'Implement', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. determinism GREEN against 0x47a0_3c8f_6701_f240 (renderer-only → hash-neutral); the godot-reader gate MUST be green WITH the codex staged (no parse errors, codex data present in res://); livesim green. Report all gates PASS/FAIL with any exact error. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'inv2_preserved', 'morphology_distinct', 'inspect_rich', 'codex_staged', 'parses_clean', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'pinned literal unchanged; no biology in GDScript' },
    inv2_preserved: { type: 'boolean', description: 'GDScript reads core exports + renders; no genome/phenotype logic' },
    morphology_distinct: { type: 'boolean', description: 'each species type renders as a distinct, evidence-based, trait-driven form (mold/spore/rod/symbiont/mycoplasma all read as themselves)' },
    inspect_rich: { type: 'boolean', description: 'the specimen inspect shows genome/traits/role/gene-anchors/edit-history + codex blurbs' },
    codex_staged: { type: 'boolean', description: 'the codex data is res://-staged reproducibly (run.sh + release.yml + check_godot_snapshot byte-check) → ships + the gate exercises it (the SP-4 blocker is fixed)' },
    parses_clean: { type: 'boolean', description: 'the GDScript parses with NO errors (the headless --check / godot-reader gate is green) — the SP-4 parse-error trap is avoided' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const verdict = await agent(
  `Adversarially verify the specimen view upgrade. Read \`git diff\`. Try to REFUTE each property; default false if unconfirmable. The KEY checks: does the GDScript PARSE clean (no repeat of the SP-4 parse error)? is the codex data res://-STAGED reproducibly (run.sh + release.yml + the gate byte-check — not a hand-placed unreproducible artifact)? does each species render distinctly? is the pinned literal unchanged + no biology in GDScript?`,
  { label: 'verify', phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
)

return { chosen, impl, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, verdict }
