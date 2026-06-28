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
> **Lead thrust (chosen 2026-06-27): Gameplay / sandbox-first** (`[[gameplay-sandbox-first]]`). The Variant Lab
> epic (edit→save→reseed for player AND auto-research) leads; the discovery/ML chain + beta-hardening remainder
> are the next pipeline. **Frontier: `main` @ `8415199`** — re-planned 2026-06-28.

---

## ▶ ACTIVE QUEUE (gameplay/sandbox + Variant Lab)

| # | Status | Item | Driver | Goal | Hash | Deps |
|---|--------|------|--------|------|------|------|
| 1 | `[x]` | **variant-lab-save-reseed** | workflow | Variant Lab B+C: read-only `export_species_json` of a species' post-edit genome+niche; specimen-view "💾 Save variant" + a "Saved variants" reseed section reusing the contaminant/inoculate path | ✅ | A (done) |
| 2 | `[x]` | **variant-lab-autoresearch-edits** | workflow | Variant Lab D: give the brute-force auto-research the CRISPR-edit action — a scheduled-edits axis on `SearchConfig` (serde-default, default-OFF via `edit_budget`) threaded as the EXISTING `Action::ApplyEdit` into `capture_trace` + the verify journal; edited gems round-trip | ✅ | A (done) + discovery D2a/D2b/D3-A (done) |
| 3 | `[x]` | **oversight-ingame-ui-impl** | workflow | In-game OVERSIGHT panel: render the earned-credit ledger, request → preview (FBA KO) → commit an E. coli edit rippling via the F4 loop; drives the existing `RequestEcoliEdit`/`CommitEcoliImpact` journal | ✅ | — |
| 4 | `[x]` | **codex-browse-panel-impl** | workflow | Browsable CODEX panel (SP-4 §2.3 follow-up): a scrollable species/gene/role/flow browser over `res://data/codex/codex.json`, reusing the staged+gated `godot/codex.gd` loader (the SP-4 res:// blocker is RESOLVED) | ✅ | codex staging (done) |
| 5 | `[ ]` | **sandbox-load-starter-impl** | workflow | Wire "Load Starter" into the SP-2 composer: read `res://data/presets/primordial.json` → pre-fill roster + env + containment (the onboarding ramp); ensure `data/presets` is res:// staged + byte-gated | ✅ | SP-2 (done) |

**Queue depth (forward READY, non-done): 5** — 2 pre-authored + 3 newly-authored this pass (`variant-lab-autoresearch-edits`, `codex-browse-panel-impl`, `sandbox-load-starter-impl`). ≥5 ✅. All ✅ hash-neutral.

---

## ▶ NEXT PIPELINE (defined; promote when the active queue drains)

**Discovery / ML chain** (precisely-sequenced; `surrogate-model-spec.md`; all ✅ hash-neutral, `crates/discovery`).
**D3-A (eval log) + D3-B.1 (feature encoder) DONE** (`3ad7b9e` / `370d888`). Remaining:
- `[def]` **discovery-dramaweights-impl** — D3-B.2: the drama-weighted target `D` (M3+M5 dominant) + reweighted scorer.
- `[def]` **discovery-ridgeint-impl** — D3-B.3: integer ridge regressor (fixed-point GD, no f64, row-order-independent, `build_id` anchor). *dep: dramaweights.*
- `[def]` **discovery-steered-loop-impl** — D3-B.4: wire RidgeInt into D2b (oversample→predict→select, explore floor), retrain per gen. *dep: ridgeint.* **Composes with item #2** — the steered loop can also steer the new mid-run-EDIT axis once both land.
- `[def]` **discovery-batch-showcase** — D4: night-cron batch + gem-index sidecar + curated showcase gallery. *dep: steered-loop; ADR on the steering target (drama-weighted vs raw Q).*

**Beta-hardening remainder** (`glmTakeover/` audit folded in; mostly ✅ infra/docs — NOT blocking the Variant Lab,
largely orthogonal; only a future spatial-index re-pin touches an invariant and is flagged below):
- `[def]` **beta-contributing-md** (`slice`) — `CONTRIBUTING.md`: branch workflow + `tools/gate.sh` + ADR process + commit format.
- `[def]` **slim-hermeticity-impl** — `env_clear()` + `LC_ALL=C` on the SLiM subprocess (oracle golden-file robustness, inv #1-adjacent).
- `[def]` **replay-error-handling-impl** — `seed.json`/`actions.ndjson` corruption → `ReplayError` enum (not panic) + a corrupted-input proptest.
- `[def]` **unsafe-policy-adr** (`direct`) — ADR documenting the `forbid(unsafe_code)` rule + the one `godot-sim` `unsafe impl` exception.
- `[def]` **docs-housekeeping** (`direct`) — delete the stale untracked `docs/llm/weakspots.md` (hallucinates a non-existent Python project) + triage `docs/llm/glmTakeover/` (keep as an audit snapshot or archive); add `ADR-INDEX.md`.

**Sandbox QoL (re-scoped — partly already shipped):**
- `[def]` **oversight-ui-polish** (`slice`) — the ADR-028 follow-ups flagged by the #3 verify (all renderer-only,
  hash-neutral): default the "growth ratio q" knob to `1000` (wild-type/no-op) instead of `0` (growth-lethal KO);
  align the timeline "due epoch" marker label with the renderer's immediate-commit semantics; re-enable oversight in
  `load_session` so the credit ledger resumes after a loaded session.
- `[def]` **live-session-sparkline-impl** — `save_session`/`load_session` ALREADY EXIST (`main.gd:2503/2511` + `journal_actions`); the remaining piece is a per-gen effect sparkline on the injection/timeline markers (P4/P6 follow-up). Minor — deprioritized below the discovery chain.

**Flagged for human sign-off (do NOT auto-run):**
- 🛑 **R3-F3 resource coupling** — per-cell local Wright-Fisher selection rewrite; blocked on the R1.2/R1.3 spatial-`Cell` design collision (a re-pin + an ADR-005 change). Needs a design workflow + sign-off first.
- 🔁 **Rel-4 sqlite-vec sidecar** — only when the roster size crosses the trigger; designed, executes when warranted.

---

## ▶ LOG (append per item: date · item · PASS/RED · merge sha · note)

- 2026-06-27 — QUEUE seeded (gameplay/sandbox lead). `beta-license-dual` done in the same commit. `variant-lab-save-reseed.js` + `oversight-ingame-ui-impl.js` authored → READY. 4 DEFINED + the discovery/beta pipeline behind them.
- 2026-06-28 — **#4 `codex-browse-panel-impl` PASS** (gate GREEN, `CODEX MIRROR/INSPECT OK`; 3-skeptic verify CONFIRMED, 4/4 claims at 3/3; ZERO Rust — pinned literal `0x47a0_3c8f_6701_f240` byte-identical; reuses `codex.gd`, res:// byte-gate intact; no ADR needed). Merged `--no-ff` to `main`. Next ready: #5 `sandbox-load-starter-impl` (the last active-queue item).
- 2026-06-28 — **#3 `oversight-ingame-ui-impl` PASS** (gate GREEN; 3-skeptic verify CONFIRMED, 5/5 claims at 3/3; pinned literal `0x47a0_3c8f_6701_f240` UNMOVED on no-commit, a committed edit moves it deliberately + replays byte-equal; no economy/biology in GDScript; no wall-clock leak). **ADR-028** appended (the renderer immediate-commit path + the honest divergence from the headless `due_epoch` firewall). Merged `--no-ff` to `main`. NON-BLOCKING UX follow-ups tracked as `oversight-ui-polish`. Next ready: #4 `codex-browse-panel-impl`.
- 2026-06-28 — **#2 `variant-lab-autoresearch-edits` PASS** (Variant Lab D; gate GREEN; 3-skeptic verify CONFIRMED, 5/5 claims at 3/3; pinned literal `0x47a0_3c8f_6701_f240` UNMOVED — `edit_budget` default-0 + disjoint `EDIT_SALT` stream; edited gems round-trip). **ADR-027** appended. Merged `--no-ff` to `main`. Next ready: #3 `oversight-ingame-ui-impl`.
- 2026-06-28 — **#1 `variant-lab-save-reseed` PASS** (gate GREEN; 3-skeptic verify CONFIRMED, 5/5 claims at 3/3; pinned literal `0x47a0_3c8f_6701_f240` UNMOVED — read-only `export_species_spec` + renderer-only save/reseed UI). Merged `--no-ff` to `main`. Next ready: #2 `variant-lab-autoresearch-edits`.
- 2026-06-28 — **Re-plan @ `main` 8415199.** Reconciled the real frontier: D3-A (`3ad7b9e`) + D3-B.1 (`370d888`) + PERF-1/2 (`ed558d7`/`81ef729`) + dual-LICENSE (`8415199`) all LANDED since the seed. Confirmed `beta-license-dual` `[x]` done; codex res:// staging + `godot/codex.gd` loader landed (SP-4 blocker RESOLVED); `save_session`/`load_session` already exist. **Authored 3 new READY workflows:** `variant-lab-autoresearch-edits` (Slice D — the user's explicit "auto-research must get the edit action" gap), `codex-browse-panel-impl`, `sandbox-load-starter-impl`. Re-scoped `live-session-save-load` → `live-session-sparkline-impl` (save/load done; only the sparkline remains) + deprioritized. glmTakeover reconciled: NOT blocking — its valid items are the beta-hardening pipeline above. Queue depth 5 READY (gameplay/sandbox) ✅.
