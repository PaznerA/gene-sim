export const meta = {
  name: 'verify-specimen-testing-unblock',
  description:
    'Adversarially verify the renderer-only specimen/testing-unblock slice (Item 1 inject button, Item 2 brush→variant + extinct struck-through, Item 3 Load Starter preset). All changes are GDScript + staging scripts + release.yml — ZERO Rust, so the pinned hash 0x47a0_3c8f_6701_f240 is trivially unmoved. Three independent skeptics read `git diff` and hunt: an inv #2 violation (any genotype→phenotype / biology computed in GDScript rather than projected from a core export); an inv #3 risk (any Rust touched, or a snapshot/channel change that could move the hash); GDScript correctness bugs (parse/runtime: bad RichTextLabel sizing, wrong _extinct keying, an out-of-range dominant_species_id index, a null-deref on an older cdylib, a roster-clear that leaves stale rows); and UX faithfulness to the three user asks.',
  whenToUse: 'After implementing the specimen-testing-unblock slice + gate GREEN, before merge.',
  phases: [{ title: 'Verify' }],
}

const VSCHEMA = {
  type: 'object',
  required: [
    'no_biology_in_gdscript',
    'hash_neutral_no_rust',
    'gdscript_correct',
    'graceful_degrade',
    'ux_faithful',
    'issues',
  ],
  properties: {
    no_biology_in_gdscript: {
      type: 'boolean',
      description:
        'inv #2: every new GDScript line is pure PRESENTATION — a projection of a core export (observe_species phenotype, dominant_species_id plane, species_key, population_size) or a lookup/string/layout. NO genotype→phenotype, no trait math, no biology decided in GDScript.',
    },
    hash_neutral_no_rust: {
      type: 'boolean',
      description:
        'inv #3: the diff touches ZERO Rust (only godot/*.gd + run.sh + tools/check_godot_snapshot.sh + release.yml + a staged preset). The pinned literal 0x47a0_3c8f_6701_f240 cannot move. No snapshot magic/channel-count change.',
    },
    gdscript_correct: {
      type: 'boolean',
      description:
        'No parse/runtime bug in the new GDScript: _append_edit_variant_for / _active_species_id / _dominant_species_at handle empty logs + out-of-range cells; _specimen_caption RichTextLabel sizes + positions like the Label it replaces; _extinct is keyed by SpeciesId (row group) consistently; _poll_population_alerts maintains _ever_alive/_extinct without false-flagging; the inject button + Load Starter wire to existing members that exist at call time.',
    },
    graceful_degrade: {
      type: 'boolean',
      description:
        'Older cdylib / file-replay / absent preset all degrade gracefully (has_method guards, FileAccess existence checks, fallbacks) — no crash when observe_species / dominant_species_id / the preset file is missing.',
    },
    ux_faithful: {
      type: 'boolean',
      description:
        "Matches the three user asks: (1) a discoverable whole-species Inject button; (2) a brush stroke surfaces a variant in the specimen view AND an extinct species stays struck-through-but-kept (not removed); (3) one-click Load Starter prefills a multi-species roster + env + containment.",
    },
    issues: { type: 'array', items: { type: 'string' }, description: 'concrete problems found (file:line), empty if none' },
  },
}

phase('Verify')
const skeptics = (
  await parallel(
    [0, 1, 2].map((i) => () =>
      agent(
        `Adversarially verify the gene-sim "specimen testing unblock" slice on branch auto/specimen-testing-unblock-2026-06-23. Read \`git diff main...HEAD\` (or \`git diff\` if uncommitted) for the FULL change set, then read the surrounding context in godot/main.gd, godot/main_menu.gd, tools/check_godot_snapshot.sh, run.sh, .github/workflows/release.yml. Also read CLAUDE.md invariants #2 and #3.\n\n` +
          `Skeptic #${i} — default each boolean FALSE unless you can positively confirm it from the code. Hunt hard for:\n` +
          `  • inv #2 (STOP THE LINE): ANY biology / genotype→phenotype / trait arithmetic decided in GDScript instead of read from a core export. The new code should only PROJECT core data (observe_species phenotype, dominant_species_id, species_key, population_size) into pixels/labels.\n` +
          `  • inv #3: confirm ZERO Rust files changed (the hash literal 0x47a0_3c8f_6701_f240 cannot move if no Rust + no snapshot channel/magic change). If ANY crates/** file is in the diff, set hash_neutral_no_rust=false and flag it.\n` +
          `  • GDScript correctness: _dominant_species_at index bounds; _append_edit_variant_for with an empty/missing log; _specimen_caption RichTextLabel (does it size/position to match the Label it replaces, so the grid bounds + hit-test + _emphasise_focus still line up? note holder child 0 must stay the glyph for _specimen_at); _extinct keyed by SpeciesId vs _live_species_logs keys; _poll_population_alerts not false-flagging a not-yet-spawned species; the inject button + Load Starter referencing members that exist when pressed; the roster-clear leaving no stale rows.\n` +
          `  • graceful degrade: older cdylib without observe_species/dominant_species_id; file-replay; a missing/malformed preset JSON.\n` +
          `  • UX faithfulness to the three asks (inject button discoverable; brush→variant + extinct struck-through-but-KEPT; Load Starter one-click multi-species prefill).\n\n` +
          `Report the structured verdict. Be specific with file:line in issues. Do NOT edit anything.`,
        { label: `verify:skeptic${i}`, phase: 'Verify', schema: VSCHEMA, agentType: 'reviewer' },
      ),
    ),
  )
).filter(Boolean)

const tally = (k) => skeptics.filter((s) => s[k]).length
const confirmed =
  tally('no_biology_in_gdscript') >= 2 &&
  tally('hash_neutral_no_rust') >= 2 &&
  tally('gdscript_correct') >= 2 &&
  tally('graceful_degrade') >= 2 &&
  tally('ux_faithful') >= 2
return {
  skeptics,
  tallies: {
    no_biology_in_gdscript: tally('no_biology_in_gdscript'),
    hash_neutral_no_rust: tally('hash_neutral_no_rust'),
    gdscript_correct: tally('gdscript_correct'),
    graceful_degrade: tally('graceful_degrade'),
    ux_faithful: tally('ux_faithful'),
  },
  all_issues: skeptics.flatMap((s) => s.issues || []),
  verdict: confirmed ? 'CONFIRMED — renderer-only, hash-neutral, faithful' : 'NEEDS WORK',
}
