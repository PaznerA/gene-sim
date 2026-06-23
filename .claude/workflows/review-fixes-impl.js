export const meta = {
  name: 'review-fixes-impl',
  description:
    'Fix the midnight-review findings before manual testing: R2 (the inv #3 BLOCKER — save/load + --replay drop the roster/selected-species/consortium/containment → contaminated & multi-species runs reload to a different hash; persist them + re-apply on replay/load BEFORE replaying the journal + a file-boundary round-trip test) and the major UX nice-to-fixes (menu containment no-op → seed the default consortium when level>0; expose all 7 baked airborne contaminants in the in-run panel + inoculate picker; 5→6 tool prose). Hash-neutral for the pinned plant run.',
  whenToUse:
    'After the midnight-review (docs/llm/proposals/midnight-review-draft.md). Makes the sandbox/contamination UI sound for manual testing (live + save/load/replay). Mostly GDScript + a harness/godot-sim persistence fix; hash-neutral. Stops for human commit.',
  phases: [
    { title: 'Implement' },
    { title: 'Gate' },
    { title: 'Verify' },
  ],
}

phase('Implement')
const [r2Done, uxDone] = await parallel([
  () => agent(
    `Fix review finding R2 (the inv #3 BLOCKER) for gene-sim — Rust harness + godot-sim ONLY (do NOT touch godot/*.gd). READ docs/llm/proposals/midnight-review-draft.md §2 (R2, with the exact file:lines) first, then crates/harness/src/replay.rs (SeedJson/EnvConfig — persists only seed+entity_count+climate; run_episode + replay()), crates/harness/src/lib.rs (GeneSimEnv set_roster/set_species/register_contaminant/set_containment + reset), crates/godot-sim/src/lib.rs (save_session/load_session).\n\n` +
    `THE BUG: save/load + --replay rebuild GeneSimEnv::new → set_environment → reset → replay-actions, and NEVER re-apply the roster / selected species / registered consortium (keys+endowments) / containment. So a journaled RegionInoculate resolves its species_key against an empty registry → clean no-op → spawns nothing on replay (but DID spawn live) → run_stats().hash DIVERGES. Same for a multi-species roster + a non-default selected species. This is an invariant #3 break.\n\n` +
    `THE FIX: extend the persisted session (SeedJson and/or the saved-session format) to ALSO persist the roster (species keys + per-species counts), the selected species, the registered consortium (contaminant keys + endowments), and the ContainmentLevel + consortium config. On run_episode / replay() / load_session, RE-APPLY them (set_roster / set_species / register_contaminant / set_containment) BEFORE replaying the journal. Keep it serde-additive + back-compatible: an OLD session.json with none of these fields must still load (serde-default → the current single-species behavior). Add a FILE-BOUNDARY round-trip test: record a CONTAMINATED MULTI-SPECIES episode to disk → replay() from disk → assert IDENTICAL run_stats().hash; + a save→reload-without-manual-reregister variant. The pinned single-species-plant run (run_headless) is untouched → literal 0x47a0_3c8f_6701_f240 unchanged (if it would move, STOP and report). Do NOT commit. Report file:line.`,
    { label: 'impl:r2', phase: 'Implement', agentType: 'implementer' },
  ),
  () => agent(
    `Fix the major/minor UX review findings for gene-sim — GDScript ONLY (do NOT touch crates/**). READ docs/llm/proposals/midnight-review-draft.md §3 (R1/R4 rows) first, then godot/main.gd + godot/main_menu.gd.\n\n` +
    `1. R1 — MENU CONTAINMENT no-op: \`_apply_menu_containment\` (godot/main.gd ~line 597) always pushes an EMPTY consortium, so the core returns an empty schedule regardless of level → selecting "Open" in the menu + START schedules ZERO immigration. FIX: when the menu ContainmentLevel > Sealed(0), pass a non-empty default consortium (the kebab keys of the core's default_mode_a: "bacillus","pseudomonas","aspergillus-niger" — or the full baked set) into set_containment, so "Open" actually contaminates.\n` +
    `2. R1/R4 — only 2 of 7 contaminants exposed: \`CONTAMINANT_KEYS := ["mycoplasma","bacillus"]\` (main.gd ~line 225) is the sole source for the CONTAMINATION consortium checkboxes AND the Inoculate-tool species picker, but 7 airborne Mode-A specs are baked (bacillus/pseudomonas/staph/cutibacterium/aspergillus-niger/penicillium/mycoplasma). FIX: discover the contaminant stems from res://data/species/ at UI build (the docstring already CLAIMS this — make it true) filtered to the airborne Mode-A set, OR at minimum extend CONTAMINANT_KEYS to the 7 baked keys; fix the now-accurate docstring. (Symbionts carsonella/syn3 stay EXCLUDED — they can't airborne-arrive.)\n` +
    `3. R1 — 5→6 tool prose drift: the palette has 6 tools (TOOL_INOCULATE=5) but comments at main.gd ~183, 205, 206, 750, 761 still say "5". FIX: s/5/6/ in those comment sites (the code is already correct).\n` +
    `4. (if quick) R2-minor — PCR/cull picker default-to-species-0 + positional retarget: surface the resolved target in the status line and/or preserve selection by species_key.\n` +
    `All renderer-only (inv #2): GDScript moves keys/counts/config; the core does the biology. Hash-neutral (the pinned run issues none of this). Do NOT commit. Report file:line.`,
    { label: 'impl:ux', phase: 'Implement' },
  ),
])

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. determinism MUST be GREEN against 0x47a0_3c8f_6701_f240 (the pinned run is untouched by the R2 persistence + the UX fixes → hash-neutral); the NEW R2 file-boundary round-trip test passes; livesim/godot-reader green. Report all gates PASS/FAIL with any exact error. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['r2_fixed', 'r2_roundtrip_test', 'hash_neutral', 'menu_contaminates', 'all_contaminants_exposed', 'issues'],
  properties: {
    r2_fixed: { type: 'boolean', description: 'save/load + replay now re-apply roster/species/consortium/containment before replaying the journal → a contaminated multi-species run reloads to the SAME hash (inv #3 restored)' },
    r2_roundtrip_test: { type: 'boolean', description: 'a real file-boundary round-trip test exists + passes (record contaminated multi-species → replay from disk → identical hash); old sessions still load (serde-default back-compat)' },
    hash_neutral: { type: 'boolean', description: 'the pinned single-species-plant literal 0x47a0 is unchanged; the persistence/UX is additive' },
    menu_contaminates: { type: 'boolean', description: 'selecting Open/Clean/Lab in the menu now seeds a non-empty consortium → immigration actually arrives' },
    all_contaminants_exposed: { type: 'boolean', description: 'all 7 baked airborne contaminants are reachable from the in-run consortium + inoculate picker (not just 2)' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const verdict = await agent(
  `Adversarially verify the review fixes. Read \`git diff\`. Try to REFUTE each property; default false if unconfirmable. The KEY check: R2 — does a CONTAMINATED MULTI-SPECIES run now record→replay (from disk) to the SAME hash (run it / read the new test)? Is an OLD session still loadable (back-compat)? Is the pinned literal unchanged? Do the menu + in-run UI now expose contamination correctly?`,
  { label: 'verify', phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
)

return { r2Done, uxDone, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, verdict }
