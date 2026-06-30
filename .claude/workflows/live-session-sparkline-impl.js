export const meta = {
  name: 'live-session-sparkline-impl',
  description:
    'live-session-sparkline (renderer-only, hash-neutral — the P4/P6 follow-up): a per-gen EFFECT sparkline on the timeline intervention markers. Each journaled intervention (CRISPR / PCR / Antibiotic / Nutrient / Toxin / Inoculate / OVERSIGHT) already shows as a per-tool coloured tab at its generation on timeline.gd; this slice attaches a small EFFECT sparkline = the per-gen trajectory of a run metric (mean fitness and/or allele freq) over the window AFTER the marker fired, sampled from the EXISTING histories (main.gd _fit_history / _allele_history / _snaps), so the player can read what that intervention DID. TASTEFUL / NO RE-CLUTTER (the user\'s standing "screen is spammed" concern): show the sparkline for the SELECTED/hovered marker (or as a bounded mini-glyph), NOT a dense overlay on every marker at once. ZERO RUST: presentation only — reads the off-hash render histories, computes NO biology (inv #2); deterministic (the series is built from the ordered history, no RNG / no hash-order iteration; redraw only on marker/snapshot/selection change, NOT per-frame — inv #3 renderer discipline); the pinned literal 0x47a0_3c8f_6701_f240 is byte-identical. Read godot/timeline.gd (_markers ~:29, set_markers ~:44, setup ~:49, _draw ~:60, _get_tooltip ~:126) + godot/main.gd (_injections ~:232, _fit_history/_allele_history ~:379, _snaps + _publish_frame ~:1056 where the timeline is fed) first. Then gate + adversarially verify.',
  whenToUse: 'A minor renderer-only QoL slice: per-gen effect sparkline on the timeline markers, so an intervention\'s effect is legible. Hash-neutral; orthogonal to the worker-thread perf work.',
  phases: [{ title: 'Impl' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Impl')
const s1 = await agent(
  'Implement live-session-sparkline (RENDERER-ONLY GDScript; ZERO Rust; the pinned literal 0x47a0_3c8f_6701_f240 stays byte-identical — presentation only). READ FIRST: godot/timeline.gd (the marker model _markers ~:29 = [{generation, tool, applied, label}]; set_markers ~:44; setup(gens) ~:49; set_index ~:55; _draw ~:60 — the gen axis + the per-tool marker tabs; _gui_input ~:114 + _get_tooltip ~:126 — hover/selection + the per-marker tooltip; TOOL_STYLE ~:16) + godot/main.gd (_injections ~:232 the marker dicts; _fit_history / _allele_history ~:379 the rolling [0,1] per-gen sparkline data; _snaps the rolling snapshot buffer; _publish_frame ~:1056 where _timeline.setup(gens) + set_markers(_injections) are fed; the marker-building sites _record_tool_outcome / the recorded-edit projection ~:1670). CLAUDE.md inv #2 (render-only — read the histories, NO genotype->phenotype) + inv #3 (renderer determinism: build the series from the ORDERED history, no Dictionary/hash-order iteration, no randf/randi/Time/OS; queue_redraw only on a marker/snapshot/selection STATE change, never _process/per-frame).\n\n' +
  '  - EFFECT SERIES: for an intervention marker at generation G, compute its effect series = a run metric (mean fitness from _fit_history, and/or allele freq from _allele_history — pick the most legible; the histories are per render-tick aligned with _snaps[].generation) sampled over the window of gens AFTER G (a bounded window, e.g. the next ~12-24 recorded points up to the next marker / the run end). Build it in main.gd from the EXISTING history (no new core call), normalize to [0,1] for drawing, and attach it to the marker dict (e.g. marker["effect"] = PackedFloat32Array(...)) when feeding _timeline.set_markers. Deterministic + ordered.\n' +
  '  - DRAW (timeline.gd): render the effect as a small sparkline (draw_polyline, the per-tool colour) for the SELECTED/hovered marker only (reuse the _gui_input/_get_tooltip selection-by-nearest-x logic) — a bounded mini-sparkline near that marker tab OR a small strip — so the timeline does NOT get re-cluttered (the user dislikes a spammed screen). Do NOT draw a dense sparkline on every marker at once. If no marker is selected/hovered, draw nothing extra (the timeline looks as it does today). Keep it a pure function of the marker\'s stored effect series.\n' +
  '  - inv #2: the sparkline is presentation — it reads _fit_history/_allele_history (already off-hash render projections) + the marker\'s gen; it computes NO biology. inv #3: the series is sliced from the ordered history (no hashing/RNG); the extra draw fires only when the selection/markers/snapshot change (queue_redraw on set_markers / the selection change), never per-frame.\n' +
  '  - BUILD + macOS-SAFE smoke: build the cdylib (ZERO Rust diff). Run the godot smoke (livesim_smoke.gd) so the project loads; if a --shot is possible (macOS-safe: timeout + FILE capture, never a $(godot…) pipe; WINDOWED; SKIP cleanly if no display) capture a timeline with a selected marker showing its effect sparkline. If no display, prove it at the code level + the smoke loads.\n' +
  '  - CONFIRM zero Rust diff: git diff --stat shows NO crates/ change. Do NOT commit. Report: how the effect series is built (proving ordered/no-hash-order + reads existing history), the selected-marker-only draw (proving no re-clutter), and confirm 0x47a0 unmoved + zero Rust + no per-frame redraw.',
  { label: 'impl', phase: 'Impl', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  'Run bash tools/gate.sh for gene-sim (generous timeout ~15 min). live-session-sparkline is RENDERER-ONLY — it must be GREEN: fmt, clippy, test, determinism MUST stay 0x47a0_3c8f_6701_f240 (zero crates/ change — confirm git diff --stat shows no Rust diff), the godot snapshot byte gate (GSS6/channels=14, colony tests still pass), license green, godot-reader + livesim_smoke green (the modified timeline.gd + main.gd must load + run). Report every gate PASS/FAIL with exact errors + EXPLICITLY confirm 0x47a0 unmoved + zero Rust diff. No fixes, no commit.',
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['renderer_only_zero_rust_hash_unmoved', 'per_marker_effect_sparkline_from_history', 'tasteful_no_reclutter', 'deterministic_no_per_frame_no_biology', 'issues'],
  properties: {
    renderer_only_zero_rust_hash_unmoved: { type: 'boolean', description: 'GDScript-only (timeline.gd + main.gd): git diff shows NO crates/ (Rust) change; the pinned literal 0x47a0_3c8f_6701_f240 is byte-identical by construction; the snapshot format is unchanged.' },
    per_marker_effect_sparkline_from_history: { type: 'boolean', description: 'An intervention marker carries an EFFECT series — a run metric (fitness/allele) over the gens AFTER it fired — built from the EXISTING per-gen histories (_fit_history/_allele_history/_snaps), and the timeline draws it as a sparkline; reads history, no new core call.' },
    tasteful_no_reclutter: { type: 'boolean', description: 'The sparkline is shown for the SELECTED/hovered marker only (or a bounded mini-glyph), NOT a dense overlay on every marker — the timeline is not re-cluttered (the user\'s standing anti-spam concern); with nothing selected the timeline looks as today.' },
    deterministic_no_per_frame_no_biology: { type: 'boolean', description: 'inv #3: the effect series is sliced from the ORDERED history (no Dictionary/hash-order iteration, no randf/randi/Time/OS); the extra draw fires only on a marker/snapshot/selection state change (queue_redraw), NOT from _process/per-frame. inv #2: presentation only — no genotype->phenotype computed in GDScript.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    'Adversarially verify live-session-sparkline (renderer-only, godot/timeline.gd + main.gd). Read git diff + CLAUDE.md inv #2/#3. Skeptic #' + i + ' — default each boolean FALSE unless PROVEN. Hunt: ANY crates/ (Rust) diff (must be GDScript-only — if Rust changed, confirm 0x47a0_3c8f_6701_f240 byte-identical and flag it); the effect series computed from a NEW core call or computing biology (inv #2 — must read the existing off-hash histories); a per-frame redraw (a _process/Timer driving queue_redraw — must be selection/markers/snapshot-change only); Dictionary/hash-order iteration or randf/randi/Time/OS in the series build (renderer non-determinism); a DENSE sparkline overlay on every marker that re-clutters the timeline (must be selected/hovered-only or a bounded mini-glyph). Report the structured verdict with file:line. Do NOT edit.',
    { label: 'verify:skeptic' + i, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['renderer_only_zero_rust_hash_unmoved', 'per_marker_effect_sparkline_from_history', 'tasteful_no_reclutter', 'deterministic_no_per_frame_no_biology']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  impl: typeof s1 === 'string' ? s1.slice(0, 800) : s1,
  gate: typeof gate === 'string' ? gate.slice(0, 600) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — per-marker effect sparkline from history, selected-only (no re-clutter); renderer-only, 0x47a0 byte-identical' : 'NEEDS WORK',
}
