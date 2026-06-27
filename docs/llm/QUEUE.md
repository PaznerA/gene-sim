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
> **Lead thrust (chosen 2026-06-27): Gameplay / sandbox-first** (`[[gameplay-sandbox-first]]`). Discovery/ML
> chain + beta-hardening remainder are queued below as the next pipeline.

---

## ▶ ACTIVE QUEUE (gameplay/sandbox)

| # | Status | Item | Driver | Goal | Hash | Deps |
|---|--------|------|--------|------|------|------|
| 1 | `[x]` | **beta-license-dual** | direct | `LICENSE-MIT` + `LICENSE-APACHE` at root matching the declared `MIT OR Apache-2.0` SPDX (beta-distribution blocker) | ✅ | — |
| 2 | `[ ]` | **variant-lab-save-reseed** | workflow | Slice B: read-only `export_species_json` of a species' post-edit genome+niche; Slice C: specimen-view "💾 Save variant" + a "Saved variants" reseed section reusing the contaminant/inoculate path | ✅ | — |
| 3 | `[ ]` | **oversight-ingame-ui-impl** | workflow | In-game OVERSIGHT panel: render the earned-credit ledger, let the player request → preview (FBA KO result) → commit an E. coli edit that ripples via the F4 loop; renderer drives the existing `RequestEcoliEdit`/`CommitEcoliImpact` journal | ✅ | — |
| 4 | `[def]` | **codex-browse-panel-impl** | workflow | Browsable CODEX panel (SP-4 §2.3 follow-up): a scrollable species/gene/role/flow browser over `data/codex/codex.json`, read-only loader (`godot/codex.gd` is staged into res://) | ✅ | — |
| 5 | `[def]` | **sandbox-load-starter-impl** | workflow | Wire "Load Starter" into the SP-2 composer: the menu reads `data/presets/primordial.json` → pre-fills roster + env + containment (the sandbox onboarding ramp) | ✅ | SP-2 (done) |
| 6 | `[def]` | **live-session-save-load-impl** | workflow | Mid-run `save_session`/`load_session` via the action journal (the `run_stats()` clone-fold) + a per-gen effect sparkline on the injection markers (P4/P6 follow-ups) | ✅ | P4/P6 (done) |

**Queue depth (forward, non-done): 5** (1 READY+1 authored READY + 3 DEFINED). ≥5 ✅.

---

## ▶ NEXT PIPELINE (defined; promote when the active queue drains)

**Discovery / ML chain** (precisely-sequenced B→C→D; `surrogate-model-spec.md`; all ✅ hash-neutral, `crates/discovery`):
- `[def]` **discovery-dramaweights-impl** — D3-B.2: the drama-weighted target `D` (M3+M5 dominant) + reweighted scorer.
- `[def]` **discovery-ridgeint-impl** — D3-B.3: integer ridge regressor (fixed-point GD, no f64, row-order-independent, `build_id` anchor). *dep: dramaweights.*
- `[def]` **discovery-steered-loop-impl** — D3-B.4: wire RidgeInt into D2b (oversample→predict→select, explore floor), retrain per gen. *dep: ridgeint.*
- `[def]` **discovery-batch-showcase** — D4: night-cron batch + gem-index sidecar + curated showcase gallery. *dep: steered-loop; ADR on steering target.*

**Beta-hardening remainder** (`glmTakeover/` audit; mostly ✅ infra/docs):
- `[def]` **beta-contributing-md** (`slice`) — `CONTRIBUTING.md`: branch workflow + `tools/gate.sh` + ADR process + commit format.
- `[def]` **slim-hermeticity-impl** — `env_clear()` + `LC_ALL=C` on the SLiM subprocess (oracle golden-file robustness, inv #1-adjacent).
- `[def]` **replay-error-handling-impl** — `seed.json`/`actions.ndjson` corruption → `ReplayError` enum (not panic) + a corrupted-input proptest.
- `[def]` **unsafe-policy-adr** (`direct`) — ADR documenting the `forbid(unsafe_code)` rule + the one `godot-sim` `unsafe impl` exception.
- `[def]` **docs-housekeeping** (`direct`) — delete the stale (untracked) `docs/llm/weakspots.md` (hallucinates a non-existent Python project); add `ADR-INDEX.md`.

**Flagged for human sign-off (do NOT auto-run):**
- 🛑 **R3-F3 resource coupling** — per-cell local Wright-Fisher selection rewrite; blocked on the R1.2/R1.3 spatial-`Cell` design collision (a re-pin + an ADR-005 change). Needs a design workflow + sign-off first.
- 🔁 **Rel-4 sqlite-vec sidecar** — only when the roster size crosses the trigger; designed, executes when warranted.

---

## ▶ LOG (append per item: date · item · PASS/RED · merge sha · note)

- 2026-06-27 — QUEUE seeded (gameplay/sandbox lead). `beta-license-dual` done in the same commit. `variant-lab-save-reseed.js` brought into git (was untracked) + `oversight-ingame-ui-impl.js` authored → both READY. 4 DEFINED + the discovery/beta pipeline behind them.
