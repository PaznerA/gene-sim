# TASKS — backlog, current slice, acceptance criteria

> The `/iterate` loop reads the **top unstarted slice** from here. A slice is the smallest vertical change
> that leaves the build green and advances the bar (SPEC §1.2). One slice = one commit/PR.
> Status keys: `[ ]` unstarted · `[~]` in progress · `[x]` done · `🛑` needs human sign-off (invariant/large).
> Stage exit gates are in SPEC §8; test gates in SPEC §10.

---

## ▶ CURRENT SLICE

### [x] S0 — Stage 0: Headless deterministic core skeleton  ✅ DONE (gate green; ADR-001, ADR-002)
**Goal:** A Cargo workspace whose headless sim core runs N seeded instances and is bit-reproducible — no graphics, no CRISPR yet.

**Scope (fewest crates):** `crates/genome`, `crates/sim-core`, `crates/harness` (+ empty `crates/crispr`, `crates/oracle-slim` stubs so the workspace is whole).

**Deliverables**
- Cargo workspace + 5 member crates (`genome`, `crispr`, `sim-core`, `harness`, `oracle-slim`).
- Parametric **Genome** data model in `crates/genome` (Loci → typed Parameters + ontology tags). Canonical version mirrored into `docs/llm/TAXONOMY.md`.
- **Empty but fully deterministic** Bevy ECS tick loop in `crates/sim-core`: fixed system ordering, single threaded `rand_chacha::ChaCha8Rng`, no `HashMap` iteration in sim logic.
- `crates/harness` binary: `--seed`, `--runs`, `--generations`, `--hash-only` (and master-seed/run-index derivation). Headless. Emits a per-run stats hash.
- `tools/check_determinism.sh` (SPEC §W8): same seed twice → identical hash.
- Baseline **entity-count × tick-rate** criterion bench recorded in DECISIONS.md (§11).

**Acceptance criteria (Definition of Done — SPEC §8 Stage 0)**
- `cargo run -p harness -- --seed 42 --runs 1 --generations 200` runs headless and prints stats.
- `cargo run -p harness -- --seed 42 --runs 8` produces per-run stats.
- Determinism gate **GREEN**: `./tools/check_determinism.sh` → identical hash twice.
- Gates 1–3 (fmt, clippy `-D warnings`, `cargo test --workspace`) green.
- Baseline bench recorded as the perf threshold (§11).

**Invariants in play:** #2 genome-in-core, #3 determinism (the load-bearing one this slice), #4 headless-first, #7 pinned versions. No GPL anything yet.

---

## BACKLOG

### Stage 1 — CRISPR mechanic (`crates/crispr`) — SPEC §8
- [x] **S1.1** Cas-variant data table in `data/cas_variants.ron` (SpCas9 NGG, SaCas9 NNGRRT, Cas12a TTTV, SpRY/NG, base/prime) + a loader. *Table is data, not code (SPEC §4).* AC: loader round-trips the table; unit test asserts ≥5 variants with PAM + cut offset + edit type. ✅ DONE (7 variants; gate green; reviewer APPROVE; ADR-003).
- [x] **S1.2** PAM finding via **rust-bio** (MIT) in `crates/crispr`: given a locus sequence + Cas variant, return PAM/cut sites. AC: unit tests on known sequences for NGG and TTTV; property test: every reported site actually matches the PAM regex. ✅ DONE (both strands, IUPAC-degenerate; gate green; reviewer APPROVE; ADR-004).
- [x] **S1.3** `Score` traits (`OnTargetScore`, `OffTargetScore`) + in-core default impls (heuristic on-target eff, naive off-target hit count). *Pluggable behind a trait — invariant #5.* AC: trait + default impl unit-tested; swapping impls compiles without touching sim-core. ✅ DONE (object-safe + generic-swappable; gate green; reviewer APPROVE).
- [x] **S1.4** Edit application: `(CasVariant, target_locus, guide)` → gate on on-target eff + off-target count → mutate Parameter(s); failed-edit path = off-target perturbation elsewhere (never a silent success). AC: unit + property tests — edit never yields an invalid genome; failed edits never silently succeed. ✅ DONE (seeded ChaCha8 threaded; both §10.4 props; gate green; adversarial reviewer APPROVE).
- [x] **S1.5** `GenotypePhenotypeMap` (Parameters → Traits, weighted-sum / simple GRN) feeding selection in `sim-core`. AC: trait values deterministic for a fixed genome; selection responds to a trait; property test: allele freq ∈ [0,1]. ✅ DONE (WeightedSumMap + constant-N Wright-Fisher selection, allele_freq directional; gate green incl. re-baselined bench; reviewer APPROVE; ADR-005). **← Stage 1 COMPLETE.**

### Stage 2 — Genetics realism (`crates/oracle-slim`, SLiM subprocess) — SPEC §8
- [x] 🛑 **S2.1** `tools/install_slim.sh`: build SLiM from source at the pinned tag (SPEC §W2), record `slim -version` in DECISIONS.md. *Touches invariant #1 + #7 — human sign-off before linking decisions.* AC: `slim -version` matches the pinned tag. ✅ DONE (human signed off; SLiM v5.2 / commit f11de0d built + installed; license gate confirms no GPL crate; oracle-slim depless).
- [x] **S2.2** `crates/oracle-slim` subprocess driver: generate an Eidos model, run `slim -seed <derived> -d ... model.slim` via `std::process::Command`. **No GPL crate in the dep tree.** AC: driver produces a `.trees` file for a fixed seed; `cargo tree -p oracle-slim` shows zero GPL crates. ✅ DONE (std-only, zero deps; runs slim v5.2 → `.trees`; graceful skip when slim absent; reviewer APPROVE on invariant #1).
- [x] **S2.3** `scripts/slim_analyze.py` (tskit/pyslim): read back allele freqs / fitness from `.trees`. AC: parses the S2.2 output into a stats dict. ✅ DONE (parses oracle-slim `.trees` → JSON stats: samples/sites/mutations/π/mean+max allele freq ∈ [0,1]; `examples/produce_trees.rs` chains S2.2→S2.3; **SLiM genetics confirmed reproducible** for a fixed seed — de-risks S2.4; `.venv` pinned in `scripts/requirements.txt`).
- [x] **S2.4** Golden-file oracle gate: pinned seed → allele freq within tolerance of `data/golden/<case>.json` (SPEC §8 Stage 2, §10.6). AC: gate passes within tolerance; determinism preserved. ✅ DONE (`slim_analyze.py --check` + `tools/check_slim_oracle.sh`, wired into `tools/gate.sh` as gate 7/8; golden `slim_case1.json` pins SLiM v5.2; verified pass + tamper-fail). *Note: accepted the in-model neutral-mutation warning for now (deliberate); MU=0 + msprime overlay remains an option if richer realism is wanted.*
- [x] **S2.5** `scripts/check_license.sh` (gate #8): assert no GPL crate in `cargo tree`; assert `oracle-slim` only shells out. AC: script exits non-zero if a GPL crate appears; wired into `/gate`. ✅ DONE (delivered early in the dev-loop hardening; SPDX-OR-aware GPL detector + oracle-slim depless check; wired into `tools/gate.sh` as gate 8/8). **← Stage 2 COMPLETE.**

### Stage 3 — AI harness (`crates/harness`) — SPEC §8
- [ ] **S3.1** Gym-like env: `reset()` / `step(action)` / `seed()` (SPEC §2.2, §5). Action = `EditAction` at **species/operator** granularity (invariant #6). AC: env trait + unit test of one reset/step/seed cycle.
- [ ] **S3.2** Replay logs: `seed.json` (master + derived seeds + pinned versions) + `actions.ndjson`. Replaying `seed + actions` is bit-identical (SPEC §5, §6). AC: replay of a logged run reproduces the same stats hash.
- [ ] **S3.3** Parallel batch runner `tools/run_batch.sh` (SPEC §W7): hundreds of deterministic runs; per-generation stats to Parquet. AC: M parallel runs reproduce; columnar stats written.
- [ ] **S3.4** Confirm the ~10k-named-agent ceiling (invariant #6): actions stay operator/species level, never per-organism. AC: a test/assert that the action space is species-granular.

### Stage 4 — Godot UI (LAST) (`godot/`) — SPEC §8
- [ ] 🛑 **S4.1** `tools/install_godot.sh`: pin Godot minor (SPEC §W3), `godot/` project skeleton, `godot --headless --quit` smoke. *Build order gate — only after the core is headless + deterministic (invariant #4).* AC: pinned version recorded; headless smoke passes.
- [ ] **S4.2** Snapshot reader in `godot/`: read `data/runs/<id>/snapshots/*.bin` (bincode, SPEC §5). **GDScript reads only — no biology (invariant #2).** AC: loads a snapshot and reports entity count.
- [ ] **S4.3** 2D TileMap ecosystem view of one scope (field/forest/pond). AC: renders a live run from snapshots.
- [ ] **S4.4** ≥2 toggleable data-layer shaders (per-cell data texture: density, allele freq, fitness, edit penetrance) + viewport zoom scopes (SPEC §W10). AC: layers toggle; zoom switches scope.
- [ ] **S4.5** L-system morphology driven by genome trait params → visible plant change. AC: an edit visibly changes branching/leaf structure; **zero biology math in GDScript**.

### Stage 5 — Ontology + LLM modifiers — SPEC §8
- [ ] **S5.1** Load SO / GO (`go-basic.obo`) / NCBI-tax via `scripts/parse_ontology.py` (obonet) → in-game ontology graph (SPEC §W4, §6). AC: parses OBO into a graph; node/edge counts asserted.
- [ ] **S5.2** Fixed JSON schema for LLM-generated ontology nodes / modifier functions + schema validation. AC: invalid extension rejected; valid one accepted.
- [ ] **S5.3** Graph validation: a new node must subclass an existing SO/GO term before admission (the safe extension boundary, SPEC §4). AC: property test — an LLM-added node always validates against schema + graph before admission.
- [ ] **S5.4** Daisy-chain kill-switch containment model: payload spreads only while daisy elements remain; diluted ~50%/gen; self-exhausts (SPEC §8 Stage 5, §6). AC: in sim, the drive dilutes ~50%/gen and self-exhausts.

---

## FOLLOW-UPS / TECH DEBT (non-blocking; pick up when convenient)
- [ ] **F1** sim-core selection write-back: replace the per-generation `BTreeMap<u32,f64>` with a `Vec` indexed
  by contiguous `OrgId` (O(N) vs O(N log N) + allocation). Would lift the Stage 1 perf baseline (ADR-005).
- [ ] **F2** sim-core `metabolism`: it draws from `SimRng` *inside* `Query<&mut Energy>` iteration — safe today
  (single archetype, no structural changes) but harden (snapshot to ordered Vec, or draw outside the query) if
  any system later adds/removes components per-organism. (Reviewer note, S1.5.)

## DONE
- **S0** — Stage 0 headless deterministic core skeleton. DoD met: `cargo run -p harness -- --seed 42
  --runs 1 --generations 200` works; `--runs 8` produces 8 distinct-seed runs; determinism gate GREEN
  (`3393427b072eb803`, superseded by `fde0e0b6…` after S1.5); baseline bench recorded. See CHANGELOG +
  DECISIONS (ADR-001, ADR-002).
- **Stage 1 (S1.1–S1.5)** — CRISPR mechanic complete: Cas-variant table (S1.1), PAM finding via rust-bio
  (S1.2), pluggable Score traits (S1.3), gated edit application (S1.4), GP map + Wright-Fisher selection
  (S1.5). ADR-003/004/005. Every slice ran through the multi-agent loop (implementer → tools/gate.sh →
  reviewer APPROVE) and was committed individually. Determinism hash now `fde0e0b61b9e23e6`.
