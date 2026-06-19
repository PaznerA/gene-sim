# Changelog

All notable changes per slice. One slice = one entry. Format loosely follows Keep a Changelog.

## [Unreleased]

### Meta / scaffolding
- Repo bootstrapped: `CLAUDE.md` (7 invariants + per-slice loop), `docs/llm/SPEC.md` moved to its canonical
  location, and the persistent context files (`TASKS.md`, `DECISIONS.md`, `TAXONOMY.md`, `GLOSSARY.md`,
  `SNIPPETS.md`).
- `.claude/skills/{iterate,gate,slice-done}` and `.claude/agents/{planner,implementer,gatekeeper,reviewer}` added.
- ADR-001 (native macOS Apple-Silicon toolchain; SLiM-from-source; Crisflash off-target oracle) and
  ADR-002 (Stage 0 determinism strategy) recorded.
