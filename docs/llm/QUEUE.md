# QUEUE — the workflow zásobník for looped development

> The stack `/roadmap-iterate` pops from and `/roadmap-plan` refills. One queue item = one multi-agent
> **Workflow** (`.claude/workflows/*.js`) = one merge to `main`. Keep **≥5** forward items defined at all times.
> Guardrails: `autonomous-roadmap.md §0` + SPEC §2.1. The pinned determinism literal is
> `0x47a0_3c8f_6701_f240` — hash-neutral items must leave it byte-identical; a 🔁 re-pin moves it deliberately.
>
> **Status:** `[ ]` READY (tracked `.js` exists, or driver `direct`/`slice`) — runnable now ·
> `[def]` DEFINED (robust spec below, `.js` not yet authored — `/roadmap-plan` converts it to READY) ·
> `[~]` in progress · `[x]` done · `RED` failed gate/verify (left for human) · 🛑 needs human sign-off.
> **Driver:** `workflow` = run the named `.js` · `slice` = one implementer+gate+reviewer pass · `direct` = trivial inline edit.
>
> **Lead thrust (chosen 2026-06-28): Discovery / auto-research.** The first brute-force batch validated the whole
> pipeline (21 verified gems in ~60s/run; the Variant Lab D edit axis produced the #1 gem; 19/21 distinct community
> shapes; M1 saturates → validates the drama-weighted target). Next: make the search SCENARIO-targeted over multiple
> starters, branch from discovered gems, and let the player WATCH a gem replay. **Frontier: `main` @ `b865644`.**

---

## ▶ ACTIVE QUEUE (discovery / auto-research)

| # | Status | Item | Driver | Goal | Hash | Deps |
|---|--------|------|--------|------|------|------|
| 1 | `[x]` | **discovery-scenarios-impl** | workflow | Named `SearchSpace` SCENARIO presets (predator-prey / decomposer / contamination-open / spore-resilience / edit-rescue / extreme-climate) biasing species set + count/containment/temp ranges + `edit_budget`, + a `--space <name>` CLI flag + a multi-starter batch — **the "more starters" ask** | ✅ | discovery D2a/D2b + Variant Lab D (done) |
| 2 | `[x]` | **discovery-continue-from-gem-impl** | workflow | A runner that LOADS a saved gem → seeds a fresh evolutionary search FROM it (branch + keep evolving/editing the discovered community); every continued gem round-trips — **the "continuation after -X gens" ask** | ✅ | gems exist · discovery infra (done) |
| 3 | `[x]` | **discovery-load-gem-replay-impl** | workflow | Renderer reads a saved gem → configures a live run + replays the gem edits → the player WATCHES the scenario. (v1 RED on edit-replay divergence; **v2 PASS** — resolution moved to a read-only core `gem_edit_schedule` reusing `edits_to_actions` + off-hash `Gem.gens_requested`. ADR-030.) | ✅ | gems exist · Variant Lab D (done) |
| 4 | `[x]` | **starter-map-library-impl** | workflow | Promote the curated gems into named committed starter maps: **gen-1** (fresh config) + **gen-N checkpoints** (edits recorded in the scrub-back timeline) + an RCT-style selector. 7 shipped (6 gen-1 + 1 checkpoint). ADR-031. | ✅ | #2 continue-from-gem + #3 load-gem-replay |
| 5 | `[x]` | **scenario-gif-preview-impl** | workflow | Auto-GIF of a scenario's KEY EVENTS (off-hash D1 trace: booms/crashes/takeovers + edit gens) → macOS-safe frame capture → an animated GIF via the linked MIT `gif` crate (ADR-032, GPL-clean + pinned) for the RCT selector. **Scenarios arc COMPLETE.** | ✅ | #4 starter-map-library |

**Queue depth (forward READY, non-done): 5** — the **scenarios arc** (`scenarios` → `continue-from-gem` →
`load-gem-replay` → `starter-map-library` [RCT-style selector] → `scenario-gif-preview`). ≥5 ✅. All ✅ hash-neutral.
Grounded in the wave-1+2 research (`proposals/starter-map-research.md` + `starter-candidates.json`). **Right after this
arc → the VISUAL-POLISH epic below** (the user: the screen is "spammed"/cluttered — declutter it).

---

## ▶ VISUAL-POLISH EPIC — leads right after the scenarios arc (the screen is cluttered)

> User brief (2026-06-28): the play screen is "zaspamovaná" (per-organism dot spam) + unreadable. Develop **COLONIES**
> (map polygons that layer better than individual organisms + unify a species/variant; a CRISPR brush edit creates a NEW
> colony — Cities-Skylines DISTRICTS); each zoom scope "pops" a selected colony open to individual organisms by organism
> size; **plants** always-visible + most-realistic, in ≥1 colony. Colonies are an OFF-HASH render aggregation (a per-cell
> variant/colony channel on the snapshot, like `dominant_species_id`) → inv #2/#3, `0x47a0` untouched. Also the LOD lever
> for bigger maps (`[[perf-bigger-maps-needs-structural-change]]`).

- `[x]` **visual-declutter-colony-design** (`workflow`, DESIGN) — DONE → `proposals/visual-declutter-colony-draft.md`
  (470 lines: ADR-029 draft + the airtight off-hash argument — `hash_world` omits `Species`, so a heritable `Variant`
  tag is hash-neutral the same way — + the 6-slice plan). The colony impl slices below come from §7 of the draft.
- `[x]` **S1 `colony-snapshot-channel-impl`** 🛑→**DONE (2026-06-29, ADR-029)** — the off-hash heritable `Variant(u16)` tag +
  `NextVariantId` + `dominant_variant_id` GSS6 channel + brush mint/stamp in `apply_edit_region` + the `snapshot.gd`/byte-gate
  bump (channels 13→14). **Gate GREEN; `0x47a0_3c8f_6701_f240` BYTE-IDENTICAL (NOT a re-pin); 3-skeptic verify 3/3 on all
  five invariant booleans; 187/187 sim-core tests.** Merged `--no-ff`. ADR-029 + CHANGELOG written.
- `[x]` **S2 `colony-polygon-render-impl`** ✅ — **DONE (2026-06-29)** — `godot/colonies.gd`: deterministic row-major union-find CC → marching-squares contour → DP+Chaikin → fill/outline/label; `main.gd` scope-layer swap (Field=polygons, hides dot-spam). **Gate GREEN; zero Rust (literal byte-identical); verify 3/3.** Merged `--no-ff`. *(deferred to S4: hole-cut nested districts; brushed `--shot`.)*
- `[x]` **S3 `lod-pop-impl`** ✅ — **DONE (2026-06-29)** — per-colony footprint (`cell×zoom×size_scale`) pop ladder (closed-form, no per-frame redraw); plants pop first by `size_scale`; un-popped microbe cells emit zero sprites (de-spam holds). **Gate GREEN; zero Rust (literal byte-identical); verify 3/3.** Merged `--no-ff`.
- `[x]` **S4 `brush-colony-binding-impl`** ✅ — **DONE (2026-06-29)** — hole-cut nested family district (`_trace_boundaries`+`_draw_holed_fill`, ±0.09 hue shift) tracking its members via the heritable variant key; renderer-side colony registry; click→`set_selected_colony` select-pop capped (viewport + 700-sprite budget); new headless `colony_s4_test.gd` in the gate. **Gate GREEN; zero Rust (literal byte-identical); verify 3/3.** Merged `--no-ff`. Closes the S2/S3 deferred cosmetics.
- `[ ]` **S5 `plant-realism-impl`** ✅ — plant canopy hulls, always-visible floor, ≥1-colony guarantee. *dep: S2 ✓ — READY NEXT.*
- `[def]` **S6 `colony-polish-impl`** ✅ *(optional)* — viewport-cull/sprite-budget, district inspect panel, big-map draw-count check. *dep: S3.*

---

## ▶ NEXT PIPELINE (defined; promote when the active queue drains)

**Discovery / ML chain** (precisely-sequenced; `surrogate-model-spec.md`; all ✅ hash-neutral, `crates/discovery`).
**D3-A (eval log) + D3-B.1 (feature encoder) DONE** (`3ad7b9e` / `370d888`). The first batch's **M1 saturation**
empirically validates the drama-weighted target → `discovery-dramaweights-impl` is the **next to promote**:
- `[def]` **discovery-dramaweights-impl** — D3-B.2: the drama-weighted target `D` (M3+M5 dominant) + reweighted scorer.
- `[def]` **discovery-ridgeint-impl** — D3-B.3: integer ridge regressor (fixed-point GD, no f64, row-order-independent, `build_id` anchor). *dep: dramaweights.*
- `[def]` **discovery-steered-loop-impl** — D3-B.4: wire RidgeInt into D2b (oversample→predict→select, explore floor), retrain per gen. *dep: ridgeint.* Composes with the Variant Lab D edit axis + the named scenario spaces.
- `[def]` **discovery-batch-showcase** — D4: night-cron batch (over the named scenario spaces) + a gem-index sidecar + a curated, committed showcase gallery (the replayable gems the player browses). *dep: steered-loop + scenarios; ADR on the steering target.*

**Beta-hardening remainder** (`glmTakeover/` audit folded in; ✅ infra/docs):
- `[def]` **beta-contributing-md** (`slice`) — `CONTRIBUTING.md`: branch workflow + `tools/gate.sh` + ADR process + commit/trailer format.
- `[def]` **slim-hermeticity-impl** — `env_clear()` + `LC_ALL=C` on the SLiM subprocess (oracle golden-file robustness, inv #1-adjacent).
- `[def]` **replay-error-handling-impl** — `seed.json`/`actions.ndjson` corruption → `ReplayError` enum (not panic) + a corrupted-input proptest.
- `[def]` **unsafe-policy-adr** (`direct`) — ADR documenting the `forbid(unsafe_code)` rule + the one `godot-sim` `unsafe impl` exception.
- `[def]` **docs-housekeeping** (`direct`) — delete the stale untracked `docs/llm/weakspots.md` (hallucinates a non-existent Python project) + triage `docs/llm/glmTakeover/`; add `ADR-INDEX.md`.

**Polish & QoL:**
- `[def]` **starter-promote-hardening** (`slice`) — the ADR-031 follow-up: `promote_gen1` must reject firing-edit gems (or recompute the gen-1 `source_hash` from an edit-free replay) + store `gens` (+ an edit flag) in the gen-1 doc so the library is self-contained re-verifiable. Guards against silent breakage when CRISPR edits become hash-active.
- `[def]` **oversight-ui-polish** (`slice`) — the ADR-028 #3-verify follow-ups (renderer-only): default the "growth ratio q" knob to `1000` (wild-type) not `0` (lethal KO); align the timeline "due epoch" marker label with the immediate-commit semantics; re-enable oversight in `load_session`.
- `[def]` **live-session-sparkline-impl** — `save_session`/`load_session` already exist; add a per-gen effect sparkline on the injection/timeline markers (P4/P6 follow-up). Minor.

**Sign-off granted 2026-06-28 ("zelená všem blockerům") — but gated by readiness, not approval:**
- 🛑 **R3-F3 resource coupling** — SIGNED OFF, **but still UNDESIGNED** (blocked on the R1.2/R1.3 spatial-`Cell` design
  collision; a re-pin + an ADR-005 change). An undesigned invariant rewrite is NOT auto-run even with sign-off — it needs
  a **design workflow first** (`r3-f3-spatial-cell-design`, to author), then the executed re-pin. Lower priority than the
  scenarios/colony epics; queue the design when those drain.
- 🔁 **Rel-4 sqlite-vec sidecar** — SIGNED OFF; designed; executes when the roster size crosses the trigger (conditional —
  not warranted now).

---

## ▶ LOG (append per item: date · item · PASS/RED · merge sha · note)

- 2026-06-29 — **#5 `scenario-gif-preview-impl` PASS → SCENARIOS ARC COMPLETE (5/5).** Gate GREEN; 3-skeptic verify CONFIRMED 4/4 at 3/3; `0x47a0` UNMOVED (sim-core untouched; the off-hash `ecology.rs` `detect_events` refactor is byte-identical, scorer 73/73). New `keyframe.rs` (off-hash key-event detector) + `gifenc.rs` (PNG→GIF via the LINKED MIT `gif` 0.13.3 + `png` 0.17.16 + color_quant — GPL-clean per `check_license.sh`, pinned, **ADR-032**) + macOS-safe `make_starter_gif.sh` (timeout+file, no pipe); `*.gif` gitignored. (Fixed a workflow-parse bug — a raw backtick in a prompt — before launch.) Merged `--no-ff`. **Next: S1 `colony-snapshot-channel-impl` 🛑 (signed off) — the visual-polish epic.**
- 2026-06-29 — **#4 `starter-map-library-impl` PASS** (gate GREEN, 10/10 incl. the new GALLERY gate; 3-skeptic verify CONFIRMED 5/5 at 3/3; `0x47a0` UNMOVED; committed library empirically replay-verified). **7 starters shipped** to `data/presets/starters/` (6 gen-1 across the dynamics taxonomy + 1 `branch-point` gen-N checkpoint with a recorded edit) + the RCT selector (`gallery.gd`). **ADR-031**. One non-blocking latent trap (gen-1 drops edits but copies the edited `source_hash` — safe today, hash-neutral edits) tracked as `starter-promote-hardening`. Merged `--no-ff`. Next ready: #5 `scenario-gif-preview-impl`.
- 2026-06-28 — **#3-v2 `discovery-load-gem-replay-impl` PASS** (the RED re-run, fixed). Gate GREEN; 3-skeptic verify CONFIRMED 4/4 at 3/3; edit replay now byte-faithful to `edits_to_actions` (resolution in a read-only core `gem_edit_schedule` #[func] + off-hash `Gem.gens_requested`); `0x47a0` UNMOVED. **ADR-030** appended. Merged `--no-ff`. Next ready: #4 `starter-map-library-impl`.
- 2026-06-28 — **(parallel b) `visual-declutter-colony-design` DONE** (ran concurrently with #3 v2). Delivered `proposals/visual-declutter-colony-draft.md` (ADR-029 draft + 6-slice plan). Headline: colonies are an off-hash heritable `Variant(u16)` tag + a `dominant_variant_id` GSS6 channel (sibling of `dominant_species_id`); the inv #3 case is airtight (`hash_world` omits `Species`, so `Variant` is hash-neutral too; single-plant config → all `Variant(0)` → `0x47a0` byte-identical, NOT a re-pin); brush = a 2-line `ApplyEditRegion` extension (Cities-Skylines districts, survives replay); renderer derives the polygon geometry (inv #2). **S1 `colony-snapshot-channel-impl` flagged 🛑 STOP-THE-LINE** (the only core/snapshot touch — needs human sign-off). Merged `--no-ff`.
- 2026-06-28 — **#3 `discovery-load-gem-replay-impl` RED → v2 fix authored.** Gate GREEN but verify refuted `replays_gem_config_and_edits` 0/3 (config replay sound; EDIT replay diverged from `edits_to_actions`: (1) raw target vs `loci[edit.target % loci.len()].id` → 81/147 edits failed `UnknownTargetLocus`; (2) `gem.gens` vs the unserialized `gens_requested` → wrong gen on early-stopped gems). The gate missed it (the `--gem` smoke reported *dispatched*, not *applied*) — the adversarial verify caught it. WIP preserved on `auto/discovery-load-gem-replay-2026-06-28` (`6e48a35`, NOT merged). **v2 authored** = renderer + a read-only core `gem_edit_schedule` #[func] (resolves via `edits_to_actions`) + off-hash `Gem.gens_requested`; hash-neutral. STOPPED the run (verify-refute) — awaiting human go to re-run v2 (a renderer→renderer+tiny-core re-scope).
- 2026-06-28 — **#2 `discovery-continue-from-gem-impl` PASS** (gate GREEN; verify CONFIRMED, 4/4 at 3/3; `0x47a0` UNMOVED — meta-level; `discover_from_gem` pre-seeds from the gem + branches; children round-trip, stale anchors dropped at write). Merged `--no-ff`. Next ready: #3 `discovery-load-gem-replay-impl`.
- 2026-06-28 — **#1 `discovery-scenarios-impl` PASS** (gate GREEN; 3-skeptic verify CONFIRMED, 4/4 at 3/3; pinned literal `0x47a0_3c8f_6701_f240` UNMOVED — default `--space` path golden-byte-identical; 6 named presets fixed-order/in-bounds/distinct; unknown name degrades with a note). Merged `--no-ff` to `main`. Next ready: #2 `discovery-continue-from-gem-impl`.
- 2026-06-28 — **User brief folded in (scenarios + GIF + RCT selector + visual-polish/colony epic).** Refined `starter-map-library` gallery → RCT-style scenario selector (left list / big right desc + animation + thick scrub slider). Authored `scenario-gif-preview-impl` (auto-GIF of key events; off-hash + macOS-safe + GPL-clean) → active #5. Authored `visual-declutter-colony-design` (DESIGN: colonies as off-hash render aggregation, brush-creates-colony à la Cities-Skylines districts, LOD pop by zoom×size, plants always-visible/realistic; ADR-029 draft) → leads the new VISUAL-POLISH epic right after the scenarios arc. `oversight-ui-polish` → Polish pipeline.
- 2026-06-28 — **Research waves 1+2 + starter-map capstone queued.** Ran 60 evolutionary runs (8 640 configs, 572 verified gems) over the default space. Findings (`proposals/starter-map-research.md`): decomposer keystone (Δqual +303k), a sustainability cliff on long horizons (boom-bust 16%→38%; sustainable core = plant+ecoli), predator regulates not oscillates, edits +quality, M3/M5 discriminate (validates dramaweights). Curated 11 starter candidates → `proposals/starter-candidates.json`. Authored `starter-map-library-impl` (gen-1 + gen-N-checkpoint maps with recorded-intervention timelines) → queued #4 (dep on #2 continue-from-gem + #3 load-gem-replay). `beta-contributing-md` → pipeline.
- 2026-06-28 — **Re-plan #2 @ `main` b865644 → discovery/auto-research lead.** First brute-force batch validated the pipeline (21 verified gems, ~60s/run, edit axis produced the #1 gem, 19/21 distinct shapes, M1 saturates). Authored 3 discovery-research workflows (`discovery-scenarios-impl`, `discovery-continue-from-gem-impl`, `discovery-load-gem-replay-impl`) → READY; active queue rebuilt (5 READY: 3 research + `oversight-ui-polish` + `beta-contributing-md`). `discovery-dramaweights-impl` flagged next-to-promote (M1-saturation-validated). The 5 completed gameplay items are in the entries below.
- 2026-06-28 — **#5 `sandbox-load-starter-impl` ALREADY SHIPPED** (no new merge). The feature landed earlier in `597a8d4` (`main_menu.gd:295-365`). Workflow VERIFIED the as-committed impl: gate GREEN; verify 4/4 at 3/3; `data/presets` res:// staged + byte-gated; `0x47a0` unmoved.
- 2026-06-28 — **#4 `codex-browse-panel-impl` PASS** (gate GREEN, `CODEX MIRROR/INSPECT OK`; verify 4/4 at 3/3; ZERO Rust — `0x47a0` byte-identical; reuses `codex.gd`). Merged `1ba13b8`.
- 2026-06-28 — **#3 `oversight-ingame-ui-impl` PASS** (gate GREEN; verify 5/5 at 3/3; `0x47a0` unmoved on no-commit, a committed edit moves it deliberately + replays byte-equal). **ADR-028** appended. Merged `b4e368f`. UX follow-ups tracked as `oversight-ui-polish`.
- 2026-06-28 — **#2 `variant-lab-autoresearch-edits` PASS** (Variant Lab D; gate GREEN; verify 5/5 at 3/3; `0x47a0` UNMOVED — `edit_budget` default-0 + disjoint `EDIT_SALT`; edited gems round-trip). **ADR-027**. Merged `7fb3150`.
- 2026-06-28 — **#1 `variant-lab-save-reseed` PASS** (gate GREEN; verify 5/5 at 3/3; `0x47a0` UNMOVED — read-only export + renderer save/reseed). Merged `5f43c28`.
- 2026-06-27 — QUEUE seeded (gameplay/sandbox lead). `beta-license-dual` done (`8415199`).
