export const meta = {
  name: 'sp4-codex-content-design',
  description:
    'SP-4 codex content: web-research + draft engaging, evidence-based phenology/ontology/taxonomy descriptions for the in-game codex — the current species (abstract plant autotroph, E. coli K-12 decomposer, Bdellovibrio predator), their anchor genes (gltA/ptsG/pflB/pta/ldhA + GO/SO terms), trophic roles, traits, the four trophic flows (light influx / mineralization / predation / chem), and life-cycle/phenology. Maps the content onto the in-game inspect/tooltip/codex surface. DESIGN/RESEARCH ONLY — no sim code, no cargo/gate (a content proposal doc; parallel-safe).',
  whenToUse:
    'Parallel fast-progress content design while the predator/SP-1 implementation runs. Produces docs/llm/proposals/sp4-codex-content-draft.md (codex entries + the UI surface plan) + enriches TAXONOMY.md/GLOSSARY.md.',
  phases: [
    { title: 'Research' },
    { title: 'Design' },
  ],
}

phase('Research')
const RSCHEMA = {
  type: 'object',
  required: ['entries', 'phenology', 'ontology', 'sources'],
  properties: {
    entries: { type: 'string', description: 'per organism/gene/role: an accurate, engaging, evidence-based description (what it is, why it matters, its famous facts)' },
    phenology: { type: 'string', description: 'life-cycle / timing biology (e.g. Bdellovibrio biphasic attack/growth phases; bacterial growth phases; the plant abstraction\'s seasonality)' },
    ontology: { type: 'string', description: 'the GO/SO term meaning for the anchor genes (gltA/GO-4108 citrate synthase; ptsG/GO-8982; pflB/GO-8861; pta/GO-8959; ldhA/GO-8720; so_term gene) — what the molecular function IS' },
    sources: { type: 'array', items: { type: 'string' }, description: 'URLs / citations' },
  },
}
const TOPICS = [
  'E. coli K-12 MG1655 as the DECOMPOSER: the organism (model bacterium, ~4.6 Mb), and the 5 anchor genes the sim uses as CRISPR/trait levers — gltA (citrate synthase, TCA entry, GO:0004108), ptsG (glucose PTS transporter, GO:0008982), pflB (pyruvate formate-lyase, GO:0008861), pta (phosphate acetyltransferase / acetate overflow, GO:0008959), ldhA (D-lactate dehydrogenase, GO:0008720): molecular function, why each is a meaningful knockdown target, the central-carbon-metabolism story',
  'Bdellovibrio bacteriovorus as the PREDATOR: the biphasic life cycle (free-swimming ATTACK phase → invades the prey periplasm → intracellular GROWTH phase → lyses the host → release), host range (gram-negative bacteria incl. E. coli), genome, host-independent survival/dormancy — the phenology that justifies the sim dormancy mechanic; why it is the "living antibiotic"',
  'the ABSTRACT PLANT autotroph + trophic ROLES + the 4 flows: photosynthesis/autotrophy as the primary producer; the trophic-role taxonomy (Autotroph/Heterotroph/Mixotroph/Decomposer/Predator); the four sim flows (solar light influx, decomposer mineralization of detritus→nutrient, predation, chemical/allelopathy) framed as real microbial-ecology phenomena (the soil microbiome nutrient cycle, allelopathy, predator-prey)',
]
const research = (await parallel(TOPICS.map((topic, i) => () =>
  agent(
    `Web-research this topic for the gene-sim SP-4 codex (in-game educational descriptions) and return ACCURATE, CITED, engaging content: ${topic}.\n\n` +
    `Use web search (find WebSearch/WebFetch via ToolSearch). Prioritize authoritative sources (UniProt/EcoCyc/NCBI/reviews). The content must be evidence-based AND readable (a curious player should learn something true + interesting). Return structured findings.`,
    { label: `research:t${i}`, phase: 'Research', schema: RSCHEMA },
  ),
))).filter(Boolean)

phase('Design')
const draft = await agent(
  `Write docs/llm/proposals/sp4-codex-content-draft.md — the SP-4 codex content + UI surface plan for gene-sim, from this researched material. READ docs/llm/TAXONOMY.md + docs/llm/GLOSSARY.md (the existing data-model/terms), data/species/*.json (the species), and crates/sim-core/src/gp.rs (the Trait/TrophicRole/GO anchors) first.\n\n` +
  `Deliver:\n` +
  `1. CODEX ENTRIES — for each current species (abstract plant, E. coli K-12 decomposer, Bdellovibrio predator), each anchor gene (gltA/ptsG/pflB/pta/ldhA with GO/SO), each trophic role, and each of the 4 trophic flows: a short, accurate, ENGAGING description (phenology = life-cycle/timing; ontology = the GO/SO molecular-function meaning; taxonomy = classification/relationships), with the famous facts that make it memorable. Cite sources.\n` +
  `2. UI SURFACE PLAN — how this content reaches the player: inspect panel (click an organism/cell), tooltips (genes/traits/roles), and a CODEX panel (browsable encyclopedia), tied to the existing godot inspect/specimen/relations UI; renderer-only (inv #2).\n` +
  `3. EXTENSIBILITY — the content schema (so future species — the contamination/symbiont set: Mycoplasma, Bacillus, Carsonella, Hodgkinia… — slot in) + a note linking to the contamination epic (those genomes are codex gold).\n` +
  `4. Paste-ready enrichment snippets for docs/llm/TAXONOMY.md + GLOSSARY.md.\n\n` +
  `Accurate + cited + readable. Do NOT run cargo/gate (a content doc; parallel-safe). Do NOT commit. End with a one-paragraph roadmap entry for SP-4.\n\n` +
  `Research:\n${JSON.stringify(research, null, 2)}`,
  { label: 'draft', phase: 'Design', agentType: 'implementer' },
)

return { research, draft }
