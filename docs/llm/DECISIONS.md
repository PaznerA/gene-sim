# DECISIONS — ADR log & pinned versions

> Append-only. Each ADR: Context · Decision · Consequences. Load-bearing choices only.
> Invariant #7 (SPEC §2.1.7): SLiM tag, Godot minor, Bevy version, Rust toolchain — all pinned here.
> Cross-version reproducibility is **not** guaranteed; the determinism gate runs on one pinned platform/build.

## Pinned versions (the reproducibility contract — SPEC §2.1.7, §6)

| Component | Pinned version | Status | Notes |
|---|---|---|---|
| Reference platform | macOS (Darwin 25.3) / Apple Silicon **M4 Max**, 14 cores | active | The single determinism reference platform (SPEC §6). |
| Rust toolchain | **stable 1.96.0** (`ac68faa20`, 2026-05-25) | installed | Native aarch64-apple-darwin. `rust-toolchain.toml` pins it in-repo. |
| `bevy_ecs` | **0.19.0** (locked in Cargo.lock) | installed (Stage 0) | ECS only, **no render plugins** (SPEC §2.2). |
| `rand_chacha` | **0.10.0** (`ChaCha8Rng`; uses `rand_core` 0.10.1) | installed (Stage 0) | The one portable, reproducible RNG (invariant #3). Runtime tree uses its re-exported `rand_core`. |
| `serde` (+derive) | **1** (locked 1.0.228) | installed (Stage 1, S1.1) | (De)serialization for the Cas-variant data table. MIT/Apache-2.0. |
| `serde_json` | **1** (locked 1.0.150) | installed (Stage 3, S3.2) | JSON for replay logs — seed.json + actions.ndjson (SPEC §5). MIT/Apache-2.0. |
| `ron` | **0.12** (locked 0.12.1) | installed (Stage 1, S1.1) | Rusty Object Notation — git-friendly config/data (SPEC §5). MIT/Apache-2.0. See ADR-003. |
| `bio` (rust-bio) | **4.0** (locked 4.0.0) | installed (Stage 1, S1.2) | Sequence ops / PAM finding — the SPEC §2.2 chosen lib. MIT. See ADR-004. |
| SLiM | **tag `v5.2`** (commit `f11de0d`) | **installed (Stage 2, S2.1)** | Built from source via `tools/install_slim.sh` → `slim -version` = "SLiM version 5.2". GPL-3 — **subprocess only, never linked** (inv. #1). Binary at `~/.local/bin/slim`. |
| Crisflash | latest release | NOT yet built — Stage 2+ | Off-target oracle (CPU). Optional realism. |
| crisprScore | (Bioconductor) | optional — not on critical path | On-target realism only (SPEC §2.2). |
| Python (analysis) | **3.13.14** | installed (Stage 2, S2.3) | For the `.trees` analysis scripts; in the gitignored `.venv` (`scripts/requirements.txt`). |
| `tskit` / `pyslim` / `numpy` | **1.0.3 / 1.1.1 / 2.4.6** | installed (Stage 2, S2.3) | `.trees` read-back + stats. MIT / MIT / BSD. |
| `msprime` | **1.4.2** | installed, optional | **GPL-3** — used ONLY by standalone analysis scripts (separate process, never linked); same pattern as the SLiM subprocess, so invariant #1 is unaffected. Optional (neutral-mutation overlay, S2.4). |
| `pyarrow` | **24.0.0** | installed (Stage 3, S3.3) | Apache-2.0 — columnar Parquet for batch analytics (SPEC §5). Analysis-only (separate process), never linked. |
| Godot | **4.6** pin (ADR-010); **dev on 4.7 via gdext forward-compat** | confirmed working on 4.7 | Thin 2D UI, built LAST (inv. #4); `tools/install_godot.sh` (GODOT_PIN 4.6). The `godot-sim` cdylib targets gdext api-4-6 and **loads under the installed 4.7** (runtime ≥ API) — verified by `tools/check_livesim.sh` (init line `API v4.6, runtime v4.7`). No separate 4.6 install needed. |
| godot-rust (gdext) | **0.5.3** (locked), `api-4-6`, edition 2024 | installed (P1b) | `crates/godot-sim` cdylib `LiveSim` binding over `harness`/`sim-core` (ADR-010). MPL-2.0 (no GPL — separate link unit, inv #1). Workspace-detached; built via `tools/check_livesim.sh`. |

> Rows marked "NOT yet …" record the **intended** pin; the exact tag/minor is confirmed and the Status
> flipped in the slice that installs the tool (S2.1 for SLiM, S4.1 for Godot). Bevy/RNG/Rust rows below
> reflect what is actually installed and locked in `Cargo.lock`.

---

## ADR-001 — Native macOS Apple-Silicon toolchain; SLiM-from-source; Crisflash off-target oracle

- **Date:** 2026-06-19
- **Status:** Accepted
- **Stage:** 0 (toolchain baseline; binds choices that surface at Stages 2 & 4)

### Context
The reference/build machine is a Mac Studio **M4 Max** (Apple Silicon, arm64). The PoC's determinism
contract is *same source build + same platform + same seed* (SPEC §6) — so we fix one platform and run the
whole toolchain **natively** on it (no Rosetta, no VM for the core). Two external science oracles have
platform-sensitive choices that must be locked now so later stages don't drift:
- **SLiM** is GPL-3 and must never be linked (invariant #1) and is version-scoped for reproducibility
  (invariant #7, SPEC §12).
- **Off-target scoring**: Cas-OFFinder is OpenCL-based, and **Apple has deprecated OpenCL** on macOS
  (SPEC §12; research §2/§5) — making it a poor native fit on Apple Silicon.

### Decision
1. **Run the toolchain natively on macOS / Apple Silicon (M4 Max).** The Rust core, harness, benches, and
   the determinism gate all build and run native aarch64. This machine is *the* determinism reference (SPEC §6).
2. **Build SLiM from source at a pinned git tag** (`tools/install_slim.sh`, SPEC §W2) — pinned **`v5.2`**
   (latest stable v5.x; confirm `slim -version` when built in S2.1). Invoked **only as a CLI subprocess**
   from `crates/oracle-slim`; never linked (invariant #1).
3. **Off-target oracle = Crisflash (C, CPU)**, **not** Cas-OFFinder — because Apple deprecated OpenCL.
   (Cas-OFFinder remains a fallback only inside a Linux container, off the native path.)
4. **crisprScore (on-target realism) is optional** and off the critical path (SPEC §2.2); Stage 1 ships an
   in-core heuristic on-target score and a naive in-core off-target count — zero external deps.
5. **Pin every version** in the table above. Confirmed-installed at Stage 0: **Rust stable 1.96.0**,
   **`bevy_ecs` 0.19.0**, **`rand_chacha` 0.10.0** (locked in `Cargo.lock`). Deferred-but-pinned:
   **SLiM `v5.2`** (Stage 2), **Godot 4.x exact minor** (Stage 4), **Crisflash** (Stage 2+).

### Consequences
- **+** Maximum native performance on M4 Max; one clean determinism reference platform.
- **+** GPL-3 stays at the process boundary → licensing freedom for a future closed/commercial release preserved.
- **+** No OpenCL dependency on the off-target path → fewer Apple-Silicon footguns.
- **−** SLiM-from-source adds a CMake build step (and a pinned-tag maintenance burden); conda SLiM is the
  quicker, less-reproducible escape hatch (SPEC §W2) but is not the pinned path.
- **−** Crisflash and crisprScore each need their own build/runtime; both are deferred to Stage 2+ so no
  slice before then blocks on a heavyweight dependency (SPEC §0.5).
- **−** Cross-platform bitwise determinism remains out of scope (SPEC §6, §12); the gate is single-platform.

---

## ADR-002 — Determinism strategy for the headless tick loop (Stage 0)

- **Date:** 2026-06-19
- **Status:** Accepted
- **Stage:** 0 (slice S0)

### Context
Invariant #3 (SPEC §2.1.3, §6) requires that the same master seed + same build + same platform produce
bit-identical output. Bevy's default parallel system scheduler and any `HashMap` iteration in sim logic
would break this.

### Decision
- One master seed per run; all sub-randomness derives from a single `rand_chacha::ChaCha8Rng` stored as a
  Bevy resource and threaded explicitly through systems. No thread-local/global RNG anywhere in sim logic.
- The tick loop uses a **fixed, explicit system execution order** (single-threaded schedule for sim logic)
  and a fixed number of generations; no wall-clock or frame-rate dependence.
- Sim logic uses ordered/indexed collections only — **no `HashMap` iteration** in any code that affects state.
- The harness reduces end-of-run state to a stable hash (ordered field hashing). `--hash-only` prints just
  that hash; `tools/check_determinism.sh` runs the same seed twice and asserts equality.

### Consequences
- **+** `tools/check_determinism.sh` is a meaningful, hard merge gate from Stage 0 onward.
- **+** The pattern (seeded ChaCha8 resource + explicit ordering + no HashMap) is reusable for every later
  stage; documented in SNIPPETS.md.
- **−** Sim logic forgoes Bevy's automatic parallelism (acceptable at PoC entity counts; revisit per SPEC §11
  if the perf gate forces it — parallelism would then need a deterministic reduction).

---

## ADR-003 — Cas-variant table format & pins: RON + serde (Stage 1, S1.1)

- **Date:** 2026-06-19
- **Status:** Accepted
- **Stage:** 1 (slice S1.1)

### Context
The Cas-variant table must be **data, not code** (SPEC §4) and live in `data/` as a git-friendly, human-
readable file (SPEC §5 names RON or JSON as the config/data format). Loading it needs a (de)serializer.

### Decision
- Encode the seed table as **RON** at `data/cas_variants.ron`; load via **`serde`** + **`ron`**. The default
  table is embedded with `include_str!` so it ships in the binary and tests are hermetic, while remaining an
  editable RON file. Variants are parsed into an ordered `Vec<CasVariant>` (load order preserved — inv. #3).
- **Pins:** `serde = "1"` (locked 1.0.228), **`ron = "0.12"`** (locked 0.12.1). Note: the implementer's brief
  suggested `ron = "0.8"`, but 0.8 is not in the registry; **0.12 is the current minor**, so it was pinned
  instead (consistent with the repo's caret style; exact versions locked in `Cargo.lock`). Both are MIT/Apache-2.0.

### Consequences
- **+** Table is editable data, diff-friendly, and validated at load (clean `LoadError` on malformed RON).
- **+** No GPL added — license gate stays green (inv. #1).
- **−** RON `0.x` is pre-1.0; a future minor bump could change syntax. Pinned + lock-file'd; re-confirm on bump.
- **−** Only an embedded/string loader exists today; a runtime path-based loader can be added when a stage
  needs user-supplied tables (noted by the reviewer; not required by S1.1).

---

## ADR-004 — rust-bio for sequence ops; IUPAC degeneracy in-house (Stage 1, S1.2)

- **Date:** 2026-06-19
- **Status:** Accepted
- **Stage:** 1 (slice S1.2)

### Context
PAM finding (SPEC §4) needs DNA sequence handling (reverse-complement, alphabet) and degenerate-motif
matching (IUPAC codes: N, R, Y, V, …). SPEC §2.2 pre-chose **rust-bio** (`bio`) for sequence ops / PAM
finding. SPEC §0.4 requires an ADR when any subsystem is built from scratch instead of reusing the chosen FOSS.

### Decision
- Use **`bio` (rust-bio), pinned `4.0` (locked 4.0.0, MIT)** for sequence primitives — specifically
  `bio::alphabets::dna::revcomp` for the reverse strand and DNA alphabet handling.
- Implement **IUPAC degenerate matching in-house** (a small `iupac_matches` table + a windowed PAM scan).
  This is CRISPR domain logic, **not** a reimplementation of a rust-bio component — rust-bio's pattern
  matchers are exact/approximate string search, not IUPAC-degenerate PAM semantics. So §0.4's "reinventing"
  clause does not apply; no human sign-off required.
- `find_pam_sites` returns an ordered, `(position, strand)`-sorted `Vec<PamSite>` (inv. #3). All coordinates
  are in the forward-sequence frame; the cut-site convention is documented on `PamSite`.

### Consequences
- **+** Reuses the SPEC-chosen lib for the hard sequence primitives; keeps the small, CRISPR-specific
  degeneracy logic transparent and testable (proptest: no false-positive PAM sites).
- **+** License stays clean — `bio`'s full tree (~160 crates) is permissive; GPL gate green.
- **−** `bio` is a large dependency (longer cold builds). Acceptable; it's the chosen lib and will be reused
  for off-target search / FM-index in later stages.

---

## ADR-005 — In-core selection model: constant-N Wright-Fisher with a fitness floor (Stage 1, S1.5)

- **Date:** 2026-06-19
- **Status:** Accepted
- **Stage:** 1 (slice S1.5)

### Context
S1.5 needs *selection that responds to a trait* in the headless core, while staying deterministic (inv. #3)
and not going extinct (so the harness keeps producing meaningful stats). It must remain a lightweight in-core
default — the rigorous pop-gen genetics is the Stage 2 SLiM oracle's job (SPEC §8), not this.

### Decision
- Each organism carries a per-individual `Genotype ∈ [0,1]` (seeded at spawn from the single `SimRng`).
- **Fitness** = `0.05 + base_growth · genotype`, where `base_growth` is the genome's `GrowthRate` trait from
  the `WeightedSumMap` GP map (expressed once into a `BaseGrowthRate` resource — genotype→phenotype stays in
  sim-core, inv. #2). The `0.05` floor keeps every weight strictly positive (no zero-weight degeneracy / div-by-zero).
- **Selection** = constant-population **Wright-Fisher** resampling: each generation draw exactly N offspring
  with probability ∝ fitness, via an ordered cumulative-weight table + binary search, consuming the threaded
  `SimRng` in `OrgId` order. Constant N ⇒ no extinction. `allele_freq` = mean `Genotype` over the id-sorted
  population (∈ [0,1]), reported in `RunStats` and folded into the determinism hash.

### Consequences
- **+** Deterministic, transparent, directional selection (the AC `allele_freq` shift); a clean stand-in until
  the SLiM oracle (Stage 2) carries real genetics, which can then be validated against this baseline behavior.
- **+** No extinction edge cases; the harness always yields stats.
- **−** It's a toy model (one scalar genotype, one trait → fitness); not population genetics. Intentional for the PoC.
- **−** The per-generation write-back uses a `BTreeMap` (O(N log N) + allocation) — correct and ordered, but a
  `Vec` indexed by contiguous `OrgId` would be O(N). Tracked as a perf follow-up (TASKS); drove the Stage 1 re-baseline.

---

## ADR-006 — Renderer architecture & verification harness (Stage 4, S4.2–S4.3)

- **Date:** 2026-06-20
- **Status:** Accepted
- **Stage:** 4 (slices S4.2, S4.3)

### Context
Stage 4 opens the renderer. It must stay a **thin, read-only** layer over the headless core (inv. #2: no
biology in GDScript) and remain testable under the headless gate (inv. #4) even though real rendering needs a
GPU. The first windowed run also surfaced a Godot headless trap.

### Decision
- **Snapshot bridge (S4.2):** sim-core emits a derived per-cell `GridSnapshot` (`std`-only `"GSS1"` binary,
  channels density/allele_freq/fitness) produced **off** the determinism-hash path (no RNG draw, no mutation),
  so emitting snapshots can never change the hash (inv. #3). GDScript only parses + draws these bytes.
- **No `class_name` globals in the renderer.** Godot only registers `class_name` globals during an editor
  *import* pass, so a fresh `godot --headless` run (CI / the gate) leaves a bare `Snapshot` identifier
  unresolved. The reader is loaded via `preload("res://snapshot.gd")` (+ a self-preload const for its own
  static factory) — resolved at parse time, needs no `.godot/` cache. **Rule for all renderer scripts.**
- **Scene built in code (S4.3).** `main.gd` constructs the node tree (terrain `TileMapLayer` from a
  procedurally-generated grass atlas, a per-cell data-overlay `Sprite2D`, an organism dot layer, a `Camera2D`,
  a HUD `CanvasLayer`) rather than authoring a fat `.tscn`. Keeps the read-only presentation logic in one
  reviewable place and avoids binary scene churn. Organism dot scatter is deterministic hash *jitter* —
  presentation only, **not** a spatial model (the core owns placement).
- **Dual verification harness.** Renderers can't be screenshot under headless (dummy GPU). So: (a) a headless
  `--check` smoke builds the full scene and prints `render scene OK` — wired into the gate
  (`tools/check_godot_snapshot.sh`, step 9/9, skip-if-absent) to catch GDScript parse/logic errors in CI; and
  (b) a windowed `--shot <png>` captures the real viewport for human/agent visual review. The gate enforces
  (a); (b) is for eyeballing the actual pixels.

### Consequences
- **+** UI is gated headless (inv. #4 holds for Stage 4); the `class_name`/headless regression can't recur.
- **+** Snapshots are provably hash-neutral; biology stays in the core (inv. #2/#3 intact).
- **−** The headless `--check` builds the scene but can't validate pixels — true visual checks need the
  windowed `--shot` (a human/agent step, not the automated gate). Acceptable: the gate proves *construction*,
  the screenshot proves *appearance*.
- **−** Scene-in-code means no editor-authored layout; fine for a thin PoC renderer, revisit if the UI grows.

---

## ADR-007 — L-system specimen morphology: trait export + renderer mapping (Stage 4, S4.5)

- **Date:** 2026-06-20
- **Status:** Accepted
- **Stage:** 4 (slice S4.5)

### Context
S4.5 must make a genome **edit visibly change plant morphology** while keeping **all biology in the core**
(inv. #2): the renderer may not compute genotype→phenotype. The species genome (and thus its phenotype) is
constant across a run — only an **edit** changes it — so the demo is baseline-vs-edited, not per-generation.

### Decision
- **Trait export, not genome export.** `harness --specimens <DIR>` writes `specimens.json`: the baseline
  species-genome **trait vector** plus one per fixed demo CRISPR edit. Each is expressed by the core's
  `WeightedSumMap` GP map through a **separate `GeneSimEnv`** (its own seeded RNG), so it never touches the
  hashed `run_headless` stream — exporting specimens cannot change the determinism hash (inv. #3). The
  renderer reads trait scalars; it never sees genome internals or runs the GP map.
- **Any edit outcome qualifies.** `apply_edit` mutates the genome on **both** Applied and Failed paths (a
  failed edit perturbs other loci), so every specimen's phenotype differs from baseline — the "an edit
  changes morphology" demo holds regardless of gate pass/fail.
- **trait→visual mapping is presentation.** `godot/lsystem.gd` is a parametric bracketed turtle L-system that
  draws from numeric params only. `main.gd::_plant_params_from_traits` maps each `[0,1]` trait to a visual
  param (growth→size/reach, reflectance→spread+leaf hue, drought→taper+tip colour, fecundity→leaf size,
  kill-switch→jitter). This is the renderer's job per SPEC ("L-system rule params"); the biology (genome→
  trait) already ran in the core. The intra-branch jitter is a deterministic hash, not a model.
- **UI controls change view state only.** The control bar (view toggle, play/pause, step, layer dropdown)
  and keys never synthesise a genome or compute traits — they pick *which exported data* to show.

### Consequences
- **+** The CRISPR mechanic's effect is visible end-to-end (edit → trait delta → plant shape) with biology
  confined to the core; the renderer stays a thin reader (inv. #2 holds for the richest UI feature).
- **+** Reproducible, hash-neutral export; gated headless via `--check` (builds the L-system) — inv. #3/#4.
- **−** Specimens are a fixed preset list (two demo edits), not interactive editing — applying an edit live
  would require the renderer to call the core. Deferred: a future harness/IPC hook could stream specimens on
  demand. For the PoC, pre-exported baseline-vs-edited is enough to demonstrate the mechanic.
- **−** Plant morphology is constant within a run (species genome is static); per-generation morphing would
  need per-organism genomes in the core (not modelled). Intentional for the PoC.

---

## ADR-008 — Terrain/soil substrate: hash-neutral static SoilField (roadmap R1.0)

- **Date:** 2026-06-20
- **Status:** Accepted
- **Stage:** Roadmap R1 (slice R1.0), multi-agent designed + adversarially vetted; human signed off.

### Context
The roadmap (TASKS §ROADMAP) wants a terrain/soil substrate that eventually drives **spatial** selection
(target: R1.3 local per-cell Wright-Fisher + dispersal) and is the substrate for Stage-5 LLM env-modifiers.
The design workflow surfaced three traps that force the **first** slice to be substrate-only: (1) the advertised
soil→DroughtTolerance coupling is currently **impossible** — `DroughtTolerance = 1.0·p2` maps to the killswitch
`Bool(false)` = 0.0 and collides with `KillSwitchLinkage` (gp.rs); (2) `check_determinism.sh` only compares
`run==run`, so a reproducible-but-*changed* hash passes silently; (3) a 4th+ snapshot channel can't ride the
current renderer (RGBF / `.rgb` / `--layer 0..3`) without real Stage-4 shader work, violating "Godot LAST".

### Decision
R1.0 ships the **substrate only, provably hash-neutral, no coupling**:
- `crates/sim-core/src/soil.rs`: a static `SoilField` (3 channels — moisture / nutrients / pH, each `[0,1]`),
  generated once in `Simulation::reset` from `derive_seed(seed, SOIL_STREAM_BASE + …)` — the stateless
  splitmix64 — drawing **zero** from the threaded `SimRng` and **never** folded into `hash_world`. Value-noise
  from a 5×5 control lattice, bilinearly interpolated (multiply-add only, no transcendentals → cross-platform).
- Exported as **3 new read-only snapshot channels**; `CHANNEL_COUNT` 3→6 and magic **GSS1→GSS2** (a bumped
  magic makes a stale 3-channel reader fail loudly, not silently). `godot/snapshot.gd` change is **parse-only**
  (reads + exposes the soil planes; the detail panel shows them) — **no** shader / overlay / `--layer` work.
- An `EnvironmentModifier` trait (invariant #5 seam) + in-core `LinearTraitMatchModifier` default are present
  but **UNWIRED** — selection coupling lands in R1.1+, Stage-5 admits validated LLM impls behind the same trait.
- A Rust test **pins the exact pre-soil hash literal** (`0xc530…7ab1`, seed = the harness run-0 derived seed)
  — since the literal was measured *before* soil existed, matching it on the with-soil build **proves** soil is
  hash-neutral (and guards the silent-change gap in `check_determinism.sh`).

### Consequences
- **+** Determinism intact (proven by the pinned literal); perf within criterion noise (no re-baseline — soil
  gen is O(cells) once per `reset`, off the hot selection loop); ADR-005 untouched; "Godot LAST" respected.
- **+** Clean substrate + invariant-#5 seam for the phased coupling (R1.1 global → R1.2 per-cell → R1.3 local)
  and Stage-5; the renderer already surfaces per-cell soil in the click-detail panel.
- **−** Soil does **nothing** to the sim yet (by design). The DroughtTolerance dead-trait must be fixed
  (R1.0a — chosen: **per-individual heritable**) before any coupling; spatial selection (R1.2+) is the real
  ADR-005 change, separately ADR'd.

### `derive_seed` stream registry (keep disjoint — inv #3)
- `1`, `2` — snapshot organism placement (`x`, `y`) — `Simulation::snapshot`.
- `[SOIL_STREAM_BASE, SOIL_STREAM_BASE + SOIL_CHANNELS·LATTICE²)` (base `0x0050_4F49_4C00_0000`) — soil control points.
- Future spatial phases (R1.2 Cell, R1.3 dispersal) must reserve new disjoint ranges here before use.

---

## ADR-009 — Per-individual heritable drought tolerance + global soil-coupled selection (R1.0a + R1.1)

- **Date:** 2026-06-20
- **Status:** Accepted
- **Stage:** Roadmap R1 (slices R1.0a + R1.1). Extends ADR-005.

### Context
R1.0 shipped the soil substrate but it was inert. To make terrain **shape evolution** (and unblock the dead
DroughtTolerance trait — gp.rs maps it to the killswitch `Bool(false)`), the human chose (b) **per-individual
heritable** drought tolerance. R1.1 then wires the soil into selection. The crux is doing this without breaking
ADR-005's constant-N Wright-Fisher or determinism (#3).

### Decision
- **R1.0a:** a per-organism `DroughtTol(f64)` ECS component — heritable standing variation in `[0,1]`, seeded
  once at spawn from the single `SimRng` (one extra draw per organism, in a fixed `genotype, energy, drought`
  order) and **inherited** (not resampled) from the fitness-sampled parent each generation. Folded into
  `hash_world` (it is per-individual state). It deliberately does **not** touch the species GP map — the
  species-level DroughtTolerance trait (used by the specimen view) is independent and stays as-is.
- **R1.1:** `selection()` weight becomes `fitness(base, genotype) × EnvironmentModifier::fitness_factor(soil,
  drought)`, using the in-core `LinearTraitMatchModifier` (a drought-tolerant individual is favoured on drier
  soil) fed the **field-wide mean** soil sample (a `MeanSoil` resource computed once per run — "global"
  coupling, the smallest real step on the spatiality spectrum). The factor is strictly positive (band
  `[0.5,1.5]`), so weights never zero → ADR-005's **constant-N, no-extinction** structure is preserved. The
  selection loop draws **exactly N** RNG words as before (offspring *inherit*, never resample drought), so the
  only stream shift came from R1.0a's spawn draw — determinism stays reproducible (new pinned hash literal).

### Consequences
- **+** Terrain now drives selection: a test proves the population's mean drought tolerance moves toward the
  terrain target `(1 − mean_moisture)`. The `EnvironmentModifier` seam (inv #5) is live and static-dispatched.
- **+** ADR-005 intact (constant-N, no extinction); determinism intact (pinned literal `8722…44aa`).
- **−** Perf: a per-parent modifier call in the hot loop → ~+6 % at 1 k entities (within noise at 10 k);
  re-baselined in-slice (see below).
- **−** Coupling is **global/non-spatial** (mean soil) — "weak-but-real". Spatial selection (a per-cell
  `soil_factor` via a `Cell` component, offspring inheriting the sampled parent's cell) is R1.2; full local
  Wright-Fisher + dispersal is R1.3 (the target), each a further ADR-005 change.

---

## ADR-010 — Live-sim driving via a gdext GDExtension; repin Godot 4.7→4.6 (roadmap R6/R5, P0 decision gate)

- **Date:** 2026-06-20
- **Status:** Accepted (human signed off)
- **Stage:** Roadmap R6/R5 (P0). Multi-agent designed + adversarially vetted. **Touches inv #2/#3/#4/#7.**

### Context
The gameplay batch needs a LIVE, continuous, interactively-editable sim (open-ended run + manual CRISPR
interventions). Today the renderer only replays offline snapshot files. A design workflow weighed (A) a Rust
GDExtension embedding sim-core, (B) an IPC/subprocess server, (C) file-tailing. The crux is *largely
pre-solved*: `sim-core::Simulation` is already stepwise/single-seeded/edit-able and `harness::GeneSimEnv` +
`replay.rs` already give a `reset/step/apply_edit/observe` surface with a `seed.json`+`actions.ndjson` replay
contract. Adversarial vetting found that **stable godot-rust (gdext) supports Godot api 4.2–4.6 only**, while
we pinned **4.7** — a stop-the-line pin conflict.

### Decision
- **Option A:** a new workspace crate `crates/godot-sim` (gdext **cdylib**) embedding `sim-core` (+ `harness`,
  `crispr`, `genome`) that registers ONE node `LiveSim` exposing `reset(seed)`, `step(n)`,
  `apply_edit(cas,target,guide)`, `observe()`, `snapshot(w,h)->PackedByteArray` (GSS2 bytes — reuses the
  existing `snapshot.gd`/shader), `save_session(dir)`. GDScript only **calls** these → all biology stays in
  Rust (inv #2 safe; the violation would be biology *written in* GDScript). Reject B (most build cost, no
  benefit) and C (worst fit for interactive edits). gdext is **MPL-2.0** — `scripts/check_license.sh` already
  anticipates it; the cdylib is a separate link unit so inv #1 (GPL boundary) is untouched.
- **Repin Godot 4.7 → 4.6** (the human's choice over forward-compat or a git-pinned gdext rev): build the
  cdylib against gdext **api-4-6** and run the project in Godot **4.6** — a clean, *released* gdext target
  (preserves inv #7 pin discipline). The renderer uses no 4.7-specific API, so this is safe; the GDScript
  gate runs on whatever `godot` is installed. `tools/install_godot.sh` GODOT_PIN→4.6; `project.godot`
  `config/features` migrates to "4.6" when first opened in 4.6.
- **Determinism (inv #3):** proven by **replay-equality**, NOT a second cdylib hash literal (avoid a second
  platform-pinned hash across link units). `LiveSim` journals `reset`+`Advance(n)`+`ApplyEdit` in call order;
  `save_session` writes `seed.json`+`actions.ndjson` via `record_episode`'s shape; `harness::replay`
  reproduces the live session's hash bit-identically. The gate-blocking proof is a **pure-Rust** replay test
  (no Godot needed); the gdext/Godot smoke is skip-if-absent + skip-if-dylib-unbuilt.
- **`run_stats()` impurity fix (must-do, inv #3):** `hash_world` draws a final `rng.next_u64()`, so a mid-run
  `save_session` would desync replay. Mitigation: **clone the `ChaCha8Rng`, fold for the hash, discard** — a
  hash read never advances the single stream.
- **Cadence (inv #3):** the open-ended play loop advances a **fixed integer N generations/tick** (speed =
  integer multiplier), NEVER delta/wall-clock, so the journaled `Advance(n)` sum reproduces.
- **Sessions auto-journal** to `data/runs/<id>/` for the reproducibility story.

### Consequences
- **+** Live/continuous + interactive edits with full determinism via the existing replay contract; near-total
  renderer reuse (snapshot bytes); the hard part (stepwise edit-able core) already exists + is headless-gated.
- **+** Renderer-only work (timeline markers, isometric, sprites) is hash-neutral and rides the normal loop
  *while* the live-sim crate is built — visible gameplay unblocked early.
- **−** Repinning to Godot 4.6 means installing 4.6 before building/using the live-sim crate (P1+); the
  renderer keeps working on the currently-installed 4.7 in the meantime.
- **−** Multi-species (R3) is sequenced AFTER the live seam + intervention (it rewrites the same `selection()`
  loop as R1.2/R1.3 spatial — doing it first means rewriting selection twice); it gets its own design workflow.

---

## ADR-011 — Real spatial dynamics: per-organism Position, inherited dispersal, region-scoped CRISPR edit, gamification (roadmap R1.2/R1.3 + R5)

**Status: COMPLETE.** All slices landed gate-green: S-A (Position + off-stream placement, RE-PIN #1
`3ba0…82ba`), S-B (inherited dispersal, RE-PIN #2 `0413…ce77`), S-C (snapshot by real position, hash-neutral),
S-D (region edit in core/crispr/harness), S-E (gdext binding), S-F (brush UI), S-G (local soil coupling
RE-PIN #3 `c01e…e40e` + the mission/edit-budget game loop). Three deliberate re-pins, all ledgered in the
`determinism_hash_is_pinned` comment.

### Context
The grid was **visualization only**: `Simulation::snapshot` placed each organism into a cell by a pure function
of its `OrgId` (`derive_seed(id,1/2) % dims`) — "not biology" (ADR-006/008). Organisms had **no position**, so a
*selective* intervention ("a brush of adjustable size — modify only part of the population in a region, not the
whole species") had no spatial substrate to act on, and `apply_edit` only shifts the SPECIES fitness landscape
(via `BaseGrowthRate`), so the population evolves toward an edit over generations — there is no per-region hook.
The human asked for real spatial work + a selective brush + deeper gamification. Designed via a multi-agent
design workflow (understand → design → ADR/plan).

### Decision
Promote the visualization layout into **real per-organism spatial biology** in `sim-core`, sliced so each
determinism re-pin is isolated, and carve the sub-species region edit behind an explicit invariant-#6 ruling.
- **S-A (done):** add a `Position{x,y}` component on a canonical `WORLD_DIMS` grid (= `soil::SOIL_DIMS` 32×32,
  1:1 with soil). Initial placement is **off the `SimRng` stream** via a new disjoint `derive_seed` family
  `PLACEMENT_STREAM_BASE = 0x0050_4C41_4300_0000` ("PLAC"), so the spawn draw order is byte-identical and the
  ONLY hash delta is `Position` entering `hash_world`. **RE-PIN #1:** `8722…44aa` → `3ba0…82ba`.
- **S-B:** Wright-Fisher offspring INHERIT the sampled parent's position + one bounded deterministic dispersal
  step (exactly one `next_u64`/offspring) → lineages cluster into emergent regions/clines. **RE-PIN #2.**
- **S-C:** `snapshot` aggregates by REAL `Position` (resampled onto the render grid), retiring the OrgId-hash
  layout. Hash-neutral (snapshot draws no RNG).
- **S-D:** region-scoped edit — `Region::Disc{cx,cy,radius}` + `organisms_in_region` (OrgId-sorted, no HashMap);
  run the SAME crispr PAM/score gate against the species locus but return a **signed Genotype delta** applied to
  every in-region organism (per-individual perturbation, NOT a region-local genome). The gate RNG is drawn
  **once** per brush (region-size-independent), via the `with_genome_and_rng` replace/restore dance. New
  `Action::ApplyEditRegion(EditAction, Region)` carries **no organism handle** (Region is a cell descriptor).
  Hash-neutral on the no-edit pinned run.
- **S-E/S-F:** gdext `apply_edit_region` binding + renderer **brush UI** (adjustable radius, paint on the
  ortho/iso map; renderer requests, core computes membership + biology — inv #2).
- **S-G:** optional local-cell soil coupling (behind the inv-#5 `EnvironmentModifier` seam, `sample_at(x,y)`)
  + first gamification: an **objective mission + edit budget** (e.g. "establish a drought-tolerant population in
  the arid region within N generations"), score = efficiency. Conditional **RE-PIN #3** if the pinned config
  ships local coupling on.

### Invariant #6 ruling (human-adjudicated)
A region-scoped edit is **sub-species** granular. The human ruled it **allowed, AND accessible to AI policies**
(not human-operator-only). Guard rails preserved so #6's core ("organisms are ECS entities, not RL agents; no
per-organism targeting") still holds: the `Region` descriptor targets **cells, not entities** (no organism
handle — `action_space_is_species_granular` is updated to assert `Region` carries none), the gate yields **one
outcome per brush regardless of contained count**, and a **minimum radius** prevents a 1-cell brush from being
de-facto per-organism. This is a deliberate broadening of #6 from "species-only" to "species-or-cell-region".

### Consequences
- **+** Emergent spatial structure becomes real + visible; the snapshot flips from derived-viz to a read-only
  projection of real biology; a brush has a real `Position` to scope to; local-soil coupling + spatial
  gamification become expressible. Every slice is headless-testable (inv #4) and deterministic (inv #3).
- **−** Multi-part core change (component + spawn + selection + snapshot + harness/binding/UI) → **three
  isolated determinism re-pins** (S-A substrate, S-B dispersal, S-G optional coupling), each in its own commit
  with a ledger line; forgetting to fold `Position` into `hash_world` would be a silent determinism hole.
- **Re-pin procedure (each):** implement → `cargo test -p sim-core determinism_hash_is_pinned -- --nocapture`
  prints the new actual → replace the literal + append a dated ledger note in the test comment → `tools/gate.sh`
  green. Defaults adopted: off-stream placement; 9-cell single-draw dispersal, clamp at edges; `WORLD_DIMS` =
  `SOIL_DIMS`; uniform single delta; failed region edit = no-op + reason.

---

## ADR-012 — Climate environment (lat/lon/season/temperature) + pre-sim main menu (Phase E)

### Decision
Give a run a player-set **climate** that shapes selection, plus a **main menu** to configure it — sliced so the
only invariant-touching step (climate→selection coupling) is a single ledgered determinism re-pin.
- **E1 (done):** `climate::EnvParams { lat, lon, avg_temp, season }` (Default = neutral temperate) + a
  `ClimateField` derived from them as a PURE multiply/add/clamp/`match` function — **NO sin/cos/acos** (libm
  differs across platforms → would break inv #3; soil.rs precedent). Built in a new `Simulation::reset_with_env`
  next to `SoilField`, off the seed (zero `SimRng` draws); `reset(config)` delegates with the default env so all
  32 `SimConfig` literals + the pinned config stay byte-identical. Inserted as `ClimateFieldRes`, NOT yet read →
  **hash-neutral** (the unchanged pinned literal `0xc01e…e40e` is the proof, exactly as soil R1.0 proved).
  `CLIM_STREAM_BASE = 0x0043_4C49_4D00_0000` reserved for future per-cell variation.
- **E2:** thin gdext `LiveSim.set_environment(lat,lon,temp,season)` + `harness::GeneSimEnv` threading +
  `replay::EnvConfig`/`SeedJson` persistence (so save/load + replay reproduce the env). Hash-neutral.
- **E3 (🛑 RE-PIN, done):** new heritable per-individual `ThermalTol` (template = `DroughtTol`; spawn draw order
  genotype,energy,drought,thermal; inherited; folded into `hash_world`) + a `ClimateModifier`/`TemperatureMatch
  Modifier` (own inv-#5 seam alongside soil's `EnvironmentModifier`), multiplying a strictly-positive factor
  (band [0.5,1.5]) into the selection weight (GLOBAL coupling first). **Refinement:** the thermal pressure scales
  with climate EXTREMITY — a TEMPERATE world (temperature ≈ 0.5, the neutral default) is selection-neutral on
  `ThermalTol`, so the default/pinned config's re-pin captures ONLY the structural change (the 4th spawn draw +
  `ThermalTol` in the hash), the soil signal stays undisturbed, and only player-set hot/cold extremes adapt the
  trait. ONE deliberate re-pin: `…c01e…e40e → 0x9fad_2c9f_d298_f73a`, ledgered in `determinism_hash_is_pinned`.
- **E4:** a main-menu Godot overlay (`main_menu.gd`, preload, no class_name) shown before `_setup_live` in the
  windowed `--live` path: seed (random|fixed), lat/lon/temp/season, entity count, a PREVIEW row computed by the
  CORE (`observe()`, not GDScript — inv #2), Start → `set_environment` + reset via the existing `_do_reset`
  in-place reseed (no relaunch). Headless/`--check`/`--shot` early-return before the menu and feed the SAME
  setters from CLI flags (`--lat/--lon/--temp/--season/--entities`) → byte-identical to going through the menu.

### Consequences
+ Runs become meaningful beyond a bare seed; climate becomes a selection lever (and visible variety once it
  couples). + Off-stream field keeps E1/E2 hash-neutral; E3 is one isolated, ledgered re-pin. + No transcendentals
  → cross-platform determinism preserved. + Menu is pure config (renderer read-only, inv #2). − One re-pin; the
  env must be journaled for save/load replay. Defaults (human-approved): global coupling first, climate-ON in the
  pinned config, season 4-enum, transcendental-free LUT/polynomial, `TemperatureMatchModifier` on the existing seam.

---

## ADR-013 — Ecology substrate: a conserved fixed-point "joule" economy (CHEMOSTAT-J), the foundational epic

**Status: ACCEPTED (human sign-off 2026-06-21) — IN PROGRESS.** Supersedes **ADR-005** (constant-N /
no-extinction). Re-grounds the R3 multi-species, Rel relations, and Phase-T trait DRAFTS (now folded in as
phases, not separate ADRs — see `docs/llm/proposals/`). Designed by the bold/anti-safe `ecology-substrate-design`
workflow (18 agents) + adversarial pressure-test; full draft in `docs/llm/proposals/ecology-substrate-draft.md`.
This is a **stop-the-line, multi-week, multi-crate rewrite with 6+ deliberate re-pins** — the human explicitly
rejected the safe incremental path ("be on the edge") and approved the honest cost.

### Context
Today selection (`crates/sim-core/src/lib.rs:218`) is an abstract constant-N Wright-Fisher pool that multiplies
a per-individual `fitness` weight by `[0.5,1.5]` static-field match factors. Organisms never INTERACT — they
react independently to frozen `SoilField`/`ClimateField`; `Energy` is decorative; `gp.rs` expresses 5 standalone
scalars (3 dead) with no trade-offs and no trophic role; constant-N (ADR-005) makes extinction impossible. That
is the shortcut. The user mandates a foundation organisms genuinely interact THROUGH.

### Decision
Adopt **CHEMOSTAT-J**: one conserved fixed-point energy/mass currency — the `i64` "joule" `J` (the unit IS the
quantum, no float scale) — as the substrate spine. Every load-bearing quantity (per-cell resource pools,
per-organism stores, biomass, trophic transfers, chemical concentrations, reproduction endowments) is `J` over a
global LEDGER conserved exactly modulo **three named, audited taps**: INFLUX (solar minted/tick), RESPIRATION/LOSS
(maintenance + trophic-efficiency dissipation), OVERFLOW (the explicit sink for cap-saturation, so no quantum is
ever silently destroyed). **"Fitness" is deleted as a stored input and re-emerges only as a MEASUREMENT** (realized
lineage net-J). The four pillars are four classes of `J` transfer over the one ledger:
1. resource/metabolic pools (dynamic, depletable, regenerating); 2. genome→**allocation budget** `[u16;5]` summing
to 1000 permille + trophic role (autotroph/heterotroph/mixotroph/decomposer); 3. trophic web (energy transfers →
emergent `FlowMatrix` = relations); 4. chemical/signal diffusion field.

`selection()`, `fitness()`, the `[0.5,1.5]` band + `0.05` floor, the no-op `metabolism()`, the 5-scalar
`WeightedSumMap`, and `unit_f64` are DELETED from the sim path, replaced by a fixed-order pipeline:
`influx → diffuse/decay chem → emit → metabolism(uptake/convert) → trophic_transfer → maintenance →
reproduce_or_die → measure_relations`. **Population becomes a free variable; extinction is permitted and desired.**

**Human-approved keystone sign-offs (2026-06-21):**
- **Full commit** to CHEMOSTAT-J as the foundation, accepting the multi-week / 6+ re-pin / red-for-weeks cost.
- **Extinction approved** — supersede ADR-005, delete constant-N + the positivity band/floor (the irreversible
  policy break, gated at phase F3).
- **Cross-platform determinism gate** — stand up an **x86_64 + aarch64 CI matrix as a HARD gate before F3**
  (today's single-target CI silently blesses `f64` divergence; the "determinism as a property of the `i64` type"
  thesis must be proven on two arches before selection becomes resource-driven).
- Recommended defaults adopted: this is **ADR-013** (next free accepted number; the 013/014/015 draft *numbers*
  are retired, their content re-grounded as F-phases); genome `f64` stays ON DISK and is converted to integer at
  expression via a single audited chokepoint (`fixed::to_unit_u16`); trophic contention resolves against a
  **frozen start-of-tick prey snapshot**; start with a **minimal resource-channel inventory** and grow it.

### Determinism contract (invariant #3, hardened by the adversarial pass)
All pools/metabolism/diffusion/transfers are **integer / fixed-point**, ordered, bit-reproducible. One canonical
det-rounding module — **`crates/sim-core/src/fixed.rs` (phase F-1, LANDED)** — owns every division as
largest-remainder apportionment (floor + leftover to largest remainder, **ties to the lowest index**),
**conserving the total exactly**; it is reused by the budget simplex, diffusion remainders, and trophic division.
A semantic **`ledger_closes`** invariant (Σ all `J` == initial + influx − respired − overflow each tick) is a gate
STRONGER than the bit-hash. Every order-dependent pass collects into a Vec sorted by `(cell, SpeciesId, OrgId)`
before iterating (never `HashMap`/Query order). Each structural phase deliberately re-pins
`determinism_hash_is_pinned` (currently `0x9fad_2c9f_d298_f73a`) per the ADR-011 procedure.

### Epic phases (10; see the proposal for full per-phase detail)
`F-1` fixed-point apportionment contract (`fixed.rs`) — **LANDED, hash-neutral** ·
`F0a` Ledger + `ledger_closes` scaffolding (hash-neutral) · `F0b` `f64→i64` type migration (re-pin) ·
`F1` dynamic resource pools, off-stream (near hash-neutral) · `F2` genome→Strategy allocation budget (re-pin) ·
**`F3` 🛑 real metabolism + emergent births/deaths — breaks ADR-005 (re-pin; needs the multi-arch CI gate first)** ·
`F4` multi-species container (R3 spine) + trophic web + emergent `FlowMatrix` (Rel re-ground; re-pin) ·
`F5` chemical/signal diffusion field (re-pin) · `F6` emergent measurements + relations VIEW (mostly neutral) ·
`F7` Godot UI LAST (read-only render of pools/energy/FlowMatrix/chem; build-order Stage 4).

### Consequences
+ Organisms genuinely interact through one conserved economy; competition, extinction, and relations EMERGE.
+ Traits become budget allocations with real trade-offs (Phase T dissolves). + Determinism becomes a property of
the integer type, proven on two arches. − A long red period, 6+ re-pins, every replay/golden artifact regenerated
per re-pin, a GSS2→GSS3 snapshot break, and `ParamValue::Numeric` `f64` converted at a load-time chokepoint.
− ADR-005's no-extinction guarantee is gone (intended). The first phase (`fixed.rs`) is in and gate-green.

---

## Baseline benchmarks — perf threshold (SPEC §11, §10.7)

Reference platform: Apple M4 Max, native aarch64, `release` profile (`lto = "thin"`, `codegen-units = 1`).
Source: `cargo bench -p sim-core` (`crates/sim-core/benches/tick.rs`), run via `GATE_BENCH=1 tools/gate.sh`.
The perf gate (§10.7) fails on a regression **below the CURRENT baseline**. Re-baseline at each stage that
changes the hot path, in the same slice (this is anticipated — see the Stage 0 row note).

### Current baseline — R1.1 exit: Wright-Fisher selection + soil-coupled `EnvironmentModifier`
| Workload (entities × generations) | Median wall time | Throughput |
|---|---|---|
| 1 000 × 50  | **1.737 ms** | ~29 M organism-updates/s |
| 5 000 × 50  | **10.88 ms** | ~23 M organism-updates/s |
| 10 000 × 50 | **25.79 ms** | ~19 M organism-updates/s |

**Headline (current):** ~**19 M organism-updates/s** at 10 k entities — unchanged from the Stage 1 baseline.
R1.1 wired a per-parent `EnvironmentModifier::fitness_factor` call into the selection hot loop (soil → drought
coupling), which adds a few f64 ops per parent: a **fixed-ish overhead** that shows as ~+6 % on the smallest
workload (1 k) but is within noise at 10 k (the headline). Re-baselined in-slice per ADR-005's hot-path rule;
the prior Stage-1 row (1.645 / 10.48 / 25.97 ms) is superseded. Same cheap win still tracked (F1: `BTreeMap`
→ `Vec` write-back); the `EnvironmentModifier` is static-dispatched (a unit struct), so it already inlines.

### Historical — Stage 0 (slice S0): empty deterministic loop (no selection)
| 1 000 × 50 → **302.6 µs** · 5 000 × 50 → **1.438 ms** · 10 000 × 50 → **2.856 ms** (~175 M updates/s). |
| Superseded by the Stage 1 row above once real selection landed; kept for the record. |
