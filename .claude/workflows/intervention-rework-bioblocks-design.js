export const meta = {
  name: 'intervention-rework-bioblocks-design',
  description:
    'DESIGN + light research ONLY (no production code): the INTERVENTION REWORK — "BioBlocks". Rework today\'s low-level tool brush (TOOL_CRISPR apply_edit_region poke-a-locus + the player-snapshot Variant Lab) into a pleasant, block-based ("BioBlocks") composer — snap standard BioBrick part blocks (promoter/RBS/CDS/terminator, Sequence-Ontology-typed, grammar-guided so only compatible shapes connect = the closed-world FELT) — PLUS a library of ready-made iGEM Registry BBa_* devices (one-click "připravené" edits). RCT-style browser + datasheets + effect preview + the OVERSIGHT credit economy by device complexity. Builds ON the SBOL+BioBricks foundation (parts = SBOL Components SB3, snap-validation = the SB1 validator, apply = a journaled SBOL-grounded edit). Renderer-side UI (inv #2 — GDScript marshals inert part/device ids; the catalog/grammar-validation/device-resolution/genotype->phenotype stay in crates/sbol+crates/genome). Light web-research the iGEM Registry (BBa_* parts/devices to seed the library + the DATA LICENSING vs inv #1 + the non-commercial stance). Adversarially verify (inv #1/#2/#3 + SBOL-grounding coherence), then EXPAND the seed docs/llm/proposals/intervention-rework-bioblocks-draft.md into a spec + an ADR-draft + the IR1..IR5 slice plan. DESIGN ONLY — doc-only, hash-neutral, NO code/Cargo. The impl slices gate on SBOL SB1-SB3.',
  whenToUse: 'On the user go for the intervention-rework epic. The buildable UX + library spec, for the gameplay layer on the SBOL foundation. Design can run now; the impl slices depend on SBOL SB1-SB3.',
  phases: [{ title: 'Lenses' }, { title: 'Synthesize' }, { title: 'Review' }],
}

phase('Lenses')
const LENS = [
  { key: 'bioblocks-ux', angle: 'the BioBlocks COMPOSER UX: a block-based (Scratch/Blockly idiom) snap canvas for genetic parts where SHAPE encodes the SO role so only grammar-compatible blocks connect (the assembly grammar is felt, not read); the RCT-style library browser (left list + big right canvas/preview, the scenario-selector idiom the user liked); the effect preview before commit; how it reads as a little gene cassette. How much to lead with the one-click ready-device library vs the compose-from-blocks canvas (library-first for "příjemné", composer for depth).' },
  { key: 'igem-library-and-licensing', angle: 'the iGEM ready-made library: WHICH real iGEM Registry BBa_* parts + devices (knockout / overexpression cassette / reporter / logic gate / metabolic switch) seed the library, their datasheets (function/strength), and grounding them as SBOL Components (SB3). THE LICENSING (inv #1, web-research parts.igem.org terms): can a NON-commercial game reference BBa_* ids + functions; may it bundle sequences; the data-use terms — vs the non-monetization stance. Flag what must be reference-only.' },
  { key: 'rework-and-determinism', angle: 'reworking the CURRENT interventions onto the part model + the determinism: TOOL_CRISPR (apply_edit_region) -> "apply the composed/selected device"; the regional operators (PCR/Antibiotic/Nutrient/Toxin/Inoculate) reskinned; the Variant Lab (_saved_variants) -> the player saved-devices shelf; the OVERSIGHT credit cost ∝ device complexity. The APPLY path: a device -> a validated (closed-world, SBOL SB1) journaled edit — reuse apply_edit/apply_edit_region or a new ApplyDevice action that resolves to them; KEEP the pinned single-plant config neutral (no devices -> 0x47a0_3c8f_6701_f240 byte-identical, the ADR-029 colony-brush precedent). inv #2 (UI renderer-only, biology in core) + inv #3.' },
]
const lenses = await parallel(LENS.map((l) => () =>
  agent(
    'DESIGN LENS "' + l.key + '" for the intervention-rework "BioBlocks" epic (DESIGN ONLY — no code). READ: docs/llm/proposals/intervention-rework-bioblocks-draft.md (the seed) + docs/llm/proposals/sbol-biobricks-integration-draft.md (the SBOL foundation it builds on — the parts catalog SB3, the SB1 validator, the closed-world gate) + godot/main.gd (the CURRENT interventions to rework: TOOL_CRISPR/PCR/ANTIBIOTIC/NUTRIENT/TOXIN/INOCULATE ~:244-261, _apply_active_tool, the brush, the Variant Lab _saved_variants ~:311-324 + reseed, the OVERSIGHT panel) + CLAUDE.md inv #1/#2/#3. Angle: ' + l.angle + '\n\nDeliver a concrete design for THIS angle (web-research the iGEM angle if relevant — cite parts.igem.org / the registry terms): the UX/data/rework specifics, how it preserves the invariants, the concrete RISKS + mitigations, and how it grounds on SBOL. Return structured text for a synthesis judge. Do NOT edit any file.',
    { label: 'lens:' + l.key, phase: 'Lenses', agentType: 'general-purpose' },
  ),
))
const lensText = lenses.filter(Boolean).map((t, i) => '### ' + LENS[i].key + '\n' + (typeof t === 'string' ? t : JSON.stringify(t))).join('\n\n')

phase('Synthesize')
const spec = await agent(
  'You are the JUDGE. Using the lenses below + the seed docs/llm/proposals/intervention-rework-bioblocks-draft.md (READ IT) + the SBOL spec it builds on, EXPAND the seed in place into a buildable spec. Lenses:\n\n' + lensText.slice(0, 9000) + '\n\n' +
  'Edit docs/llm/proposals/intervention-rework-bioblocks-draft.md so it contains, with citations where claimed (esp. iGEM licensing): (1) the BioBlocks composer UX + the RCT-style library browser (concrete layout, the shape-encodes-role grammar, the effect preview); (2) the iGEM ready-edits library — the seed device set (real BBa_* where verifiable, else clearly placeholder), grounded as SBOL Components, + the inv #1 LICENSING verdict (reference-only vs bundle, per parts.igem.org terms + the non-commercial stance); (3) the rework of the current tools/Variant-Lab/OVERSIGHT onto the device model; (4) the APPLY path (device -> a validated SBOL-grounded JOURNALED edit; reuse apply_edit/apply_edit_region or a new ApplyDevice; the pinned config stays neutral -> 0x47a0 byte-identical); (5) the inv-audit (inv #2 UI renderer-only; inv #3 journaled+hash-neutral-for-pinned; inv #1 iGEM data; inv #5 library as data); (6) the IR1..IR5 slice plan with deps on SBOL SB1-SB3 + which are hash-neutral; (7) an ADR-draft block (reserve a free ADR number beyond the current max — note ADR-035 reserved on a branch, ADR-036 worker-thread, ADR-037 SBOL closed-world; state your number). Keep "DESIGN ONLY — sign-off for the SBOL-dependent + any hash-touching slices" at the top. Then RETURN a ~350-word executive summary. WRITE the doc; touch NO production code/Cargo.',
  { label: 'judge-expands-spec', phase: 'Synthesize', agentType: 'general-purpose' },
)

phase('Review')
const RSCHEMA = {
  type: 'object',
  required: ['ui_renderer_only_biology_in_core', 'apply_journaled_hash_neutral_for_pinned', 'igem_licensing_inv1_clean', 'grounds_on_sbol_coherently', 'issues'],
  properties: {
    ui_renderer_only_biology_in_core: { type: 'boolean', description: 'inv #2: the BioBlocks composer + library browser are renderer-side (GDScript marshals inert part/device ids + composition order); the parts catalog, the assembly-grammar/closed-world validation, the device→genome resolution, and genotype→phenotype stay in crates/sbol+crates/genome. No biology in GDScript.' },
    apply_journaled_hash_neutral_for_pinned: { type: 'boolean', description: 'inv #3: applying a device is a deterministic JOURNALED edit (reusing apply_edit/apply_edit_region or a new ApplyDevice resolving to them); new action variants are hash-relevant only for runs that use them — the pinned single-plant config issues none → 0x47a0_3c8f_6701_f240 byte-identical (the ADR-029 colony-brush precedent).' },
    igem_licensing_inv1_clean: { type: 'boolean', description: 'inv #1: the iGEM Registry data-use is checked (cited) — BBa_* ids + functions referenced; sequence bundling only if the terms + the non-commercial stance permit, else reference-only; no license violation. Any heavy/networked SBOL/registry tool stays subprocess-only.' },
    grounds_on_sbol_coherently: { type: 'boolean', description: 'The design coherently builds ON the SBOL foundation: the part blocks ARE SBOL Components (SB3), the snap-validation is the SB1 validator (the closed-world gate), a device is an SBOL design; the IR impl slices correctly gate on SBOL SB1-SB3; it absorbs/refines SB6 without contradiction.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const reviews = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    'Adversarially review the EXPANDED intervention-rework "BioBlocks" design in docs/llm/proposals/intervention-rework-bioblocks-draft.md (DESIGN-ONLY — accurate, implementable, invariant-safe; no code yet). Read the doc + the SBOL spec it depends on + godot/main.gd (the current interventions) + CLAUDE.md inv #1/#2/#3/#5. Skeptic #' + i + ' — default each boolean FALSE unless the doc establishes it (with citations for iGEM licensing). Hunt: biology/genome logic placed in GDScript (inv #2 — the composer must marshal ids only, validation/resolution in core); a device-apply that is NOT a journaled deterministic edit or that would move the pinned literal for the single-plant config (inv #3); an iGEM data-licensing miss (bundling registry sequences without verifying the terms / the non-commercial stance — inv #1); a device-apply that bypasses the SBOL closed-world validation; an incoherent SBOL dependency (parts NOT grounded as SBOL Components, or the IR slices not gating on SB1-SB3); an uncited/hallucinated BBa_* part or registry claim asserted as fact. Report the structured verdict with the design section + EXPLICITLY whether iGEM licensing is inv #1-clean and the pinned config stays neutral. Do NOT edit.',
    { label: 'review:skeptic' + i, phase: 'Review', schema: RSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => reviews.filter((s) => s[k]).length
const keys = ['ui_renderer_only_biology_in_core', 'apply_journaled_hash_neutral_for_pinned', 'igem_licensing_inv1_clean', 'grounds_on_sbol_coherently']
const sound = keys.every((k) => tally(k) >= 2)
return {
  summary: typeof spec === 'string' ? spec.slice(0, 1400) : spec,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: reviews.flatMap((s) => s.issues || []),
  verdict: sound ? 'DESIGN SOUND — BioBlocks intervention rework ready to present (renderer-only, journaled+pinned-neutral, iGEM licensing clean, SBOL-grounded)' : 'DESIGN NEEDS WORK — gaps flagged before sign-off',
}
