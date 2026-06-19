# Changelog

All notable changes per slice. One slice = one entry. Format loosely follows Keep a Changelog.

## [Unreleased]

### S1.3 — pluggable Score traits + in-core default impls (feat, Stage 1)
- `crates/crispr`: `OnTargetScore`/`OffTargetScore` traits (match TAXONOMY §3.3) — the invariant-#5 swappable
  science boundary (object-safe + generic-usable; proven by an alternate impl substituting with no trait/
  sim-core change). `GuideSequence` (validated ACGT, mirrors `DnaSequence`).
- `DefaultOnTargetScore`: pure heuristic `clamp_[0,1](0.5·gc + 0.3·length + 0.2·pam)` (gc peaks at 50%, length
  favors 17–24 nt, pam = valid PAM adjacent to the guide's locus match). `DefaultOffTargetScore { mismatch_budget=3 }`:
  naive Hamming near-match count across all loci, both strands, iterating the ordered `Vec` (inv. #3).
- No new deps. Tests: efficiency ∈ [0,1], off-target absent=0/present>0/monotone-in-budget, determinism,
  pluggability (generic + `dyn`), proptest (efficiency always in unit interval). Loop: implementer → gate
  (GREEN) → reviewer (APPROVE). TAXONOMY §3.2 `GuideSequence` synced to the validated form.

### S1.2 — PAM finding via rust-bio (feat, Stage 1)
- `crates/crispr`: `find_pam_sites(seq, variant)` (+ `_in` for `genome::DnaSequence`) returning ordered,
  `(position, strand)`-sorted `PamSite { position, strand, cut_site }` on both strands. `Strand` enum;
  public `iupac_matches` (full IUPAC set, case-insensitive, U→T). Reverse strand via `bio::alphabets::dna::revcomp`.
- Cut-site convention documented on `PamSite` (forward frame; forward `position+cut_offset`, reverse
  `(position+pam_len-1)-cut_offset`). Determinism preserved (sorted Vec, no HashMap; inv. #3).
- Dep: `bio` (rust-bio) `4.0`, MIT, GPL-free tree verified (ADR-004 — rust-bio for seq ops, IUPAC degeneracy
  kept in-house per SPEC §0.4).
- Tests: NGG/TTTV known sequences incl. reverse hit + cut math, TTTT-excluded, IUPAC table, determinism;
  proptest: every reported site truly matches the PAM (no false positives). Loop: implementer → gate (GREEN)
  → reviewer (send-back for the missing `bio` pin → fixed → APPROVE).

### S1.1 — Cas-variant data table + loader (feat, Stage 1)
- `data/cas_variants.ron`: seed table of 7 Cas variants (SpCas9 NGG, SaCas9 NNGRRT, AsCas12a TTTV, Cas9-NG,
  SpRY NRN, BE4 base editor, PE2 prime editor) — *data, not code* (SPEC §4).
- `crates/crispr`: `CasVariant`/`CasVariantId`/`EditType` matching TAXONOMY §3.1; `load_cas_variants_from_str`
  (clean `LoadError`) + `default_cas_variants()` embedding the RON via `include_str!`. Ordered `Vec` (inv. #3).
- Deps pinned: `serde = "1"`, `ron = "0.12"` (both MIT/Apache; ADR-003 — 0.8 not in registry, 0.12 is current).
- Tests: round-trip (+proptest), ≥5 variants, literature PAMs, all edit types, PAM-relaxed, non-zero base
  window, malformed-RON error. Driven through the multi-agent loop (implementer → gate → reviewer: APPROVE).

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
