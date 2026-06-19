# gene-sim — Claude Code entry context

> A 2D, data-layer-driven CRISPR ecosystem simulator (PoC). **Headless sim core first, Godot UI last.**
> The single source of truth is **[docs/llm/SPEC.md](docs/llm/SPEC.md)** — read it (and the invariants
> below) at the start of every slice. This file just keeps the invariants + the loop in session context.

## The 7 invariants — STOP THE LINE if violated (SPEC §2.1)

Violating one is a "stop the line" event: **halt, surface to the human, do not work around it.**

1. **GPL stays at the process boundary.** SLiM (GPL-3) and any other GPL tool are invoked as
   **separate CLI subprocesses only** — never linked into the game binary. `crates/oracle-slim`
   shells out and must not depend on any GPL crate. (Preserves licensing freedom for a future
   closed/commercial release.)
2. **Genome lives in the sim core; render is read-only.** Genotype→phenotype logic exists only in
   `crates/genome` / `crates/sim-core`. `godot/` consumes snapshots and never computes biology.
   **No genome logic in GDScript. Ever.**
3. **Determinism.** One master seed per run derives all sub-seeds (sim-core RNG + SLiM `-seed`).
   Same seed + same build + same platform → identical bytes. Use `rand_chacha::ChaCha8Rng`
   threaded explicitly through the sim — **never** thread-local/global RNG, **never** iterate a
   `HashMap` in sim logic (use ordered/indexed collections).
4. **Headless-first.** Every sim feature must work and be tested with no renderer attached before
   any UI work touches it.
5. **Science is pluggable behind a trait.** On-target / off-target scoring sit behind Rust traits
   with a lightweight in-core default impl and optional subprocess-backed "realistic" impls.
   Swapping impls must not touch sim-core logic.
6. **Agent granularity ceiling.** AI agents act at the **operator/species** level, not per-organism.
   Individual organisms are ECS entities, not RL agents.
7. **Versions are pinned.** SLiM tag, Godot minor, Bevy version, Rust toolchain — all pinned and
   recorded in [docs/llm/DECISIONS.md](docs/llm/DECISIONS.md). Cross-version reproducibility is not guaranteed.

## Persistent context (read at the start of every slice) — SPEC §7.1

- [docs/llm/SPEC.md](docs/llm/SPEC.md) — invariants + architecture (north star).
- [docs/llm/TASKS.md](docs/llm/TASKS.md) — backlog, the **current** slice, acceptance criteria. The loop reads the top unstarted slice from here.
- [docs/llm/DECISIONS.md](docs/llm/DECISIONS.md) — ADRs + pinned versions. Append-only.
- [docs/llm/TAXONOMY.md](docs/llm/TAXONOMY.md) — canonical genome/ontology data model (data-model source of truth).
- [docs/llm/GLOSSARY.md](docs/llm/GLOSSARY.md) · [docs/llm/SNIPPETS.md](docs/llm/SNIPPETS.md) — domain terms; reusable patterns + gotchas.

## The per-slice loop (SPEC §7.2)

A **slice** is the smallest vertical change that leaves the build green and demonstrably advances the
bar (SPEC §1.2). Run it with `/iterate`:

1. **LOAD** — read SPEC invariants + the top slice in TASKS.md + DECISIONS.
2. **PLAN** — restate the slice goal + acceptance criteria in TASKS.md.
   If the slice is >~1 day **or** touches an invariant (§2.1) → **STOP, ask the human.**
3. **IMPLEMENT** — code AND tests together, fewest crates touched. Respect invariants
   (no GPL linking, no genome logic in `godot/`, seeded RNG only).
4. **GATE** — run `/gate`. Any red → fix or revert. **Never proceed on red.**
5. **REFLECT** — load-bearing choice made → append an ADR to DECISIONS.md. Update CHANGELOG.
6. **COMMIT** — conventional commit; one slice = one commit/PR.
7. **CLOSE** — mark the slice done in TASKS.md, emit a 3-line summary.
   Default: **STOP for human review.** Only with an explicit `--auto` flag: continue to the next slice.

## Multi-agent split (SPEC §7.3)

Subagents in `.claude/agents/` (spawn via the Task tool, context-isolated):
**planner** (decompose → slices, no code) · **implementer** (one slice: code + tests) ·
**gatekeeper** (run `/gate`, block on red, no code) · **reviewer** (diff vs invariants + licensing, no code).
Handoffs are files: TASKS.md entries (planner→implementer), the diff/PR (implementer→gatekeeper→reviewer).

## Build order (fixed)

Stage 0 headless core → 1 CRISPR mechanic → 2 SLiM genetics oracle → 3 AI harness → 4 Godot UI (LAST) → 5 ontology + LLM modifiers.
**Do not start the renderer until the core runs headless and deterministic.** Reuse > reinvent (SPEC §2.2);
if reinventing seems justified, write an ADR and stop for human sign-off.
