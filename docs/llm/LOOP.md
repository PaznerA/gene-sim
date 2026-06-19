# LOOP.md — the robust iterative-development loop (runbook)

> How gene-sim is built, one slice at a time. This is the operational companion to SPEC §7. The `iterate`
> skill (`/iterate`) executes this; you can also just say "run the next slice" / "jeď dál" — the loop does
> not depend on slash-command registration.

## 0. Why this exists / robustness properties
- **Deterministic gate.** All quality checks run through ONE script, `tools/gate.sh`. Humans and agents run
  the identical command; there is no "it passed for me" drift. Green is green.
- **Resumable.** All loop state lives in `docs/llm/TASKS.md` (which slice is next) + git history (what
  landed). The loop can stop at any point and resume later with no in-memory state. A crash, a context
  reset, or "continue tomorrow" all just re-read TASKS.md and the latest commit.
- **Invariant-safe.** Every slice is gated (`tools/gate.sh`) and reviewed against SPEC §2.1; the two HARD
  gates (determinism #4, license #1) can never be skipped.
- **Bounded autonomy.** Autonomous runs stop at well-defined boundaries (see §3), so "let it run" never
  means "let it run off a cliff."

## 1. Roles (multi-agent, context-isolated — SPEC §7.3)
Defined in `.claude/agents/`. The **main session orchestrates**; each role runs in its own context.

| Role | When | Does | Never |
|---|---|---|---|
| **planner** | a NEW goal needs decomposing into slices | writes slices + acceptance criteria into TASKS.md; flags 🛑 invariant/large slices | writes code |
| **implementer** | executing a slice | code + tests, fewest crates, smallest surface | links GPL / puts biology in godot/ / unseeded RNG |
| **gatekeeper** | after implement | runs `tools/gate.sh`, reports PASS/FAIL, blocks on red | edits code to pass |
| **reviewer** | after green gate | checks diff vs §2.1 invariants + licensing | edits anything; waves through a violation |

> **Invoking the roles.** After a session restart, spawn them by `subagent_type` (e.g. `implementer`).
> In a session where `.claude/agents/` was created mid-session (not yet registered), spawn a
> `general-purpose` agent and paste the role file's body as the prompt — same isolation, no dependency on
> registration. (Same caveat as skills — see §6.)

## 2. The per-slice procedure
For the **top unstarted slice** in `docs/llm/TASKS.md`:
1. **LOAD** — re-read SPEC §2.1 invariants, the slice + its acceptance criteria, DECISIONS pins, TAXONOMY.
2. **GUARD** — if the slice is 🛑 (touches an invariant §2.1) or > ~1 day → **STOP, ask the human.** Do not
   start coding.
3. **IMPLEMENT** (implementer) — code + tests together, fewest crates. Honor every invariant.
4. **GATE** (gatekeeper) — `tools/gate.sh`. Any red ⇒ **STOP THE LINE**: fix or revert; never commit on red.
5. **REVIEW** (reviewer) — diff vs §2.1 + licensing. Any violation ⇒ **SEND BACK** (back to step 3) and, if
   it's an invariant breach, surface to the human.
6. **REFLECT** — load-bearing choice → append an ADR to DECISIONS.md; update CHANGELOG.md.
7. **COMMIT** — one conventional commit per slice, with the `Co-Authored-By` trailer.
8. **CLOSE** — mark the slice `[x]` done in TASKS.md (move it to DONE), emit a 3-line summary.

## 3. Autonomy & stop conditions (DEFAULT = autonomous)
The loop runs slices back-to-back and **STOPS + surfaces to the human** on the FIRST of:
- **RED gate** — `tools/gate.sh` fails (any gate). Report the failing gate + repro.
- **🛑 invariant slice** — the next unstarted slice touches an invariant §2.1 (e.g. S2.1 adds the SLiM
  subprocess boundary; S4.1 starts the renderer). These need human sign-off before code.
- **Backlog empty** — no unstarted slice remains in TASKS.md.
- **Human interrupt** — any message from the human.

Modes: **autonomous** (default) · **`--once`** (one slice, then stop) · **`--bench`** (also run the perf gate).

> Natural bounded runs: from S1.1 the loop autonomously completes Stage 1 (S1.1→S1.5) and halts at **S2.1**
> (🛑 invariant #1 — SLiM/GPL boundary). That's the intended "let it run a stage" envelope.

## 4. The gate (single source of truth)
```bash
tools/gate.sh                 # fmt · clippy -D warnings · test · determinism · proptest · (bench skip) · license
GATE_BENCH=1 tools/gate.sh    # + criterion perf bench (slow) — run at stage exits (§11)
```
HARD gates (never skip): **#4 determinism** (`tools/check_determinism.sh`), **#7 license**
(`scripts/check_license.sh` — no GPL crate; oracle-slim dependency-free).

## 5. Triggering the loop
- `/iterate` (after the skill is registered — see §6), or `/iterate --once`.
- Or plain language to the main session: "run the next slice" / "continue" / "jeď dál".
- Both follow this runbook identically.

## 6. Known Claude Code gotcha — skill/agent registration
`.claude/skills/` and `.claude/agents/` are scanned at session start. If either **top-level directory did
not exist** when the session began (as on first scaffolding), new skills/agents register only after a
**session restart** (or `/reload-skills`, CC ≥ 2.1.152). Until then: drive the loop via natural language and
spawn roles as `general-purpose` agents with the role body as the prompt (§1). Frontmatter must be valid —
use `name`/`description` (the field `invocation:` does **not** exist and is silently ignored).
