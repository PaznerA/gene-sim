export const meta = {
  name: 'midnight-review',
  description:
    'Big pre-manual-testing review of the SANDBOX & PRESENTATION phase + the ADR-019 contamination epic (everything on main since the CHEMOSTAT-J ecology). Fans out review over 4 dimensions — UI coherence/UX/wiring, correctness bugs, determinism/conservation integrity, manual-testing readiness — adversarially verifies the findings (real vs noise), and synthesizes a PRIORITIZED report: must-fix-before-testing / nice-to-fix / a what-to-test + what-to-watch-for guide. READ-ONLY review; produces docs/llm/proposals/midnight-review-draft.md.',
  whenToUse:
    'Before the human starts manual testing of the new sandbox/intervention/contamination UI. Comprehensive multi-agent review; no code changes.',
  phases: [
    { title: 'Review' },
    { title: 'Verify' },
    { title: 'Synthesize' },
  ],
}

phase('Review')
const FSCHEMA = {
  type: 'object',
  required: ['findings', 'severity_notes'],
  properties: {
    findings: { type: 'array', items: { type: 'object', additionalProperties: true }, description: 'each finding: { title, file, line, severity (blocker|major|minor|nit), what, why, suggested_fix }' },
    severity_notes: { type: 'string', description: 'overall read of this dimension + the single most important thing' },
  },
}
const DIMENSIONS = [
  { key: 'ui-coherence', prompt: 'UI COHERENCE / UX / WIRING. Review the godot UI as a usable WHOLE a manual tester will drive. READ godot/main.gd (the panels: _build_intervention_ui, _build_contamination_ui, the 6-tool palette CRISPR/PCR/Antibiotic/Nutrient/Toxin/Inoculate + per-tool param sub-panels, the brush, timeline markers), godot/main_menu.gd (the SP-2 multi-species roster composer + ContainmentLevel selector), godot/panel.gd (PanelChrome), godot/timeline.gd. Hunt: dead/confusing controls (e.g. the "5 palette buttons" comment vs 6 TOOL_KEYS — is the inoculate tool actually in the palette + a radio member?), panel overlap/layout/stacking issues, controls that issue an Action that no-ops silently (e.g. a tool with no target species selected, a roster row with count 0, a containment level with no consortium), the menu→sandbox→intervene→contaminate→observe FLOW (can a tester actually compose a 3-species roster, set Open containment, seed a contaminant, and see it?), missing affordances/labels, and the deferred-SP-4 gap (no codex UI). Be concrete with file:line.' },
  { key: 'correctness', prompt: 'CORRECTNESS BUGS in the recent core+harness+godot work. READ the harness Action dispatch (crates/harness/src/lib.rs — RegionPcrAmplify/Cull/Nutrient/Toxin/Inoculate + the roster set_roster + the containment schedule), the sim-core mechanics (region_pcr_amplify, region_cull, region_nutrient, region_inoculate, the spore SporeReservoir/germinate, the host_coupling symbiont pass), crates/godot-sim/src/lib.rs (set_roster/inoculate/set_containment/register_contaminant the boundary). Hunt: off-by-one / empty-region / unresolved-key edge cases, integer overflow, a tool that mints or destroys J incorrectly, a count/strength/channel parameter mis-mapped, the roster precedence vs species vs default, the default consortium referencing species, replay/journal round-trip gaps. file:line.' },
  { key: 'determinism', prompt: 'DETERMINISM / CONSERVATION INTEGRITY of the new mechanics. READ the ledger (crates/sim-core/src/ledger.rs — the intervention/immigration/spore buckets + ledger_closes), the new passes (immigration/spore germinate/host_coupling) + their conservation, and the hash-neutrality claims. RUN: `cargo test -p sim-core` (the ledger/conservation/determinism tests) + `bash tools/check_determinism.sh` + confirm the pinned literal 0x47a0_3c8f_6701_f240. Hunt: any path where an intervention/immigration/spore/symbiont leaks or destroys J (ledger_closes broken), a new SimRng draw, HashMap iteration in sim logic, a non-conserved FlowMatrix edge (host_coupling row-sum), run-to-run instability, OR a claimed-hash-neutral that actually moved. Report what you ran + the results. file:line.' },
  { key: 'test-readiness', prompt: 'MANUAL-TESTING READINESS + known gaps. RUN `bash tools/gate.sh` and report the full per-gate result (does the project parse + headless-render + livesim-smoke cleanly?). Then assemble a concrete WHAT-TO-TEST + WHAT-TO-WATCH-FOR guide for the human: the happy-path flows (compose a roster → run → use each of the 6 tools → set containment → seed a contaminant → watch establish/displace/die → cull → observe spores regerminate → inoculate a symbiont onto its host), the known DEFERRED/incomplete bits (SP-4 codex UI absent; loaded-session immigration markers pending the journal_actions export; symbiont S5b provisioning; item-8 predator/decomposer still crashes — which is correct emergence), and any rough edges that will surprise a tester. file:line where relevant.' },
]
const reviews = (await parallel(DIMENSIONS.map((d) => () =>
  agent(
    `You are reviewing the gene-sim SANDBOX & PRESENTATION + ADR-019 contamination work on main (commit 4c874ce) BEFORE the human manually tests it. Dimension: ${d.prompt}\n\n` +
    `Context: pinned literal 0x47a0_3c8f_6701_f240; the whole midnight session was hash-neutral. The deferred items: SP-4 codex UI (gate RED — parse + res:// staging), item 8 predator/decomposer (the predator's overshoot-crash is CORRECT emergence per the open-system principle, NOT a bug). Be precise (file:line), separate real issues from noise, and rank by severity. This is READ-ONLY (you may run gate/tests/determinism scripts; do NOT edit code).`,
    { label: `review:${d.key}`, phase: 'Review', schema: FSCHEMA },
  ),
))).filter(Boolean)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['confirmed', 'rejected'],
  properties: {
    confirmed: { type: 'array', items: { type: 'object', additionalProperties: true }, description: 'findings confirmed real (with file:line evidence) + corrected severity' },
    rejected: { type: 'array', items: { type: 'string' }, description: 'findings that are noise / already-handled / false, with why' },
  },
}
const verified = await agent(
  `Adversarially verify the gene-sim review findings below — separate REAL issues from noise. For each claimed finding, check it against the actual code (file:line); CONFIRM it (with evidence + a corrected severity) or REJECT it (already-handled / intended / false). Be skeptical: a "bug" that's actually the documented open-system emergence (predator crash) or a hash-neutral-by-design choice is NOT a bug.\n\nFindings:\n${JSON.stringify(reviews, null, 2)}`,
  { label: 'verify', phase: 'Verify', schema: VSCHEMA },
)

phase('Synthesize')
const report = await agent(
  `Write docs/llm/proposals/midnight-review-draft.md — the pre-manual-testing review report for gene-sim, from the VERIFIED findings. Structure:\n` +
  `1. VERDICT — one paragraph: is the sandbox/contamination UI ready for manual testing? the single most important thing.\n` +
  `2. 🔴 MUST-FIX BEFORE TESTING — confirmed blockers/majors that would break or badly confuse a test session (with file:line + the fix). If none, say so.\n` +
  `3. 🟡 NICE-TO-FIX — minors/nits (the 5-vs-6 tool comment, layout, labels, deferred SP-4) — quick wins.\n` +
  `4. ✅ WHAT-TO-TEST + WHAT-TO-WATCH-FOR — a concrete manual-test guide: the happy-path flows (compose roster → run → each tool → containment → seed contaminant → cull → spores regerminate → symbiont onto host) + what CORRECT behaviour looks like + the known-deferred bits (so the tester doesn't report them as bugs: SP-4 codex absent, predator crash = correct emergence, loaded-session markers pending).\n` +
  `5. STATE — determinism/gate confirmation (literal 0x47a0, gate green) from the determinism reviewer.\n\n` +
  `Keep it scannable + actionable. Cite file:line. Do NOT commit. End with a 3-line TL;DR.\n\n` +
  `Verified findings:\n${JSON.stringify(verified, null, 2)}`,
  { label: 'report', phase: 'Synthesize' },
)

return { reviews, verified, report }
