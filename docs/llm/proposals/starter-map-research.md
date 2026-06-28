# Starter-map research — discovery sweeps (wave 1+2) + the curated candidate shortlist

> 2026-06-28. The brute-force auto-research (D2a/D2b + Variant Lab D edit axis) was run as two sweeps over the
> default 7-species `SearchSpace`, then the gem corpus was analyzed for emergent behavior and curated into a
> shortlist of starter-map candidates. The selected configs are in **`starter-candidates.json`** (this dir) — the
> durable, reproducible handoff the `starter-map-library-impl` workflow consumes (each carries its `master_seed`,
> roster, scheduled edits, and `recorded_hash` under `BUILD_ID = ecology-d0@47a03c8f6701f240`).

## Method
- **Wave 1** — 30 evolutionary runs (seeds 500–529), gens 250–400, `edit_budget ∈ {0,2,3}`, keep 16 → ~4 320 configs.
- **Wave 2** — 30 evolutionary runs (seeds 700–729), gens **300–600** (longer horizons), `edit_budget ∈ {0,2,3}`,
  keep 20 → corpus **8 640 evaluated configs · 572 round-trip-verified gems** (accumulated).
- Every run `--save-evals` (the full `(config → ScoreVec)` corpus, not just kept gems). All hash-neutral —
  `0x47a0_3c8f_6701_f240` unmoved throughout (the search is meta-level; sim runs are pure functions of configs).

## Key emergent findings
1. **Decomposer (E. coli) is the universal keystone.** Presence shifts drama-quality by **+303k** (others +30–60k)
   and trophic flow **M4 by +8869** (≈ doubles it); it is in every top community. Empirically confirms the
   ADR-013 F4 decomposer-loop as the engine of ecosystem complexity.
2. **Longer horizons reveal a sustainability cliff.** From wave-1 (250 gen) → wave-2 (300–600 gen): **boom-bust
   16% → 38%**, **coexistence 14.6% → 9.7%**, M1-coexistence saturation **79.5% → 48.7%**. Most "coexistence" at
   gen-250 is actually **boom-bust reservoir draw-down** when run longer.
3. **Sustainability needs the autotroph + the decomposer together.** Autotroph-free communities look great
   short-term but collapse long-term; over long horizons the plant's survival contribution rises (ΔM6 +1933). The
   sustainable core is the obligate **plant (solar) → detritus → E. coli (recycle) → nutrient → plant** loop.
4. **Predator paradox.** Bdellovibrio does NOT add dynamism (ΔM3 **−425**, the most negative of any species) — it
   adds coexistence (+684 M1) + trophic flow (+897 M4). In this conserved-joule model the predator *regulates*
   rather than drives oscillation, counter to naïve Lotka-Volterra. (Marginal-mean comparison — worth a controlled
   follow-up.)
5. **Mid-run edits (Variant Lab D) raise mean quality** (+19k aggregate; +45k on short horizons). The edit axis
   genuinely helps surface better scenarios.
6. **Environment.** Warm (>0.65) → best survival + coexistence; Open containment (immigration) modestly raises
   events + richness + long-horizon survival (re-seeding) — the open-system recovery.
7. **Metrics.** M1/M6 saturate (poor discriminators); **M3 (dynamism) / M5 (events) / M4 (trophic)** discriminate —
   strongly validates the drama-weighted target (`discovery-dramaweights-impl`, M3+M5 dominant).

## Curated candidate shortlist (11 → see `starter-candidates.json`)
Coverage: 4 limit-cycle · 3 coexistence · 2 boom-bust · 1 eventful · 1 drift · 7 with edits (→ gen-N checkpoints)
· 4 fresh gen-1 · 4 sustainability-tested (≥500 gen) · 4 autotroph-free · 7 predator.

**Recommended final 8** (a balanced playable set — adjust as desired):

| slug | kind | dynamics | roster | note |
|------|------|----------|--------|------|
| `minimal-symbiosis` | gen-545 checkpoint | limit-cycle | default+ecoli (dyad) | the canonical SUSTAINABLE pair; 1 late edit in the timeline |
| `full-chain` | gen-1 | limit-cycle | default+ecoli+bdellovibrio | sustainable plant→decomposer→predator loop |
| `tended-garden` | gen-266 checkpoint | limit-cycle | default+ecoli+pseudomonas | sustainable + 2 recorded edits |
| `predators-reef` | gen-44 checkpoint | coexistence | ecoli+bacillus+pseudomonas+bdellovibrio | dramatic, autotroph-free (transient), 1 edit |
| `the-crash` | gen-1 | boom-bust | default+ecoli+pseudomonas+staph+aspergillus | 5-species boom → crash |
| `upheaval` | gen-1 | eventful | default+ecoli+bacillus+bdellovibrio | the rare eventful type |
| `wild-cycle` | gen-1 | limit-cycle | ecoli+pseudomonas+aspergillus+bdellovibrio | autotroph-free dramatic cycle |
| `open-drift` | gen-78 checkpoint | drift | default+ecoli+bacillus+aspergillus+bdellovibrio | slow open-system drift, 1 edit |

## How the build consumes this
`starter-map-library-impl` (queued) promotes each selected candidate from `starter-candidates.json`:
- **gen-1** → a fresh-config starter JSON (roster + env + containment + metadata) under `data/presets/starters/`.
- **gen-N checkpoint** → the gem replayed to gen N via `record_episode` so the scheduled edits are RECORDED in the
  session journal (`actions.ndjson`) — the scrub-back timeline with the interventions already in it.
Each committed starter carries its `source_hash` and must replay to it (reproducible). The renderer "Starters"
gallery loads gen-1 via the Load Starter path and gen-N via `load_session` (timeline markers + scrubbable).

## Design feedback into the scenario presets (`discovery-scenarios-impl`)
- `decomposer-coexistence` → pin `ecoli` (proven keystone).
- `sustainable` starters → pin `default` + `ecoli` together (the only durable core); reward long-horizon survival.
- `predator-prey` → reframe as "predator-regulated coexistence" (the data shows the predator stabilizes, not
  oscillates) or keep as an open hypothesis to probe.
