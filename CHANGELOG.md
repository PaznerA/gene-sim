# Changelog

All notable changes per slice. One slice = one entry. Format loosely follows Keep a Changelog.

## [Unreleased]

### Dev loop hardened (chore)
- `tools/gate.sh`: single robust gate runner — fmt · clippy `-D warnings` · test · determinism · proptest ·
  bench (opt-in `GATE_BENCH=1`) · license; PASS/FAIL/SKIP/N-A per item, non-zero exit on any red.
- `scripts/check_license.sh`: real licensing gate (promoted from the S2.5 stub) — SPDX-`OR`-aware GPL
  detector via `jq` (flags only crates with no GPL-free choice; allows `MIT OR … OR LGPL`) + asserts
  `crates/oracle-slim` is dependency-free. Guards invariant #1 from day one.
- `docs/llm/LOOP.md`: durable runbook for the robust loop — roles, per-slice procedure, **autonomous-until-
  red/invariant** mode, stop conditions, resumability (state in TASKS.md + git), and the skill/agent
  mid-session registration gotcha.
- Skills fixed: removed the invalid `invocation: user` frontmatter field (silently ignored by Claude Code —
  the cause of `/iterate` not registering); `gate` now calls `tools/gate.sh`; `iterate` encodes autonomous
  multi-agent mode. CLAUDE.md / SNIPPETS.md point at the new machinery.

### S0 — Stage 0: headless deterministic core skeleton (feat)
- Cargo workspace with 5 crates: `genome`, `crispr` (stub), `sim-core`, `harness`, `oracle-slim` (stub).
- `crates/genome`: parametric `Genome` model — `Locus` / `Parameter` / `ParamValue` (Numeric/Enum/Bool with
  domains) / `DnaSequence` (validated ACGT) / `OntologyTags`, plus a deterministic `sample_genome()`.
  Mirrors docs/llm/TAXONOMY.md §1.
- `crates/sim-core`: empty-but-deterministic Bevy ECS tick loop (`bevy_ecs` 0.19) — single seeded
  `ChaCha8Rng` resource, explicit `.chain()` system order, id-sorted end-of-run hash, `derive_seed`
  splitmix64 sub-seeding. `genome` wired into the core.
- `crates/harness`: headless CLI (`--seed/--master-seed/--run-index/--runs/--generations/--entities/
  --hash-only`); per-run derived seeds; writes `data/runs/<run_id>/{seed.json,stats.ndjson}`.
- `tools/check_determinism.sh` (SPEC §W8); criterion bench `crates/sim-core/benches/tick.rs`.
- Property tests behind the `proptest` feature (genome domain invariants; same-config-same-hash).
- **Gates green:** fmt, clippy `-D warnings`, 12 unit tests, determinism, 3 property tests, bench baseline
  recorded in DECISIONS.md (~175 M organism-updates/s on M4 Max). License gate N/A until Stage 2 (S2.5).
- Fixed a seed-derivation collision (`stream | 1` collapsed streams 0 and 1) caught while verifying DoD.

### Meta / scaffolding
- Repo bootstrapped: `CLAUDE.md` (7 invariants + per-slice loop), `docs/llm/SPEC.md` moved to its canonical
  location, and the persistent context files (`TASKS.md`, `DECISIONS.md`, `TAXONOMY.md`, `GLOSSARY.md`,
  `SNIPPETS.md`).
- `.claude/skills/{iterate,gate,slice-done}` and `.claude/agents/{planner,implementer,gatekeeper,reviewer}` added.
- ADR-001 (native macOS Apple-Silicon toolchain; SLiM-from-source; Crisflash off-target oracle) and
  ADR-002 (Stage 0 determinism strategy) recorded.
