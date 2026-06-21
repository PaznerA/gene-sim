# Let-loose session — 2026-06-21 (branch `let-loose/campaign-grader`)

> Scheduled autonomous "let loose" build session (renewed token budget). Spirit: unleash imagination, build a
> vertical slice of a game direction ON the engine, adversarially reviewed, gate-green, opened as a **draft PR
> (not merged)**.

## What I imagined

A design workflow (`let-loose-direction-design`, 7 agents) scoped all four directions — single-player CAMPAIGN,
CO-OP, SpacetimeDB MMO, score-vs-AI — on the CURRENT engine and judged them. **Campaign won** because it is the
only direction that, besides being the most buildable-now + most deterministic, *reduces* an existing tension
rather than just preserving the boundary: it pulls the mission's zone-read + win/score out of GDScript and into
a headless Rust grader. The twist that makes it more than a campaign: because every scenario is a
`(seed, climate, action-journal)` tuple replayed through the proven `run_episode` path, **each cleared mission is
a bit-exact, re-gradable replay artifact** — so the campaign doubles as a regression corpus AND the seam for a
future *score-vs-AI / leaderboard* mode (an AI operator emits an action-journal exactly like a human and is graded
by the same `evaluate()`).

## What I built (all gate-green, on the current Wright-Fisher engine)

- **`sim_core::Simulation::region_allele(region, grid_w, grid_h) -> RegionReadout{mean, populated_cells}`** — the
  CORE re-implementation of `godot/main.gd::_eval_mission`'s zone read (mean-of-populated-cell-means over a disc).
  Reuses `snapshot()` so it is the same f32 per-cell `allele_freq` the renderer sees; RNG-free → **hash-neutral**
  (pinned determinism literal unchanged). `Region::contains` made `pub`.
- **`harness::campaign`** — `Scenario` / `Objective{Suppress|Establish}` / `Campaign` (serde, JSON manifest),
  `evaluate(scenario, actions) -> ScenarioResult` and `evaluate_campaign(campaign, journals_dir)`. `evaluate`
  is **faithful to the live mission**: it checks the objective every generation, **latches the win on the first
  met frame within the deadline** (like `_mission_status`), scores from the latch, and **refuses over-budget
  edits** (like `_can_spend_edit`). Pure function of `(scenario, actions)` (inv #3).
- **`harness --campaign <manifest> --journals <dir>`** — grades a campaign headlessly, printing per-scenario
  Won/Lost/n.a. + score + the total.
- **`data/campaign/intro.json`** — a 2-scenario starter campaign ("Drought Belt" Suppress in an arid climate,
  "Cold Snap" Establish in a cold climate).
- **Tests**: 1 in sim-core (region_allele determinism + empty-region), 8 in harness (latch-from-first-frame,
  unmeetable-loss, deadline boundary via the gen-0 region allele, over-budget-refused, determinism, campaign
  round-trip + NotAttempted, shipped-manifest-loads). Plus the full `tools/gate.sh` 10/10.

## The 3 most interesting things

1. **Replay-as-grading**: the determinism contract turns "did the player win?" into a pure replay — the same
   action-journal regrades bit-identically, so missions become a self-checking regression corpus.
2. **Faithful win-latching in Rust**: `evaluate` steps one generation at a time and latches the first met frame,
   so an authored/AI solution that wins early and overshoots still wins — matching the live mission exactly,
   adversarially verified.
3. **The AI-vs-score seam falls out for free**: an AI operator that emits an `actions.ndjson` is graded by the
   identical `evaluate()` — the score-vs-AI mode is now a thin layer away.

## Adversarial review (3 reviewers) + how I addressed it

- **[HIGH] inv-#2 honesty** — the slice does NOT yet retire the GDScript path (`_eval_mission` is unchanged; no
  GDScript calls the new core read). → **Reworded all framing to honest tense**: this adds the core `region_allele`
  SEAM + a headless grader; it does not yet reduce the live violation. The follow-up is spelled out below.
- **[MEDIUM ×2] batch-vs-continuous grading** — the first draft graded only the final state, mis-grading an
  overshooting journal + testing the deadline against the final gen. → **Rewrote `evaluate` to latch the first met
  frame within the deadline** (now genuinely faithful) + **skip over-budget edits**.
- **[MEDIUM] radius-0 fidelity** — `Region::contains` clamps `radius 0→1`, diverging from `_eval_mission`'s raw
  radius. → **Narrowed the docstring** to "bit-for-bit for `radius ≥ MIN_REGION_RADIUS`".
- **[LOW] campaign conflates not-attempted/lost** — → added **`Status::NotAttempted`**. **[LOW]** removed the bogus
  `# Errors` doc on the infallible `evaluate_campaign`; documented the `<dir>/<index>/` journals convention in USAGE.

## What is stubbed / the clearly-scoped follow-up (to actually reduce the inv-#2 violation)

1. Expose `region_allele` (and ideally the win/score predicate) on the `LiveSim` gdext node (`crates/godot-sim`).
2. Rewrite `godot/main.gd::_eval_mission` to call it instead of looping over the snapshot, with a replay-mode
   fallback. Only then is the GDScript biology computation actually retired (assertable by the *absence* of
   `allele_freq`/score arithmetic in `main.gd`).
3. Author real "player solution" journals for `intro.json` (today the manifest is the challenge corpus; no
   bundled solutions). A finer `NotAttempted` vs `Corrupt` split for unreadable journals.
4. Re-ground onto the CHEMOSTAT-J engine once F3+ lands (objectives over joule/biomass, not just allele_freq).
