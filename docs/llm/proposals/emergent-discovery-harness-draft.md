# Proposal — Autonomous emergent-run discovery harness (search → score → save the gems)

> **Status:** DRAFT / roadmap epic. The big-picture direction the user asked to record + integrate.
> **One line:** the deterministic headless core + the seed.json/journal replay make it ideal to run
> thousands of headless sims, SCORE each for "interestingness" (emergent events), and SAVE the gems
> (seed + init config + edit journal → bit-identically replayable) as curated showcases of emergent
> systems — autonomously, eventually guided by a learned model rather than pure brute force.

## 1. WHY this is the right fit

- **Determinism (inv #3) is the enabler.** A run is a pure function of (master seed + init config + the
  journaled action/edit sequence). A saved `(seed, EnvConfig, journal)` replays byte-identically (the R2
  persistence fix already round-trips roster/species/consortium/containment + the action journal). So a
  "gem" found by the search is a permanent, shareable, replayable artifact — no flakiness.
- **Headless-first (inv #4).** `run_headless` + the harness env step the sim with no renderer. Thousands of
  search trials are embarrassingly parallel ACROSS PROCESSES (each trial is its own seed/config — unlike the
  *within-tick* parallelism that ADR-020 measured doesn't pay, cross-trial parallelism scales perfectly).
- **The gameplay payoff** ([[gameplay-sandbox-first]]): the saved gems are the emergent-systems showcase —
  the sandbox's reason to exist. The search finds dramatic runs (booms, crashes, coexistence limit-cycles,
  trophic cascades, contamination recoveries) that manual testing would rarely stumble on.
- **Open-system honesty** ([[no-hardcoded-balance-open-system]]): we DISCOVER interesting open-system
  dynamics; we do NOT tune for forced stability. "Interesting" rewards drama + structure, not a flat line.

## 2. THE LOOP

```
  propose config  →  headless run (record journal)  →  score interestingness  →  keep if top-K
       ↑                                                                              │
       └──────────────────  search policy (random → evolutionary → learned)  ────────┘
```

A **config** = { roster (species keys + start counts), env (seed/climate/containment+consortium), an
optional SCHEDULED edit/intervention sequence (tick → Action) }. A **trial** runs it headless for N gens,
recording the journal + a per-gen metrics trace. A **score** reads the trace. The **search** proposes the
next configs. **Gems** (top-K by score, deduped) are saved as replayable `(seed, EnvConfig, journal)` +
their metric fingerprint + a one-line auto-caption.

## 3. PHASES / SLICES

### D0 — the INTERESTINGNESS SCORER (RNG-free, reads existing exports) — START HERE
A pure scorer over a run's per-generation trace (population per species, the FlowMatrix, allele/fitness,
extinction/boom/immigration events — all already exportable). It is the load-bearing piece; everything
else is plumbing + search. Candidate signals (combine into one score, weights tunable):
- **Coexistence / diversity** — how many species persist + an evenness (Shannon/Simpson) over the run;
  reward sustained multi-species coexistence over a monoculture or total collapse.
- **Dynamism** — variance / oscillation / limit-cycle strength in the population trajectories (a living,
  moving system beats a flat line — but a system that instantly dies scores low too).
- **Emergent EVENTS** — count + magnitude of booms (>k×), crashes (→ near-0), takeovers (rank flips),
  trophic cascades (a predator crash → prey boom → producer crash), contamination recoveries (an immigrant
  establishes + reshapes the web).
- **Trophic structure** — a non-trivial FlowMatrix (real measured flows across ≥2 trophic levels), not an
  all-zero / single-edge matrix.
- **NOVELTY** — distance of this run's metric fingerprint from the already-saved gems (novelty search keeps
  the gem set diverse, not 100 near-identical booms).
All integer/quantized + deterministic so a gem's score is reproducible. Lives in a new `crates/discovery`
(std-only analysis crate, reads the harness trace) — keeps it off the sim hot path.

### D1 — the TRACE export
A lightweight per-generation metrics trace from the harness (population[species], FlowMatrix snapshot, the
event journal) → a compact ndjson/bin the scorer reads. Render-only/analysis (off the hash path).

### D2 — the SEARCH harness (gradient-free first)
A driver that proposes configs, runs them headless (record_episode), scores, and keeps top-K + novel.
Start simple + strong: **random search** over a config space, then **evolutionary** (mutate/crossover the
best configs' roster counts + edit schedules), then **Bayesian / surrogate-guided**. Cross-trial parallel
(N processes). Saves gems to `data/runs/gems/<score>-<seed>.json` (the EnvConfig + journal + fingerprint +
caption). Resumable; budget-bounded.

### D3 — the SURROGATE ML MODEL ("brute force gradient")
Train a model (config-features → predicted interestingness) on the accumulated (config, score) pairs to
GUIDE the search toward promising regions — the "ML model" the user wants. Classic + simple first
(gradient-boosted trees / a small MLP over the config feature vector); it just biases the proposal
distribution. Optional later: a learned config EMBEDDING for novelty.

### D4 — the autonomous BATCH + the SHOWCASE
Wire D2/D3 into the autonomous-roadmap playbook: overnight discovery batches that grow the gem library;
each gem is a one-click "load + watch" in the sandbox (the SP-2 composer loads the EnvConfig; the journal
replays the edits) — the emergent-systems gallery. A small curated set ships as showcase presets.

## 4. WORKFLOW DEFINITIONS (to author when we start)

- **emergent-scorer-design** — design panel (3 lenses: ecology-meaning / determinism+reproducibility /
  signal-vs-noise) → judge → pin the interestingness metric set + weights → the D0 scorer spec + an ADR.
- **discovery-harness-impl** — implement D1 trace + D0 scorer (`crates/discovery`) + D2 random/evolutionary
  search → gate → verify (a known dramatic seed scores high, a flat/dead seed scores low; gems replay
  bit-identically).
- **discovery-batch** (autonomous) — run a budgeted search, score, dedupe-by-novelty, save the top-K gems +
  auto-captions; append a gallery index. Fits the night-batch cron model.
- **surrogate-model-impl** (D3, later) — train + wire the config→interestingness surrogate to bias D2.

## 5. INVARIANTS

- **#3 determinism:** the scorer + search are RNG-free / off the sim hash path (they READ traces); a gem is
  a `(seed, EnvConfig, journal)` that replays byte-identically (the R2 round-trip already guarantees this).
- **#1 boundary / #5 pluggable:** `crates/discovery` is a std-only analysis crate (like oracle-fba /
  relations-index); the scorer is a trait so the metric set is swappable. A future GPL ML lib (if any) stays
  at the process boundary; the default surrogate is a small non-GPL model.
- **#4 headless-first:** the whole loop is headless; the renderer only LOADS the saved gems.
- **#6 species/operator granularity:** the search acts at the config/operator level (rosters, scheduled
  interventions), never per-organism.

## 6. FIRST STEP

D0 (the scorer) + D1 (the trace) — once we can SCORE a run's interestingness reproducibly, the rest is
search plumbing. Pair it with the **starter preset** (`data/presets/`) as the search's seed/anchor config.
