export const meta = {
  name: 'oversight-ui-polish-impl',
  description:
    'OVERSIGHT UI polish (renderer-only, hash-neutral — the ADR-028 #3-verify follow-ups). Three small GDScript fixes in godot/main.gd: (1) the growth-ratio q SpinBox defaults to 1000 (wild-type / no-op) instead of 0 (growth-lethal KO) so opening the panel + committing does not accidentally knock out growth — _make_spin(0, 1000, 10, 0) -> default 1000 (~:1547); (2) align the timeline "due epoch N" marker label (~:1657/:1659) with the renderer IMMEDIATE-COMMIT semantics — the commit happens NOW, so the label must not imply a deferral the path does not perform (clarify the epoch is the effect/accounting epoch, not a pending commit); (3) re-enable OVERSIGHT in load_session so the credit ledger RESUMES after a loaded checkpoint (re-activate _oversight_panel + _refresh_oversight_panel on the post-load_session resync). ZERO RUST: no sim-path change — the q knob is a UI default (the pinned determinism config runs headless, not through the UI), so the pinned literal 0x47a0_3c8f_6701_f240 is byte-identical; inv #2 (the renderer marshals only ints/strings; the credit economy / FBA->factor map stays in the core). Read docs/llm/DECISIONS.md ADR-028 (the Accepted #3-verify follow-ups) first. Then gate + adversarially verify.',
  whenToUse: 'A renderer-only polish pivot: the three ADR-028 OVERSIGHT UI follow-ups (safe q default, honest due-epoch label, ledger resumes after load_session). Hash-neutral, player-facing.',
  phases: [{ title: 'Impl' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Impl')
const s1 = await agent(
  'Implement the OVERSIGHT UI polish (RENDERER-ONLY GDScript in godot/main.gd; ZERO Rust; the pinned literal 0x47a0_3c8f_6701_f240 stays byte-identical — these are UI changes, the sim runs headless). READ FIRST: docs/llm/DECISIONS.md ADR-028 (the "Accepted" #3-verify follow-ups paragraph ~:1244 — the three fixes) + the real surface in godot/main.gd: the OVERSIGHT panel build (_oversight_ratio = _make_spin(0, 1000, 10, 0) ~:1547 + its tooltip ~:1548), the commit handler that builds the marker label ("OVERSIGHT q=%d → %.3f× · due epoch %d · req %d" ~:1657 + the "✓ committed ... due epoch %d" status ~:1659 + the comment ~:1654 explaining due_epoch), the oversight panel activation (_oversight_panel.set_active(...) ~:1581/:3540 + _refresh_oversight_panel ~:1590), and the load_session / checkpoint resync path (~:713-736 _load_checkpoint / play_checkpoint → load_session → resync). CLAUDE.md inv #2 (render-only — GDScript marshals only ints/strings; the credit economy / FBA→factor map / spend gate stay in the core; the oversight #[func]s are has_method-guarded) + inv #3 (no sim change — the q knob is a UI default, not the pinned headless config).\n\n' +
  '  - FIX 1 — SAFE q DEFAULT: change the growth-ratio q SpinBox default from 0 (growth-LETHAL knockout) to 1000 (wild-type / no-op): _make_spin(0, 1000, 10, 1000) so opening the panel + committing without dragging the knob is a NO-OP, not a lethal KO. Keep min=0/max=1000/step=10. (Confirm the fallback default reads in _oversight_preview/_oversight_commit already use 1000 — they do at ~:1624/:1646 — so the control + the fallbacks now agree.)\n' +
  '  - FIX 2 — HONEST due-epoch LABEL: the renderer path commits the edit IMMEDIATELY (ADR-028 immediate-commit), so a "due epoch N" label that implies the commit is DEFERRED is misleading. Reword the timeline marker (~:1657) + the status line (~:1659) so the epoch reads as the effect/accounting epoch (e.g. "effective epoch N" / "applied now · epoch N") — NOT a pending/deferred commit. Keep the q/factor/req fields. Match the ADR-028 wording (the marker should reflect that the commit already happened).\n' +
  '  - FIX 3 — OVERSIGHT RESUMES AFTER load_session: after a checkpoint load (the load_session resync path), RE-ACTIVATE the oversight panel + refresh the ledger so the credit economy resumes (call the same _oversight_panel.set_active(_live != null and _live.has_method("oversight_state") [and the ecosystem-view guard]) + _refresh_oversight_panel() that the fresh-run path uses). Guard with has_method so an older cdylib without oversight_state degrades gracefully. Verify a loaded checkpoint shows a live (non-stale) ledger readout.\n' +
  '  - inv #2: no biology/economy logic added to GDScript — FIX 3 just re-invokes the existing activation/refresh; the ledger numbers still come from the core oversight_state() #[func].\n' +
  '  - BUILD + macOS-SAFE smoke: build the cdylib (must still build with ZERO Rust diff). Run the godot smoke (livesim_smoke.gd) so the project loads; if a --shot is possible (macOS-safe: timeout + FILE capture, never a $(godot…) pipe; WINDOWED; SKIP cleanly if no display), capture the OVERSIGHT panel showing the q default = 1000 + the reworded label. If no display, prove the three fixes at the code level + the smoke loads.\n' +
  '  - CONFIRM zero Rust diff: git diff --stat shows NO crates/ change. Do NOT commit. Report: the three fixes (with the exact before/after for each), the proof the q control + its fallbacks agree on 1000, and confirm 0x47a0 unmoved + zero Rust diff.',
  { label: 'impl', phase: 'Impl', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  'Run bash tools/gate.sh for gene-sim (generous timeout ~15 min). The OVERSIGHT UI polish is RENDERER-ONLY — it must be GREEN: fmt, clippy, test, determinism MUST stay 0x47a0_3c8f_6701_f240 (zero crates/ change — confirm git diff --stat shows no Rust diff, so the literal is byte-identical by construction), the godot snapshot byte gate (GSS6/channels=14, the colony tests still pass), license green, godot-reader + livesim_smoke green (the modified main.gd must load + run). Report every gate PASS/FAIL with exact errors + EXPLICITLY confirm 0x47a0 unmoved + zero Rust diff. No fixes, no commit.',
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['renderer_only_zero_rust_hash_unmoved', 'q_knob_defaults_wildtype', 'due_epoch_label_matches_immediate_commit', 'oversight_resumes_after_load_session', 'issues'],
  properties: {
    renderer_only_zero_rust_hash_unmoved: { type: 'boolean', description: 'GDScript-only (godot/main.gd): git diff shows NO crates/ (Rust) change; the pinned literal 0x47a0_3c8f_6701_f240 is byte-identical by construction (the q knob is a UI default, not the headless determinism config); inv #2 — GDScript marshals only ints/strings, the credit economy/FBA map stays in the core.' },
    q_knob_defaults_wildtype: { type: 'boolean', description: 'The growth-ratio q SpinBox now DEFAULTS to 1000 (wild-type / no-op), NOT 0 (growth-lethal KO), so opening the panel + committing without touching the knob is a no-op; the control default + the _oversight_preview/_commit fallbacks (1000) agree.' },
    due_epoch_label_matches_immediate_commit: { type: 'boolean', description: 'The timeline marker + status label no longer imply a DEFERRED commit — they reflect the renderer IMMEDIATE-COMMIT semantics (the epoch reads as the effect/accounting epoch, not a pending commit), matching ADR-028.' },
    oversight_resumes_after_load_session: { type: 'boolean', description: 'After a checkpoint load_session resync, the OVERSIGHT panel re-activates + the credit ledger readout refreshes (resumes live), has_method-guarded for older cdylibs; a loaded session no longer leaves a dead/stale oversight panel.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    'Adversarially verify the OVERSIGHT UI polish (renderer-only, godot/main.gd). Read git diff + docs/llm/DECISIONS.md ADR-028 + CLAUDE.md inv #2/#3. Skeptic #' + i + ' — default each boolean FALSE unless PROVEN. Hunt: ANY crates/ (Rust) diff (must be GDScript-only — if Rust changed, confirm 0x47a0_3c8f_6701_f240 byte-identical and flag it); the q knob still defaulting to 0 (lethal KO) or the control/fallback defaults disagreeing; a due-epoch label that STILL implies a deferred commit (not aligned with immediate-commit); oversight NOT actually re-activated after load_session (a dead ledger on a loaded checkpoint) or the re-activation not has_method-guarded; biology/economy logic added to GDScript (inv #2 — must stay a core #[func] read). Report the structured verdict with file:line. Do NOT edit.',
    { label: 'verify:skeptic' + i, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['renderer_only_zero_rust_hash_unmoved', 'q_knob_defaults_wildtype', 'due_epoch_label_matches_immediate_commit', 'oversight_resumes_after_load_session']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  impl: typeof s1 === 'string' ? s1.slice(0, 800) : s1,
  gate: typeof gate === 'string' ? gate.slice(0, 600) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — safe q default + honest due-epoch label + oversight resumes after load; renderer-only, 0x47a0 byte-identical' : 'NEEDS WORK',
}
