export const meta = {
  name: 'verify-morphotype-glyphs',
  description:
    'Adversarially verify the renderer-only per-cell MORPHOTYPE glyph slice (Roadmap #5): organisms.gd now routes a non-plant cell to its dominant species morphotype glyph (rod/cocci/vibrioid/pleomorph/symbiont/mold) at the Cells scope, fed by species_visual_map.gd build_table carrying "morph". ZERO Rust → pinned hash 0x47a0_3c8f_6701_f240 trivially unmoved. Three skeptics read the diff and hunt: an inv #2 violation (any biology / genotype→phenotype in the new draw code rather than a per-species morphotype LOOKUP + trait-free primitives), an inv #3 risk (any Rust touched), a _draw runtime crash (degenerate polygon / null / OOB) in the 5 new morphotype draws, a broken per-zoom LOD gate (morphotype glyphs must only draw at the Cells scope; Field stays sized colored dots), and a determinism leak (the per-organism jitter must be _hash01, never randf/time).',
  whenToUse: 'After implementing the morphotype-glyph slice + a visual shot + gate GREEN, before merge.',
  phases: [{ title: 'Verify' }],
}

const VSCHEMA = {
  type: 'object',
  required: ['no_biology_in_gdscript', 'hash_neutral_no_rust', 'draw_safe_no_crash', 'lod_gate_correct', 'deterministic_jitter', 'ux_faithful', 'issues'],
  properties: {
    no_biology_in_gdscript: { type: 'boolean', description: 'inv #2: the morphotype is a per-species LOOKUP (SpeciesVisualMap.morph_for via _species_table); the 5 new draw fns are trait-free geometric primitives modulated only by already-expressed fitness/density scalars + the species color. No genotype→phenotype, no biology decided in GDScript.' },
    hash_neutral_no_rust: { type: 'boolean', description: 'inv #3: ZERO Rust changed (only godot/organisms.gd + godot/species_visual_map.gd). Pinned literal 0x47a0_3c8f_6701_f240 cannot move. Flag any crates/** edit.' },
    draw_safe_no_crash: { type: 'boolean', description: 'No runtime crash in _draw_cocci/_draw_vibrioid/_draw_pleomorph/_draw_symbiont/_draw_mold: no degenerate draw_colored_polygon (they use circles/lines/polylines), no OOB, no null; radii/widths are maxf-floored; the dispatcher falls back to the rod for an unknown morph. Confirmed renderable (a multi-species --zoom shot drew them without error).' },
    lod_gate_correct: { type: 'boolean', description: 'Per-zoom refinement: the morphotype glyphs draw only inside the `_sprites_on and not lod_dots_only` branch (the Cells scope); at the Field scope (cell < LOD_MIN_CELL) the draw still falls to the sized colored _draw_dot. So Field = colored density dots, Cells = morphotype glyphs.' },
    deterministic_jitter: { type: 'boolean', description: 'inv #3: every per-organism variation in the new draws comes from _hash01(x,y,k) — never randf()/Time — so a snapshot renders byte-identically. The new draws add no RNG.' },
    ux_faithful: { type: 'boolean', description: 'The morphotype routing matches the species (cocci=staph, vibrioid=Bdellovibrio, mold=Aspergillus/Penicillium, pleomorph=Mycoplasma, symbiont=Carsonella/Syn3, rod=E.coli/Bacillus), echoing the specimen view; size hierarchy (plant/mold large ≫ rods/cocci ≫ vibrioid/symbiont tiny) reads. Completes ADR-021 follow-up.' },
    issues: { type: 'array', items: { type: 'string' }, description: 'concrete problems (file:line), empty if none' },
  },
}

phase('Verify')
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify the gene-sim "per-cell morphotype glyphs" slice on branch auto/map-size-tuning-2026-06-23. Read \`git diff main...HEAD\` (or \`git diff\`) AND the full godot/organisms.gd (esp. _draw, _cell_visual, _draw_morph + the 5 new _draw_<morph> fns) and godot/species_visual_map.gd (build_table + morph_for). Also read CLAUDE.md inv #2/#3.\n\n` +
    `Skeptic #${i} — default each boolean FALSE unless positively confirmed. Hunt for:\n` +
    `  • inv #2: ANY biology / genotype→phenotype decided in GDScript. The morphotype must be a per-species LOOKUP (SpeciesVisualMap.morph_for, carried in _species_table); the glyph shapes must be trait-free primitives modulated only by already-expressed fitness/density + the species color.\n` +
    `  • inv #3: confirm ZERO Rust changed (→ hash 0x47a0_3c8f_6701_f240 cannot move). Flag any crates/** edit.\n` +
    `  • _draw crash risk in the 5 new morph draws: degenerate polygon, null, OOB index, zero/negative radius. Confirm they use circles/lines/polylines (no draw_colored_polygon triangulation trap) and maxf-floor sizes.\n` +
    `  • per-zoom LOD gate: do the morphotype glyphs draw ONLY in the Cells-scope branch (_sprites_on and not lod_dots_only)? At the Field scope (cell < LOD_MIN_CELL) does it still fall to the sized _draw_dot? (Field = dots, Cells = glyphs.)\n` +
    `  • determinism: every per-organism jitter is _hash01(x,y,k), never randf()/Time (a snapshot must render byte-identically — and this is render-only off the hash anyway).\n` +
    `  • UX: morph routing matches the species + echoes the specimen view; the size hierarchy reads.\n\n` +
    `Report the structured verdict with file:line in issues. Do NOT edit anything.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)

const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['no_biology_in_gdscript', 'hash_neutral_no_rust', 'draw_safe_no_crash', 'lod_gate_correct', 'deterministic_jitter', 'ux_faithful']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  skeptics,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — morphotype glyphs renderer-only, hash-neutral, safe, faithful' : 'NEEDS WORK',
}
