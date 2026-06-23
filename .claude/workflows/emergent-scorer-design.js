export const meta = {
  name: 'emergent-scorer-design',
  description:
    'Design the D0 INTERESTINGNESS SCORER for the emergent-run discovery harness (docs/llm/proposals/emergent-discovery-harness-draft.md §D0). A 3-lens design panel (ecology-meaning / determinism+reproducibility / signal-vs-noise) each proposes a concrete metric set + weights grounded in what the headless core ALREADY exports (per-gen population[species], the i64 FlowMatrix, allele/fitness, extinction/boom/immigration events), then a judge synthesizes ONE pinned scorer spec: the metric list with the exact integer/quantized formula per metric over the per-gen trace, the combine weights, the D1 trace schema the scorer reads, the crates/discovery trait shape, and the test oracle (a known dramatic seed scores high, a flat/dead seed low). Output is a SPEC (no code) the implementer + an ADR will consume.',
  whenToUse: 'Starting the emergent-discovery epic (Roadmap #6 D0/D1), before any crates/discovery code.',
  phases: [{ title: 'Ground' }, { title: 'Design' }, { title: 'Judge' }],
}

// Ground the design in the ACTUAL available exports + determinism constraints first (so the metrics are buildable).
phase('Ground')
const ground = await agent(
  `Scout the gene-sim repo to ground a discovery-harness interestingness SCORER design. Read: docs/llm/proposals/emergent-discovery-harness-draft.md (the epic), CLAUDE.md (the 7 invariants), docs/llm/DECISIONS.md ADR-013 (CHEMOSTAT-J economy) + ADR-014 (relations sidecar) + ADR-021 (GSS5). Then find + summarize, with file:line, EXACTLY what a headless run already exposes that a per-generation trace could record WITHOUT touching the sim hash (inv #3): the harness entry points (run_headless / the harness crate / record_episode / seed.json + journal replay), the per-species observation (observe_species/observe_all: population, allele, fitness, the species key/role), the FlowMatrix export (flow_matrix: the flat i64 s*s), and any existing per-gen stats / event hooks (extinction/boom/immigration). Note which are std-only-reachable from a NEW crates/discovery analysis crate vs. only across the godot-sim boundary. List the concrete signals available + any gaps the D1 trace must add. Return a compact grounding brief (bullet list, file:line anchors).`,
  { label: 'ground', phase: 'Ground', agentType: 'Explore' },
)

// Three independent lenses each propose a full scorer spec.
phase('Design')
const LENSES = [
  { key: 'ecology-meaning', focus: 'ECOLOGICAL MEANING: which metrics capture genuinely interesting emergent ecology — sustained multi-species coexistence + evenness (Shannon/Simpson), trophic structure (a non-trivial multi-level FlowMatrix), trophic cascades (predator crash→prey boom→producer crash), contamination recoveries (an immigrant establishes + reshapes the web), limit cycles. Reward drama + structure, NOT forced stability (memory: open-system, extinction is valid). Tie each metric to a real ecological phenomenon.' },
  { key: 'determinism-repro', focus: 'DETERMINISM + REPRODUCIBILITY (inv #3): every metric must be an INTEGER/QUANTIZED, RNG-free function of the per-gen trace so a gem score is byte-reproducible; the scorer reads traces, never perturbs the sim hash; a gem = (seed, EnvConfig, journal) that replays bit-identically (the R2 round-trip). Specify exact integer formulas (fixed-point where ratios are needed), the quantization, and how the score stays stable across platforms.' },
  { key: 'signal-vs-noise', focus: 'SIGNAL vs NOISE + SEARCHABILITY: the score must rank runs usefully for a gradient-free then learned search — monotonic-ish, not dominated by one trivial signal, robust to run length, with a NOVELTY term (fingerprint distance from saved gems) so the search keeps the gem set diverse not 100 near-identical booms. Specify the run fingerprint vector, the novelty metric, normalization, and degenerate-case handling (instant death scores low, flat monoculture scores low, a single boom is not maximal).' },
]
const designs = await parallel(LENSES.map((l) => () =>
  agent(
    `Design the D0 interestingness scorer for the gene-sim emergent-discovery harness through ONE lens.\n\nGROUNDING (what the core already exports — build only on these):\n${ground}\n\n` +
    `YOUR LENS — ${l.focus}\n\n` +
    `Propose a CONCRETE, BUILDABLE spec: (1) the metric list, each with its EXACT integer/quantized formula over the per-generation trace (population[species] per gen, the flat i64 FlowMatrix per gen or its end-state, allele/fitness, the extinction/boom/immigration event list); (2) the combine function + weights into one scalar (or small vector) score; (3) the D1 TRACE SCHEMA the scorer needs (the minimal per-gen record — keep it compact); (4) the crates/discovery trait shape (a Scorer trait so the metric set is swappable, inv #5); (5) a TEST ORACLE — name 2-3 concrete run archetypes that MUST score high (e.g. a predator/prey limit cycle, a contamination recovery) and 2-3 that MUST score low (instant collapse, flat monoculture). Respect inv #2 (the scorer only READS exports), #3 (RNG-free/integer/off-hash), #4 (headless), #6 (config/operator-level). Be specific enough that an implementer could build crates/discovery from your spec. Return the full spec as text.`,
    { label: `design:${l.key}`, phase: 'Design', agentType: 'general-purpose' },
  ),
))

// Judge synthesizes ONE pinned spec from the three.
phase('Judge')
const SPEC_SCHEMA = {
  type: 'object',
  required: ['metrics', 'combine', 'trace_schema', 'scorer_trait', 'test_oracle', 'open_questions'],
  properties: {
    metrics: { type: 'array', items: { type: 'object', required: ['name', 'formula', 'weight', 'why'], properties: { name: { type: 'string' }, formula: { type: 'string', description: 'exact integer/quantized formula over the per-gen trace' }, weight: { type: 'string', description: 'combine weight + rationale' }, why: { type: 'string', description: 'the emergent phenomenon it captures' } } } },
    combine: { type: 'string', description: 'how the metrics combine into the final score (+ the novelty term + fingerprint)' },
    trace_schema: { type: 'string', description: 'the compact per-generation D1 trace record the scorer reads (fields + types), and where the harness emits it (off-hash)' },
    scorer_trait: { type: 'string', description: 'the crates/discovery Scorer trait shape (Rust signature sketch) so the metric set is swappable (inv #5)' },
    test_oracle: { type: 'array', items: { type: 'object', required: ['archetype', 'expect'], properties: { archetype: { type: 'string' }, expect: { type: 'string', description: 'high | low + why' } } } },
    open_questions: { type: 'array', items: { type: 'string' }, description: 'decisions needing human sign-off before D0/D1 implementation' },
  },
}
const spec = await agent(
  `You are the judge synthesizing ONE pinned D0 interestingness-scorer spec for the gene-sim discovery harness from three lens designs. Keep the strongest, most BUILDABLE, determinism-safe ideas from each; resolve conflicts toward inv #3 (integer/RNG-free/off-hash) + buildability on the ACTUAL exports.\n\n` +
  `GROUNDING:\n${ground}\n\n` +
  `ECOLOGY-MEANING DESIGN:\n${designs[0]}\n\nDETERMINISM-REPRO DESIGN:\n${designs[1]}\n\nSIGNAL-VS-NOISE DESIGN:\n${designs[2]}\n\n` +
  `Produce the pinned spec: the final metric set (each with an exact integer/quantized formula + weight + the phenomenon it captures), the combine function incl. the novelty/fingerprint term, the compact D1 trace schema + where the harness emits it off-hash, the crates/discovery Scorer trait shape, the test oracle (high/low archetypes), and the open questions needing human sign-off. This spec feeds the discovery-harness-impl workflow + an ADR.`,
  { label: 'judge', phase: 'Judge', schema: SPEC_SCHEMA, agentType: 'general-purpose' },
)
return { ground: typeof ground === 'string' ? ground.slice(0, 1200) : ground, spec }
