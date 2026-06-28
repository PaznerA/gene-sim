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
| 1 | `[ ]` | **discovery-scenarios-impl** | workflow | Named `SearchSpace` SCENARIO presets (predator-prey / decomposer / contamination-open / spore-resilience / edit-rescue / extreme-climate) biasing species set + count/containment/temp ranges + `edit_budget`, + a `--space <name>` CLI flag + a multi-starter batch — **the "more starters" ask** | ✅ | discovery D2a/D2b + Variant Lab D (done) |
| 2 | `[ ]` | **discovery-continue-from-gem-impl** | workflow | A runner that LOADS a saved gem → seeds a fresh evolutionary search FROM it (branch + keep evolving/editing the discovered community); every continued gem round-trips — **the "continuation after -X gens" ask** | ✅ | gems exist · discovery infra (done) |
| 3 | `[ ]` | **discovery-load-gem-replay-impl** | workflow | Renderer reads a saved `data/runs/gems/*.json` → configures a live run (reset/roster/env/containment) + schedules the gem edits via `apply_edit` → the player WATCHES the discovered scenario; renderer-only, reuses existing `#[func]`s | ✅ | gems exist · Variant Lab D (done) |
| 4 | `[ ]` | **starter-map-library-impl** | workflow | Promote the curated gems (`proposals/starter-candidates.json`) into 5–10 named, committed starter maps: **gen-1** (fresh config) + **gen-N checkpoints** (replayed so the edits are RECORDED in the scrub-back timeline) + a renderer "Starters" gallery (gen-1 via Load Starter, gen-N via `load_session`) | ✅ | #2 continue-from-gem + #3 load-gem-replay |
| 5 | `[ ]` | **oversight-ui-polish** | slice | The ADR-028 #3-verify follow-ups (renderer-only): default the "growth ratio q" knob to `1000` (wild-type) not `0` (lethal KO); align the timeline "due epoch" marker label with the immediate-commit semantics; re-enable oversight in `load_session` | ✅ | OVERSIGHT UI (done) |

**Queue depth (forward READY, non-done): 5** — 4 discovery/auto-research workflows (`scenarios` → `continue-from-gem`
→ `load-gem-replay` → `starter-map-library`, a precisely-sequenced arc) + `oversight-ui-polish`. ≥5 ✅. All ✅
hash-neutral. Grounded in the wave-1+2 research (`proposals/starter-map-research.md` + `starter-candidates.json`).

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

**Sandbox QoL:**
- `[def]` **live-session-sparkline-impl** — `save_session`/`load_session` already exist; add a per-gen effect sparkline on the injection/timeline markers (P4/P6 follow-up). Minor.

**Flagged for human sign-off (do NOT auto-run):**
- 🛑 **R3-F3 resource coupling** — per-cell local Wright-Fisher selection rewrite; blocked on the R1.2/R1.3 spatial-`Cell` design collision (a re-pin + an ADR-005 change). Needs a design workflow + sign-off first.
- 🔁 **Rel-4 sqlite-vec sidecar** — only when the roster size crosses the trigger; designed, executes when warranted.

---

## ▶ LOG (append per item: date · item · PASS/RED · merge sha · note)

- 2026-06-28 — **Research waves 1+2 + starter-map capstone queued.** Ran 60 evolutionary runs (8 640 configs, 572 verified gems) over the default space. Findings (`proposals/starter-map-research.md`): decomposer keystone (Δqual +303k), a sustainability cliff on long horizons (boom-bust 16%→38%; sustainable core = plant+ecoli), predator regulates not oscillates, edits +quality, M3/M5 discriminate (validates dramaweights). Curated 11 starter candidates → `proposals/starter-candidates.json`. Authored `starter-map-library-impl` (gen-1 + gen-N-checkpoint maps with recorded-intervention timelines) → queued #4 (dep on #2 continue-from-gem + #3 load-gem-replay). `beta-contributing-md` → pipeline.
- 2026-06-28 — **Re-plan #2 @ `main` b865644 → discovery/auto-research lead.** First brute-force batch validated the pipeline (21 verified gems, ~60s/run, edit axis produced the #1 gem, 19/21 distinct shapes, M1 saturates). Authored 3 discovery-research workflows (`discovery-scenarios-impl`, `discovery-continue-from-gem-impl`, `discovery-load-gem-replay-impl`) → READY; active queue rebuilt (5 READY: 3 research + `oversight-ui-polish` + `beta-contributing-md`). `discovery-dramaweights-impl` flagged next-to-promote (M1-saturation-validated). The 5 completed gameplay items are in the entries below.
- 2026-06-28 — **#5 `sandbox-load-starter-impl` ALREADY SHIPPED** (no new merge). The feature landed earlier in `597a8d4` (`main_menu.gd:295-365`). Workflow VERIFIED the as-committed impl: gate GREEN; verify 4/4 at 3/3; `data/presets` res:// staged + byte-gated; `0x47a0` unmoved.
- 2026-06-28 — **#4 `codex-browse-panel-impl` PASS** (gate GREEN, `CODEX MIRROR/INSPECT OK`; verify 4/4 at 3/3; ZERO Rust — `0x47a0` byte-identical; reuses `codex.gd`). Merged `1ba13b8`.
- 2026-06-28 — **#3 `oversight-ingame-ui-impl` PASS** (gate GREEN; verify 5/5 at 3/3; `0x47a0` unmoved on no-commit, a committed edit moves it deliberately + replays byte-equal). **ADR-028** appended. Merged `b4e368f`. UX follow-ups tracked as `oversight-ui-polish`.
- 2026-06-28 — **#2 `variant-lab-autoresearch-edits` PASS** (Variant Lab D; gate GREEN; verify 5/5 at 3/3; `0x47a0` UNMOVED — `edit_budget` default-0 + disjoint `EDIT_SALT`; edited gems round-trip). **ADR-027**. Merged `7fb3150`.
- 2026-06-28 — **#1 `variant-lab-save-reseed` PASS** (gate GREEN; verify 5/5 at 3/3; `0x47a0` UNMOVED — read-only export + renderer save/reseed). Merged `5f43c28`.
- 2026-06-27 — QUEUE seeded (gameplay/sandbox lead). `beta-license-dual` done (`8415199`).
