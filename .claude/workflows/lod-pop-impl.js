export const meta = {
  name: 'lod-pop-impl',
  description:
    'ADR-029 S3 (renderer-only): the LOD POP ladder. Replace the binary scope-layer swap (Field=colonies XOR closer=organisms) with the per-colony footprint ladder keyed on the ON-SCREEN organism footprint footprint_px = _cell * cam.zoom.x * size_scale(species) (NOT the field-space _cell — the wiring bug all lenses flagged). Ladder: < ~3 px district polygon only; ~3-7 px polygon + density stipple; >= ~7 px POP OPEN to per-cell morph sprites (the EXISTING organisms.gd _draw_plant/_draw_morph, untouched) clipped to the popped colonies while the polygon fades to a thin outline. A 6-8 px crossfade (polygon alpha 1.0->0.15, sprite alpha 0->1) as a PURE FUNCTION OF FOOTPRINT — NO per-frame timer, NO time-based easing (deferred); redraw fires ONLY on zoom/scope/state change (queue_redraw on _set_zoom), preserving organisms.gd "redraw only on state change" discipline (inv #3 in the renderer). Because size_scale is in the formula, plant colonies (SIZE_PLANT 2.2) pop FIRST (lowest zoom) and microbe haze stays aggregated — "by organism size, pop open" for free. ZERO RUST: pinned literal 0x47a0_3c8f_6701_f240 trivially unmoved; inv #2 (organisms.gd still owns morphology, colonies.gd owns polygon geometry — no genotype->phenotype added). Read docs/llm/proposals/visual-declutter-colony-draft.md section 4 (4.1 the footprint metric, 4.2 the ladder) first. Then gate + adversarially verify.',
  whenToUse: 'After ADR-029 S2 (colonies.gd district polygons). Threads zoom->footprint into both layers + the pop ladder so zooming in resolves large/plant colonies to individuals while microbe colonies stay polygons.',
  phases: [{ title: 'Impl' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Impl')
const s1 = await agent(
  'Implement ADR-029 S3 — the LOD POP ladder (RENDERER-ONLY GDScript; ZERO Rust; the pinned literal 0x47a0_3c8f_6701_f240 stays trivially unmoved). READ FIRST: docs/llm/proposals/visual-declutter-colony-draft.md section 4.1 (footprint_px = _cell * cam.zoom.x * size_scale(species) — the on-screen footprint, NOT the field-space _cell) + section 4.2 (the three-rung ladder + the 6-8 px crossfade as a pure function of footprint, no per-frame timer). Then READ the REAL surface that S2 + the existing renderer expose: godot/colonies.gd (set_snapshot(snap, cell, species_table) ~:76; _cell ~:37; per-colony _colony_draw entries with count/species/variant; MIN_COLONY_CELLS ~:27; _draw ~:420; redraw only on set_snapshot/set_iso — inv #3 discipline), godot/organisms.gd (the LOD test lod_dots_only := _cell < LOD_MIN_CELL ~:186 keys on the FIELD-SPACE _cell — this is the bug to fix; _cell_visual ~:149 gives per-cell size_scale; _draw_plant ~:259 / _draw_morph; MAX_DOTS_PER_CELL ~:21), godot/main.gd (_set_zoom ~:3844-3854 sets _cam.zoom; _update_scope_layers ~:5499 currently does a BINARY visible swap of _colonies vs _organisms; _show ~:5396 feeds set_snapshot to both layers; _scope_label ~:5480 reads _cam.zoom.x), godot/species_visual_map.gd (SIZE_* ~:21-27, the size_scale table). CLAUDE.md inv #2 (render-only — organisms.gd KEEPS sole ownership of morphology; colonies.gd owns polygon geometry; NO genotype->phenotype added) + inv #3 (renderer determinism: NO randf/randi/Time/OS; redraw only on state change, never _process/per-frame; the crossfade is a closed-form function of footprint).\n\n' +
  '  - THREAD THE ZOOM: add a way to feed cam.zoom.x into BOTH colonies.gd and organisms.gd (e.g. a set_zoom(zoom: float) that stores it + queue_redraw). Call it from main.gd._set_zoom (and on _show) so a wheel/scope event re-pops without any per-frame work. Compute the footprint per the §4.1 formula footprint_px = _cell * zoom * size_scale.\n' +
  '  - REPLACE THE BINARY SWAP with the per-colony ladder (main.gd._update_scope_layers): both layers may now be visible at once. Each COLONY decides its own rung from ITS footprint (using its species size_scale from the table): < ~3 px -> polygon only; ~3-7 px -> polygon + a density stipple (internal heat from the density channel); >= ~7 px -> the polygon fades to a thin district OUTLINE and the cells of that colony POP to per-cell sprites. Because size_scale is in the footprint, plant colonies (SIZE_PLANT 2.2) cross the threshold first — keep that property (do NOT clamp it away).\n' +
  '  - colonies.gd: per-colony, ramp the fill alpha across the 6-8 px crossfade band (1.0 -> ~0.15) as a PURE FUNCTION of that colony footprint; below the band draw full fill (+ stipple in the mid band), above it draw only the thin outline + label. No timer.\n' +
  '  - organisms.gd: change lod_dots_only / the sprite gate to key on the EFFECTIVE on-screen footprint (_cell * zoom * size_scale per cell), NOT the raw _cell; draw per-cell morph sprites ONLY for cells whose colony has popped (footprint >= the pop threshold), ramping sprite alpha 0 -> 1 across the same 6-8 px band (pure function of footprint). Clip the popped sprites to the popped colonies so a not-yet-popped microbe region stays a polygon with zero sprites (the de-spam holds). REUSE _draw_plant/_draw_morph untouched (single source of truth for morphology). Keep the existing iso/terrain offset path.\n' +
  '  - NO PER-FRAME REDRAW: queue_redraw fires only on set_snapshot / set_zoom / set_iso / scope change — never from _process or a Timer (inv #3 renderer discipline). Time-based easing stays DEFERRED behind a future "allow animated redraw" decision; the crossfade is closed-form on footprint only.\n' +
  '  - inv #2: no biology added — the ladder is presentation. colonies.gd computes geometry/alpha; organisms.gd computes glyphs; neither computes genotype->phenotype.\n' +
  '  - BUILD + macOS-SAFE smoke: build the cdylib (must still build with ZERO Rust diff). Attempt macOS-safe --shot(s) (timeout + FILE capture, never a $(godot...) pipe; WINDOWED for --shot; SKIP cleanly + say so if no display) at TWO zooms on a sample run: a zoomed-OUT Field shot (microbe colonies are polygons, plants may already be popping) + a zoomed-IN shot (large/plant colonies popped to individuals while microbe colonies stay polygons). If no display, prove it at the code level (the per-colony rung selection + the per-cell footprint gate) + run the godot smoke (livesim_smoke.gd) so the project loads. Confirm queue_redraw is NOT called per-frame (no _process redraw).\n' +
  '  - CONFIRM zero Rust diff: git diff --stat shows NO crates/ change. Do NOT commit. Report: the footprint formula + where zoom is threaded into each layer, the per-colony rung selection + the crossfade closed form, the organisms.gd per-cell pop gate (proving microbe colonies stay polygons while plants pop first), and confirm no per-frame redraw + zero Rust diff.',
  { label: 'impl', phase: 'Impl', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  'Run bash tools/gate.sh for gene-sim (generous timeout ~15 min). ADR-029 S3 is RENDERER-ONLY — it must be GREEN: fmt, clippy, test, determinism MUST stay 0x47a0_3c8f_6701_f240 (S3 touches ZERO Rust — confirm git diff --stat shows no crates/ change, so the literal is byte-identical by construction), the godot snapshot byte gate (still GSS6/channels=14), license green, godot-reader + livesim_smoke green (the modified colonies.gd + organisms.gd + main.gd must load + run). Report every gate PASS/FAIL with exact errors + EXPLICITLY confirm 0x47a0 unmoved + zero Rust diff. No fixes, no commit.',
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['renderer_only_zero_rust_hash_unmoved', 'footprint_ladder_pure_no_per_frame_redraw', 'plants_pop_first_by_size', 'no_biology_in_render', 'issues'],
  properties: {
    renderer_only_zero_rust_hash_unmoved: { type: 'boolean', description: 'S3 is GDScript-only: git diff shows NO crates/ (Rust) change, so the pinned literal 0x47a0_3c8f_6701_f240 is byte-identical by construction; snapshot format stays GSS6/channels=14.' },
    footprint_ladder_pure_no_per_frame_redraw: { type: 'boolean', description: 'inv #3 in the renderer: the pop ladder + the 6-8 px crossfade are a CLOSED-FORM pure function of footprint_px = _cell * cam.zoom.x * size_scale (no randf/randi/Time/OS); queue_redraw fires ONLY on set_snapshot/set_zoom/set_iso/scope change — NEVER from _process or a Timer (no per-frame redraw). zoom is threaded from main.gd._set_zoom into BOTH colonies.gd and organisms.gd.' },
    plants_pop_first_by_size: { type: 'boolean', description: 'Because size_scale is IN the footprint formula, plant colonies (SIZE_PLANT 2.2) cross the ~7 px pop threshold at a lower zoom than microbes — large/plant colonies pop to individuals while microbe colonies stay polygons. Proven by the formula + a --shot (two zooms) OR the code path; the de-spam holds (un-popped microbe cells draw zero sprites).' },
    no_biology_in_render: { type: 'boolean', description: 'inv #2: the LOD ladder is PRESENTATION. organisms.gd keeps sole ownership of morphology (_draw_plant/_draw_morph reused untouched); colonies.gd owns polygon geometry/alpha. No genotype->phenotype computed in either.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    'Adversarially verify ADR-029 S3 (the LOD pop ladder — renderer-only). Read git diff (godot/colonies.gd + godot/organisms.gd + godot/main.gd) + docs/llm/proposals/visual-declutter-colony-draft.md section 4 + CLAUDE.md inv #2/#3. Skeptic #' + i + ' — default each boolean FALSE unless PROVEN. Hunt: ANY crates/ (Rust) diff (must be GDScript-only — if Rust changed, confirm 0x47a0_3c8f_6701_f240 byte-identical and flag it); a per-frame redraw (a _process or Timer calling queue_redraw — FORBIDDEN, redraw must fire only on state/zoom/scope change); time-based easing instead of a closed-form footprint function (deferred — must be a pure function of footprint); randf/randi/Time/OS in the ladder (renderer non-determinism); the footprint keyed on the raw field-space _cell instead of _cell*zoom*size_scale (the bug §4.1 flags); plants NOT popping first / microbe colonies re-spamming (the de-spam broken — un-popped cells must draw zero sprites); biology computed in the ladder (inv #2). Report the structured verdict with file:line. Do NOT edit.',
    { label: 'verify:skeptic' + i, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['renderer_only_zero_rust_hash_unmoved', 'footprint_ladder_pure_no_per_frame_redraw', 'plants_pop_first_by_size', 'no_biology_in_render']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  impl: typeof s1 === 'string' ? s1.slice(0, 800) : s1,
  gate: typeof gate === 'string' ? gate.slice(0, 600) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — LOD pop ladder; footprint-keyed, plants pop first, no per-frame redraw; renderer-only, 0x47a0 byte-identical' : 'NEEDS WORK',
}
