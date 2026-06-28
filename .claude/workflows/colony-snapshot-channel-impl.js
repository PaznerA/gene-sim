export const meta = {
  name: 'colony-snapshot-channel-impl',
  description:
    'ADR-029 S1 (the ONE 🛑 STOP-THE-LINE colony slice — human-signed-off 2026-06-28; expected ✅ hash-neutral, NOT a re-pin): the off-hash heritable Variant(u16) tag + NextVariantId resource + the dominant_variant_id GSS6 snapshot channel + the brush→colony bind, modelled BYTE-FOR-BYTE on the existing off-hash Species tag + dominant_species_id channel. crates/sim-core: a Variant(u16) component (default 0 = the founding colony of its species) inherited through ReproRow/Child/spawn exactly as Species is; a NextVariantId(u16) monotonic resource (minted += 1, zero SimRng, like NextOrgId); apply_edit_region mints one id + stamps every covered org (the Cities-Skylines district bind — a 2-line extension of the existing covered loop, no new action/wire/RNG); a dominant_variant_id channel (magic GSS5→GSS6, CHANNEL_COUNT 13→14, appended LAST; per-cell ordinal-sorted tally, lowest-id tiebreak, zero SimRng, never folded into hash_world). godot/snapshot.gd GSS6 reader + tools/check_godot_snapshot.sh channels 13→14. THE INVARIANT CASE (airtight per the draft §2.4): hash_world OMITS Species (only OrgId is the sort key), so a heritable spawn-assigned off-SimRng tag is hash-neutral — the pinned single-species-plant config issues zero ApplyEditRegion → every org stays Variant(0) → dominant_variant_id uniformly 0.0 → 0x47a0_3c8f_6701_f240 BYTE-IDENTICAL. NOT a re-pin; actions.ndjson byte-identical (ids derived from event order, not journaled). STOP-THE-LINE: if the literal moves, HALT — do not work around it. Read docs/llm/proposals/visual-declutter-colony-draft.md first. Then gate + adversarially verify.',
  whenToUse: 'After ADR-029 sign-off (granted). The single core/snapshot slice of the colony epic; S2-S6 (renderer) depend on it. The gate STOP-THE-LINE determinism check is the safety net.',
  phases: [{ title: 'Impl' }, { title: 'Gate' }, { title: 'Verify' }],
}

phase('Impl')
const s1 = await agent(
  `Implement ADR-029 S1 — the colony off-hash channel (the ONE 🛑 core slice, signed off; it MUST leave the pinned literal byte-identical — NOT a re-pin). READ FIRST: docs/llm/proposals/visual-declutter-colony-draft.md IN FULL (§2.2 the channel, §2.4 the airtight hash-neutrality argument, §3 the brush bind, §7 S1, and the Critical files list with the exact line anchors). Then READ the real surface (anchors drift): crates/sim-core/src/lib.rs — the off-hash Species component (~:646) + NextOrgId resource (~:540) as the model; the ReproRow (~:1177) / Child (~:1496) / spawn (~:1586) inheritance path; apply_edit_region (~:3198) covered loop + region_inoculate (~:2578); the snapshot() per-cell tally (~:2195-2257); the hash_world tuple (~:3284, which OMITS Species — the off-hash proof) + the pinned pins (~:3443, :3607); the stale doc comment (~:1708 claiming Species is in hash_world's tuple — FALSE, fix it). crates/sim-core/src/snapshot.rs — GSS5 magic + CHANNEL_COUNT 13 + the dominant_species_id channel (~:46/:51/:97) as the model. godot/snapshot.gd (the GSS5 reader) + tools/check_godot_snapshot.sh (~:94 channels=13). CLAUDE.md inv #3 (determinism — zero SimRng, no HashMap iteration, ordinal-sorted tallies) + inv #2.\n\n` +
  `  - Add #[derive(Component)] Variant(u16) (default 0) on every organism + #[derive(Resource)] NextVariantId(u16); spawn Variant(0) at reset; mint via += 1 (wrapping, zero SimRng).\n` +
  `  - Inherit the parent Variant to offspring EXACTLY as Species is copied: add variant:u16 to ReproRow + Child, populate it in the canonical-order pass, spawn Variant(c.variant) alongside Species at the spawn site.\n` +
  `  - apply_edit_region: before the covered loop mint one id (cid = next_variant.0; next_variant.0 = next_variant.0.wrapping_add(1)); inside the loop add &mut Variant to the query + stamp variant.0 = cid on every covered org (the brush→district bind — no new action, no new wire field, no new RNG draw). Optionally stamp region_inoculate spawns with a fresh id (same discipline).\n` +
  `  - dominant_variant_id as the GSS6 14th channel: snapshot.rs magic GSS5→GSS6, CHANNEL_COUNT 13→14, APPEND dominant_variant_id LAST (offsets 0..12 never reorder); in snapshot() add &Variant to the query tuple + keep a per-cell ordinal-sorted Vec<(variant_id,count)> beside the species tally, emit the most-populous id (lowest-id tiebreak), write f32::from(best_variant). Line-for-line the dominant_species_id block (no RNG, no mutation, sorted not hashed).\n` +
  `  - godot/snapshot.gd: MAGIC GSS6, channel_count 14, parse dominant_variant_id LAST (load_from + parse_bytes + _channels_complete). tools/check_godot_snapshot.sh channels=13 → 14. Correct the lib.rs:~1708 doc comment (Species is off-hash; only OrgId is the hash sort key).\n` +
  `  - TESTS (clone the dominant_species_id precedents): dominant_variant_id u16-in-f32 byte round-trip; a single-species/no-edit run → dominant_variant_id uniformly 0.0; a brush ApplyEditRegion mints a DISTINCT in-region dominant_variant_id WHILE run_headless().hash is BYTE-IDENTICAL to the no-brush run; replay of a brushed actions.ndjson reproduces identical district ids.\n` +
  `  - THE STOP-THE-LINE CHECK: cargo test -p sim-core --features determinism MUST keep both pins 0x47a0_3c8f_6701_f240 (lib.rs:3443, :3607) GREEN UNCHANGED. If ANY change moves the literal, STOP and report it — do NOT re-pin, do NOT work around it. Build the cdylib. Do NOT commit. Report every core touch + the GSS6 bump + confirm the literal is UNMOVED + the brush-mints-variant-but-hash-identical test passes.`,
  { label: 'impl', phase: 'Impl', agentType: 'implementer' },
)

phase('Gate')
const gate = await agent(
  `Run \`bash tools/gate.sh\` for gene-sim (generous timeout ~20 min — sim-core determinism tests are heavy). ADR-029 S1 must be GREEN: fmt, clippy, test (incl. the new dominant_variant_id round-trip + uniform-zero + brush-mints-but-hash-identical + replay-district-id tests), **determinism MUST stay 0x47a0_3c8f_6701_f240 BYTE-IDENTICAL** (this is the STOP-THE-LINE slice — a MOVED literal is a determinism FAIL → report it as STOP-THE-LINE, do NOT pass it off as a re-pin), the godot snapshot byte gate now asserting GSS6 / channels=14, license green, godot-reader + livesim green. Report every gate PASS/FAIL with exact errors + EXPLICITLY whether 0x47a0_3c8f_6701_f240 is unmoved. No fixes, no commit.`,
  { label: 'gate', phase: 'Gate', agentType: 'gatekeeper' },
)

phase('Verify')
const VSCHEMA = {
  type: 'object',
  required: ['pinned_literal_byte_identical', 'variant_tag_off_hash', 'channel_models_dominant_species_id', 'brush_bind_no_new_rng_or_action', 'no_biology_in_render', 'issues'],
  properties: {
    pinned_literal_byte_identical: { type: 'boolean', description: 'THE STOP-THE-LINE CHECK (inv #3): the pinned literal 0x47a0_3c8f_6701_f240 is BYTE-IDENTICAL at lib.rs:3443 + :3607 (both pins green UNCHANGED — this is NOT a re-pin); a test proves a brush mints a distinct dominant_variant_id WHILE run_headless().hash equals the no-brush hash; the single-species/no-edit channel is uniformly 0.0.' },
    variant_tag_off_hash: { type: 'boolean', description: 'inv #3: Variant(u16)/NextVariantId are NOT in the hash_world tuple (which omits Species — verified), assigned with ZERO SimRng; hash_world + snapshot both sort by OrgId so adding a component to the archetype never reaches the hash; actions.ndjson is byte-identical (ids derived from event order, not journaled).' },
    channel_models_dominant_species_id: { type: 'boolean', description: 'dominant_variant_id is the GSS6 14th channel appended LAST (offsets 0..12 unchanged), computed by an ordinal-sorted per-cell tally (no HashMap, lowest-id tiebreak, zero SimRng), line-for-line the dominant_species_id block; snapshot.gd + check_godot_snapshot.sh both move to GSS6/14 in this slice (no stale 13-channel reader).' },
    brush_bind_no_new_rng_or_action: { type: 'boolean', description: 'The brush→colony bind in apply_edit_region mints + stamps a Variant id with NO new Action, NO new serde/wire field, NO new SimRng draw (DrawCount/final_word unchanged) — a pure data write on the existing covered loop; Variant is inherited through ReproRow/Child/spawn like Species.' },
    no_biology_in_render: { type: 'boolean', description: 'inv #2: the core only projects per-cell colony IDENTITY (the inert dominant_variant_id ordinal); no genotype->phenotype added; the doc comment at lib.rs:~1708 (Species is off-hash) is corrected.' },
    issues: { type: 'array', items: { type: 'string' } },
  },
}
const skeptics = (await parallel([0, 1, 2].map((i) => () =>
  agent(
    `Adversarially verify ADR-029 S1 (the colony off-hash channel — the STOP-THE-LINE core slice). Read \`git diff\` (crates/sim-core + godot/snapshot.gd + tools/check_godot_snapshot.sh) + docs/llm/proposals/visual-declutter-colony-draft.md §2.4 + CLAUDE.md inv #2/#3. Skeptic #${i} — default each boolean FALSE unless PROVEN. Hunt HARD (this is the slice most likely to move the hash): a MOVED pinned literal 0x47a0_3c8f_6701_f240 (run/inspect the determinism tests — if it moved, this is a STOP-THE-LINE FAIL, NOT a quiet re-pin); Variant or NextVariantId leaking into hash_world or being assigned via a SimRng draw; a NEW SimRng draw / changed DrawCount in apply_edit_region (would shift the stream); HashMap iteration / unsorted tally in the variant channel (non-deterministic); the dominant_variant_id channel NOT appended last / reordering offsets 0..12; a stale 13-channel reader or gate left behind (GSS6 coupling); the offspring Variant copy routed through a hashed/RNG pass; biology computed in render. Report the structured verdict with file:line + EXPLICITLY whether the literal is unmoved. Do NOT edit.`,
    { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
  ),
))).filter(Boolean)
const tally = (k) => skeptics.filter((s) => s[k]).length
const keys = ['pinned_literal_byte_identical', 'variant_tag_off_hash', 'channel_models_dominant_species_id', 'brush_bind_no_new_rng_or_action', 'no_biology_in_render']
const confirmed = keys.every((k) => tally(k) >= 2)
return {
  impl: typeof s1 === 'string' ? s1.slice(0, 800) : s1,
  gate: typeof gate === 'string' ? gate.slice(0, 600) : gate,
  tallies: Object.fromEntries(keys.map((k) => [k, tally(k)])),
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — colony off-hash channel; 0x47a0 byte-identical (NOT a re-pin); brush mints districts hash-neutrally' : 'NEEDS WORK / STOP-THE-LINE if the literal moved',
}
