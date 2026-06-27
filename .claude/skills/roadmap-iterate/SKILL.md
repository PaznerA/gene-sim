---
name: roadmap-iterate
description: Loop through the prepared workflow queue (docs/llm/QUEUE.md). Pop the next ready item, run its Workflow (or slice/direct), gate + adversarially verify, merge to main, mark done, continue. AUTONOMOUS until the queue empties, a gate goes red, a 🛑/undesigned-re-pin item is next, or the human interrupts. --once for a single item; --review to stop before merge.
argument-hint: "[--once] [--review] [workflow-name]"
---
Drive the **workflow queue** (`docs/llm/QUEUE.md`) end-to-end: run → gate → verify → merge → advance. This is the
consumer half of the loop; `/roadmap-plan` is the producer. Coarser-grained than `/iterate` (one queue item = one
multi-agent Workflow = one merge, vs `/iterate`'s one TASKS.md slice = one commit). Honors SPEC §2.1 + the
`autonomous-roadmap.md §0` guardrails. **Resumable** — all state is in `QUEUE.md` + git.

## Mode
- **Default: AUTONOMOUS.** Run ready items back-to-back. **STOP and surface to the human** on the FIRST of:
  - the next item is **🛑** (touches an invariant §2.1) or an **undesigned re-pin** — needs sign-off;
  - a Workflow returns **gate RED** or the adversarial verify **refuted** a claim (`verdict: NEEDS WORK`);
  - **no ready item** remains (all `[x]`/blocked-on-deps) — if queue depth `< 3`, recommend `/roadmap-plan`;
  - the human interrupts.
- `--once`: run exactly one item, then stop. `--review`: run + gate + verify, but **STOP before merge** (leave the
  branch for human review). `[workflow-name]`: run that specific queued item next.

## Per item (top `[ ]` whose deps are all `[x]`)
1. **GUARD.** If 🛑 or an undesigned re-pin → **STOP, ask the human.** Do not run it.
2. **RUN** by `driver`:
   - `workflow` → `Workflow({ name })`; it runs in the background, a `<task-notification>` arrives on completion.
     Its agents edit the working tree in place and it returns `{ verdict, tallies, gate, issues }` — **it does not
     commit** (that's this skill's job).
   - `slice` → run one `/iterate`-style pass (implementer → gatekeeper → reviewer) on the working tree.
   - `direct` → make the trivial edit inline, then gate.
3. **JUDGE.** Read the returned summary. **Proceed only if** the gate is **GREEN** and (for hash-neutral items)
   every verify claim tallied **≥2/3 → `CONFIRMED`**. Otherwise mark the item `RED` in `QUEUE.md`, leave the tree
   for human review, and **STOP** (never commit on red; never weaken a gate to pass).
4. **MERGE** (skip if `--review`). The established pattern (`[[no-ci-wait-autonomous-roadmap]]` — merge on LOCAL
   gate green + adversarial verify, do **not** block on GitHub CI):
   - Commit the item's tree on this session's worktree branch — one conventional commit, message names the queue
     item + `hash-neutral` (or the re-pin `0x….→0x….` ledger line) + `gate green`, with the `Co-Authored-By` trailer.
   - `main` is not checked out here → merge via a temp worktree: `git worktree add <TMP> main` ·
     `git -C <TMP> merge --no-ff <branch> -F <msgfile>` (message via a **file** — backticks in `-m` trigger a bash
     command-substitution bug) · verify the merged tree equals the gate-green commit · `git worktree remove <TMP>`.
     For a **designed re-pin**, push the branch + run the multi-ISA CI matrix to validate cross-arch *before* main
     (the one thing a single local arch can't prove); hash-neutral items merge straight off the local green gate.
5. **CLOSE.** Mark the item `[x]` (+ commit/merge sha) in `QUEUE.md`; add a CHANGELOG line; emit a 3-line summary
   (what landed · hash status · next item). Then continue to the next ready item (unless `--once`).

## End of run
Emit a per-item rollup (landed / RED / skipped-🛑) + the next ready item. If queue depth `< 3` or empty, say so
and recommend **`/roadmap-plan`** to refill before the next run.

## Hard rules (SPEC §2.1)
GPL stays at the subprocess boundary (never linked); no genome logic in `godot/`; seeded ChaCha8 only (no
global/thread RNG, no HashMap iteration in sim state); AI agents at species granularity; pinned versions. The two
HARD gates (determinism #3, license #1) can never be skipped. A moved pinned literal that wasn't a *designed*
re-pin is a determinism FAIL → STOP THE LINE.
