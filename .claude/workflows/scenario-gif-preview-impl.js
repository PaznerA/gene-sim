export const meta = {
  name: 'scenario-gif-preview-impl',
  description:
    'AUTO-GIF preview of a scenario\'s KEY EVENTS — feeds the RCT-style scenario selector\'s right-panel animation. Two stages: (1) crates/harness — a key-event detector that reads a gem/starter\'s D1 PerGenTrace (off-hash) and picks the KEY generations (booms/crashes/takeovers from per-gen population deltas + the scheduled edit gens) → a frame schedule; (2) a capture+assemble tool — headless godot replays the gem + captures a frame at each key gen (file-capture, macOS-PIPE-safe — NOT $(godot…)), then assembles an animated GIF (the MIT `gif` crate in-process, pinned per inv #7 — GPL-clean inv #1; or an imagemagick/ffmpeg subprocess at the process boundary as a fallback) → data/presets/starters/<slug>.gif. HASH-NEUTRAL: the GIF is render output from the OFF-HASH snapshot + the off-hash trace; no sim-path change, no biology in render (inv #2/#3); the pinned literal 0x47a0_3c8f_6701_f240 is UNMOVED. Then gate + adversarially verify.',
  whenToUse: 'After starter-map-library (the selector + committed starters). The auto-preview GIF the selector shows for each scenario\'s key events.',
  phases: [{ title: 'KeyEvents' }, { title: 'CaptureAssemble' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('KeyEvents')
const s1 = await agent(
  `Implement the KEY-EVENT detector for the scenario GIF preview (crates/harness; reads the OFF-HASH D1 trace — inv #3). READ FIRST: crates/harness/src/capture.rs (capture_trace -> PerGenTrace: per-gen GenRow{gen, pop:[per-species], allele_q, flow}) + crates/discovery/src/trace.rs (PerGenTrace/GenRow fields). READ crates/discovery/src/ecology.rs (how the scorer detects events — booms/crashes/takeovers — from the per-gen population series; REUSE that event logic so the GIF keys off the SAME events the scorer rewards). READ the gem JSON shape (config + edits + gens + recorded_hash) + crates/harness/src/discover.rs (env_config_for, edits_to_actions — the edit gens). CLAUDE.md inv #3 (the trace is off-hash; no sim change).\n\n` +
  `  - Add a key-event detector: given a gem (or its config), capture/replay its PerGenTrace and compute the KEY generations to snapshot — the boom/crash gens (large per-species population deltas, same threshold logic the scorer's M5/event detection uses), the takeover/dominance-flip gens, and the scheduled EDIT gens (from config.edits via the edits_to_actions gen mapping). Return an ordered, deduped, capped (e.g. <= 12) list of (gen, label) frame keys, always including gen-1, a few evenly-spaced context frames, and the final gen, so the GIF reads as a coherent short story of the run.\n` +
  `  - This is pure off-hash analysis of the trace (zero SimRng, no sim mutation). A test: the detector is deterministic per gem + the key gens are within [1, gens] + include the edit gens.\n` +
  `  - VERIFY 0x47a0_3c8f_6701_f240 UNMOVED (off-hash trace read; cargo test). Do NOT commit. Report the detector signature + the frame-schedule shape + the reused event logic.`,
  { label: 'keyevents', phase: 'KeyEvents', agentType: 'implementer' },
)

phase('CaptureAssemble')
const s2 = await agent(
  `Implement the CAPTURE + ASSEMBLE half of the scenario GIF preview, on the Stage-1 key-event schedule:\n${typeof s1 === 'string' ? s1.slice(0, 700) : ''}\n\n` +
  `READ tools/check_godot_snapshot.sh (the macOS-SAFE headless capture: \`timeout\` + FILE capture via run_godot — a \`$(godot…)\` pipe-capture HANGS on macOS; you MUST reuse this discipline) + the godot --shot path in godot/main.gd (how --shot/--steps/--view render a frame to a PNG; the gem/starter loader from discovery-load-gem-replay if present). READ run.sh (the cdylib build + data staging). CLAUDE.md inv #1 (GPL stays at the process boundary — any external encoder is a subprocess) + inv #2 (render-only) + inv #7 (pin any new crate).\n\n` +
  `  - Capture: a headless tool/script that, for a gem + its frame schedule, replays the run and captures ONE PNG frame per key gen via the macOS-safe file-capture (timeout + file, never pipe). Reuse the gem/starter loader (discovery-load-gem-replay) so the captured run is the discovered scenario (incl. its edits).\n` +
  `  - Assemble: encode the PNG frames into an animated GIF at data/presets/starters/<slug>.gif. PREFER the MIT \`gif\` crate (+ a quantizer like \`color_quant\`/\`image\`) in a small Rust tool/bin — in-process, GPL-clean (inv #1), pinned in Cargo.toml (inv #7). FALLBACK: an \`imagemagick\`/\`ffmpeg\` subprocess at the process boundary (inv #1) if a pure-Rust path is too heavy — guarded + documented. Frame delay tuned so the GIF is a readable ~2-4s loop.\n` +
  `  - A small index hook: the GIF path sits next to the starter (<slug>.gif) so the RCT selector shows it; staged into res:// alongside the starter library (run.sh + the byte-gate) OR served from the gen dir — your call, documented.\n` +
  `  - Build + (macOS-safe) headless smoke: produce a GIF for one sample gem; assert the file exists + is a valid non-empty GIF with >1 frame. Do NOT commit. Report the capture path (macOS-safe), the encoder choice + its license/pin, and the sample GIF.`,
  { label: 'captureassemble', phase: 'CaptureAssemble', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim (generous timeout ~15 min). The scenario-GIF slice must be GREEN: fmt, clippy, test (incl. the key-event detector test), determinism MUST stay 0x47a0_3c8f_6701_f240 (the GIF is render output from the off-hash snapshot — a moved hash is a FAIL), license green (any new encoder crate MUST be MIT/Apache, NOT GPL — inv #1; check_license.sh), godot-reader + livesim green. Report every gate PASS/FAIL with exact errors. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['key_events_from_offhash_trace', 'capture_macos_safe_and_offhash', 'gif_encoder_gpl_clean_pinned', 'no_biology_in_render', 'issues'],
  properties: {
    key_events_from_offhash_trace: { type: 'boolean', description: 'inv #3: the key gens come from the OFF-HASH PerGenTrace (per-gen pop deltas / takeovers reusing the scorer event logic + the edit gens); deterministic per gem; zero SimRng; 0x47a0_3c8f_6701_f240 unmoved.' },
    capture_macos_safe_and_offhash: { type: 'boolean', description: 'Frames are captured from the OFF-HASH snapshot via the macOS-safe file-capture (timeout + file, NEVER a $(godot…) pipe — the documented macOS hang); the capture changes no sim state.' },
    gif_encoder_gpl_clean_pinned: { type: 'boolean', description: 'inv #1/#7: the GIF encoder is MIT/Apache (the `gif` crate) pinned in Cargo.toml, OR an external imagemagick/ffmpeg SUBPROCESS at the process boundary — never a GPL crate linked into the binary; check_license.sh green.' },
    no_biology_in_render: { type: 'boolean', description: 'inv #2: the GIF pipeline is render/tooling only — no genotype->phenotype; it reads the off-hash trace + snapshot, computes no biology.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify the scenario-GIF preview slice (gene-sim). Read \`git diff\` + CLAUDE.md inv #1/#2/#3/#7. Skeptic #${i} — default each boolean FALSE unless confirmed. Hunt: a MOVED pinned literal 0x47a0_3c8f_6701_f240 or any sim-path change (must be off-hash render/tooling); a $(godot…) PIPE capture (hangs on macOS — must be timeout+file); a GPL crate linked for GIF encoding (inv #1 — must be MIT/Apache `gif` or an external subprocess) or an unpinned new crate (inv #7); biology computed in the GIF pipeline; key-event gens NOT derived from the off-hash trace. Report the structured verdict with file:line. Do NOT edit.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['key_events_from_offhash_trace', 'capture_macos_safe_and_offhash', 'gif_encoder_gpl_clean_pinned', 'no_biology_in_render']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  keyevents: typeof s1 === 'string' ? s1.slice(0, 600) : s1,
  captureassemble: typeof s2 === 'string' ? s2.slice(0, 600) : s2,
  gate: typeof gate === 'string' ? gate.slice(0, 500) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — auto-GIF of key events; off-hash + macOS-safe + GPL-clean; hash-neutral' : 'NEEDS WORK',
}
