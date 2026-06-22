export const meta = {
  name: 'ecoli-oversight-gameloop-design',
  description:
    'ADR-017 S4/S5 DESIGN (STOP-THE-LINE): the earned-edit OVERSIGHT game loop (RNG-free score→credit accrual) + the multi-fidelity firewall (Action::RequestEcoliEdit/CommitEcoliImpact with a due_epoch buffer so async deep-compute never leaks wall-clock into the hash). Produces a signoff-ready ADR draft + slice plan + hash-neutral Action scaffolding ONLY — never wires the load-bearing modifier.',
  whenToUse:
    'BATCH 3. The player-agency payoff of the vision: earn credits, edit E. coli, watch the impact ripple across the ecosystem (computed in the background). Design + adversarial determinism verify + hash-neutral serde scaffolding; STOPs before the load-bearing wire (needs F3/F4 + human sign-off).',
  phases: [
    { title: 'Diverge' },
    { title: 'Judge' },
    { title: 'Adversarial' },
    { title: 'SafeInfra' },
  ],
}

phase('Diverge')
const ASCHEMA = {
  type: 'object',
  required: ['economy', 'firewall', 'actions', 'determinism_crux', 'vision_fit', 'repin_notes', 'open_questions'],
  properties: {
    economy: { type: 'string', description: 'RNG-free score→credit accrual at the harness layer (adds 0 bytes to hash); how the player earns edit-credits from sim performance' },
    firewall: { type: 'string', description: 'the multi-fidelity firewall: a due_epoch buffer so the async deep E. coli compute (FBA/oracle subprocess) is journaled and its result only enters the sim at a fixed future epoch — wall-clock NEVER leaks into the hash' },
    actions: { type: 'string', description: 'Action::RequestEcoliEdit + Action::CommitEcoliImpact (serde-default, actions.ndjson back-compatible); how they slot into the existing journaled-replay Action stream' },
    determinism_crux: { type: 'string', description: 'the firewall acceptance test: the hash is byte-identical whether the oracle is absent, slow, or returns different bytes — until the impact is committed at its due_epoch' },
    vision_fit: { type: 'string', description: 'how this realizes the earned-edit OVERSIGHT mode: edit the REAL E. coli, impact computed in the background, ripples across the ecosystem (ties to F4 decomposer loop)' },
    repin_notes: { type: 'string', description: 'why the load-bearing modifier wire (S6) is a later deliberate re-pin; what stays hash-neutral now (scaffolding) vs later (the committed modifier)' },
    open_questions: { type: 'array', items: { type: 'string' } },
  },
}
const ANGLES = [
  'game design & player agency: a legible earn→spend→observe loop where editing E. coli feels consequential and the background-compute delay is a feature, not lag',
  'determinism & async firewall: the due_epoch buffer + journaled slip so an external, variable-latency oracle can NEVER perturb the deterministic hash (the hardest constraint)',
  'biology & FBA fidelity: the edit→impact must be grounded in real E. coli metabolism (a frozen FBA KO table / oracle-fba subprocess), quantized to an integer modifier, not a fabricated number',
]
const proposals = (await parallel(ANGLES.map((angle, i) => () =>
  agent(
    `Design the gene-sim ADR-017 S4/S5 "earned-edit OVERSIGHT game loop + multi-fidelity firewall" through this angle: ${angle}.\n\n` +
    `North-star vision: a fast abstract 30FPS sim where the player periodically EARNS the right to edit the REAL E. coli (the soil decomposer, per the F4 draft); the edit's impact is computed in the BACKGROUND (FBA/oracle) and ripples across the ecosystem. Context: the harness already has journaled replay (seed.json + actions.ndjson) + species-granular Actions + a mission/score grader. crates/oracle-slim is the GPL-subprocess template (inv #1: external tools at the process boundary only). The crux: an external oracle has VARIABLE latency, but the sim hash must stay byte-identical and deterministic — so the result must be buffered to a fixed due_epoch and journaled (no wall-clock leak into the hash, inv #3). The load-bearing EcoliEditModifier wire is a LATER deliberate re-pin (ADR-017 S6), depends on F3/F4 — do NOT design it as merged now. READ docs/llm/proposals/f4-trophic-decomposer-draft.md + the harness Action/replay code first.\n\n` +
    `Return a concrete, file-level architecture. Do NOT write code.`,
    { label: `diverge:angle${i}`, phase: 'Diverge', schema: ASCHEMA },
  ),
))).filter(Boolean)

phase('Judge')
const winner = await agent(
  `Judge these ${proposals.length} OVERSIGHT-loop architectures on {determinism safety of the firewall, player-agency legibility, biological fidelity, re-pin blast radius}. Synthesize ONE winning design, grafting the best ideas. Be explicit about the due_epoch buffer and the firewall acceptance test.\n` +
    proposals.map((p, i) => `\n--- Architecture ${i} ---\n${JSON.stringify(p, null, 2)}`).join('\n'),
  { label: 'judge', phase: 'Judge', schema: ASCHEMA },
)

phase('Adversarial')
const VSCHEMA = {
  type: 'object',
  required: ['wallclock_leaks', 'replay_breaks', 'firewall_test_spec', 'verdict'],
  properties: {
    wallclock_leaks: { type: 'array', items: { type: 'string' }, description: 'paths where oracle latency/availability/output could perturb the hash' },
    replay_breaks: { type: 'array', items: { type: 'string' }, description: 'ways the new Actions could break actions.ndjson back-compat or replay determinism' },
    firewall_test_spec: { type: 'string', description: 'the exact acceptance test: hash byte-identical with oracle absent / slow / different-bytes until commit at due_epoch' },
    verdict: { type: 'string', description: 'ready for human sign-off? what must change first?' },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially stress-test this OVERSIGHT firewall design for wall-clock leaks and replay breaks. Skeptic #${i}, default to flagging. Design:\n${JSON.stringify(winner, null, 2)}\n\nHunt: any way oracle latency/availability/output bytes change the sim hash before due_epoch; any actions.ndjson back-compat break; any non-determinism in the credit accrual. Specify the firewall acceptance test that must exist.`,
    { label: `adversarial:skeptic${i}`, phase: 'Adversarial', schema: VSCHEMA },
  ),
))).filter(Boolean)

phase('SafeInfra')
const infra = await agent(
  `Produce the HASH-NEUTRAL artifacts ONLY for gene-sim ADR-017 S4/S5 — do NOT wire the load-bearing EcoliEditModifier, do NOT change the pinned literal 0xf795_eac4_112f_acd5:\n` +
  `1. Write docs/llm/proposals/ecoli-oversight-gameloop-draft.md = the ADR-017 S4/S5 design: the winning architecture, the adversarial findings + the firewall acceptance test, the exact slice breakdown (S4 economy, S5 Actions+firewall, S6 load-bearing wire as a LATER re-pin), and what stays hash-neutral vs what re-pins.\n` +
  `2. If safe and clearly hash-neutral: add the serde-default Action variants (RequestEcoliEdit/CommitEcoliImpact) as INERT scaffolding (parsed/round-tripped but not yet acted on) with an actions.ndjson back-compat test — ONLY if you can do it without touching the determinism hash; otherwise leave it to the signed-off slice and just specify it in the draft.\n\n` +
  `Winning design:\n${JSON.stringify(winner, null, 2)}\n\nAdversarial findings:\n${JSON.stringify(skeptics, null, 2)}\n\n` +
  `Run \`bash tools/gate.sh\` — it MUST stay green. Do NOT commit. End your report with: "STOP-THE-LINE: the load-bearing EcoliEditModifier (S6) requires F3/F4 + human re-pin sign-off."`,
  { label: 'safe-infra', phase: 'SafeInfra', agentType: 'implementer' },
)

return { winner, skeptics, infra }
