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
- [x] **S3.1** Gym-like env: `reset()` / `step(action)` / `seed()` (SPEC §2.2, §5). Action = `EditAction` at **species/operator** granularity (invariant #6). AC: env trait + unit test of one reset/step/seed cycle. ✅ DONE (stepwise `Simulation` in sim-core + `GeneSimEnv` in harness; species-granular `Action`; determinism hash unchanged; gate green; reviewer APPROVE).
- [x] **S3.2** Replay logs: `seed.json` (master + derived seeds + pinned versions) + `actions.ndjson`. Replaying `seed + actions` is bit-identical (SPEC §5, §6). AC: replay of a logged run reproduces the same stats hash. ✅ DONE (`harness::replay` record/replay share one path → bit-identical hash; serde on LocusId/GuideSequence/Action; validation-preserving guide deser; gate green; reviewer APPROVE).
- [x] **S3.3** Parallel batch runner `tools/run_batch.sh` (SPEC §W7): hundreds of deterministic runs; per-generation stats to Parquet. AC: M parallel runs reproduce; columnar stats written. ✅ DONE (`harness --per-gen-stats` → per_gen.csv; `run_batch.sh` parallel via xargs (two batches byte-identical); `scripts/aggregate_parquet.py` → columnar Parquet (8 runs → 400×9); pyarrow pinned; hash unchanged; reviewer APPROVE).
- [x] **S3.4** Confirm the ~10k-named-agent ceiling (invariant #6): actions stay operator/species level, never per-organism. AC: a test/assert that the action space is species-granular. ✅ DONE (satisfied by S3.1: `Action` has no per-organism variant — unrepresentable by construction; `action_space_is_species_granular` compile-guard test). **← Stage 3 COMPLETE.**

### Stage 4 — Godot UI (LAST) (`godot/`) — SPEC §8
- [x] 🛑 **S4.1** `tools/install_godot.sh`: pin Godot minor (SPEC §W3), `godot/` project skeleton, `godot --headless --quit` smoke. *Build order gate — only after the core is headless + deterministic (invariant #4).* AC: pinned version recorded; headless smoke passes. ✅ DONE (human signed off; Godot **4.7** pinned; `godot/` project + read-only `main.gd` (inv #2); headless smoke "UI booted … OK"). Build-order precondition met (Stages 0–3 headless+deterministic).
- [x] **S4.2** Snapshot reader in `godot/`: read `data/runs/<id>/snapshots/*.bin` (SPEC §5). **GDScript reads only — no biology (invariant #2).** AC: loads a snapshot and reports entity count. ✅ DONE (`sim-core::GridSnapshot` derived read-only grid + `std`-only `"GSS1"` format off the hash path (inv #3); `harness --snapshots`; `godot/snapshot.gd` read-only parser + `to_data_image()`; `main.gd --snap` reports `WxH/gen/pop/cells/channels` headless. Fixed the `class_name`/global-cache headless trap via `preload`. New gate 9/9 `check_godot_snapshot.sh` (skip-if-absent) locks it in; full gate green.)
- [x] **S4.3** 2D TileMap ecosystem view of one scope (field/forest/pond). AC: renders a live run from snapshots. ✅ DONE (`main.gd` builds, all read-only (inv #2): grass `TileMapLayer` + per-cell data-overlay `Sprite2D` + organism dot layer (`organisms.gd`) + `Camera2D` + HUD. `--run <dir>` plays `snap_*.bin` ordered by gen on a timer (auto-discovers newest run); gen0→gen60 visibly tracks selection. Verified by windowed `--shot` PNG capture; headless `--check` render smoke wired into gate 9/9 alongside the reader. ADR-006.)
- [x] **S4.4** ≥2 toggleable data-layer shaders (per-cell data texture: density, allele freq, fitness, edit penetrance) + viewport zoom scopes (SPEC §W10). AC: layers toggle; zoom switches scope. ✅ DONE (`data_layer.gdshader` samples the RGBF data texture; `D` cycles 3 GPU layers density/allele_freq/fitness; wheel + keys 1/2/3 zoom scopes field/patch/cells + arrow pan; HUD shows layer+scope. Verified via windowed `--shot --layer/--zoom`; headless `--check` builds the ShaderMaterial path (gate 9/9). *Note: edit-penetrance layer deferred — needs a 4th snapshot channel from the core (follow-up F3).* ADR-006.)
- [x] **S4.5** L-system morphology driven by genome trait params → visible plant change. AC: an edit visibly changes branching/leaf structure; **zero biology math in GDScript**. ✅ DONE (`harness --specimens` exports `specimens.json` — baseline + per-edit species-genome trait vectors via a separate `GeneSimEnv`, off the hash path; `godot/lsystem.gd` parametric turtle L-system + `_plant_params_from_traits` (trait→visual mapping, no biology); specimen view (key `V`) shows baseline vs edited plants side by side — the growth-knockdown edit visibly stunts the plant, the kill-switch edit greens+grows it. UI control bar (view toggle, play/pause, step, layer dropdown). Gate `--check` builds the L-system; full gate green. ADR-007.) **← Stage 4 COMPLETE.**

### Stage 5 — Ontology + LLM modifiers — SPEC §8
- [ ] **S5.1** Load SO / GO (`go-basic.obo`) / NCBI-tax via `scripts/parse_ontology.py` (obonet) → in-game ontology graph (SPEC §W4, §6). AC: parses OBO into a graph; node/edge counts asserted.
- [ ] **S5.2** Fixed JSON schema for LLM-generated ontology nodes / modifier functions + schema validation. AC: invalid extension rejected; valid one accepted.
- [ ] **S5.3** Graph validation: a new node must subclass an existing SO/GO term before admission (the safe extension boundary, SPEC §4). AC: property test — an LLM-added node always validates against schema + graph before admission.
- [ ] **S5.4** Daisy-chain kill-switch containment model: payload spreads only while daisy elements remain; diluted ~50%/gen; self-exhausts (SPEC §8 Stage 5, §6). AC: in sim, the drive dilutes ~50%/gen and self-exhausts.

---

## ROADMAP — beyond the PoC: a *Bibites*-like ecosystem sandbox

> **North-star:** grow the single-species deterministic PoC into a **multi-species, editable, open-ended
> ecosystem sandbox** where a player or LLM agent **combines species**, **shapes terrain + environment**,
> **intervenes with CRISPR edits** (and watches them on a timeline), and observes emergence — inspired by
> **[The Bibites](https://thebibites.com/)** (and similar artificial-life sandboxes). The fixed PoC build
> order (Stages 0–5) is the foundation; these epics extend it.
>
> **Gating rule:** every epic that touches the **sim model** is >1 day and risks invariants #2/#3/#6 + the
> perf gate → **🛑 design (ideally a design workflow) + ADR + human sign-off BEFORE core code** (per LOOP §2).
> Renderer epics are invariant-safe presentation and run on the normal per-slice loop. Determinism (#3) is the
> load-bearing constraint for all core work; re-baseline the perf gate in any slice that touches the hot loop.
> Keep the gene-sim differentiators vs. Bibites: **real CRISPR mechanic, real SO/GO ontology, deterministic
> reproducibility, daisy-chain biosafety.**

- **R1 — Terrain + soil/environment substrate (core)** — designed (workflow) + signed off. Decisions: 3 soil
  channels (moisture/nutrients/pH) from the start; DroughtTolerance becomes **per-individual heritable**
  (R1.0a); target = **full local model (R1.3)**, reached via phases. Sub-slices:
  - [x] **R1.0** Static seed-derived `SoilField` (3 channels) + 3 read-only snapshot channels (GSS1→GSS2,
    parse-only Godot) + unwired `EnvironmentModifier` seam + **pinned-hash test proving hash-neutrality**.
    ✅ DONE (`crates/sim-core/src/soil.rs`; zero `SimRng` draws, off `hash_world`; perf within noise; click-
    detail panel shows per-cell soil; full gate green; ADR-008 + derive_seed stream registry).
  - [ ] 🛑 **R1.0a** Make `DroughtTolerance` a live **per-individual heritable** parameter (decolide from the
    killswitch Bool slot; resampled like `Genotype`). Prerequisite for any coupling. Changes the hash once
    (update the pinned literal in-slice); own ADR. *Invariants:* #2/#3.
  - [ ] 🛑 **R1.1** Wire `EnvironmentModifier` into `selection()` — **global** soil-modulated fitness
    (constant-N preserved; ADR extends ADR-005). First real coupling; static dispatch; re-baseline perf;
    fold soil digest into `hash_world` at a fixed position.
  - [ ] 🛑 **R1.2** Passive `Cell(u32)` component (placement via `derive_seed`, zero new draws) + **per-cell**
    soil_factor; offspring inherit the **sampled parent's** cell. Spatial selection on a global pool. ADR-005 change.
  - [ ] 🛑 **R1.3** **Local** per-cell Wright-Fisher + dispersal (define empty-cell / deme-size rules; pick
    grid/N so patterns are signal not drift). Largest ADR-005 rewrite — the target model.
  - [ ] 🛑 **R1.4** Dynamic soil (pH/nutrient dynamics; zero-RNG or after-selection in the schedule) +
    Stage-5 LLM `EnvironmentModifier` admission behind the trait — the Track-B payoff.
- [ ] 🛑 **R2 — Environment parameters / climate (core).** Global + time-varying knobs (seasonal moisture,
  temperature…) layered on R1's static soil via deterministic schedules; makes runs dynamic over time.
  *Depends:* R1.
- [ ] 🛑 **R3 — Multi-species core (KEYSTONE).** The headline: multiple species, each with its own genome +
  phenotype, coexisting in one world with **inter-species interaction** (start: competition for shared
  soil/resources via local fitness; later: trophic/predation). A big change to the single-`GenomeRes` model,
  selection, the snapshot (per-species channels), and the action space (the operator picks *which species* to
  edit — inv #6). *Depends:* R1 (shared substrate). *Invariants:* #2/#3/#6, perf. Largest core epic — its own
  design workflow + sign-off.
- [ ] 🛑 **R4 — Ecosystem editor + scenario load/save.** Define a scenario (species roster + genomes, terrain,
  env params, master seed) as a **deterministic, replayable** serialized file (RON/JSON), with a Godot editor
  UI to compose / save / load / launch runs — the "sandbox" surface. *Depends:* R1–R3 (the things being
  edited). Reuses the `seed.json` + `actions.ndjson` replay contract (SPEC §5/§6). Core scenario serialization
  + renderer editor UI (renderer stays read-only re: biology — it edits *scenario config*, not genomes-in-GDScript).
- [ ] **R5 — Manual intervention + injection timeline.** Interactive CRISPR edits applied at a chosen time
  (later: place/species) from the UI, driving the core via the existing gym `Action::ApplyEdit`; a **timeline
  widget** visualizing *when* injections happened + their downstream effect (population / allele_freq / trait
  deltas), built on `actions.ndjson` + per-gen stats. *Depends:* R6 for real-time (works on replay otherwise).
  *Invariants:* #2 (renderer **requests** an action; the core applies it — no biology in GDScript), #6.
- [ ] 🛑 **R6 — Endless / open-ended run (core + harness + renderer).** Replace fixed-N runs with an unbounded,
  streamable sim: the core runs open-ended, snapshots **stream** to disk (ring buffer / append), and the
  renderer plays live with pause / resume / scrub; determinism preserved via the seeded stream. Enables R5
  real-time intervention + a living sandbox. *Invariants:* #3 (determinism over an unbounded run), #4.
- [ ] **R7 — UI control panel + sandbox UX (renderer).** Incremental renderer UX toward the sandbox: species
  roster panel, scenario load/save buttons, the R5 injection timeline, environment/terrain inspectors (read
  R1/R2 channels), richer detail panels (extend the ontology surface). Read-only presentation (inv #2); pairs
  with R3–R5 on the normal loop.
- [ ] **R8 — Isometric trait-driven sprites (renderer).** Generate isometric organism/plant sprites in the
  ecosystem view reflecting the (species) trait vector + local terrain, instead of dots (the "do budoucna"
  idea). Read-only presentation from exported traits + soil. *Depends:* R1 (terrain) + R3 (per-species traits).
- [ ] **Stage 5 — Ontology + LLM modifiers (S5.1–S5.4, above)** connects here as the **env-modifier engine**:
  LLM/ontology-defined functions act on the R1/R2 soil/environment substrate to modify fitness, behind the
  invariant-#5 trait boundary, validated against the SO/GO graph before admission (SPEC §4). The just-shipped
  detail-panel ontology surface is the UI hook.

**Suggested sequence:** R1 (in flight) → Stage 5 graph/validation (parallel, renderer-light) → R2 → R6
(unblocks live intervention) → R5 → R3 (keystone) → R4 (editor) → R7/R8 (UX/visual, ongoing). Re-plan after R1
sign-off; each core epic gets its own design workflow + ADR before code.

---

## FOLLOW-UPS / TECH DEBT (non-blocking; pick up when convenient)
- [ ] **F1** sim-core selection write-back: replace the per-generation `BTreeMap<u32,f64>` with a `Vec` indexed
  by contiguous `OrgId` (O(N) vs O(N log N) + allocation). Would lift the Stage 1 perf baseline (ADR-005).
- [ ] **F2** sim-core `metabolism`: it draws from `SimRng` *inside* `Query<&mut Energy>` iteration — safe today
  (single archetype, no structural changes) but harden (snapshot to ordered Vec, or draw outside the query) if
  any system later adds/removes components per-organism. (Reviewer note, S1.5.)
- [ ] **F3** Render the **edit-penetrance** data layer (SPEC §W10 lists it as a 4th channel). Needs sim-core to
  add an `edit_penetrance` channel to `GridSnapshot` (derived, read-only, off the hash path like the others)
  and bump `CHANNEL_COUNT`/the `"GSS1"` layout; the shader already supports selecting by `layer` index. (S4.4.)

## DONE
- **S0** — Stage 0 headless deterministic core skeleton. DoD met: `cargo run -p harness -- --seed 42
  --runs 1 --generations 200` works; `--runs 8` produces 8 distinct-seed runs; determinism gate GREEN
  (`3393427b072eb803`, superseded by `fde0e0b6…` after S1.5); baseline bench recorded. See CHANGELOG +
  DECISIONS (ADR-001, ADR-002).
- **Stage 1 (S1.1–S1.5)** — CRISPR mechanic complete: Cas-variant table (S1.1), PAM finding via rust-bio
  (S1.2), pluggable Score traits (S1.3), gated edit application (S1.4), GP map + Wright-Fisher selection
  (S1.5). ADR-003/004/005. Every slice ran through the multi-agent loop (implementer → tools/gate.sh →
  reviewer APPROVE) and was committed individually. Determinism hash now `fde0e0b61b9e23e6`.
- **Stage 2 (S2.1–S2.5)** — Genetics realism: SLiM v5.2 built (subprocess-only), `oracle-slim` driver (zero
  deps), tskit `.trees` analysis, golden oracle gate (pins genetics to v5.2), license gate. Invariant #1 clean.
- **Stage 3 (S3.1–S3.4)** — AI harness: gym-like `reset/step/seed` env (species-granular actions, inv. #6),
  bit-identical replay logs (seed.json + actions.ndjson), parallel batch runner + columnar Parquet stats.
- **Stage 4 (S4.1–S4.5)** — Godot UI (LAST): 4.7 skeleton (S4.1), read-only snapshot reader + headless UI
  gate (S4.2), 2D ecosystem view playing a live run — terrain TileMap + organism dots + data overlay + HUD
  (S4.3), data-layer shaders + zoom scopes (S4.4), L-system plant morphology from core-exported trait vectors
  + UI control bar (S4.5). ADR-006/007. **Zero biology in GDScript** throughout (inv. #2); every UI feature
  gated headless via `--check`/`--snap` (inv. #4); determinism hash unchanged across all of Stage 4.
- **Post-Stage-4 renderer round** (multi-agent designed + adversarially vetted; A+C + mouse): visual polish
  (inferno overlay, teardrop leaves + flowers + ground/shadow, grass blades, edge vignette), specimen UX
  (selector + 5-trait readout with delta-vs-baseline + focus emphasis), ecosystem control bar (speed slider,
  scope buttons, generation scrubber), and mouse controls (drag-pan, hover tooltip, click detail). Track-B
  prep: `harness --specimens` now also exports the species genome's **SO/GO ontology tags**, surfaced in the
  click-detail panel. All read-only (inv. #2); determinism hash unchanged; full gate green per slice.
