export const meta = {
  name: 'verify-relations-graph',
  description:
    'Adversarially verify the renderer-only relations node-link GRAPH slice (Roadmap #4): new godot/relations_graph.gd (species nodes sized by population + colored via SpeciesVisualMap, edges = measured FlowMatrix flows), the Graph/Matrix toggle + set_data feed in main.gd, and the new --roster / --steps shot flags. ZERO Rust → pinned hash 0x47a0_3c8f_6701_f240 trivially unmoved. Three skeptics read the diff + the new file and hunt: an inv #2 violation (any biology / flow derivation / genotype→phenotype in GDScript rather than projecting the core-measured FlowMatrix + population exports); an inv #3 risk (any Rust touched); a FlowMatrix-index ↔ species-array MISALIGNMENT (edges drawn for the wrong pair); a _draw runtime crash / unguarded index / null font; a broken degrade path (file-replay / older cdylib without flow_matrix or observe_species); and whether the new --roster/--steps flags arm the roster BEFORE reset (inv #3 load-bearing order).',
  whenToUse: 'After implementing the relations-graph slice + a visual shot + gate GREEN, before merge.',
  phases: [{ title: 'Verify' }],
}

const VSCHEMA = {
  type: 'object',
  required: ['no_biology_in_gdscript', 'hash_neutral_no_rust', 'index_alignment_correct', 'draw_safe_and_degrades', 'roster_armed_before_reset', 'ux_faithful', 'issues'],
  properties: {
    no_biology_in_gdscript: { type: 'boolean', description: 'inv #2: relations_graph.gd + the new main.gd code only PROJECT core exports (the measured FlowMatrix flat i64, observe_species names/keys/roles/population_size, SpeciesVisualMap color lookup) into nodes/edges/labels. No flow derivation, no trophic math, no genotype→phenotype decided in GDScript. The only arithmetic is display scaling (max-abs / max-pop normalization + ring layout).' },
    hash_neutral_no_rust: { type: 'boolean', description: 'inv #3: the diff touches ZERO Rust (only godot/*.gd + the new relations_graph.gd). The pinned literal 0x47a0_3c8f_6701_f240 cannot move. If ANY crates/** file is in the diff, set false.' },
    index_alignment_correct: { type: 'boolean', description: 'The graph node arrays (names/keys/roles/pops from observe_species in SpeciesId order) and the FlowMatrix index (flat[i*s+j], same SpeciesId order by construction) are aligned: edge a→b reads flat[b*s+a] oriented source→sink exactly like main.gd _format_flow_summary, so an edge is never drawn for the wrong species pair, and set_data degrades to nodes-only when s != n / flat is short.' },
    draw_safe_and_degrades: { type: 'boolean', description: '_draw has no unguarded index / null-font / degenerate-polygon crash; an empty roster (n=0) returns early; a single species draws one centered node; the arrowhead triangle is non-degenerate. The Graph/Matrix toggle swaps visibility without leaving both shown or both hidden.' },
    roster_armed_before_reset: { type: 'boolean', description: 'The new --roster flag parses "stem:count,…" and calls _apply_roster INSIDE _apply_cli_environment (which runs before _do_reset), so the multi-species roster + the single RNG seed-once happen in the load-bearing order (inv #3). --steps advances the deterministic core only. Neither flag changes the pinned-config hash (they are opt-in shot flags).' },
    ux_faithful: { type: 'boolean', description: 'Matches the user ask "I expected a node-link GRAPH but only see a heatmap": the graph is the DEFAULT representation, edges/arrows agree with the narrated Primary-flows line + the matrix, nodes are sized by population and colored consistently with the ecosystem map (same SpeciesVisualMap table).' },
    issues: { type: 'array', items: { type: 'string' }, description: 'concrete problems (file:line), empty if none' },
  },
}

phase('Verify')
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify the gene-sim "relations node-link graph" slice on branch auto/relations-graph-2026-06-23. Read \`git diff main...HEAD\` (or \`git diff\` if uncommitted) AND the full new file godot/relations_graph.gd, plus the changed regions of godot/main.gd (the RelationsGraph preload, _build_relations_ui toggle, _refresh_relations set_data feed, _relations_species_arrays, _on_relations_rep_selected/_apply_relations_rep, and the --roster/--steps CLI flags + _parse_roster_arg). Also read godot/relations_heatmap.gd (the proven sibling), godot/species_visual_map.gd, and CLAUDE.md inv #2/#3.\n\n` +
    `Skeptic #${i} — default each boolean FALSE unless you can positively confirm it from the code. Hunt hard for:\n` +
    `  • inv #2: ANY biology / flow derivation / trophic or genotype→phenotype math done in GDScript instead of PROJECTING the core-measured FlowMatrix + populations. The graph must only lay out finished integers + look up a color.\n` +
    `  • inv #3: confirm ZERO Rust changed (→ the hash 0x47a0_3c8f_6701_f240 cannot move). Flag any crates/** edit.\n` +
    `  • FlowMatrix index ↔ species-array MISALIGNMENT: is edge a→b really reading flat[b*s+a] with the same source→sink orientation as main.gd _format_flow_summary? Could a roster/observe_species order mismatch draw an edge for the wrong pair? Does set_data degrade to nodes-only (no edges) when s != n or flat is short?\n` +
    `  • _draw safety: unguarded array index, null theme font, degenerate arrowhead polygon, n==0 early return, single-node centering.\n` +
    `  • degrade paths: file-replay (no _live), older cdylib without flow_matrix()/observe_species() → graph shows nodes-or-nothing without crashing; the heatmap still works.\n` +
    `  • the --roster/--steps shot flags: does --roster arm via _apply_roster BEFORE _do_reset (load-bearing, inv #3)? Do they leave the pinned-config (no-flag) hash untouched?\n` +
    `  • UX: graph is the DEFAULT; edges agree with the narrated flows + the matrix; nodes sized by population + colored like the map.\n\n` +
    `Report the structured verdict with specific file:line in issues. Do NOT edit anything.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)

const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['no_biology_in_gdscript', 'hash_neutral_no_rust', 'index_alignment_correct', 'draw_safe_and_degrades', 'roster_armed_before_reset', 'ux_faithful']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  skeptics,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — graph is renderer-only, hash-neutral, aligned, faithful' : 'NEEDS WORK',
}
