# Autonomous roadmap & playbook

> Operational brain for the unattended overnight runs. A cron fires a prompt that says
> "read this file and execute BATCH N autonomously." Follow this playbook exactly.
> **No human is available during a batch. Do NOT ask questions. Obey the STOP rules below.**

Authored 2026-06-21 21:00 CEST. North-star vision (the reason for all of it):
a **fast abstract plant/animal 30 FPS sim** + a **deep real E. coli earned-edit mode**
(E. coli is the soil microbe that closes the nutrient cycle) + a **3rd species** +
**vector-DB relations**. Fill core gaps first, then expand species logic, then make it feel alive.

---

## 0. Autonomy guardrails (NON-NEGOTIABLE)

1. **Never merge a determinism re-pin autonomously.** The pinned literal
   `0xf795_eac4_112f_acd5` (`crates/sim-core/src/lib.rs::determinism_hash_is_pinned`) must NOT change
   during a batch. Workflows tagged 🔁 are **design + hash-neutral-infra/data ONLY** — they STOP before
   the births/deaths or trophic-coupling merge and leave a signoff-ready package for the human.
2. **Respect all 7 invariants** (CLAUDE.md §2.1). If a slice would touch one beyond what its workflow
   already scopes, STOP that workflow, log it, continue with the next independent one.
3. **Gate is law.** Every workflow ends by running `tools/gate.sh`. **Green → commit. Red → do NOT
   commit; log the failure; move on.** Never commit on red. Never weaken a gate to make it pass.
4. **Branch, don't push.** First action of a batch: ensure we are on the auto branch
   `auto/night-2026-06-21` (create from `main` if missing). Commit each green workflow there
   (conventional commit, one workflow = one commit, with the Co-Authored-By + Claude-Session trailers
   from CLAUDE.md/Bash rules). **Never push, never touch `main`.** The human reviews the branch in the morning.
5. **No user input.** Do not call AskUserQuestion or EnterPlanMode. If genuinely blocked, log a
   `BLOCKED:` line in the progress log (§4) and proceed to the next item.

---

## 1. Execution protocol (how to run a batch)

For each queued workflow, in order:

1. `Workflow({ name: "<workflow-name>" })` — it runs in the background; a `<task-notification>` arrives on completion.
2. On completion, read its returned summary:
   - If it reports the **gate green** and (for hash-neutral workflows) the adversarial verify **confirmed**
     → `git add -A && git commit` on the auto branch with a conventional message naming the slice.
   - If it reports **gate red** or the verify **refuted hash-neutrality** → do NOT commit; append a
     `RED:`/`REJECTED:` entry to the progress log; leave the tree dirty for human review; continue.
   - If it is a 🔁 **design-only** workflow → it commits its docs/proposals + hash-neutral infra (gate
     green); confirm it ended with its `STOP-THE-LINE:` line and append a `SIGNOFF-READY:` entry.
3. Launch the next workflow. Independent workflows may be chained; do not parallelize two workflows that
   write the same files (the queue order below is already conflict-safe).
4. When the batch queue is empty → write the batch summary to the progress log (§4) and **schedule the
   next batch** if this playbook defines one and it isn't already scheduled.

Commit message convention: `feat(<area>): <slice> — <hash-neutral|design-only>, gate green` + trailers.

---

## 2. BATCH 1 — fires 2026-06-21 22:00 CEST (`cron 2 22 21 6 *`, one-shot, durable)

Goal: ship a **visibly-alive multi-species sim** + land the **Strategy core substrate** + produce
**two signoff-ready re-pin packages** (F3, F4). Ordered, conflict-safe queue:

| # | Workflow | Goal | Hash | On done |
|---|----------|------|------|---------|
| 1 | `ecoli-visibility-impl` | res:// species-loading fix + first genuine microbe specimen view + per-species observe | ✅ neutral | commit if green+confirmed |
| 2 | `f2-strategy-substrate-impl` | genome→`Strategy{budget[5] simplex, TrophicRole, affinity}` cached UNWIRED | ✅ neutral | commit if green + ≥2/3 skeptics say hash-neutral |
| 3 | `f3-metabolism-keystone-design` 🔁 | F3 metabolism+lifecycle DESIGN + multi-ISA CI gate + `ledger_closes()` harness + ADR draft | infra ✅ / merge ✋ | commit docs+infra if green; STOP before births/deaths |
| 4 | `f4-trophic-decomposer-design` 🔁 | F4 trophic loop + FlowMatrix DESIGN + decomposer species spec + ADR draft | data ✅ / merge ✋ | commit docs+data if green; STOP before coupling |
| 5 | `ui-multispecies-liveliness` | all species rendered, trait-driven sprites across zoom scopes + enriched specimen view | ✅ neutral | commit if green+confirmed |

Rationale for order: core/boundary (1,2) → signoff-ready keystone designs (3,4) → liveliness polish (5).
3 and 4 are safe to run unattended **because they never merge the re-pin** — they produce
`docs/llm/proposals/f3-metabolism-keystone-draft.md` and `f4-trophic-decomposer-draft.md` + hash-neutral
infra/data and STOP. The FlowMatrix contract that batch 1 pins in the F4 draft is what batch 2's relations
view consumes.

---

## 3. BATCH 2 — fires 2026-06-22 04:00 CEST (`cron 3 4 22 6 *`, one-shot, durable)

Precondition check (first thing): read the §4 progress log. If batch 1 did NOT complete (queue unfinished
or the session was down), **resume batch 1 from the first unfinished item instead**, then proceed here.

Goal: the **relations UI** + continue expanding species logic (design-only, signoff-ready).

| # | Workflow | Goal | Hash | On done |
|---|----------|------|------|---------|
| 1 | `ui-panels-and-relations-view` | expanded per-species panels + new Relations heatmap view reading the F4 FlowMatrix contract (degrades gracefully) | ✅ neutral | commit if green+confirmed |
| 2 | _plan-the-next-package_ | After 1 lands: assess the morning state, then DESIGN the next species-logic unit and queue it as **BATCH 3** (see §5). Do this by authoring a short design-only workflow on the spot (pattern: design panel → judge → ADR draft + slice plan, no re-pin) and scheduling a `cron` for a sensible next slot (e.g. the following evening). | design ✅ | write BATCH 3 into this file + CronCreate |

Candidate next units for BATCH 3 (pick by the morning state, most foundational first):
- **ADR-017 S4/S5 — earned-edit OVERSIGHT game loop + multi-fidelity firewall (design)**: RNG-free
  score→credit accrual; `Action::RequestEcoliEdit`/`CommitEcoliImpact` with a `due_epoch` buffer so the
  async deep-compute never leaks wall-clock into the hash. The player-agency payoff of the vision.
- **ADR-013 F5 — chemical/signal diffusion field (design)**: toxin/kin/alarm, double-buffered, Σ-conserved;
  enables allelopathy/chemotaxis. GSS2→GSS3 snapshot bump.
- **ADR-017 S8 — relations vector-DB sidecar (design)**: sqlite-vec at the process boundary (inv #1),
  view-only ANN overlay on top of the on-hash FlowMatrix. The "vector-DB relations" leg of the vision.

---

## 4. Progress log (append-only; the cron reads this to resume)

<!-- Each batch appends: timestamp, per-workflow PASS/RED/SIGNOFF-READY/BLOCKED, commit sha, notes. -->

- 2026-06-21 21:00 CEST — playbook authored; batch 1 + batch 2 crons set. Queue not yet started.
- 2026-06-21 22:02 CEST — **BATCH 1 START.** Branch `auto/night-2026-06-21` created from main; setup committed `53fb3d2`. Queue: [1 ecoli-visibility-impl, 2 f2-strategy-substrate-impl, 3 f3-metabolism-keystone-design🔁, 4 f4-trophic-decomposer-design🔁, 5 ui-multispecies-liveliness].
  - [x] 1 ecoli-visibility-impl — **PASS** (gate GREEN 9/9 + bench-skip; determinism literal unchanged → hash-neutral confirmed independently). Added: harness `build_species_from_str` res:// boundary, godot-sim `set_species_json` #[func], `godot/microbe.gd` microbe specimen view, run.sh species mirror + `godot/data/` gitignored, species-mirror gate. Fixes the cwd species-not-found bug. Commit `d7391cf`.
  - [x] 2 f2-strategy-substrate-impl — **PASS** (gate GREEN; determinism literal unchanged → hash-neutral; verified UNWIRED: `selection()` fitness uses base_growth only, `.strategy` only in `species_strategy()` accessor + reset-time re-express). Added gp.rs `Strategy{budget[5],role,affinity}` + `TrophicRole` + `express_strategy` (first caller of fixed.rs apportion) + `role_for`; cached in `SpeciesEntry.strategy`. Commit `42dea23`.
  - [x] 3 f3-metabolism-keystone-design🔁 — **SIGNOFF-READY** (gate GREEN; literal unchanged; sim path INTACT — metabolism/selection/fitness untouched, NO births/deaths merged). Delivered hash-neutral: `docs/llm/proposals/f3-metabolism-keystone-draft.md` (ADR-013 F3 design: PoolStock i64, uptake→convert→excrete, energy-funded reproduce_or_die replacing constant-N, Biomass+Age, carcass→detritus, MaxPopulation guard), ledger.rs `closes()` assertion harness + tests, `tools/check_determinism_multi_isa.sh` + gate.sh wire (SKIP local / CI matrix is the gate) + ci.yml x86_64+aarch64 job. **⏸ HUMAN RE-PIN SIGN-OFF REQUIRED before implementing births/deaths.** Commit `eb18034`.
  - [x] 4 f4-trophic-decomposer-design🔁 — **SIGNOFF-READY** (pure design; literal unchanged; schedule chain `(advance_tick,metabolism,selection)` INTACT; NO trophic.rs/relations.rs/coupling merged — only the draft + log changed, so gate is identical to GREEN @ eb18034, not re-run for a 2-markdown-file change). Delivered: `docs/llm/proposals/f4-trophic-decomposer-draft.md` — obligate plant→detritus→**E.coli(re-roled Decomposer via niche.trophic_role)**→free_nutrient loop (deletes F3's free_nutrient INFLUX tap so nutrient is endogenous), emergent MEASURED FlowMatrix S×S (inverts fabricated-cosine ADR-014), mineralize_rate gene-anchored on pta/GO-8959, CRISPRi ripple levers (ptsG/gltA/pta). **⏸ HUMAN RE-PIN SIGN-OFF REQUIRED before F4 coupling.**

---

## 5. Morning hand-off (what the human signs off)

When the human returns:
1. Review the `auto/night-2026-06-21` branch commits (hash-neutral impls + design docs).
2. Read `docs/llm/proposals/f3-metabolism-keystone-draft.md` + `f4-trophic-decomposer-draft.md` and the
   multi-ISA CI gate; **sign off the F3 then F4 re-pins** (these are the only blocked merges).
3. Smoke-test the Godot build (`./run.sh`) for the liveliness + relations UI.
4. Anything tagged `RED:`/`BLOCKED:` in §4 is the human's first fix target.
