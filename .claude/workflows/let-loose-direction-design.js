export const meta = {
  name: 'let-loose-direction-design',
  description: 'Pick the most compelling + buildable-NOW game direction (campaign / co-op / SpacetimeDB-MMO / score-vs-AI) for gene-sim and scope a concrete vertical slice on the CURRENT engine, with seams for the CHEMOSTAT-J joule economy. DESIGN ONLY — no code.',
  phases: [
    { title: 'Scope', detail: 'one deep scoping per game direction → a buildable vertical slice' },
    { title: 'Judge', detail: 'score on wow × buildability-now × determinism-leverage × invariant-fit' },
    { title: 'Pick', detail: 'choose the winner + finalize the vertical-slice spec' },
  ],
}

const GROUND = [
  'PROJECT gene-sim: an evidence-based deterministic CRISPR ecosystem sim-game. Headless Rust core + read-only Godot renderer. This is an exploratory "let loose" session (renewed token budget) — be BOLD + creative, but the slice must be BUILDABLE NOW and pass tools/gate.sh when marked done.',
  '',
  'CURRENT RUNNABLE ENGINE (build the vertical slice on THIS — it passes the gate today):',
  ' - Deterministic Wright-Fisher sim (crates/sim-core): selection on heritable Genotype + DroughtTol + ThermalTol, coupled to a per-cell SoilField + a player ClimateField (lat/lon/temp/season). Per-organism Position on a 32x32 world grid; lineages disperse + cluster. One seeded ChaCha8Rng; a pinned determinism hash.',
  ' - Player intervention: species-level + REGION-scoped CRISPR edits (the selective brush, ADR-011), a mission + edit-budget game loop (S-G2, "suppress the zone"), all species/region-granular (inv #6).',
  ' - Save/load + REPLAY: a deterministic journal (seed.json + actions.ndjson) reproduces a run bit-exact (crates/harness/replay.rs). This makes MATCHES perfectly replayable + verifiable — a huge asset for competitive/score modes.',
  ' - Godot renderer (read-only snapshots): ecosystem view, data-layer overlays, specimen/L-system view, the main menu, panels.',
  ' - LiveSim gdext node drives an open-ended live run from GDScript (#[func] calls into the core).',
  '',
  'NEW ENGINE FOUNDATION IN PROGRESS (ADR-013 CHEMOSTAT-J — conserved i64-joule economy): LANDED so far = crates/sim-core/src/fixed.rs (largest-remainder apportionment, conserves a total exactly) + crates/sim-core/src/ledger.rs (the conserved-energy Ledger + ledger_closes invariant, scaffolding). NOT yet running = the joule pools / metabolism / emergent births+deaths / multi-species / trophic web (phases F0b…F7). So the slice should build on the CURRENT sim and leave clean SEAMS for the joule economy, not depend on emergent ecology that does not exist yet.',
  '',
  'HARD INVARIANTS (never violate): #3 determinism (seeded ChaCha8, fixed-point, no transcendentals, no HashMap iteration) — replayable matches depend on this; #2 biology in the Rust core, renderer read-only; #1 GPL/external services (incl. any networking/DB like SpacetimeDB, or an AI/LLM opponent) at the PROCESS BOUNDARY only, never linked into the game binary; #6 agency at species/region granularity (an AI opponent acts as an operator over species/regions, NOT per-organism); #7 pinned versions.',
  '',
  'READ to ground: docs/llm/proposals/sci-game-features-scopes-draft.md (the zoom-scopes + feature roadmap) + ecology-substrate-draft.md (the joule engine) + r3-multispecies / rel-relations drafts; docs/llm/DECISIONS.md (ADR-011 the mission loop, ADR-013 the engine); crates/harness/src/{lib.rs,replay.rs,main.rs}; crates/sim-core/src/lib.rs (selection, the region edit, the mission hooks); crates/godot-sim/src/lib.rs (LiveSim).',
].join('\n')

const SCOPE_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['direction', 'concept', 'why_compelling', 'vertical_slice', 'engine_hooks', 'determinism_leverage', 'invariant_fit', 'boundary_services', 'what_is_stubbed', 'wow_factor'],
  properties: {
    direction: { type: 'string', enum: ['campaign', 'co-op', 'spacetimedb-mmo', 'score-vs-ai'] },
    concept: { type: 'string', description: 'the game concept in 3-4 sentences' },
    why_compelling: { type: 'string' },
    vertical_slice: { type: 'string', description: 'the SMALLEST end-to-end playable/scored thing to build NOW on the current engine' },
    engine_hooks: { type: 'array', items: { type: 'string' }, description: 'exactly what it reuses (selection, region edit, replay journal, mission loop, LiveSim, fixed/ledger)' },
    determinism_leverage: { type: 'string', description: 'how it exploits replayable/seeded determinism (verifiable matches, fair scoring, …)' },
    invariant_fit: { type: 'string', description: 'how the AI/opponent/network stays species/region-granular + at the process boundary' },
    boundary_services: { type: 'string', description: 'any subprocess service needed (AI opponent, SpacetimeDB, matchmaking) — kept off the binary (inv #1)' },
    what_is_stubbed: { type: 'array', items: { type: 'string' } },
    wow_factor: { type: 'string', description: 'the single most exciting moment of this slice' },
  },
}

const JUDGE_SCHST = {
  type: 'object', additionalProperties: false,
  required: ['ballots'],
  properties: { ballots: { type: 'array', items: {
    type: 'object', additionalProperties: false,
    required: ['direction', 'wow', 'buildable_now', 'determinism_leverage', 'invariant_fit', 'extensibility', 'total', 'note'],
    properties: {
      direction: { type: 'string' },
      wow: { type: 'integer', description: '1-5' },
      buildable_now: { type: 'integer', description: '1-5, can a real vertical slice land + pass the gate now' },
      determinism_leverage: { type: 'integer', description: '1-5' },
      invariant_fit: { type: 'integer', description: '1-5' },
      extensibility: { type: 'integer', description: '1-5, grows toward the joule engine + the other modes' },
      total: { type: 'integer' },
      note: { type: 'string' },
    },
  } } },
}

const PICK_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['theme', 'direction', 'pitch', 'slice_spec', 'build_plan', 'acceptance', 'stretch', 'engine_seams', 'risks'],
  properties: {
    theme: { type: 'string', description: 'a short kebab name for the branch, e.g. genesis-duel' },
    direction: { type: 'string' },
    pitch: { type: 'string', description: 'the chosen game in a punchy paragraph' },
    slice_spec: { type: 'string', description: 'precisely what the vertical slice is' },
    build_plan: { type: 'array', items: {
      type: 'object', additionalProperties: false,
      required: ['step', 'what', 'touches'],
      properties: { step: { type: 'string' }, what: { type: 'string' }, touches: { type: 'array', items: { type: 'string' } } },
    } },
    acceptance: { type: 'string', description: 'what makes the slice "done" + gate-green' },
    stretch: { type: 'array', items: { type: 'string' } },
    engine_seams: { type: 'string', description: 'how it plugs into the CHEMOSTAT-J joule economy once F3+ lands' },
    risks: { type: 'array', items: { type: 'string' } },
  },
}

phase('Scope')
const DIRS = [
  { key: 'campaign', brief: 'A SINGLE-PLAYER CAMPAIGN: a sequence of authored scenarios (a world + an objective + an edit budget) with progression. Build on the existing mission loop (S-G2). Scope the smallest 2-3 scenario campaign with win/score conditions.' },
  { key: 'co-op', brief: 'A CO-OP mode: two operators share one world, each steering different species/regions toward a shared goal. Determinism + the replay journal make shared deterministic state easy. Scope a local hot-seat / async-journal co-op slice (no live netcode yet).' },
  { key: 'spacetimedb-mmo', brief: 'A future SpacetimeDB-backed MMO: a persistent shared world many operators touch. SpacetimeDB stays a PROCESS-BOUNDARY service (inv #1). Scope the SMALLEST honest slice: the boundary protocol + a deterministic authoritative tick the server could host + one operator action round-tripped. Be honest about what is stubbed.' },
  { key: 'score-vs-ai', brief: 'A score-based VERSUS-AI duel: the player and an AI "rival geneticist" each control a species/region under one shared deterministic environment, competing on an evolutionary objective (dominate allele freq / survive a climate shock / out-adapt). The AI opponent is a species/region operator (inv #6) running at the process boundary or as a scripted policy in-core. Replayable, verifiable matches. Scope a headless match loop + a score.' },
]
const scopes = await parallel(DIRS.map((d) => () =>
  agent(GROUND + '\n\nSCOPE the "' + d.key + '" direction: ' + d.brief + '\nGround every hook in real files. The vertical slice must be buildable on the CURRENT engine + pass the gate. Be bold but honest about what is stubbed. Analysis only.',
    { label: 'scope:' + d.key, phase: 'Scope', schema: SCOPE_SCHEMA, effort: 'high' })))
const scopesJson = JSON.stringify(scopes.filter(Boolean), null, 1)

phase('Judge')
const judges = await parallel(['game-feel', 'engineering-realist'].map((lens) => () =>
  agent(GROUND + '\n\nSCOPED DIRECTIONS:\n' + scopesJson
    + '\n\nAs the "' + lens + '" judge, score EACH direction (1-5) on wow, buildable_now, determinism_leverage, invariant_fit, extensibility. The "engineering-realist" must be harsh on buildable_now (can a real, gate-green vertical slice land THIS session on the current engine?) and on inv #1 (networking/AI at the boundary). Return a `ballots` array, one per direction.',
    { label: 'judge:' + lens, phase: 'Judge', schema: JUDGE_SCHST })))
const judgesJson = JSON.stringify(judges.filter(Boolean).flatMap((j) => j.ballots || []), null, 1)

phase('Pick')
const pick = await agent(
  GROUND + '\n\nSCOPED DIRECTIONS:\n' + scopesJson + '\n\nJUDGE BALLOTS:\n' + judgesJson
  + '\n\nPick the winning direction (highest combined wow × buildability-now × determinism-leverage, invariant-clean) and FINALIZE the vertical-slice spec: a kebab `theme` for the branch, the pitch, the precise slice, a concrete ordered build plan (steps + touched files), the acceptance (gate-green) criteria, stretch goals, the seams to the CHEMOSTAT-J joule economy, and the risks. Favor a slice that is genuinely playable/scored end-to-end on the current engine. Design only.',
  { label: 'pick:winner', phase: 'Pick', schema: PICK_SCHEMA, effort: 'high' })

return { pick, scopes: scopes.filter(Boolean), judges: judges.filter(Boolean) }
