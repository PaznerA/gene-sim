export const meta = {
  name: 'discovery-scenarios-impl',
  description:
    'Named SearchSpace SCENARIO presets for the brute-force auto-research — the "more starters" ask. crates/discovery::search: add named SearchSpace constructors that BIAS the search toward a drama type (predator-prey limit-cycle / decomposer-coexistence / contamination-open / spore-resilience / edit-rescue / extreme-climate) by tuning the per-species include_bp + count ranges, the containment range, the temp band, the season range, and edit_budget. crates/harness::main: a --space <name> CLI flag routing --discover/--evolve to the chosen named space (default name = the current SearchSpace::default, byte-identical), plus a multi-starter batch helper. HASH-NEUTRAL: a named space is an ALTERNATIVE meta-level search config (std+serde, no SimRng, no sim-path change); SearchSpace::default and every existing discovery test stay byte-identical, and the pinned literal 0x47a0_3c8f_6701_f240 is UNMOVED. Then gate + adversarially verify.',
  whenToUse: 'After the first brute-force batch validated the pipeline (Variant Lab D + D2a/D2b). Lets the auto-research target SCENARIOS across multiple named starters instead of only the one default space.',
  phases: [{ title: 'Impl' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Impl')
const s1 = await agent(
  `Implement the named SearchSpace SCENARIO presets + the --space CLI flag for gene-sim (crates/discovery + crates/harness; std+serde meta-level — inv #1/#5; NO sim change). READ FIRST: crates/discovery/src/search.rs IN FULL — SpeciesAxis {key, count_lo, count_hi, include_bp}, SearchSpace {species, containment_lo/hi, temp_lo/hi, season_lo/hi, edit_budget}, SearchSpace::default (the widened 7-free-living-species anchor: default/ecoli/bacillus/pseudomonas/staph/aspergillus-niger/bdellovibrio, with the autotroph "default" at include_bp=SCALE and the rest < SCALE), and the determinism tests (propose_is_byte_identical, propose_respects_space_bounds, widened_space_has_seven_free_living_axes). READ crates/harness/src/main.rs — run_discover + run_discover_evolved (they build SearchSpace { edit_budget, ..SearchSpace::default() } then call harness::discover::discover_in_space / discover_evolved_in_space which take space: &SearchSpace first), the --discover arg block (val("--space") routing point), and the flag-doc block near the top. CLAUDE.md inv #3 (determinism, integer-only, no rand crate, no HashMap iteration) + inv #5 (std+serde boundary).\n\n` +
  `  - Add named SearchSpace constructors to search.rs (e.g. SearchSpace::scenario(name) -> Option<SearchSpace>, or named fns) for: \n` +
  `      * "predator-prey": bdellovibrio include_bp = SCALE (always present) + a strong prey base (ecoli/staph higher include_bp + counts), plant optional; containment 0..=1; temp mid-warm; edit_budget 0. (hunts M3 limit-cycles + M4 predation.)\n` +
  `      * "decomposer": default + ecoli at include_bp = SCALE (both always present), the rest low; containment 0; mid temp; edit_budget 0. (hunts M1/M2 stable coexistence.)\n` +
  `      * "contamination-open": containment 2..=3 (Lab/Open so airborne immigration fires); smaller starting counts; edit_budget 0. (hunts M5 invasion/displacement events.)\n` +
  `      * "spore-resilience": bacillus + aspergillus-niger at high include_bp; temp pushed toward an edge; containment any; edit_budget 0. (hunts M5 crash->regerminate + M6 survival.)\n` +
  `      * "edit-rescue": edit_budget 2..=3 ON; broad species set; (hunts edit-driven M3+M5 flips — the Variant Lab D axis.)\n` +
  `      * "extreme-climate": temp pinned to an edge band (e.g. temp_lo/hi = 150..=300 cold, OR a hot variant 700..=850); broad species; edit_budget 0. (hunts M6 + M1 under stress.)\n` +
  `    Each MUST keep the species axes in a FIXED order with the autotroph anchor first (never reorder — the roster/field order is the determinism contract), be in-bounds (count_lo<=count_hi, ranges within 0..=3 / q16 0..=1000), and never produce an empty roster (the autotroph fallback holds). A name unknown -> None (the caller falls back to default with a logged note).\n` +
  `  - crates/harness/src/main.rs: add a --space <name> flag. When present, build the named space (via the new constructor) and pass it to discover_in_space / discover_evolved_in_space; when absent OR "default", use the existing SearchSpace { edit_budget, ..default() } path BYTE-IDENTICALLY. Document the flag + the names in the flag-doc block. (Keep --edit-budget working: a named space may set its own edit_budget; --edit-budget can still override, your call — document it.)\n` +
  `  - Optionally add a small multi-starter batch convenience (a doc note or a tiny scripts/ helper is fine — NOT required to be production code), but the core deliverable is the named spaces + the flag.\n` +
  `  - Tests (crates/discovery): each named space proposes VALID in-bounds non-empty configs over many trials (mirror propose_respects_space_bounds); the named spaces are DISTINCT from each other + from default (different species presence / ranges); and CRUCIALLY the DEFAULT path is byte-identical — assert propose(seed,trial, &SearchSpace::default()) is unchanged and the existing discovery tests pass. (harness): a --space smoke (the flag routes to the named space; default/absent is the unchanged path).\n` +
  `  - VERIFY the pinned literal 0x47a0_3c8f_6701_f240 is UNMOVED (discovery has no sim dep; the sim runs are pure functions of configs; run cargo test -p discovery + -p sim-core). Build the workspace. Do NOT commit. Report the named-space constructors + the --space routing + confirm the default path is byte-identical + 0x47a0 unmoved.`,
  { label: 'impl', phase: 'Impl', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim (generous timeout ~15 min). The named-space scenarios slice must be GREEN: fmt, clippy, test (incl. the new named-space validity + default-byte-identity tests), determinism MUST stay 0x47a0_3c8f_6701_f240 (the named spaces are meta-level search configs — a moved hash is a FAIL), license green (discovery stays std+serde, no new deps), godot-reader + livesim green. Report every gate PASS/FAIL with exact errors. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['default_space_byte_identical', 'named_spaces_valid_and_distinct', 'hash_neutral_meta_level', 'cli_space_flag_routes', 'issues'],
  properties: {
    default_space_byte_identical: { type: 'boolean', description: 'inv #3: SearchSpace::default + the absent/"default" --space path are UNCHANGED byte-for-byte; every existing discovery test passes; the pinned literal 0x47a0_3c8f_6701_f240 is UNMOVED (determinism gate green).' },
    named_spaces_valid_and_distinct: { type: 'boolean', description: 'Each named scenario space proposes VALID in-bounds non-empty configs (autotroph-anchored, fixed species order, counts in range, env in range) over many trials, and the spaces are genuinely DISTINCT (different species presence / count / containment / temp / edit_budget).' },
    hash_neutral_meta_level: { type: 'boolean', description: 'inv #1/#5: the named spaces are std+serde meta-level search configs — no rand crate, no HashMap iteration, no float in the proposer, no sim-core/SimRng touch; the sim runs stay pure functions of their configs.' },
    cli_space_flag_routes: { type: 'boolean', description: '--space <name> routes --discover/--evolve to the named space via discover_in_space/discover_evolved_in_space; absent or "default" uses the existing path byte-identically; an unknown name degrades to default with a note (no panic).' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify the named SearchSpace scenarios slice (gene-sim). Read \`git diff\` (crates/discovery/src/search.rs + crates/harness/src/main.rs) + CLAUDE.md inv #3/#5. Skeptic #${i} — default each boolean FALSE unless confirmed. Hunt: a MOVED pinned literal 0x47a0_3c8f_6701_f240; ANY change to SearchSpace::default or the existing propose/field-index logic that perturbs the default search (the named spaces must be ADDITIVE — default byte-identical); a named space that reorders the species axes or drops the autotroph anchor (breaks determinism / produces empty rosters); an out-of-bounds range (count_lo>count_hi, containment outside 0..=3, temp outside 0..=1000); a rand-crate/HashMap/float creeping into the proposer; a --space path that panics on an unknown name instead of degrading. Report the structured verdict with file:line. Do NOT edit.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['default_space_byte_identical', 'named_spaces_valid_and_distinct', 'hash_neutral_meta_level', 'cli_space_flag_routes']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  impl: typeof s1 === 'string' ? s1.slice(0, 700) : s1,
  gate: typeof gate === 'string' ? gate.slice(0, 500) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — named scenario starters; default byte-identical; hash-neutral meta-level' : 'NEEDS WORK',
}
