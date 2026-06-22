export const meta = {
  name: 'oversight-loop-plumbing-impl',
  description:
    'ADR-017 OVERSIGHT loop — Slice A (HASH-NEUTRAL plumbing): S2 frozen FBA-derived E. coli KO table + S3 crates/oracle-fba boundary crate (std-only, quantize-before-return, mirrors oracle-slim) + S4 harness CreditLedger (RNG-free earn) + S5 due_epoch multi-fidelity firewall (RequestEcoliEdit/CommitEcoliImpact wired end-to-end, draining in (SpeciesId,req_id) order with journaled slip). The committed impact applies an IDENTITY modifier (coefficient 1.0) so the whole loop is functional + replayable WITHOUT moving the determinism hash. S6 activation (the real [0.5,1.5] coefficient) is the separate re-pin Slice B.',
  whenToUse:
    'After ADR-018 (BiGG licensing accepted) + the CHEMOSTAT-J merge. Builds the earned-edit machinery hash-neutral, isolating the determinism re-pin to a tiny later activation step. Autonomous; stops for human commit.',
  phases: [
    { title: 'Build' },
    { title: 'Gate' },
    { title: 'Verify' },
  ],
}

phase('Build')
const build = await agent(
  `Implement ADR-017 OVERSIGHT loop "Slice A" for gene-sim, HASH-NEUTRAL, per docs/llm/proposals/ecoli-oversight-gameloop-draft.md. READ that draft IN FULL first, then crates/oracle-slim/src/lib.rs (the boundary-crate template), crates/harness/src/{lib.rs,campaign.rs,replay.rs} (the Action enum + journaled replay + campaign edit_budget precedent), data/species/ecoli.json (the anchor genes), and scripts/bake_ecoli_species.py (the data-bake convention). Build all FOUR pieces, keeping the pinned determinism literal 0x4e4d_0520_722a_a069 UNCHANGED:\n\n` +
  `**S2 — frozen KO table** (hash-neutral DATA). Produce data/ecoli_ko_table.json: per anchor gene (gltA/GO-4108, ptsG/GO-8982, pflB/GO-8861, pta/GO-8959, ldhA/GO-8720) a quantized growth-ratio-vs-wild-type (u16 permille on glucose-minimal, aerobic). PREFER a real cobrapy FBA bake on the BiGG e_coli_core model IF cobra installs cleanly+quickly into .venv; OTHERWISE curate the values from CITED published FBA KO predictions for these genes and mark the table \`"source": "curated-from-literature"\` with per-gene citations + a documented cobrapy-bake upgrade path. Add scripts/bake_ecoli_ko_table.py (the bake/curate script + provenance). Determinism: the table is frozen integer data; the offline solver's float non-determinism never reaches the sim.\n\n` +
  `**S3 — crates/oracle-fba** (boundary crate, inv #1). A std-only \`#![forbid(unsafe_code)]\` crate STRUCTURALLY mirroring oracle-slim (FbaError mirroring SlimError; no GPL/heavy deps; links nothing). For a single-gene edit it does a FROZEN-TABLE lookup of data/ecoli_ko_table.json and returns a QUANTIZED u16 growth ratio (floats never escape). check_license.sh already lists oracle-fba in BOUNDARY_CRATES — confirm it stays green. NOT a workspace member if that matches oracle-slim's setup; otherwise a normal member with zero non-std deps beyond serde_json.\n\n` +
  `**S4 — harness CreditLedger** (RNG-free earn, 0 bytes to hash). In crates/harness/src/oversight.rs: \`CreditLedger{credit:u64, accrued_total:u64}\`; \`credit += clamp(quantize(per-gen objective/FlowMatrix-health delta), 0, per_gen_cap)\` folded deterministically over the per-gen stats stream the engine already produces (region_allele / observe / flow_matrix — all RNG-free read-only). Spend gated like campaign.rs edit_budget (refuse, don't replay, when credit < cost). Lives in the harness/env layer, never an ECS World resource → adds 0 bytes to hash_world.\n\n` +
  `**S5 — due_epoch firewall** (wire the inert Actions end-to-end). crates/harness/src/firewall.rs: \`EditFirewall{pending: BTreeMap<u32, Vec<PendingImpact>>}\` keyed by due_epoch, drained at each epoch boundary in ascending (SpeciesId, req_id) order (NOT HashMap-iterated in sim logic, inv #3). RequestEcoliEdit buffers a PendingImpact (req_id = a journaled monotonic counter, NEVER wall-clock); the deep oracle-fba result is committed as CommitEcoliImpact at its due_epoch (Tick-clocked, never wall-clock); if it misses, the commit deterministically SLIPS to the next epoch with an inline journaled \`slipped_from\`, plus a max-slip deterministic default (a neutral sentinel impact) so the journal always terminates. CRITICAL FOR THIS SLICE: when an impact commits, apply an **IDENTITY modifier (coefficient = 1.0, i.e. no selection change)** — so the entire firewall runs end-to-end and replays exactly, but the determinism hash does NOT move. Add the FIREWALL ACCEPTANCE TEST: the run hash is byte-identical whether the oracle is absent, slow, or returns different bytes — until commit (and with identity modifier, even after). Off-thread oracle dispatch lives in the GeneSimEnv harness driver (NOT godot/, NOT the single-threaded World).\n\n` +
  `ALL hash-path arithmetic integer; no new RNG draws in the sim; the inert→active Action serde stays back-compatible (existing actions.ndjson unchanged). The pinned literal MUST remain 0x4e4d_0520_722a_a069 — if anything would move it, STOP and report (that belongs in Slice B). Update/extend tests. Do NOT commit. Report exactly what you built, file:line, and confirm cobrapy-bake-vs-curated which path you took.`,
  { label: 'build', phase: 'Build', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. determinism MUST stay GREEN against 0x4e4d_0520_722a_a069 (Slice A is hash-neutral); license MUST stay GREEN (oracle-fba is a clean boundary crate, inv #1); livesim/godot-reader green. Report all gates PASS/FAIL with any failure's exact error. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['hash_neutral', 'firewall_no_wallclock_leak', 'replay_backcompat', 'boundary_clean', 'credit_rng_free', 'issues'],
  properties: {
    hash_neutral: { type: 'boolean', description: 'pinned literal unchanged; identity modifier applied; no new RNG/hash input' },
    firewall_no_wallclock_leak: { type: 'boolean', description: 'hash byte-identical whether oracle absent/slow/different-bytes; due_epoch is Tick-clocked; slip is journaled' },
    replay_backcompat: { type: 'boolean', description: 'existing actions.ndjson still deserializes; req_id is a journaled monotonic counter, not wall-clock' },
    boundary_clean: { type: 'boolean', description: 'oracle-fba is std-only, links nothing, quantizes-before-return; license gate green (inv #1)' },
    credit_rng_free: { type: 'boolean', description: 'CreditLedger accrual is RNG-free, deterministic on replay, 0 bytes to hash_world' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify gene-sim OVERSIGHT Slice A is hash-neutral + determinism-safe. Read git diff + the new oracle-fba/oversight.rs/firewall.rs + the firewall acceptance test. Skeptic #${i}, default each boolean false if unconfirmable. Hunt: any wall-clock (Instant/SystemTime/now) reaching the hash via due_epoch; oracle latency/availability/output changing the sim hash; a HashMap iterated in sim logic; a non-journaled/wall-clock req_id; the "identity modifier" secretly perturbing selection (it must be a true no-op this slice); the credit accrual drawing RNG or differing on replay; oracle-fba pulling a non-std/GPL dep.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA },
  ),
))).filter(Boolean)
const ok = skeptics.filter((s) => s.hash_neutral && s.firewall_no_wallclock_leak && s.boundary_clean).length
return { build, gate: typeof gate === 'string' ? gate.slice(0, 400) : gate, skeptics, verdict: ok >= 2 ? 'SLICE A CONFIRMED hash-neutral' : 'NEEDS WORK' }
