export const meta = {
  name: 'visual-declutter-colony-design',
  description:
    'DESIGN (sign-off-ready, no code) for the VISUAL-DECLUTTER / COLONY rendering architecture — the player screen is currently "spammed" with per-organism dots and unreadable. Designs: (a) COLONIES as an OFF-HASH render aggregation — a polygon footprint that unifies a species/VARIANT cluster in a spatial region, layering far better than per-organism rendering; (b) BRUSH-CREATES-COLONY — the CRISPR region brush (ApplyEditRegion) paints a new variant region into a distinct colony, à la Cities-Skylines DISTRICTS; (c) LOD "POP" — each zoom scope, by organism size, pops a selected colony open to reveal individual organisms (colony polygon when zoomed out / small, individuals when zoomed in / big); (d) PLANTS always-visible + most-realistic, belonging to >=1 colony. Must stay inv #2 (no biology in render) + inv #3 (colonies are off-hash — a per-cell variant/colony channel on the GridSnapshot, like dominant_species_id; the pinned literal 0x47a0 is untouched). Connects to [[perf-bigger-maps-needs-structural-change]] (colony aggregation = the LOD structural lever for bigger maps). Produces docs/llm/proposals/visual-declutter-colony-draft.md (an ADR-029 draft + a slice breakdown). DESIGN ONLY — the impl slices are queued from the draft after human sign-off.',
  whenToUse: 'Right after the scenarios arc — the visual-polish epic lead. The screen is cluttered; this designs the colony/LOD/plant-realism architecture before any impl.',
  phases: [{ title: 'Design' }, { title: 'Synthesize' }],
}

const BRIEF =
  `THE BRIEF (the user, 2026-06-28): the play screen is "zaspamovaná" (spammed with per-organism dots) and unreadable. ` +
  `Develop "COLONIES" — a polygon on the map that layers better than individual organisms and UNIFIES a species (incl. ` +
  `VARIATIONS after brush edits — a CRISPR brush edit CREATES A NEW COLONY). Each zoom scope should, by organism SIZE, ` +
  `"pop" selected colonies open to show individual organisms. PLANTS should be the most realistic + ALWAYS visible (future), ` +
  `belong to at least one colony, and the CRISPR brush creates new ones — like splitting a city into DISTRICTS in Cities Skylines. ` +
  `HARD CONSTRAINTS: inv #2 (render is read-only — NO genotype->phenotype in GDScript; biology stays in sim-core), inv #3 ` +
  `(colonies/variant grouping must be OFF-HASH — model the per-cell variant/colony channel on the GridSnapshot exactly like the ` +
  `existing off-hash dominant_species_id channel; the pinned literal 0x47a0_3c8f_6701_f240 must stay byte-identical). This also ` +
  `serves [[perf-bigger-maps-needs-structural-change]]: colony aggregation is the LOD/new-data-layout lever that lets bigger maps ` +
  `render without drawing every organism.`

phase('Design')
const LENSES = [
  { key: 'render-arch', prompt: `LENS A — RENDERING ARCHITECTURE. Design colonies as an OFF-HASH render aggregation: how the renderer derives polygon footprints from the snapshot (the per-cell dominant-species + a NEW off-hash per-cell variant/colony-id channel), how colonies layer/blend (z-order, fill, outline) so the map reads cleanly vs the current per-organism spam, and the draw-cost win (N colonies vs N organisms). READ godot/organisms.gd + godot/species_visual_map.gd + crates/sim-core/src/lib.rs (the GridSnapshot + the existing off-hash dominant_species_id channel = the precedent) + the GSS snapshot versioning. Propose the concrete render path + the off-hash channel.` },
  { key: 'data-determinism', prompt: `LENS B — DATA-MODEL & DETERMINISM. Design the variant/colony identity: how a brush CRISPR region edit (Action::ApplyEditRegion, a disc region) BINDS a region to a new colony/variant (the Cities-Skylines district create), where the colony id lives (an OFF-HASH snapshot channel + a renderer-side colony registry — NOT in the sim hash), and how it survives replay/journal. PROVE it is hash-neutral (inv #3): the colony channel draws no SimRng, is never folded into hash_world, exactly like dominant_species_id; the brush edit itself is the EXISTING journaled ApplyEditRegion. READ crates/harness/src/lib.rs (ApplyEditRegion + RegionSpec) + the snapshot code. Flag any part that would touch the hash as a STOP-THE-LINE.` },
  { key: 'ux-lod', prompt: `LENS C — UX & LOD. Design the zoom-scope "POP" policy: by organism SIZE × zoom scope, when a colony renders as a single polygon vs pops open to individual organisms (the thresholds, the selected-colony pop, smooth transitions), how PLANTS stay always-visible + most-realistic (and belong to >=1 colony), and how the whole thing DECLUTTERS the spammed screen into a readable, Cities-Skylines-districts-like view. READ godot/main.gd (the VIEW+SCOPE switcher + zoom scopes Field/Patch/Cells) + organisms.gd (the per-morph glyphs). Propose the LOD ladder + the plant-realism plan.` },
]
const proposals = await parallel(LENSES.map((l) => () =>
  agent(`${BRIEF}\n\n${l.prompt}\n\nDESIGN ONLY — propose a concrete, invariant-honoring approach (no code). Return your proposal as structured prose: the approach, the key data/render path, the determinism argument, risks, and the slices it implies.`,
    { label: `lens:${l.key}`, phase: 'Design', agentType: 'Plan' })))

phase('Synthesize')
const synth = await agent(
  `Synthesize ONE coherent design from the three lens proposals into a sign-off-ready draft. THE BRIEF:\n${BRIEF}\n\nLENS A (render-arch):\n${proposals[0] || '(none)'}\n\nLENS B (data-determinism):\n${proposals[1] || '(none)'}\n\nLENS C (ux-lod):\n${proposals[2] || '(none)'}\n\n` +
  `WRITE docs/llm/proposals/visual-declutter-colony-draft.md containing: (1) the problem (the spammed screen) + goals; (2) the COLONY model — an off-hash render aggregation (the per-cell variant/colony-id GridSnapshot channel, GSS bump, modelled byte-for-byte on dominant_species_id; renderer derives polygon footprints), with the airtight inv #3 hash-neutrality argument (the channel draws no SimRng, never folded into hash_world; pinned 0x47a0_3c8f_6701_f240 unmoved) + the inv #2 argument (no biology in render); (3) BRUSH-CREATES-COLONY — the ApplyEditRegion disc binds a region to a new variant/colony (Cities-Skylines districts), surviving replay; (4) the LOD "POP" ladder (zoom scope × organism size → colony polygon vs individual organisms; selected-colony pop) + PLANTS always-visible/most-realistic/>=1-colony; (5) the perf link ([[perf-bigger-maps-needs-structural-change]] — colony aggregation as the bigger-maps LOD lever); (6) an ADR-029 draft block; (7) a SLICE BREAKDOWN (e.g. colony-snapshot-channel-impl, colony-polygon-render-impl, lod-pop-impl, brush-colony-binding-impl, plant-realism-impl) each with its hash-risk (✅/🔁/🛑) + acceptance. Mark any hash-touching part STOP-THE-LINE. DESIGN ONLY — no production code. Report the draft path + the slice list + any 🛑 flags.`,
  { label: 'synthesize', phase: 'Synthesize', agentType: 'general-purpose' },
)
return {
  draft: 'docs/llm/proposals/visual-declutter-colony-draft.md',
  synthesis: typeof synth === 'string' ? synth.slice(0, 1200) : synth,
  note: 'DESIGN ONLY — colony impl slices are queued from the draft after human sign-off (the off-hash channel is hash-neutral by design; any hash-touching part is flagged STOP-THE-LINE).',
}
