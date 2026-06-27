export const meta = {
  name: 'perf2-btreemap-to-vec-impl',
  description:
    'PERF-2 (hash-neutral, inv #3): replace EVERY per-tick OrgId-keyed BTreeMap / BTreeSet in the sim-core hot path with REUSED sorted-Vec scratch buffers (push -> sort_merge -> binary_search). Covers metabolism + reproduce_or_die + emit_chem + predation + host_coupling, defines the PredationScratch / HostCouplingScratch resource structs, and adds the sort_merge_org_i64 / org_lookup helpers. BYTE-IDENTICAL: the determinism literal 0x47a0_3c8f_6701_f240 is the correctness oracle — if it stays pinned the conversion is correct; if it moves, a change altered behavior (a bug) -> fix/revert. Do NOT re-pin. Single-threaded (Bevy single-thread + RNG ordering = inv #3). Robust to a clean tree OR a partial WIP already present.',
  whenToUse:
    'The PERF-2 slice after PERF-1 (scratch-Vec hoist). Self-contained: converts all OrgId-keyed maps to sorted-Vec whether the tree is clean main or carries a partial conversion. Must keep the pinned literal byte-identical; before/after benched; adversarially verified. Stops for human commit.',
  phases: [
    { title: 'Implement' },
    { title: 'Gate' },
    { title: 'Bench' },
    { title: 'Verify' },
  ],
}

phase('Implement')
const impl = await agent(
  `Implement the gene-sim PERF-2 optimization — Rust ONLY, sim-core. Replace EVERY per-tick OrgId-keyed BTreeMap / BTreeSet in the HOT PATH with a REUSED sorted-Vec scratch buffer. This is a BYTE-IDENTICAL optimization: run_headless for the pinned config MUST keep returning hash 0x47a0_3c8f_6701_f240. THE HASH IS THE CORRECTNESS ORACLE — if it stays pinned your change is correct; if it moves, your change altered behavior (a bug) -> fix or revert THAT change. Do NOT re-pin. Do NOT change any computed value, rounding, iteration order, or RNG draw.\n\n` +
  `STEP 0 — discover the tree state. Run \`cargo check -p sim-core\` and \`git status\`/\`git diff\`. A PARTIAL conversion may already be present (metabolism + reproduce_or_die converted, the helpers added, the two scratch structs registered-but-undefined -> a non-compiling tree). If so, VERIFY those conversions match the pattern below and KEEP them; complete only what remains. If the tree is clean main, do the WHOLE thing. Either way the end state is identical: every site below converted, sim-core compiling, hash unmoved.\n\n` +
  `THE HELPERS (add to lib.rs as \`pub(crate)\` if absent; reuse if present). They are the heart of the byte-identicality:\n` +
  '```rust\n' +
  '/// Sort a (OrgId, i64) collect-buffer by key and merge consecutive duplicates by SUMMING their values —\n' +
  '/// byte-identical to a BTreeMap<u64,i64> entry().or_insert(0)+=v accumulate (sorted by key, each key once\n' +
  '/// carrying the SUM). i64 add is order-independent + bounded << i64::MAX -> no overflow.\n' +
  'pub(crate) fn sort_merge_org_i64(buf: &mut Vec<(u64, i64)>) {\n' +
  '    buf.sort_unstable_by_key(|(k, _)| *k);\n' +
  '    if buf.len() <= 1 { return; }\n' +
  '    let mut write = 0usize;\n' +
  '    for read in 1..buf.len() {\n' +
  '        if buf[read].0 == buf[write].0 { buf[write].1 += buf[read].1; }\n' +
  '        else { write += 1; if write != read { buf[write] = buf[read]; } }\n' +
  '    }\n' +
  '    buf.truncate(write + 1);\n' +
  '}\n' +
  '/// binary_search on a sorted unique-key (OrgId,i64) buf — byte-identical to BTreeMap::get.\n' +
  'pub(crate) fn org_lookup(buf: &[(u64, i64)], key: u64) -> Option<i64> {\n' +
  '    buf.binary_search_by_key(&key, |(k, _)| *k).ok().map(|i| buf[i].1)\n' +
  '}\n' +
  '```\n' +
  `THE REUSE DISCIPLINE (every site): the buffer lives in a \`#[derive(Resource, Default)]\` scratch struct; each tick \`let mut b = std::mem::take(&mut scratch.b); b.clear();\` ... fill ... use ... then store back \`scratch.b = b;\`. NEVER allocate the map fresh per tick. Mirror the existing MetabolismScratch / ReproScratch (and mineralize's MineralizeScratch ResMut param) exactly.\n\n` +
  `THE SITES, grouped by shape:\n` +
  `  A) i64 maps -> reused \`Vec<(u64,i64)>\` + sort_merge_org_i64 + org_lookup (apply via the arbitrary-order q.iter_mut()):\n` +
  `     - lib.rs metabolism Pass-3 \`by_org\` (unique keys; merge is a defensive no-op, the sort is real).\n` +
  `     - lib.rs reproduce_or_die \`maint_energy\` + \`parent_debit\` (unique keys).\n` +
  `     - chem.rs emit_chem \`spent\` (GENUINE duplicates: an org spends on BOTH the kin AND the alarm arm -> push both, sort_merge SUMS them, byte-identical to entry().or_insert(0)+=).\n` +
  `     - trophic.rs predation \`pred_credit\` + host_coupling \`symb_credit\` (GENUINE duplicates: a predator eats multiple prey / a symbiont draws from multiple hosts).\n` +
  `  B) iterate-in-place row buffers -> reused row Vec, sorted, then iterated in (cell,..) order — NO lookup at all (already the zero-lookup ideal):\n` +
  `     - lib.rs metabolism \`litterfall\` -> \`Vec<(u32 cell, u16 species, u64 org, i64 amt)>\`, sort_unstable_by_key (cell,species,org), iterate. Byte-identical to the old OrgId-keyed-BTreeMap-then-re-sort (only the final (cell,species,org) order survived).\n` +
  `     - lib.rs metabolism \`toxin_mints\` -> \`Vec<(u32 cell, u64 org, i64 amt)>\`, sort by (cell,org), iterate.\n` +
  `  C) membership BTreeSet<Entity> (only contains-queried, never iterated) -> reused sorted \`Vec<Entity>\` + binary_search:\n` +
  `     - lib.rs reproduce_or_die \`dead_set\`; trophic.rs predation + host_coupling \`despawn_set\` (x2).\n` +
  `  D) STRUCT-valued maps — THE TRAP (the plain i64 helper does NOT apply). Three-phase: build (accumulate), then get_mut (sets a flag), then get (apply). Use a reused \`Vec<(u64, T)>\` sorted by org + binary_search for get/get_mut:\n` +
  `     - trophic.rs predation \`prey_debit\`: \`BTreeMap<u64, PreyDebit{eaten:i64, dead:bool}>\`. Build: push (pr.org, PreyDebit{eaten, dead:false}); after collect, sort by org + merge consecutive dup orgs by SUMMING eaten (keys unique per build -> defensive, include it). Then the \`for r in &prey\` pass binary_searches to get_mut and set d.dead; the q.iter_mut() pass binary_searches to get and apply d.eaten. PRESERVE the exact eaten-sum + dead-flag semantics.\n` +
  `     - trophic.rs host_coupling \`host_debit\`: \`BTreeMap<u64, HostDebit{drawn:i64}>\` — same shape (sum drawn).\n` +
  `  E) DEFINE the two scratch structs in trophic.rs that own B/C/D for predation + host_coupling: \`#[derive(Resource, Default)] pub(crate) struct PredationScratch { pred_credit: Vec<(u64,i64)>, prey_debit: Vec<(u64,PreyDebit)>, despawn: Vec<Entity>, /* + the apportion weights/shares/rem if you also hoist them */ }\` and \`HostCouplingScratch { symb_credit, host_debit, despawn }\`. Add a \`mut scratch: ResMut<PredationScratch>\` / \`ResMut<HostCouplingScratch>\` param to the predation / host_coupling systems and register them in Simulation::new with \`world.insert_resource(...)\` (next to the MineralizeScratch registration).\n` +
  `  OUT OF SCOPE: the render-only OFF-HASH \`dominant_species\` \`tally: Vec<BTreeMap<u16,u32>>\` argmax (it draws no RNG, is not per-tick scratch) — leave it.\n\n` +
  `WHY byte-identical (write it into the doc comment at each site, matching the PERF-2 convention): sorted unique-key Vec iterates identically to a BTreeMap; binary_search == BTreeMap::get; sort_merge summing dups == entry().or_insert(0)+=; sorted Vec + binary_search == BTreeSet::contains. The apply passes that mutate ECS components MUST use the arbitrary-order q.iter_mut() (ECS table order is NOT canonical — the reason collect-then-apply exists), so a zero-lookup position-indexed Vec is NOT achievable there; binary_search is the correct ceiling (only the group-B litterfall/toxin_mints, applied by iterating the buffer, are lookup-free).\n\n` +
  `CRITICAL discipline: convert ONE site at a time; after EACH, \`cargo build -p sim-core\` + run the pinned-hash check (the determinism_hash_is_pinned test, or run_headless on the pinned cfg) and confirm it is STILL 0x47a0_3c8f_6701_f240. Catch a hash move immediately so you know which site broke it. Keep clippy + fmt clean. Do NOT commit. Report file:line per change, confirm sim-core COMPILES, and confirm the final pinned hash is 0x47a0_3c8f_6701_f240.`,
  { label: 'impl', phase: 'Implement', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim. determinism MUST be GREEN against 0x47a0_3c8f_6701_f240 (PERF-2 is byte-identical — a moved hash = FAIL, not a re-pin). fmt/clippy/test/proptest/ledger-closure all green. Report each gate PASS/FAIL with any exact error. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Bench')
const bench = await agent(
  `Measure gene-sim sim-core tick-throughput AFTER the PERF-2 BTreeMap->sorted-Vec conversion. Build release then run \`cargo bench -p sim-core\` (the tick_loop bench: entities 1000/5000/10000 x 50 gens). Report the new times + throughput (Kelem/s) per entity count and compute the % change vs the latest DECISIONS.md / PERF-1 baseline (read it from docs/llm/DECISIONS.md — do not invent a number). Also grep the changed functions (metabolism/reproduce_or_die/emit_chem/predation/host_coupling) to CONFIRM no \`BTreeMap::new\`/\`BTreeSet\`/\`std::collections::BTreeMap\` remains in the per-tick path (the dominant_species snapshot tally is allowed — it is off-hash render-only). Report the before/after table + the net % and whether per-tick map allocation is now zero. No commit.`,
  { label: 'bench', phase: 'Bench', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['compiles', 'hash_neutral', 'all_sites_done', 'faster_or_neutral', 'speedup_pct', 'struct_maps_correct', 'determinism_safe', 'issues'],
  properties: {
    compiles: { type: 'boolean', description: 'sim-core compiles (PredationScratch/HostCouplingScratch defined + registered)' },
    hash_neutral: { type: 'boolean', description: 'the pinned literal 0x47a0_3c8f_6701_f240 is UNCHANGED (byte-identical; not re-pinned) and the determinism gate is green' },
    all_sites_done: { type: 'boolean', description: 'by_org/litterfall/toxin_mints/maint_energy/parent_debit/dead_set + emit_chem.spent + predation(prey_debit/pred_credit/despawn) + host_coupling(host_debit/symb_credit/despawn) all converted; no per-tick BTreeMap/BTreeSet left in the hot path' },
    faster_or_neutral: { type: 'boolean', description: 'the tick_loop bench is faster-or-equal vs baseline (the win is allocation elimination; a regression means a bad conversion)' },
    speedup_pct: { type: 'string', description: 'the measured net % vs the DECISIONS.md baseline (e.g. "~5% at 10k entities")' },
    struct_maps_correct: { type: 'boolean', description: 'prey_debit(PreyDebit{eaten,dead}) + host_debit(HostDebit{drawn}) converted preserving build->get_mut(dead)->get(apply); eaten/drawn summed, dead flag intact' },
    determinism_safe: { type: 'boolean', description: 'no change to computed values / rounding / iteration order / RNG draws; integer-only; no new HashMap iteration; binary_search==get and sort_merge==entry+=' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const verdict = await agent(
  `Adversarially verify the gene-sim PERF-2 BTreeMap->sorted-Vec conversion. Read \`git diff\`. Try to REFUTE each property; default false if unconfirmable. KEY checks: (1) does sim-core COMPILE (the two scratch structs exist + are registered)? (2) is the pinned literal 0x47a0_3c8f_6701_f240 UNCHANGED — NOT re-pinned? (3) for the STRUCT-valued maps prey_debit/host_debit, is the three-phase build->get_mut(dead)->get(apply) byte-identical (eaten/drawn summed; dead flag in the same pass; binary_search returns the entry BTreeMap::get would)? (4) for the i64 maps spent/pred_credit/symb_credit, does sort_merge SUM duplicate keys exactly like entry().or_insert(0)+= (org spends kin+alarm; predator eats multiple prey)? (5) did any change alter iteration order / rounding / RNG (would have moved the hash)? (6) is the bench faster-or-equal? Report concrete file:line evidence for any refutation.`,
  { label: 'verify', phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
)

return {
  impl: typeof impl === 'string' ? impl.slice(0, 1200) : impl,
  gate: typeof gate === 'string' ? gate.slice(0, 500) : gate,
  bench: typeof bench === 'string' ? bench.slice(0, 800) : bench,
  verdict,
}
