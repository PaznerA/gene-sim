export const meta = {
  name: 'colony-polygon-render-impl',
  description:
    'ADR-029 S2 (renderer-only — the visible de-spam): a new godot/colonies.gd (Node2D sibling UNDER organisms.gd) that draws COLONY POLYGONS instead of per-organism dot-spam at Field scope. Deterministic connected-components (4-connectivity, two-pass union-find over a width*height row-major int array — NO Dictionary/hash-order iteration, inv #3 in the renderer) over (dominant_species_id, dominant_variant_id) from the GSS6 snapshot; marching-squares contour + Douglas-Peucker + one Chaikin smoothing -> fill (draw_colored_polygon, species color via SpeciesVisualMap.color_for, value by mean fitness) + outline (draw_polyline, width by cell_count) + centered label (glyph + pop). A MIN_COLONY_CELLS haze floor for specks. Wired in main.gd._show with snap.dominant_variant_id + snap.dominant_species_id + the visual table. ZERO RUST: no sim-path change, so the pinned literal 0x47a0_3c8f_6701_f240 is trivially UNMOVED; no biology in GDScript (inv #2 — geometry only, reads the two off-hash channels, computes no genotype->phenotype). Read docs/llm/proposals/visual-declutter-colony-draft.md S2 + the Critical files list first. Then gate + adversarially verify.',
  whenToUse: 'After ADR-029 S1 (the GSS6 dominant_variant_id channel landed). The first VISIBLE colony slice — turns the spammed dot map into readable district polygons. S3-S6 build on it.',
  phases: [{ title: 'Impl' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Impl')
const s1 = await agent(
  'Implement ADR-029 S2 — colony polygon rendering (RENDERER-ONLY GDScript; ZERO Rust; the pinned literal 0x47a0_3c8f_6701_f240 must stay trivially unmoved because the sim path is untouched). READ FIRST: docs/llm/proposals/visual-declutter-colony-draft.md section 4 (the LOD/footprint context) + section 7 S2 (the exact spec) + the Critical files list (the real anchors). Then READ the real renderer surface: godot/organisms.gd (the current per-cell dot draw at Field scope — _draw_plant / _draw_morph, the field-space LOD around lines 23/186 — this is the SPAM S2 replaces with polygons; reuse its species color + glyph idioms), godot/main.gd (_show feeds the snapshot to the draw layers; SCOPES ~:104; _set_zoom/_set_scope ~:2725), godot/snapshot.gd (the GSS6 reader landed in S1 — snap.dominant_species_id + snap.dominant_variant_id planes, channel_count 14), godot/species_visual_map.gd (color_for/size_for + SIZE_* ~:21-27). CLAUDE.md inv #2 (render-only, NO genotype->phenotype in GDScript) + inv #3 (DETERMINISM IN THE RENDERER: connected-components must NOT iterate a Dictionary / hash order — use a row-major width*height int array + ordered iteration only).\n\n' +
  '  - NEW godot/colonies.gd as a Node2D sibling UNDER organisms.gd (added to the scene / instantiated by main.gd the same way organisms.gd is). It owns the colony polygon layer.\n' +
  '  - CONNECTED-COMPONENTS (deterministic): over the per-cell key (dominant_species_id, dominant_variant_id), 4-connectivity, a two-pass union-find over a single row-major width*height Int array (label image), iterate cells in row-major order ONLY — NO Dictionary/hash-order iteration anywhere (inv #3 in the renderer). Empty/zero-pop cells are background. Build all per-colony aggregates (cell_count, sum-of-fitness for mean, centroid, species id, variant id) in one ordered pass into arrays indexed by the compacted colony id.\n' +
  '  - GEOMETRY: per colony, marching-squares contour of its label-image mask -> Douglas-Peucker simplify -> ONE Chaikin smoothing pass. Draw: fill via draw_colored_polygon (species base color from SpeciesVisualMap.color_for, value/brightness by mean fitness), outline via draw_polyline (width scaled by cell_count), a centered label (species glyph + population count). A MIN_COLONY_CELLS floor: tiny specks render as a soft haze, not full districts (anti-flicker, risk #2 in the draft).\n' +
  '  - WIRE in main.gd._show: pass snap.dominant_variant_id + snap.dominant_species_id + the SpeciesVisualMap table to colonies.gd; show the colony layer at Field scope (the de-spam). Keep organisms.gd available for the closer scopes (S3 will add the LOD pop crossfade — S2 just needs the Field-scope polygons drawn). Guard cleanly if the snapshot lacks the dominant_variant_id plane (older snapshot) so nothing crashes.\n' +
  '  - inv #2: colonies.gd computes GEOMETRY ONLY (connected-components / contour / label = presentation). It reads the two off-hash IDENTITY channels + the visual table; it computes NO genotype->phenotype, NO biology.\n' +
  '  - BUILD + macOS-SAFE smoke: build the cdylib (bash run.sh or the documented build) — it MUST still build with zero Rust diff. Then attempt a headless/windowed --shot at Field scope on a sample run via the macOS-SAFE capture (timeout + FILE capture, NEVER a $(godot...) pipe that hangs on macOS; --shot needs a GPU so WINDOWED; SKIP cleanly + say so if no display). If a --shot is possible, confirm the Field-scope frame shows POLYGONS not dot-spam (single-species -> 1 territory, a brushed-disc run -> a nested sub-polygon). If no display, prove de-spam at the code level (the Field-scope draw path now emits draw_colored_polygon per colony, not a dot per organism) + run the existing godot smoke (livesim_smoke.gd) so the project still loads.\n' +
  '  - CONFIRM zero Rust diff: git diff --stat must show NO crates/ changes (only godot/ + maybe a doc). The pinned literal is therefore byte-identical by construction. Do NOT commit. Report: the colonies.gd CC algorithm (proving row-major / no hash-order iteration), the main.gd wiring, the de-spam evidence (shot or code-level), and confirm zero Rust diff.',
  { label: 'impl', phase: 'Impl', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  'Run bash tools/gate.sh for gene-sim (generous timeout ~15 min). ADR-029 S2 is RENDERER-ONLY — it must be GREEN: fmt, clippy, test, determinism MUST stay 0x47a0_3c8f_6701_f240 (S2 touches ZERO Rust, so the literal is byte-identical by construction — confirm git diff --stat shows no crates/ change), the godot snapshot byte gate (still GSS6/channels=14 from S1), license green, godot-reader + livesim_smoke green (the new colonies.gd must load + the project still runs). Report every gate PASS/FAIL with exact errors + EXPLICITLY confirm 0x47a0 is unmoved and there is no Rust diff. No fixes, no commit.',
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['renderer_only_zero_rust_hash_unmoved', 'cc_deterministic_no_hash_iteration', 'no_biology_in_render', 'despam_polygons_not_dots', 'issues'],
  properties: {
    renderer_only_zero_rust_hash_unmoved: { type: 'boolean', description: 'S2 is GDScript-only: git diff shows NO crates/ (Rust) change, so the pinned literal 0x47a0_3c8f_6701_f240 is byte-identical by construction; the snapshot format stays GSS6/channels=14 (S1, unchanged); no sim-path touched.' },
    cc_deterministic_no_hash_iteration: { type: 'boolean', description: 'inv #3 in the renderer: connected-components iterates a row-major width*height int array in order (two-pass union-find), NOT a Dictionary / hash order; all per-colony aggregates are built in one ordered pass into arrays indexed by colony id. No source of renderer non-determinism.' },
    no_biology_in_render: { type: 'boolean', description: 'inv #2: colonies.gd computes GEOMETRY ONLY (connected-components / marching-squares contour / DP+Chaikin / label) from the off-hash dominant_species_id + dominant_variant_id channels + the visual table; it computes NO genotype->phenotype, no biology.' },
    despam_polygons_not_dots: { type: 'boolean', description: 'At Field scope the map now draws COLONY POLYGONS (draw_colored_polygon per colony) instead of a dot per organism — proven by a macOS-safe --shot (single-species -> 1 territory; brushed disc -> nested sub-polygon) OR, on a no-display box, by the code path (the Field-scope draw emits O(#colonies) polygons, not O(organisms) dots) plus a green livesim smoke.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    'Adversarially verify ADR-029 S2 (colony polygon rendering — renderer-only). Read git diff (godot/colonies.gd + godot/main.gd + any godot/ touch) + docs/llm/proposals/visual-declutter-colony-draft.md section 7 S2 + CLAUDE.md inv #2/#3. Skeptic #' + i + ' — default each boolean FALSE unless PROVEN. Hunt: ANY crates/ (Rust) diff (would expose the hash — S2 must be GDScript-only; if Rust changed, confirm 0x47a0_3c8f_6701_f240 is still byte-identical and flag the unexpected change); connected-components iterating a Dictionary / hash order (renderer non-determinism — must be a row-major int array + union-find); biology computed in colonies.gd (inv #2 — must be geometry only, reading the two off-hash channels); the de-spam NOT actually happening (still dot-per-organism at Field scope rather than polygon-per-colony); a stale 13-channel assumption or a crash when the dominant_variant_id plane is present. Report the structured verdict with file:line. Do NOT edit.',
    { label: 'verify:skeptic' + i, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['renderer_only_zero_rust_hash_unmoved', 'cc_deterministic_no_hash_iteration', 'no_biology_in_render', 'despam_polygons_not_dots']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  impl: typeof s1 === 'string' ? s1.slice(0, 800) : s1,
  gate: typeof gate === 'string' ? gate.slice(0, 600) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — colony polygons (de-spam); renderer-only, zero Rust, 0x47a0 byte-identical; deterministic CC; no biology in render' : 'NEEDS WORK',
}
