# Handoff — gene-sim presentation/gameplay polish + discovery epic (2026-06-23)

> Paste this as the next session's opening prompt (after the conversation compress). It is self-contained.

## Where we are
- **main @ `c38bd3d`** (clean, all CI-green). Pinned determinism literal **`0x47a0_3c8f_6701_f240`** — held
  byte-identical through everything below (every slice was hash-neutral). Read `CLAUDE.md` + `docs/llm/SPEC.md`
  (the 7 invariants) + `docs/llm/DECISIONS.md` (ADRs, incl. ADR-020 perf, ADR-021 GSS5) + `docs/llm/autonomous-roadmap.md` §9 at the start.
- **Working discipline** (proven this session): renderer-only changes are hash-neutral (godot/*.gd + the
  off-hash snapshot); per slice → branch `auto/<name>-2026-MM-DD` → implement (often via a Workflow:
  design→implement→gate→verify) → `bash tools/gate.sh` GREEN (determinism MUST stay `0x47a0`) → push →
  `gh workflow run ci.yml` → `gh run watch --exit-status` (the `assert-isa-match` multi-ISA job is the real
  gate) → merge `--no-ff -F <msgfile>` (NEVER `-m` with backticks) → push main. Verify godot UI with
  `godot --path godot -- --live --species <key> [--inject] [--view specimen|--zoom 6] --shot /tmp/x.png` then
  Read the png. Build the cdylib first: `cargo build --manifest-path crates/godot-sim/Cargo.toml` + stage
  `cp data/species/*.json godot/data/species/ && cp data/codex/*.json godot/data/codex/`.
- **Recent landed:** specimen view (evidence-based morphotypes, 2D grid species-vertical/variants-horizontal,
  edits append a variant, no-zoom-on-focus, names under models, microbe capsule fix); intervention/contamination
  UI polish; ecosystem-map species sizing (GSS5 `dominant_species_id` + `godot/species_visual_map.gd`);
  the `data/presets/primordial.json` starter; the emergent-discovery epic
  (`docs/llm/proposals/emergent-discovery-harness-draft.md`).

## Backlog (priority order)

1. **Add an explicit "💉 Inject (whole species)" button.** BUG/UX: the whole-species CRISPR inject (the only
   edit that appends a NEW specimen variant) currently fires ONLY on Enter in the Guide LineEdit
   (`godot/main.gd` `_on_guide_submitted` → `_on_inject_pressed`, ~line 1260) — there is no button, so it's
   undiscoverable. Add a labelled button in the CRISPR param panel (next to the Guide field) that calls
   `_on_inject_pressed`. Renderer-only, hash-neutral. (S)

2. **Brush edits should also surface a variant in the specimen view + extinct = struck-through-but-kept.**
   The user wants: (a) a brush (region) CRISPR edit to ALSO show a "new" variant glyph in the specimen view
   (today region edits are per-individual → no whole-species genome change → no variant by design; decide:
   either log a per-region-edit variant, or make the brush optionally do a whole-species-equivalent log).
   (b) When a species/variant goes EXTINCT (population → 0), keep its specimen glyph but render it
   STRUCK-THROUGH / greyed "for investigation" rather than removing it. Poll `observe_species()` population;
   tag the specimen entry extinct; style it in `_render_specimens`/`_emphasise_focus`. Renderer-only. (M)

3. **"Load Starter" → wire `data/presets/primordial.json` into the SP-2 composer.** The menu
   (`godot/main_menu.gd`) reads the preset JSON (roster + per-species counts + env + containment) and
   pre-fills the roster composer; a "Load Starter" button. Then a multi-species map is one click — which is
   how to actually SEE the per-species SIZE contrast (plant big / E.coli small / Bdellovibrio tiny). Renderer-only. (S/M)

4. **Relations view — there is NO graph, only a 2D heatmap panel.** The user expected a node-link GRAPH of the
   trophic web but sees only the S×S FlowMatrix heatmap (`godot/relations_heatmap.gd` + the Relations view in
   `main.gd` ~2809). Review + add a real node-link graph visualization (species as nodes sized by population,
   edges = measured FlowMatrix flows, arrow + thickness = direction/magnitude) alongside/instead of the
   heatmap. Reads the already-exported FlowMatrix + observe_all (renderer-only, hash-neutral). (M/L)

5. **Map UI improvements — size contrast tuning + per-zoom refinement.** Verify the multi-species SIZE contrast
   is strong enough (single-species shots confirmed COLOR works; the size table is
   `godot/species_visual_map.gd` — tune SIZE_* if plant/rod/predator/symbiont aren't visibly distinct). Then
   per-zoom-scope refinement: Field = species-colored aggregate density; Cells = optionally per-organism
   morphotype glyphs (rods/commas/specks), not just sized dots. (M)

6. **EPIC — emergent-run discovery (the big vision).** Start D0 (interestingness scorer) + D1 (per-gen trace)
   in a new std-only `crates/discovery`, anchored on the Primordial preset. Full plan +
   workflow defs in `docs/llm/proposals/emergent-discovery-harness-draft.md`. See memory
   `autonomous-emergent-run-discovery-ml`. The autonomous search → score → save-replayable-gems loop is the
   emergent-systems showcase + ties to the night-batch playbook. (L, multi-slice)

## Deferred / parked
- **Perf for bigger maps** (memory `perf-bigger-maps-needs-structural-change`): rayon parallelism measured to
  NOT pay (ADR-020); bigger maps need a STRUCTURAL cost-profile change (aggregate/sub-population stepping, LOD,
  a different data layout) — NOT a parallel library. Revisit when the gameplay/UI is solid.
- SP-4 codex done; OVERSIGHT in-game UI; UE5/web renderer; open-system predator/decomposer (§7 item 8); the
  contamination S5b provisioning edge + loaded-session journal_actions markers (roadmap §8/§9).

## Start with #1–#3 (quick wins that unblock testing), then #4 (relations graph) and #5 (map tuning).
