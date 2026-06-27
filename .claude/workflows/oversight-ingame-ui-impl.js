export const meta = {
  name: 'oversight-ingame-ui-impl',
  description:
    'OVERSIGHT in-game UI (ADR-017 S4/S5/S6) — the player-agency payoff. Surface the earned-credit OVERSIGHT loop in --live: render the credit ledger, let the player REQUEST → PREVIEW (the FBA knockout result) → COMMIT an E. coli edit that ripples through the F4 decomposer loop. The CORE logic already exists (harness CreditLedger + due_epoch firewall, sim-core commit_species_edit/edit_factor_q/EditEffect, the EcoliEditModifier ripple, oracle-fba KO table accepted under ADR-018); this slice (a) plumbs the oversight surface through godot-sim as new #[func]s (model on apply_edit/observe_species), and (b) builds the renderer panel (model on the CRISPR intervention panel). HASH-NEUTRAL to the pinned literal 0x47a0_3c8f_6701_f240: the oversight path is journaled player actions (RequestEcoliEdit draws zero SimRng; CommitEcoliImpact reads a committed integer; credit accrual is RNG-free + tick-clocked via due_epoch — no wall-clock leak), exercised only when the player commits, exactly like apply_edit/inoculate. Crates: godot-sim (#[func]s) + godot/*.gd (panel). Then gate + adversarially verify.',
  whenToUse: 'Gameplay/sandbox phase. After the OVERSIGHT core (CreditLedger S4 + EcoliEditModifier S6). The in-game earn/spend/trigger panel.',
  phases: [{ title: 'Plumb' }, { title: 'UI' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Plumb')
const s1 = await agent(
  `Implement the OVERSIGHT boundary plumbing — expose the earned-credit loop through godot-sim as new #[func]s so the renderer can drive it. READ FIRST (map the REAL surface; names/lines drift):\n` +
  `  - crates/harness/src/lib.rs: the CreditLedger / RNG-free earn schedule; \`commit_species_edit(species: u16, growth_ratio_q: u16)\` (~632); Action::RequestEcoliEdit { ... due_epoch } (~133) + Action::CommitEcoliImpact { species, growth_ratio_q, due_epoch } (~167); the due_epoch firewall (drain at the step boundary, never wall-clock). Find how GeneSimEnv holds the ledger + the committed/pending edits.\n` +
  `  - crates/sim-core/src/lib.rs: commit_species_edit(sid, growth_ratio_q, EditEffect) (~2429) + edit_factor_q (~259) + EditEffect — the per-species edit factor the modifier reads.\n` +
  `  - crates/oracle-fba (the KO table, ADR-018 accepted) + the EcoliEditModifier (Slice B) — how a committed (species, growth_ratio_q) maps to an FBA KO outcome for the PREVIEW.\n` +
  `  - crates/godot-sim/src/lib.rs: the #[func] patterns to MIRROR — apply_edit (~518), observe_species (~358), inoculate (~667), save_session (~992); the guarded-empty-on-bad-id / before-reset convention.\n` +
  `  CLAUDE.md inv #2 (biology/economy stays in core/harness — godot-sim only marshals), #3 (the oversight path is journaled; the DEFAULT no-edit run stays 0x47a0), #6 (species-granular).\n\n` +
  `  - Add godot-sim #[func]s (thin marshalling over the harness/LiveSim oversight surface; NO logic in godot-sim): a credit/ledger read (e.g. \`oversight_state() -> VarDictionary\` = balance + accrual rate + pending/committed edits), a \`preview_ecoli_edit(species, growth_ratio_q) -> VarDictionary\` (read-only: the predicted KO/growth outcome, drawing zero SimRng — model on observe_species), and a \`commit_ecoli_edit(species, growth_ratio_q, due_epoch) -> VarDictionary\` that journals the RequestEcoliEdit/CommitEcoliImpact pair through the existing path (Applied/Rejected + the due_epoch it commits at). Guard each like the other #[func]s.\n` +
  `  - A test: a committed oversight edit is byte-deterministic (replay-equal) and the DEFAULT pinned config (no oversight edit) leaves 0x47a0_3c8f_6701_f240 UNMOVED. Build Rust + the cdylib. Do NOT commit. Report the new #[func] signatures + confirm hash 0x47a0 unmoved + the determinism test.`,
  { label: 'plumb', phase: 'Plumb', agentType: 'implementer' },
)

phase('UI')
const s2 = await agent(
  `Implement the renderer OVERSIGHT panel on the Stage-1 plumbing:\n${typeof s1 === 'string' ? s1.slice(0, 800) : ''}\n\n` +
  `READ godot/main.gd: the CRISPR intervention panel (the Cas/locus dropdowns + guide + Inject path → apply_edit, Applied/Failed markers on timeline.gd) and the contamination panel — MIRROR these patterns. The panel framework is panel.gd (PanelChrome). Renderer-only (inv #2): GDScript only moves ints (species id, credit amounts, growth_ratio_q) + reads VarDictionary; the economy/FBA stay in core.\n\n` +
  `  - An OVERSIGHT panel (wrapped in PanelChrome, --live only, has_method-guarded so an older cdylib / file-replay degrades gracefully): show the credit balance + accrual (from oversight_state()); a species picker (E. coli + any editable species); a growth_ratio_q control; a "Preview" affordance → calls preview_ecoli_edit() and shows the predicted outcome BEFORE spending; a "Commit" button (enabled only when credit suffices) → commit_ecoli_edit(), placing an Applied marker on the timeline at the committed due_epoch. Reflect the ledger spending after a commit.\n` +
  `  - Build the cdylib + stage data + headless parse check; confirm the panel constructs without error. Do NOT commit. Report the panel wiring (state read → preview → commit) + that it builds + parses headless.`,
  { label: 'ui', phase: 'UI', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim (generous timeout ~15 min). The OVERSIGHT in-game UI must be GREEN: fmt, clippy, test (incl. the oversight replay-determinism test), determinism MUST stay against the pinned literal 0x47a0_3c8f_6701_f240 (the DEFAULT no-edit run is unchanged — a moved hash is a FAIL), license green (oracle-fba is a BOUNDARY crate — inv #1 intact), godot-reader + livesim green (the new oversight #[func]s + the panel must not break the GDExtension smoke). Report every gate PASS/FAIL with exact errors. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['default_hash_unmoved', 'oversight_replay_deterministic', 'no_economy_in_gdscript', 'no_wallclock_leak', 'ui_correct', 'issues'],
  properties: {
    default_hash_unmoved: { type: 'boolean', description: 'inv #3: the DEFAULT pinned config (no oversight edit committed) leaves 0x47a0_3c8f_6701_f240 byte-identical; determinism gate green. The oversight path is new player-action behaviour, not a re-pin.' },
    oversight_replay_deterministic: { type: 'boolean', description: 'A committed oversight edit is journaled (RequestEcoliEdit zero-SimRng + CommitEcoliImpact reads a committed integer) and replay-equal; the credit accrual is RNG-free.' },
    no_economy_in_gdscript: { type: 'boolean', description: 'inv #2: GDScript only marshals ints + reads VarDictionary; the credit economy + FBA KO preview + edit_factor are computed in harness/sim-core/oracle-fba, never in godot/*.gd.' },
    no_wallclock_leak: { type: 'boolean', description: 'inv #3: the due_epoch firewall is tick-clocked (a generation count); no std::time / wall-clock feeds the hash or the commit timing.' },
    ui_correct: { type: 'boolean', description: 'The panel reads oversight_state(), previews BEFORE spending (read-only), commits only when credit suffices, and marks the timeline at due_epoch; everything has_method-guarded; godot-reader + livesim smoke green.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify the OVERSIGHT in-game UI on this branch. Read \`git diff main...HEAD\` (or \`git diff\`), the new godot-sim oversight #[func]s, the renderer OVERSIGHT panel, and CLAUDE.md inv #2/#3/#6. Skeptic #${i} — default each boolean FALSE unless confirmed with file:line. Hunt: a MOVED pinned hash 0x47a0_3c8f_6701_f240 on the DEFAULT config (the no-edit run must be byte-identical); a preview_ecoli_edit that DRAWS SimRng or mutates the sim (must be read-only like observe_species); economy/FBA logic computed in GDScript (the ledger + KO outcome + edit_factor must be core/harness — GDScript only moves ints); any std::time / wall-clock leaking into the commit timing or the hash (due_epoch must be tick-clocked); a commit that bypasses the RequestEcoliEdit/CommitEcoliImpact journal (would break replay); and an un-guarded oversight call that crashes on an older cdylib / file-replay. Report the structured verdict. Do NOT edit.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['default_hash_unmoved', 'oversight_replay_deterministic', 'no_economy_in_gdscript', 'no_wallclock_leak', 'ui_correct']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  plumb: typeof s1 === 'string' ? s1.slice(0, 700) : s1,
  ui: typeof s2 === 'string' ? s2.slice(0, 700) : s2,
  gate: typeof gate === 'string' ? gate.slice(0, 500) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — oversight UI hash-neutral on the default config, journaled+replay-deterministic, no economy in GDScript, no wall-clock leak' : 'NEEDS WORK',
}
