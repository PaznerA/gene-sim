export const meta = {
  name: 'contamination-research-design',
  description:
    'Web-research + implementation-proposal for the CONTAMINATION-as-default-reality epic. Fans out cited web research on (A) airborne/clean-room contaminants (Mycoplasma genitalium, Bacillus subtilis, Pseudomonas, Staphylococcus, Aspergillus/Penicillium, Cutibacterium) and (B) minimal-genome/endosymbionts (Carsonella ruddii, Hodgkinia cicadicola, JCVI Syn3.0) → adversarially verifies the key biological claims → designs the contamination mechanic (deterministic journaled immigration/inoculation events, configurable consortia, a containment/sterility knob, establish/displace/die dynamics, the cull counter-play, two modes: contaminants vs symbionts/minimal-life) grounded in the conserved deterministic core + the SP-3 seed tool. Produces docs/llm/proposals/contamination-immigration-draft.md (ADR draft + slice plan + contaminant SpeciesSpec data plan + SP-4 codex hooks). DESIGN/RESEARCH ONLY — no sim code, no re-pin.',
  whenToUse:
    'Schedule right after the predator/SP-1 work. Research-heavy; uses web search; produces a signoff-ready proposal + roadmap entry. Hash-neutral (a proposal doc only).',
  phases: [
    { title: 'Research' },
    { title: 'Verify' },
    { title: 'Design' },
  ],
}

// ── Phase 1: parallel cited web research per cluster ──
phase('Research')
const RSCHEMA = {
  type: 'object',
  required: ['findings', 'genome_facts', 'ecology', 'sim_relevance', 'sources'],
  properties: {
    findings: { type: 'string', description: 'the key biology: who/what, the contamination or symbiosis mechanism, what makes it establish-or-die' },
    genome_facts: { type: 'string', description: 'genome size (bp) + gene count + notable reduction/features, per organism, with the number explicitly cited' },
    ecology: { type: 'string', description: 'trophic role / metabolism / niche; for contaminants: spores/filters/resistance/biofilms; for symbionts: host-dependence, organelle boundary, lineage splitting' },
    sim_relevance: { type: 'string', description: 'how it maps to gene-sim (a SpeciesSpec genome+trophic role; an airborne contaminant immigration event vs a host-requiring symbiont mode; cull-susceptibility)' },
    sources: { type: 'array', items: { type: 'string' }, description: 'URLs / citations for the load-bearing facts' },
  },
}
const CLUSTERS = [
  'AIRBORNE BACTERIAL CONTAMINANTS — the clean-room/cell-culture invaders: Mycoplasma genitalium (no cell wall → passes 0.22µm filters, hard to detect, penicillin-resistant; also a minimal-cell model, JCVI Syn3.0 basis), Bacillus subtilis (heat/desiccation-resistant ENDOSPORES, ubiquity), Pseudomonas (aeruginosa/fluorescens — biofilms, metabolic generalist), Staphylococcus (epidermidis/aureus — skin flora), Cutibacterium acnes (skin, anaerobe). Genome sizes, contamination mechanism, what lets each ESTABLISH in a culture',
  'FUNGAL / MOLD CONTAMINANTS — airborne spores: Aspergillus (niger/fumigatus) and Penicillium — spore dispersal, ubiquity, why they dominate contaminated plates; their trophic role (saprotroph/decomposer) and genome scale',
  'MINIMAL GENOMES & OBLIGATE ENDOSYMBIONTS — the reduced-genome axis: Carsonella ruddii (~160 kb, ~182 genes, psyllid endosymbiont), Hodgkinia cicadicola (cicada endosymbiont, genome SPLITTING into co-dependent lineages), JCVI-syn3.0 / Mycoplasma mycoides minimal cell (~531 kb, ~473 genes), the cell↔organelle blurred boundary, why these CANNOT live freely (host-dependence) — i.e. why they are a SEPARATE mode, not airborne contaminants',
  'SYNTHETIC ECOLOGY & CONTAINMENT PRACTICE — the game frame: how defined microbial consortia / synthetic communities fight contamination in practice; clean-room / BSL / sterility classes and contamination pressure; invasion & establishment ecology (when does an immigrant establish, displace residents, or die — propagule pressure, niche availability, priority effects); real examples of consortium contamination',
]
const research = (await parallel(CLUSTERS.map((cluster, i) => () =>
  agent(
    `Web-research this cluster for the gene-sim CONTAMINATION epic and return CITED findings: ${cluster}.\n\n` +
    `Use web search (find the WebSearch/WebFetch tools via ToolSearch, query "web search fetch"). Prioritize primary/authoritative sources (papers, NCBI/genome DBs, reviews). Every load-bearing NUMBER (genome size, gene count) must carry a citation. Be accurate — this feeds a real evidence-based sim + an educational codex (SP-4). Return the structured findings.`,
    { label: `research:c${i}`, phase: 'Research', schema: RSCHEMA },
  ),
))).filter(Boolean)

// ── Phase 2: adversarial fact-check of the load-bearing claims ──
phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['confirmed', 'corrected', 'contaminant_vs_symbiont_split', 'verdict' ],
  properties: {
    confirmed: { type: 'array', items: { type: 'string' }, description: 'claims verified against an independent source (with the number + citation)' },
    corrected: { type: 'array', items: { type: 'string' }, description: 'claims that were wrong/imprecise + the correction + source' },
    contaminant_vs_symbiont_split: { type: 'string', description: 'verify the biology of the split: which species can free-live & airborne-contaminate vs which are obligate host-dependent (cannot "fly in") — the load-bearing design distinction' },
    verdict: { type: 'string', description: 'are the research facts solid enough to design on? what remains uncertain?' },
  },
}
const verified = await agent(
  `Adversarially fact-check the gene-sim contamination research below. Independently web-search (ToolSearch WebSearch/WebFetch) to CONFIRM or CORRECT the load-bearing claims — especially every genome size / gene count, and the critical design distinction: which organisms can free-live & airborne-contaminate vs which are OBLIGATE host-dependent endosymbionts that cannot "fly in". Demand a citation for each number; flag anything unverifiable.\n\n` +
  `Research:\n${JSON.stringify(research, null, 2)}`,
  { label: 'verify', phase: 'Verify', schema: VSCHEMA },
)

// ── Phase 3: synthesize the implementation proposal ──
phase('Design')
const proposal = await agent(
  `Using the VERIFIED contamination research, write docs/llm/proposals/contamination-immigration-draft.md — a research synthesis + implementation proposal for gene-sim. READ first: docs/llm/proposals/ecology-substrate-draft.md (the conserved deterministic core), the SP-3 intervention design (.claude/workflows/sp3-intervention-panel-impl.js — the seed/inoculation tool + journaled region Actions), crates/genome/src/spec.rs (SpeciesSpec) + data/species/*.json, and the gp.rs TrophicRole seam.\n\n` +
  `The proposal MUST cover:\n` +
  `1. THE FRAME — contamination as the default state of reality (the clean-room metaphor) as emergent gameplay.\n` +
  `2. THE MECHANIC — deterministic, journaled IMMIGRATION/INOCULATION events: a configurable consortium (a menu set of contaminant SpeciesSpecs) that "fly in" on a schedule/trigger; each is a SpeciesSpec (genome + trophic role) spawned at a position; the event is a journaled Action (reuses the SP-3 2nd-wave seed/inoculate tool) → fully reproducible. Establish / displace-residents / die-out dynamics emerge from the conserved joule economy (a poorly-adapted contaminant starves; a well-adapted one invades). RNG-free or single-stream, conserved (immigrants' J from a named influx tap), ordered — likely HASH-NEUTRAL (no events in the pinned config).\n` +
  `3. THE CONTAINMENT / STERILITY KNOB — a sandbox parameter setting contamination PRESSURE (frequency/size/diversity of immigration events); dirtier → more pressure; the player counters with cull/antibiotic + their resident consortium. Determinism: the knob seeds a deterministic event schedule, no wall-clock.\n` +
  `4. THE TWO MODES (the verified biology split): (A) AIRBORNE CONTAMINANTS — free-living invaders (Mycoplasma, Bacillus spores, Pseudomonas, Staph, Aspergillus/Penicillium, Cutibacterium), each a baked SpeciesSpec; (B) SYMBIONTS / MINIMAL GENOMES — a separate host-dependence/minimal-life mode (Carsonella, Hodgkinia, Syn3.0) that requires a host and CANNOT airborne-contaminate (with the evidence for why).\n` +
  `5. DATA PLAN — the contaminant SpeciesSpec bake plan (real NCBI genomes where feasible, like ecoli.json/bdellovibrio.json; the trophic roles; spore/biofilm/resistance traits mapped onto the existing genome/trait seams) + the SP-4 codex hooks (these genomes are famous — feed the phenology/ontology/taxonomy descriptions).\n` +
  `6. ADR DRAFT + SLICE PLAN — the immigration-event system, the consortium config, the containment knob, the data bakes, the UI; what is hash-neutral vs a re-pin; dependencies on the SP-3 seed tool; a roadmap entry to add to docs/llm/TASKS.md.\n\n` +
  `Cite the verified facts. Keep biology in the core (inv #2); determinism-first. Run \`bash tools/gate.sh\` to confirm the doc-only change is green. Do NOT commit. End with a one-paragraph roadmap entry ready to paste into TASKS.md.\n\n` +
  `Verified research:\n${JSON.stringify(verified, null, 2)}\n\nRaw findings:\n${JSON.stringify(research, null, 2)}`,
  { label: 'proposal', phase: 'Design', agentType: 'implementer' },
)

return { research, verified, proposal }
