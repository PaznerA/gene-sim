# Handoff — gene-sim: HANDOFF backlog DONE + discovery foundation landed (2026-06-24)

> Self-contained continuation prompt. The 2026-06-23 presentation/gameplay backlog (items 1–5) AND the
> emergent-discovery epic's first phase (D0 scorer + D1 trace) are all MERGED to `main`. This points at what's next.

## Where we are — `main @ e7102ba` (clean, all gate-GREEN)
Pinned determinism literal **`0x47a0_3c8f_6701_f240`** — held byte-identical through everything (every slice
hash-neutral). Read `CLAUDE.md` + `docs/llm/SPEC.md` (7 invariants) + `docs/llm/DECISIONS.md` (ADRs incl. **ADR-022**
relations graph, **ADR-023** discovery scorer) + `docs/llm/autonomous-roadmap.md` at the start.

**Working discipline (proven, per memory `no-ci-wait-autonomous-roadmap`):** per slice → branch `auto/<name>-YYYY-MM-DD`
→ implement (often via a Workflow: design→implement→gate→verify) → `bash tools/gate.sh` GREEN (determinism MUST stay
`0x47a0`) → adversarially verify (a 3-skeptic verify workflow) → merge `--no-ff -F <msgfile>` → `git push origin main`.
**Do NOT wait on GitHub CI.** Renderer changes are hash-neutral (godot/*.gd + the off-hash snapshot). Verify godot UI
with `godot --path godot -- --live [--roster "stem:count,…"] [--steps N] [--view relations|specimen] [--zoom 1..12]
--shot /tmp/x.png` then Read the png; build the cdylib first (`cargo build --manifest-path crates/godot-sim/Cargo.toml`)
+ stage `cp data/{species,codex,presets}/*.json godot/data/{species,codex,presets}/`. **macOS godot-gate note:**
`check_godot_snapshot.sh` now uses `timeout`+file capture (a `$(godot …)` pipe-capture HANGS on macOS).

## Landed this session (all merged, hash-neutral, verified 3/3)
1. **Inject button** — explicit `💉 Inject (whole species)` in the CRISPR sub-panel (was Enter-only).
2. **Brush→variant + extinct-struck** — a brush CRISPR stroke surfaces a `region edit` variant for the dominant
   species at the cell; extinct species render struck-through-but-KEPT (`_ever_alive`/`_extinct`, un-struck on regermination).
3. **Load Starter** — `📂 Load Starter — "Primordial Soil"` in the menu → prefills roster+env+containment from
   `data/presets/primordial.json` (staged into the res:// mirror + gate).
4. **Relations node-link GRAPH** (ADR-022) — `godot/relations_graph.gd` (nodes sized by pop, colored by morphotype,
   edges = measured FlowMatrix flows), `🕸 Graph / ▦ Matrix` toggle (Graph default) + the `--roster`/`--steps` shot flags.
5. **Per-cell morphotype glyphs** — `organisms.gd` `_draw_morph` (cocci/vibrioid/pleomorph/symbiont/mold) at the Cells
   scope; Field stays sized colored dots (completes ADR-021 follow-up).
6. **Discovery D0 scorer + D1 trace** (ADR-023) — `crates/discovery` (std+serde, 6 integer metrics M1..M6 + gated
   combine + novelty + `InterestingnessScorer` trait) + `harness/src/capture.rs` `capture_trace` (off-hash). Spec:
   `docs/llm/proposals/discovery-scorer-spec.md`. Oracle: live limit-cycle A=784500 STRICTLY beats frozen coexistence
   F=355000. Weights `[14,14,22,18,18]` favour drama over forced stability (tunable via `ScoreParams`).

## Next — the discovery epic continues (priority order)
1. **D2 — the SEARCH harness** (the "autonomously find the gems" loop). A driver that PROPOSES configs (start: random
   over the Primordial roster counts + env seed/containment, then evolutionary mutate/crossover of the best), RUNS each
   headless via `harness::capture_trace`, SCORES via `discovery::DefaultScorer`, and SAVES the top-K + novel gems to
   `data/runs/gems/<score>-<seed>.json` (the `EnvConfig` + journal + fingerprint + an auto-caption) — each gem saved
   only after a `record_episode → replay == hash` round-trip (the reproducibility contract). Cross-trial parallel (N
   processes). Resumable, budget-bounded. Lives in `crates/discovery` (search module) + a harness/CLI entry. See
   `docs/llm/proposals/emergent-discovery-harness-draft.md` §D2 + the `discovery-batch` workflow def.
2. **D3 — the surrogate model** ("brute-force gradient"): train config-features → predicted-interestingness on the
   accumulated (config, score) pairs to bias D2's proposals. Classic GBT/MLP first.
3. **D4 — autonomous batch + showcase**: wire D2/D3 into the night-batch playbook; each gem = a one-click "load+watch"
   in the sandbox (the SP-2 composer loads the EnvConfig, the journal replays the edits) → the emergent-systems gallery.

## Deferred / parked
- **PERF chapter (hash-neutral micro-opt)**: PERF-1 (scratch-Vec hoist) merged; **PERF-2** (per-tick OrgId
  BTreeMap/BTreeSet → reused sorted-Vec — profiling insight: items/rows are already sorted by (cell, species,
  OrgId)) is **DONE, hash-neutral, −48% tick_loop** (ADR-026) — clean full conversion **rebased onto + composed
  with PERF-1** on the `worktree-perf2-roadmap-workflow` branch (supersedes the earlier half-broken
  `auto/perf2-btreemap-to-vec` WIP). Full `tools/gate.sh` GREEN incl. `--features determinism` (`0x47a0` byte-
  identical after the compose); back-to-back criterion `--baseline` confirms the −48% is PERF-2's MARGINAL gain
  over PERF-1 (PERF-1 was itself perf-neutral on this bench). **READY TO MERGE** to main (`git merge --no-ff
  worktree-perf2-roadmap-workflow` from main, local gate green = the merge gate per `no-ci-wait-autonomous-roadmap`).
  Optional cheap follow-up: pin a golden hash on a predator/symbiont roster (the plant-only pinned config doesn't
  exercise predation/host_coupling). See roadmap §10.
- **Perf for bigger maps** (memory `perf-bigger-maps-needs-structural-change`): the BIGGER structural cost-profile
  change (aggregate stepping / LOD / new data layout), NOT a parallel library (ADR-020), and beyond PERF-2's
  byte-identical micro-opt. Revisit when gameplay/UI is solid.
- OVERSIGHT in-game UI; UE5/web renderer; open-system predator/decomposer (§7 item 8); contamination S5b + loaded-
  session journal_actions markers (roadmap §8/§9); relations graph guild-colour + force-directed layout (ADR-022 follow-up).

## Recommendation
Continue with **D2** — the scorer (D0) + trace (D1) now exist, so the search loop is the next compounding step toward
the autonomous gem-discovery showcase. Random search first (simple + strong), then evolutionary.
