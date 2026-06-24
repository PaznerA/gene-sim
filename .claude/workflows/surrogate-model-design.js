export const meta = {
  name: 'surrogate-model-design',
  description:
    'Design D3 of the emergent-discovery epic — the SURROGATE MODEL (the "brute-force gradient") that learns config-features → predicted interestingness from the accumulated (config, score) evaluations and BIASES the D2b evolutionary proposer toward promising / DRAMATIC regions (high M3 dynamism + M5 events, not just stable coexistence). A 3-lens design panel (ml-model-choice / determinism+std-only-inv1 / search-integration) grounded on the existing discovery::search (SearchConfig, ScoreVec/breakdown, mutate/crossover, discover_evolved) → a judge pins ONE spec: the config→feature encoding, the std-only GPL-clean model (e.g. a small gradient-boosted regression tree / ridge over integer features — NO heavy GPL ML crate), the training set (the (config, ScoreVec) log of ALL evaluations, not just kept gems — a likely prerequisite slice), the target (overall Q or a drama-weighted target to steer toward limit-cycles/cascades), how it re-ranks candidate proposals to bias the proposer, the crates/discovery trait shape (a swappable Surrogate behind a seam, inv #5), and the test oracle (a known dramatic-vs-dull config set the surrogate must order). Output is a SPEC (no code) the implementer + an ADR consume.',
  whenToUse: 'After D2b (ADR-025) lands, to design D3 the surrogate model before any implementation.',
  phases: [{ title: 'Ground' }, { title: 'Design' }, { title: 'Judge' }],
}

phase('Ground')
const ground = await agent(
  `Scout the gene-sim repo to ground a D3 SURROGATE-MODEL design for the emergent-discovery harness. Read: docs/llm/proposals/emergent-discovery-harness-draft.md (§D3), docs/llm/proposals/discovery-scorer-spec.md, docs/llm/DECISIONS.md ADR-023/024/025 (the scorer + the random + evolutionary search), and the existing code with file:line — crates/discovery/src/search.rs (SearchConfig fields, SearchSpace axes, mutate/crossover/propose_evolved) + crates/discovery/src/lib.rs (ScoreVec{quality, breakdown:[u16;6], fingerprint:[u16;12]}, novelty_l1, ScoreParams) + crates/harness/src/discover.rs (discover_evolved: how configs are proposed/scored/kept; note whether ALL (config, ScoreVec) evaluations are LOGGED anywhere or only the top-K gems are saved). Report, compactly + with file:line: (1) the SearchConfig fields a feature-encoder would read; (2) what scoring signal is available per evaluation (ScoreVec.quality + the per-metric breakdown m1..m6 — esp. M3 dynamism + M5 events, the 'drama' the surrogate should steer toward); (3) whether a (config → score) TRAINING LOG exists or must be ADDED (the discover loop currently saves only top-K gems — confirm); (4) the std-only / GPL-clean constraint (inv #1 — discovery is serde-only today; any ML must be std-only or a permissive non-GPL crate kept at the boundary); (5) the determinism constraint (inv #3 — training + inference must be reproducible/integer-friendly, off the sim hash). Return a grounding brief.`,
  { label: 'ground', phase: 'Ground', agentType: 'Explore' },
)

phase('Design')
const LENSES = [
  { key: 'ml-model', focus: 'ML MODEL CHOICE + FEATURES: the config→feature encoding (species presence bits + normalized counts + containment/temp/season → a fixed integer/fixed-point feature vector) and a SIMPLE, strong, std-only model — a small gradient-boosted regression-tree ensemble or a ridge/linear model over the features — that predicts interestingness from few samples. Justify model vs data-size (we start with tens-to-hundreds of evaluations). Specify training (loss, regularization), and the prediction TARGET — argue for a DRAMA-weighted target (e.g. weight M3 dynamism + M5 events above raw Q) so the surrogate steers the search toward limit-cycles/cascades, not just stable coexistence.' },
  { key: 'determinism-std', focus: 'DETERMINISM + STD-ONLY (inv #1/#3): the surrogate must be reproducible (same training data → same model → same predictions, byte-stable) and keep crates/discovery std+serde — NO heavy GPL ML crate linked (a hand-rolled std-only model, or a permissive non-GPL crate kept BEHIND the process boundary like the ADR-014 sqlite-vec sidecar). Specify integer/fixed-point or carefully-fenced-f64 arithmetic, a deterministic training order (no HashMap iteration, no thread-rng), how the model is serialized (serde) + versioned (a build_id anchor), and how inference stays off the sim hash path.' },
  { key: 'search-integration', focus: 'SEARCH INTEGRATION: how the surrogate BIASES the D2b evolutionary proposer — e.g. each generation propose K*oversample candidate configs, predict each with the surrogate, keep the top-K predicted before paying for the expensive real headless evaluation (a cheap pre-filter); periodically retrain on the growing (config, score) log; keep an explore fraction so the surrogate cannot collapse diversity (it must still find NOVEL gems, not just exploit). Specify the (config, ScoreVec) TRAINING-LOG slice this needs (the discover loop must log every evaluation, not just kept gems), the retrain cadence, the oversample ratio, and the guard against the surrogate killing novelty.' },
]
const designs = await parallel(LENSES.map((l) => () =>
  agent(
    `Design D3 — the emergent-discovery SURROGATE MODEL — through ONE lens.\n\nGROUNDING:\n${ground}\n\nYOUR LENS — ${l.focus}\n\n` +
    `Propose a CONCRETE, BUILDABLE spec for your lens: be specific enough that an implementer could build it on the EXISTING discovery::search + discover_evolved. Respect inv #1 (std+serde / GPL-clean; heavy ML stays at the process boundary), #2 (no biology — it reads config + score numbers), #3 (deterministic/reproducible, off the sim hash), #4 (headless), #5 (a swappable Surrogate trait), #6 (config/operator level). Return the full spec as text.`,
    { label: `design:${l.key}`, phase: 'Design', agentType: 'general-purpose' },
  ),
))

phase('Judge')
const SPEC_SCHEMA = {
  type: 'object',
  required: ['feature_encoding', 'model', 'training_log', 'target', 'search_integration', 'surrogate_trait', 'test_oracle', 'open_questions'],
  properties: {
    feature_encoding: { type: 'string', description: 'the SearchConfig → fixed feature vector (presence bits + normalized counts + env), integer/fixed-point' },
    model: { type: 'string', description: 'the std-only GPL-clean model (e.g. GBT/ridge) + training (loss/regularization/order) + serde serialization + determinism' },
    training_log: { type: 'string', description: 'the (config, ScoreVec) evaluation-log slice D3 needs (the discover loop must log ALL evaluations, not just kept gems) — schema + where it is written, off-hash' },
    target: { type: 'string', description: 'the prediction target — argue for a DRAMA-weighted target (M3 dynamism + M5 events emphasized) to steer toward limit-cycles/cascades vs raw Q' },
    search_integration: { type: 'string', description: 'how the surrogate biases discover_evolved (oversample → predict → pre-filter before the expensive real eval; retrain cadence; the explore-fraction novelty guard)' },
    surrogate_trait: { type: 'string', description: 'the crates/discovery Surrogate trait shape (Rust sketch) so the model is swappable (inv #5)' },
    test_oracle: { type: 'array', items: { type: 'object', required: ['case', 'expect'], properties: { case: { type: 'string' }, expect: { type: 'string' } } }, description: 'concrete dramatic-vs-dull config cases the surrogate must order + a determinism/reproducibility check' },
    open_questions: { type: 'array', items: { type: 'string' }, description: 'decisions needing human sign-off before D3 implementation (esp. the model choice + the drama-target weighting + the std-only-vs-boundary ML decision)' },
  },
}
const spec = await agent(
  `You are the judge synthesizing ONE pinned D3 surrogate-model spec for the gene-sim discovery harness from three lens designs. Keep the strongest, most BUILDABLE, determinism-safe, std-only/GPL-clean ideas; resolve conflicts toward inv #1 (no heavy GPL ML linked) + inv #3 (reproducible) + buildability on the existing discover_evolved.\n\n` +
  `GROUNDING:\n${ground}\n\nML-MODEL DESIGN:\n${designs[0]}\n\nDETERMINISM-STD DESIGN:\n${designs[1]}\n\nSEARCH-INTEGRATION DESIGN:\n${designs[2]}\n\n` +
  `Produce the pinned spec: the feature encoding, the std-only model + training, the (config,score) training-log slice (a likely prerequisite), the DRAMA-weighted target, the search-integration (oversample→predict→pre-filter + retrain cadence + novelty guard), the Surrogate trait shape, the test oracle, and the open questions needing human sign-off (esp. model choice + drama-target weighting + std-only-vs-boundary). This feeds a surrogate-model-impl workflow + an ADR.`,
  { label: 'judge', phase: 'Judge', schema: SPEC_SCHEMA, agentType: 'general-purpose' },
)
return { ground: typeof ground === 'string' ? ground.slice(0, 1000) : ground, spec }
