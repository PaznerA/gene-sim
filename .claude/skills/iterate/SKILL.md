---
name: iterate
description: Drive development slices end-to-end (implement → gate → review → reflect → commit). Default is AUTONOMOUS — keep running slices until a gate goes red, a slice touches an invariant, the backlog empties, or the human interrupts. Pass --once for a single slice.
argument-hint: "[--once] [--bench]"
---
Run the robust development loop defined in **docs/llm/LOOP.md** (read it first), honoring SPEC §2.1 invariants.

## Mode
- **Default: AUTONOMOUS.** Run slices back-to-back. STOP and surface to the human on any of:
  - `tools/gate.sh` goes **RED**, or
  - the next slice is marked **🛑** (touches an invariant §2.1) or is estimated > ~1 day, or
  - **no unstarted slice** remains in docs/llm/TASKS.md, or
  - the human interrupts.
- `--once`: run exactly one slice, then stop. `--bench`: run the perf gate too (`GATE_BENCH=1`).

## Per slice (multi-agent, context-isolated — SPEC §7.3)
For the top unstarted slice in docs/llm/TASKS.md:
1. **implementer** — code + tests, fewest crates, smallest surface. Honor invariants.
2. **gatekeeper** — run `tools/gate.sh`; any red ⇒ **STOP THE LINE** (fix or revert; never commit on red).
3. **reviewer** — check the diff vs §2.1 invariants + licensing; **SEND BACK** on any violation.
4. **close** — ADR to DECISIONS.md if load-bearing; update CHANGELOG; conventional commit (one slice = one
   commit); mark the slice done in TASKS.md.
(Spawn the **planner** only when given a NEW goal to decompose into slices; the existing backlog is already planned.)

## Hard rules (SPEC §2.1)
GPL stays at the subprocess boundary (never linked); no genome logic in `godot/`; seeded ChaCha8 RNG only
(no global/thread RNG, no HashMap iteration in sim state); AI agents at species granularity; pin versions.

State lives in TASKS.md + git, so the loop is **resumable**: stop anytime and continue later (`/iterate`
again, or just say "continue / jeď dál"). End each autonomous run with a per-slice summary of what landed
and the next slice (and why it stopped).
