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
- `PLACEMENT_STREAM_BASE` `0x0050_4C41_4300_0000` ("PLAC") — initial organism placement (ADR-011 S-A).
- `SOIL_STREAM_BASE` `0x0050_4F49_4C00_0000` ("SOIL") — soil control points (ADR-008).
- `CLIM_STREAM_BASE` `0x0043_4C49_4D00_0000` ("CLIM") — climate field (ADR-012).
- `RESOURCE_STREAM_BASE` `0x0052_5352_4300_0000` ("RSRC") — resource pools light/nutrient/detritus (ADR-013 F1).
- `CHEM_STREAM_BASE` `0x0043_4845_4D00_0000` ("CHEM") — RESERVED for future abiotic/seeded chem-field variation
  (ADR-013 F5). **NOT yet derived** — F5 chem is ENDOGENOUS (organism-emitted, seeded all-zero), so it draws
  ZERO `derive_seed`/`SimRng`; the base is reserved here to keep the disjoint-stream discipline future-proof.
- `IMMG_STREAM_BASE` `0x0049_4D4D_4700_0000` ("IMMG") — contamination/immigration SCHEDULE (ADR-019 S2): the
  `ContainmentLevel` knob expands into a sorted `Vec` of journaled `RegionInoculate` events off this family
  (5 `derive_seed` words per event: species index, due_epoch, cx, cy, count). ZERO `SimRng` draws — the schedule
  never reorders the spawn stream. Empty (no words drawn) when the knob is Sealed (the default) → hash-neutral.
- Future spatial/substrate phases must reserve new disjoint ranges here before use.

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
workflow (18 agents) + adversarial pressure-test; the design draft is folded into this ADR + the F3/F4/F5 records.
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

### ADR-013 — landed re-pin ledger (implementation log, branch `auto/night-2026-06-21`, 2026-06-22)

The CHEMOSTAT-J epic has landed its keystone phases as deliberate, ledgered re-pins of
`determinism_hash_is_pinned`. Hash chain (each value is aarch64/Apple; **x86_64 portability is validated by the
multi-ISA CI matrix on push, BEFORE merge to `main`**):

- `0xf795_eac4_112f_acd5` (pre-F3 baseline)
- → **F3** `0x272a_9b4a_7023_0cf5` — real metabolism (PoolStock i64 uptake→convert→excrete, RNG-free) + energy-funded
  `reproduce_or_die` replacing constant-N Wright-Fisher (population emergent), Biomass+Age, carcass→detritus,
  ledger closes every tick, OrgId→u64, MaxPopulation guard.
- → **F4** `0x42fe_54f2_f6d8_360d` — obligate-loop machinery: free_nutrient INFLUX deleted (endogenous via
  decomposer mineralization), E. coli re-roled Decomposer (`niche.trophic_role`), emergent MEASURED FlowMatrix
  (S×S, row-sum==0) folded into the hash; read-only `LiveSim::flow_matrix()` export.
- → **F3.4** `0x4e4d_0520_722a_a069` — chemostat constant tuning for a living ecosystem. Decoupled per-cell SEED
  from CAP (`CELL_CAP_SCALE` ≫ `CELL_J_SCALE`) so solar flows continuously instead of spilling ~100% to overflow
  from tick 1; collapsed the per-org demand permille into one floored u128 product (the old chain of /1000 divides
  quadruple-floored a fresh org's demand to 0 → nothing ever reproduced → the gen-~240 wipeout was just AGE_MAX);
  rebalanced UPTAKE_VMAX/K_HALF, MAINTENANCE_BASE, REPRO_THRESHOLD, OFFSPRING_ENDOWMENT; added `LIEBIG_FLOOR`.

**F3.4 acceptance criterion + policy (the load-bearing decisions, per adversarial review):**
- **Acceptance = MULTI-SPECIES ROSTER coexistence, NOT single-species immortality.** The plant + E. coli
  (decomposer) roster settles to a stable coexistence attractor (plant ≈ 6600, decomposer ≈ 1450, flat band
  gen ~1750–6000, ≪ MAX_POPULATION=2M); the decomposer raises plant carrying capacity ~3.5×. A decomposer-less
  autotroph MONOCULTURE, by contrast, slowly runs down over tens of thousands of generations — and that is
  **correct emergent ecology** (no decomposer ⇒ the nutrient cycle never closes; carbon/N lock into detritus),
  NOT a tuning failure. It validates why F4's loop exists. At the pinned-hash config (50 gens) the world is
  healthy and growing (1000→2959), so the re-pin encodes a live ecosystem.
- **Policy: the decomposer loop is SOFT-MUTUALISTIC, not obligate-to-extinction** (`LIEBIG_FLOOR=350`): plants
  subsist on light alone down to the floor; the decomposer measurably *raises* carrying capacity rather than
  being strictly required for any survival. The test `f4_killing_the_decomposer_starves_the_plants` accordingly
  asserts the *relative* `plants_without < plants_with`, not extinction.
- **Open (continuation):** if a sustained non-zero single-species default is later wanted, it needs either a slow
  abiotic nutrient-weathering influx or an always-present decomposer in the default roster — deferred, tracked.

---

## ADR-018 — Data-licensing ruling: non-commercial BiGG accepted (gene-sim is not monetized)

- **Date:** 2026-06-22
- **Status:** Accepted (human ruling)
- **Stage:** ADR-017 S1 (un-gates S2/S3/S6 — the OVERSIGHT earned-edit loop)

### Context
The layered-E. coli OVERSIGHT loop (ADR-017, `docs/llm/proposals/ecoli-oversight-gameloop-draft.md`) needs the
BiGG `e_coli_core` / `iML1515` genome-scale metabolic models to bake the single-gene knockout (FBA) landscape that
makes an edit's impact real. BiGG models carry the UCSD academic **non-commercial** license. This stood as a
STOP-THE-LINE against invariant #1's *rationale* (keep licensing freedom for a future closed/commercial release).

### Decision
**gene-sim will NOT be monetized or commercially released** (human ruling). The BiGG non-commercial clause is
therefore acceptable — both for the shipped `data/species/ecoli.json` (whose sequence is public-domain NCBI CDS;
BiGG supplied only the b-number gene roster) and for baking the FBA KO landscape into a **frozen runtime table**
for the OVERSIGHT loop (ADR-017 S2). This un-gates S2 → S3 (`crates/oracle-fba`) → S6 (`EcoliEditModifier` wire).
- **Invariant #1's process boundary STAYS in force** as engineering hygiene, independent of the commercial driver:
  GPL tools remain subprocess-only, `scripts/check_license.sh` keeps gating the boundary crate list, and
  `crates/oracle-fba` quantizes-before-return so floats/model internals never cross into the deterministic core.
- **Attribution:** cite BiGG (King et al. 2016) + the `iML1515`/`e_coli_core` model papers in NOTICE/README per
  the academic terms; keep the non-commercial data out of any separately-licensed artifact.

### Consequences
- **+** Unblocks the earned-edit OVERSIGHT loop — the vision's player-agency payoff — with real FBA-grounded
  E. coli impact rather than a fabricated number.
- **−** Forecloses a future commercial/closed release that bundles BiGG-derived data (explicitly accepted). If that
  ever changes, the KO table would need a permissively-licensed or self-generated replacement (the `oracle-fba`
  boundary keeps that swap localized).
- **−** Adds an offline FBA bake dependency (cobrapy + the BiGG model) to the S2 data slice — analysis-only,
  separate process, never linked (same pattern as the SLiM/msprime subprocesses).

---

## ADR-014 — Relations sidecar: per-species SIGNATURE + view-only nearest/guild index (re-grounded)

- **Date:** 2026-06-22
- **Status:** Accepted (re-grounds the retired fabricated-cosine ADR-014 DRAFT). Continuation roadmap #5 / ADR-017
  S8 / ADR-013 Rel-phase. **HASH-NEUTRAL** (no re-pin — the pinned literal `0x47a0_3c8f_6701_f240` is unchanged).

### Context
ADR-013 F4 made the MEASURED `FlowMatrix` the on-hash relation source. The OLD ADR-014 draft proposed a
*fabricated-cosine* community matrix COUPLED INTO `selection()` as a `[0.5,1.5]` `RelationModifier` — that design
is RETIRED. This re-grounding INVERTS it: the relations signal is a READ-ONLY, OFF-HASH projection that flows
ONE-WAY into the renderer; there is NO fabricated cosine and NO `RelationModifier`. The "vector-DB relations" leg
of the vision (a sqlite-vec ANN sidecar) is scaffolded behind the process boundary but NOT wired — the actual
roster is S=2 (→3 with the future predator), where EXACT integer k-NN is correct, instant, and bit-reproducible.

### Decision — the PINNED contract (load-bearing for cross-run stability)
1. **Per-species SIGNATURE = `u16[D]`, D = 12** (PINNED, append-only so a stored sidecar index stays valid),
   exported READ-ONLY off-hash in `SpeciesId` order by `Simulation::species_signatures() -> (s, D, Vec<u16>,
   Vec<u8> role)` (`crates/sim-core/src/signature.rs`; harness + `LiveSim::species_relations()` passthroughs).
   **ONE SHARED SCALE:** every dim lives on the u16 grid `[0, UNIT_SCALE = 65535]` so L1 is block-balanced.
   - **Block A — STRATEGY/metabolic identity (9 dims)**, from the cached `gp::Strategy` (ADR-013 F2, off-hash):
     `[0..5)` `budget[5]` permille rescaled `*65535/1000`; `[5..8)` `affinity[3]` (already on the grid);
     `[8]` `mineralize_rate` permille rescaled (the F4 detritus-loop lever).
   - **Block B — MEASURED interaction (3 dims)**, from a read-only `flow_matrix()` projection (the F4
     RE-GROUNDING — measured flows, NOT a fabricated cosine): `[9]` `in_flow` = Σ max(0, row); `[10]` `out_flow`
     = Σ max(0, col); `[11]` `degree` = nonzero off-diagonal partner count. `in/out` map i64→u16 via a PINNED
     integer base-2 log/clamp against `FLOW_J_SCALE = 1<<28` — NEVER a per-call max-abs (which would make
     signatures non-comparable across snapshots). `degree` scaled by `(s−1)`.
   - **`role:u8`** = the `TrophicRole` ordinal `{Autotroph 0, Heterotroph 1, Mixotroph 2, Decomposer 3}` carried
     ALONGSIDE the vector as a label/FILTER — **NEVER a distance dim** (Autotroph and Decomposer are not
     metrically "close"; folding role into L1 corrupts the metric — adopted from Design 2).
   - **`base_growth` is DROPPED** from the distance vector (it is already echoed by budget+affinity) — so NO
     float ever enters the signature bytes. The only quantization is integer rescaling + the Block-B log/clamp.
2. **Index backend = EXACT in-Rust k-NN + single-link guild clustering** in `crates/relations-index` (std-only,
   `#![forbid(unsafe_code)]`, empty `[dependencies]`, on the oracle-fba template). Trait seam
   `NearestIndex`/`GuildIndex` (inv #5) + `InRustIndex`: integer-L1 `d(a,b)=Σ|a_k−b_k|` (u64, no float, no
   transcendental); `nearest(focal,k)` sorted `(distance asc, sid asc)` — a total order, ties → lowest
   `SpeciesId`; `guilds(T)` single-link union-find at the PINNED threshold, edges walked ascending `(i,j)`,
   guild ids canonicalized to the lowest member `SpeciesId`. `RelError {Io,Spawn,NonZeroExit,MissingOutput}`
   mirrors `FbaError`/`SlimError` (Spawn/NonZeroExit RESERVED for the sidecar). **Chosen over ANN because EXACT
   integer k-NN has ZERO of the HNSW/ANN insertion-order/float-ordering nondeterminism inv #3 forbids; sqlite-vec's
   sublinear-recall value only materializes at thousands of vectors (the future E. coli edit-variant fan-out).**
3. **PINNED constants** (load-bearing for cross-run guild/nearest stability; display-scaling choices, not biology):
   - `signature::FLOW_J_SCALE = 1 << 28` (268_435_456 J) — the Block-B in/out log/clamp saturation point.
   - `relations_index::GUILD_THRESHOLD = 240_000` — the single-link integer-L1 edge threshold `T`.
   - `signature::SIGNATURE_DIMS = 12`, the block layout above, and the L1 metric (ties → lowest SpeciesId).
4. **sqlite-vec SCALE PATH — scaffolded, probe-and-skip, NOT wired.** `resolve_reldb_bin()` (`$RELDB_BIN →
   ~/.local/bin/relations-index → PATH`, the oracle `resolve_*_bin` pattern) + an `index_via_sidecar` stub
   (returns `MissingOutput`). When a roster-size trigger trips (the future thousands-of-edit-variant fan-out), a
   separate `relations-index` CLI linking **sqlite-vec — pinned `v0.1.x` (Apache-2.0 OR MIT, GPL-clean, inv #7;
   exact patch pinned in this table when the trigger is implemented)** is shelled out to, writing run-namespaced
   `.db` sidecar rows (a FILE the sim core never opens). The boundary crate stays dependency-free FOREVER; since
   sqlite-vec never enters `Cargo.lock`, the license gate's resolved-tree scan never even sees it.

### Hash-neutrality (three independent reasons, each sufficient — the pinned literal CANNOT move)
1. **READ-ONLY OFF-HASH SOURCE.** `species_signatures()` is a pure projection: Block A reads the F2-certified
   cached `Strategy` (unread by selection); Block B reads `flow_matrix()` (folded into `hash_world` ONCE in F4 —
   READING it adds no hash input). Walks the `SpeciesRegistry` in `SpeciesId` order (no `HashMap`, inv #3), draws
   ZERO `SimRng`, mutates nothing, NEVER inserted into `hash_world`. `base_growth` is dropped → no float in the bytes.
2. **PROCESS-BOUNDARY CONSUMER.** The k-NN/clustering runs in `relations-index`, which the deterministic core
   NEVER calls during `step()/selection()/metabolism()` — structurally downstream (numbers flow core → boundary →
   renderer, never back). In the sqlite-vec path the core does not even open the `.db`.
3. **ONE-WAY VIEW SINK** — the explicit INVERSION of the retired draft: no fabricated cosine, no `RelationModifier`,
   no seam by which the output re-enters selection. The gate test `species_signatures_export_is_hash_neutral`
   asserts the pinned literal is UNCHANGED with the export + index present.

### Non-goals (explicitly out of scope this ADR)
- NO `selection()` coupling / NO `RelationModifier` (the retired draft's Rel-5). Any future coupling is a
  separate, ledgered, human-signed-off re-pin under a LATER ADR.
- The old "Rel-1 generalize-the-gate" slice is OBSOLETE: `relations-index` was already pre-registered in
  `scripts/check_license.sh` `BOUNDARY_CRATES` (the skip branch flips to ENFORCED automatically the moment
  `crates/relations-index/Cargo.toml` lands — no gate edit needed).

### Consequences
- **+** A view-only relations overlay (guild label tints + a nearest-species advisory strip with a provenance
  badge distinct from the MEASURED FlowMatrix) grounded in real F4 flows; bit-reproducible, hash-neutral; the
  core dependency graph stays clean (only godot-sim depends on `relations-index`).
- **−** The sqlite-vec scale path is deferred (scaffolded only); the in-Rust `InRustIndex` is the sole CI/gate path.

---

## ADR-019 — Contamination & immigration: deterministic journaled inoculation + the containment knob (S1+S2)

- **Date:** 2026-06-22
- **Status:** Accepted (S1+S2 CORE; HASH-NEUTRAL). Builds on ADR-013 (joule ledger) + the SP-3 region-Action
  precedent. S0 data bakes (the contaminant `SpeciesSpec` JSONs), S3 renderer panel, and the S4/S5 re-pin phases
  (spore-dormancy, Mode-B obligate symbionts) are separate, later slices.
- **Context:** the world had no *arrivals* mechanism. Contamination is the verified default state of reality
  (the clean-room frame): lower the guard and the consortium that flies in wins by default unless the residents
  already hold the niche. ADR-013's conserved joule economy already produces establish/displace/die from the
  pool contention — this epic supplies only the arrivals; nothing is scripted.
- **Decision (S1):** one journaled, RNG-FREE, conserved region Action — `Action::RegionInoculate { species_key,
  region, count, endow_j }` (externally-tagged serde-additive; existing `actions.ndjson` unchanged). A
  deterministic core spawn (`Simulation::region_inoculate`) lays `count` orgs into the region disc in canonical
  `(cell_index, slot)` order (round-robin across in-region cells), OrgIds from the monotonic `NextOrgId`, ZERO
  `SimRng` draws (heritable traits seed at a constant `0.5`, not a draw). Each org's starting J = `endow_j`
  MINTED from a NEW named `immigration` ledger tap (a SECOND source distinct from `influx`); `ledger_closes`
  extends to `Σlive == initial + influx + immigration − respired − overflow − chem_decay`. A contaminant species
  not yet in the roster is registered lazily (`Simulation::register_species`), growing every species-indexed
  resource (`EditModifierRes`, `FlowMatrix`, `PoolProvenance`, `KinProvenance`).
- **Decision (S2):** a `ContainmentLevel` knob (ISO-14644-1 ladder: Sealed/Clean/Lab/Open; default **Sealed/OFF**)
  that deterministically EXPANDS at run start — off a NEW off-stream `IMMG_STREAM_BASE` (ASCII "IMMG") `derive_seed`
  family, ZERO `SimRng` draws (the soil/resource off-stream precedent) — into a sorted `Vec` of journaled
  `(due_epoch, RegionInoculate)` events drawn from a configurable `ConsortiumConfig` (the menu set of species
  keys). The schedule is a pure function of `(master_seed, level, config)`; events fire at their epochs
  (Tick-clocked, never wall-clock), drained by the driver as journaled `RegionInoculate`s so a contaminated run
  replays from `actions.ndjson` alone.
- **Determinism / hash:** HASH-NEUTRAL — the new Action is inert until invoked, the `immigration` tap is zero at
  rest and is NOT folded into `hash_world` (it reaches the hash only through its coupling effect on the already-
  hashed Energy/Biomass, like soil/climate/EditModifier), the knob defaults Sealed → an empty schedule, and a
  registered-but-uninoculated contaminant only seeds the resolver. The pinned literal `0x47a0_3c8f_6701_f240` is
  **UNCHANGED** (`determinism_hash_is_pinned` green, byte-identical; the harness `inoculation_system_is_hash_neutral_when_inert`
  cross-checks the env path). A run that *does* inoculate grows the `FlowMatrix` dimension (a hashed input) — but
  that is reachable only off the pinned config, so it is hash-neutral there.
- **Invariants:** #2 — all biology stays in the core (the spawn/registration/ledger live in `sim-core`; GDScript
  only issues the Action via `LiveSim::inoculate`/`set_containment`); #3 — RNG-free placement / single off-stream
  family, ordered `(cell, SpeciesId, OrgId)` collections, no `HashMap` in sim logic; #6 — immigration is a
  species/region operator event, never per-organism (the `RegionInoculate` carries no organism handle). #1/#7
  untouched. **Establish/displace/die-out is NOT coded — it EMERGES** (the `adr019_well_adapted_establishes_…`
  test: a well-adapted decomposer out-harvests the conserved detritus and establishes; a near-inert one cannot
  cover maintenance and dies out — decided by the ledger, not a script).

---

## ADR-020 — Deterministic data-parallelism: rayon (compute-parallel / apply-canonical), S0 scaffold

- **Date:** 2026-06-23
- **Status:** Accepted (S0 SCAFFOLD; HASH-NEUTRAL — NO call sites yet). The full design + the byte-identity proof
  live in `docs/llm/proposals/parallel-sim-draft.md` (now COMMITTED). S1 (diffusion scatter→gather, serial) /
  S2 (metabolism compute/apply split, serial) / S3 (parallelize metabolism — the big win) / S4 (mineralize) /
  S5 (optional permanent parallel diffusion) / S6 (deferred, multi-species predation/host-coupling) are separate,
  later, independently-revertable slices each re-proven against the hash oracle.
- **Context:** the post-F5 hot path is at its single-thread floor (~0.85 M organism-updates/s, flat across N;
  the allocation-elimination sweep bought single-digit %, micro-opts are in the ~0–1% noise band). The only lever
  that moves the bar by a *multiple* is data parallelism INSIDE the heavy systems — exactly the "deterministic
  reduction" the ADR-002/ADR-013 consequence note anticipated ("*revisit if the perf gate forces it — parallelism
  would then need a deterministic reduction*"). Three passes are RNG-free + cell-independent: `metabolism` (~45%),
  `diffuse_and_decay` (~13%), `mineralize` (~5%). `reproduce_or_die` (the SOLE `SimRng` consumer) stays 100%
  sequential — the immovable Amdahl ceiling. Honest projected payoff: ~2–2.5× at 5k–10k orgs, NOT 4×.
- **Decision (S0, this slice):** add `rayon` as a **pinned workspace dependency** and the three knobs every later
  slice depends on, with **ZERO call sites** — so this slice is trivially hash-neutral. (1) `rayon = "1.12"` in
  `[workspace.dependencies]` (resolved to `1.12.0`, `Cargo.lock` pinned; pulls `rayon-core 1.13.0` +
  `crossbeam-{deque,epoch,utils}` + `either`), wired into `crates/sim-core/Cargo.toml`. (2) `crates/sim-core/src/par.rs`:
  a persistent global `OnceLock<rayon::ThreadPool>` built EXACTLY ONCE (`par::pool()`; NEVER spawn/teardown per
  tick) with a pinned worker count (`RAYON_NUM_THREADS` if a valid positive int, else `DEFAULT_NUM_THREADS = 10`,
  pinned for stable benches — correctness is schedule-independent). (3) `PAR_THRESHOLD = 2000` (bench-tuned
  sequential cutoff — below it a heavy system runs its proven serial loop verbatim; the pinned ~1k config stays
  serial = an extra byte-identity guarantee). (4) a `--no-parallel` escape hatch via env var `GENESIM_NO_PARALLEL`
  (`par::force_serial()`, cached) forcing the serial path for differential debugging. (5) `par::run(op)` —
  `pool().install(op)` unless `force_serial()` — the helper every future call site invokes. The Bevy `.chain()`
  schedule stays strictly single-threaded; rayon will live INSIDE the three heavy systems, never in the scheduler
  (no Bevy multi-threaded executor / query `par_iter`, which would scramble the canonical `(cell, species, org)`
  order the hash depends on).
- **Determinism / hash (inv #3 — the load-bearing one):** the discipline is **COMPUTE-PARALLEL / APPLY-CANONICAL**
  — the parallel region (later slices) is RNG-FREE (no parallelized pass holds a `&mut SimRng`, so the ChaCha8
  stream is physically untouchable — the only advancer, sequential `reproduce_or_die`, draws exactly D+1=4 words
  per threshold-passing birth in canonical order), DISJOINT-CELL (each task computes a contiguous whole-cell range
  from the pre-sorted vector, a pure function of frozen read-only snapshots), and every order-sensitive mutation
  (PoolStock decrement per `(channel,cell)`, PoolProvenance/FlowMatrix, litterfall/toxin cap-overflow routing,
  org Energy/Biomass via the OrgId map) is applied SEQUENTIALLY in the EXACT current order. The only cross-task
  reductions are associative-AND-commutative `i64` adds (the one f64 on the path is quantized via `to_unit_u16`
  BEFORE any thread, so no float reduction ever crosses a thread). rayon's work-stealing is nondeterministic in
  TIMING but the RESULT depends only on the disjoint-cell decomposition — never on which thread ran which chunk.
  No `HashMap` is iterated in sim logic (inv #3); rayon iterates Vec index ranges only; the BTreeMaps stay
  sequential. **At S0 none of that machinery exists yet — there are no call sites, the parallel region is empty,
  so the pinned literal `0x47a0_3c8f_6701_f240` is BYTE-IDENTICAL** (`determinism_hash_is_pinned` +
  `species_signatures_export_is_hash_neutral` green at lib.rs:3228 / :3392; `tools/check_determinism.sh` double-run
  OK). The two oracles — the local double-run + the **multi-ISA CI gate** (x86_64 hash == aarch64 hash, `--features
  determinism` HARD asserts) — are the safety net for any latent platform-dependent reduction a single-arch run
  would miss, and MUST run on every push for these slices. **If any later slice moves `0x47a0`, that slice is a
  bug and is reverted — this is NOT a re-pin.**
- **Invariants:** **#1 (GPL at the process boundary):** `rayon` (and all its transitive deps — `rayon-core`,
  `crossbeam-deque/epoch/utils`, `either`) is **MIT OR Apache-2.0** — inv #1's boundary rule is about GPL ONLY,
  so rayon linked into the sim binary is fine. No GPL crate is added; `oracle-slim` is untouched. The boundary
  discipline is preserved as hygiene. **#7 (Versions pinned):** rayon IS a new pinned dependency → recorded here
  (`1.12` → `1.12.0`) alongside the bevy/rand_chacha pins, `Cargo.lock` locked. A rayon minor bump is a
  cross-version reproducibility event to re-gate (low-risk given schedule-result-independence, but pinned like
  `bevy_ecs`/`rand_chacha`). **#3** as argued above. **#4 (headless-first):** the pool + threshold + escape hatch
  are pure sim-core; no renderer touch. **#2/#5/#6** untouched.
- **Consequences:** the persistent pool + the two knobs are the stable surface S1–S4 build on; the per-slice
  "land serial first, prove `0x47a0` unmoved, THEN add threads" discipline is the whole safety story. `par::run`
  / `par::pool` / `PAR_THRESHOLD` / `force_serial` are `#[allow(dead_code)]` / `pub` until S1 wires the first call
  site (a built-but-unused pool must not warn — satisfied via `#[allow(dead_code)]` on `run` + exercised by the
  `par::tests`). Worker count is pinned for bench stability only; correctness never depends on it.
- **⚠️ MEASURED OUTCOME (2026-06-23) — parallelism does NOT pay; S2–S4 NOT pursued. The ~2–2.5× projection was
  WRONG.** S1 (diffusion scatter→gather) landed and is kept — it is a byte-identical determinism-clarity win, and
  the parallel gather is proven (`parallel_gather_equals_serial`) but stays serial behind `DIFFUSE_PAR_THRESHOLD =
  65536` (fork/join is ~22× slower than the ≤5-add-per-cell work at the 1024-cell grid). **S2+S3 (metabolism
  compute/apply split → parallelize) were implemented, proven byte-identical AND inv-#3-correct (`parallel ==
  serial` across 1/2/8/16 threads), but BENCHED A NET SLOWDOWN (1k +8%, 5k +2%, 10k +1%, clean A/B) → REVERTED.**
  A separate surgical parallelization of ONLY the big per-item Pass-1 demand loop (the ADR's lowest-overhead
  candidate) was also byte-identical + correct but **FLAT (±1%, noise) → reverted.** A bigger-grid experiment
  (256×256 = 65536 cells, both `SOIL_DIMS` + `resource::RESOURCE_DIMS` bumped, alive population) confirmed
  parallelism STILL flat (20k orgs 110.8 vs 110.4 ms/tick; 80k 198.3 vs 197.8) **and** a bigger grid is 5–9× SLOWER
  (it hurts FPS, does not help). **Root cause:** the per-tick cost is dominated by the per-ORGANISM `metabolism`
  loop, whose per-org work (a few integer ops) is too fine-grained to beat fork/join overhead at any grid/population
  we tested; the parallelizable grid passes (diffusion) early-exit on the sparse default chem field. **The sim is
  at its single-thread FPS ceiling (~0.85 M org-updates/s); 32×32 is the FPS sweet spot (~1000+ ticks/s at typical
  loads — ample for the decoupled render loop).** The rayon scaffold (S0) + the gather (S1) STAY on main (the gather
  is a clean win; the scaffold is dormant and would only ever pay on a hypothetical huge DENSE-chem grid). Do NOT
  re-attempt S2–S4 without a fundamentally different cost profile (e.g. a much heavier per-org model, or a dense
  chem field). The pinned literal `0x47a0_3c8f_6701_f240` is unchanged throughout (S2/S3/demand/grid were all
  reverted; only the byte-identical S0+S1 remain merged).

---

## ADR-021 — GSS5 snapshot: per-cell `dominant_species_id` channel (ecosystem-map species visualization)

- **Date:** 2026-06-23
- **Status:** Accepted.
- **Context:** the ecosystem map sized every organism from one per-cell density-derived radius → on a
  multi-species roster every species looked identical (the map was unusable). The render snapshot
  (`GridSnapshot`) carried only per-cell AGGREGATES (density/allele/fitness + the soil/resource/chem planes),
  no per-cell SPECIES — so the renderer could not tell a plant cell from an E. coli cell from a Bdellovibrio
  cell.
- **Decision:** add a per-cell `dominant_species_id` channel to the snapshot. `Simulation::snapshot()` tallies
  the resident organisms per cell (a sorted `Vec<(u16,u32)>`, no HashMap — inv #3) and emits the most-populous
  `SpeciesId` (deterministic lowest-id tiebreak), `u16→f32`. `SNAPSHOT_MAGIC` **GSS4→GSS5**, `CHANNEL_COUNT`
  **12→13** (a bumped magic makes a stale 12-channel reader fail loudly — the same discipline as GSS1→GSS2
  (ADR near DECISIONS.md:295) and GSS2→GSS3 (:588)). EVERY GSS reader updated: `godot/snapshot.gd`,
  `crates/godot-sim/godot/livesim_smoke.gd` (the magic + `channels==13` assert — the classic stale-reader
  break), `tools/check_godot_snapshot.sh` (`channels=13`), the godot-sim doc comments. The renderer side
  (`godot/species_visual_map.gd` — a per-species size/color table on a real cell-size scale: plant ≫ rod ≫
  predator ≫ symbiont; `godot/organisms.gd` sizes/colors each cell by its dominant species; `main.gd` wires it)
  is pure presentation.
- **Determinism / hash (inv #3):** **HASH-NEUTRAL — the pinned literal `0x47a0_3c8f_6701_f240` is unchanged.**
  The snapshot is read-only, off `hash_world`, draws ZERO `SimRng` (exactly like the soil/resource/chem display
  channels). The per-cell tally is a sorted-Vec argmax (no HashMap iterated in sim logic); single-species runs
  emit a uniformly-0 plane. Proven by `determinism_hash_is_pinned` + the new
  `snapshot_single_species_dominant_id_is_uniformly_zero` / `..._picks_most_populous_with_lowest_id_tiebreak`
  tests (178 sim-core tests green).
- **Invariants:** **#2** biology stays in the core (the renderer only maps the exported id → a visual);
  **#1/#5/#6** untouched; **#7** this format break is recorded here (the GSS lineage discipline).
- **Consequences:** the map is now legible (species sized by real cell-scale). FOLLOW-UP: per-zoom-scope
  refinement (Field aggregate vs Cells per-organism glyphs) + wiring the `data/presets/primordial.json` starter
  into the SP-2 composer.

---

## ADR-022 — Relations node-link GRAPH (default view) + `--roster` / `--steps` shot flags

- **Date:** 2026-06-23
- **Status:** Accepted.
- **Context:** the Relations view shipped only the S×S FlowMatrix HEATMAP. Users read "relations" as a
  node-link GRAPH of the trophic web and did not recognise the matrix as one ("I don't see a graph, only a
  2D panel"). Separately, every headless `--shot`/`--check` path was single-species (`--species <stem>`), so
  a MULTI-species map / graph (the thing that actually shows per-species size contrast + measured flows) could
  not be rendered for verification without the interactive menu.
- **Decision:** (a) add `godot/relations_graph.gd` — species as ring-laid NODES (radius ∝ √population, colour
  via the shared `species_visual_map.gd` morphotype table so the graph + field agree), EDGES = the
  core-MEASURED FlowMatrix net joule flows drawn source→sink (arrowhead at the gainer, thickness/opacity ∝
  |J|/max-abs), oriented EXACTLY like `main.gd._format_flow_summary` (`flat[b*s+a]`, higher-index sink). A
  `🕸 Graph / ▦ Matrix` segmented toggle swaps the two; **Graph is the DEFAULT** representation (the user's
  expectation). Fed by `_refresh_relations` from `observe_species()` (names/keys/roles/population, SpeciesId
  order = FlowMatrix index order, by construction). (b) add two opt-in headless shot conveniences:
  `--roster "stem:count,stem:count,…"` (parsed in `_apply_cli_environment`, armed via the EXISTING
  `_apply_roster` **before** `_do_reset` — the load-bearing seed-once order) and `--steps N` (advance the
  deterministic core N gens before capture so populations establish + the FlowMatrix accumulates flows).
- **Determinism / hash (inv #3):** **HASH-NEUTRAL — pinned literal `0x47a0_3c8f_6701_f240` unchanged.** ZERO
  Rust touched (the graph + toggle + feed are all `godot/*.gd`; `--roster`/`--steps` only drive existing core
  entry points). The flags are OPT-IN — the no-flag pinned config is byte-identical (`determinism_hash_is_pinned`
  + reproducible-at-pinned-config green; full `tools/gate.sh` GREEN; godot `channels=13`/`glyphs=13`/`codex=OK`).
- **Invariants:** **#2** biology stays in the core — the graph only PROJECTS the measured FlowMatrix + the
  exported populations into nodes/edges (the only arithmetic is display scaling + ring layout, identical in kind
  to `relations_heatmap.gd`'s `_max_abs` ramp); **#4** the `--roster`/`--steps` flags keep the headless paths
  multi-species-capable; **#1/#5/#6** untouched. Adversarially verified 3/3 (no-biology, hash-neutral,
  index-alignment, draw-safe-degrades, roster-armed-before-reset, ux-faithful).
- **Consequences:** the trophic web reads as a graph at a glance (and the matrix is one click away); multi-species
  shots are now scriptable (unblocks the map size-contrast verification + future discovery showcases). FOLLOW-UP:
  optional guild-coloured nodes; a force-directed layout when S grows large (the ring suffices for small rosters).

---

## ADR-023 — Emergent-discovery D0 scorer + D1 trace: `crates/discovery` (std+serde) + the harness capture seam

- **Date:** 2026-06-23
- **Status:** Accepted (D0/D1 phase; the search loop D2+ and the surrogate model D3 are later).
- **Context:** the roadmap epic ([emergent-discovery-harness-draft.md](proposals/emergent-discovery-harness-draft.md),
  memory `autonomous-emergent-run-discovery-ml`) wants to autonomously SEARCH the (config + edit) space, SCORE each
  run for "interestingness", and SAVE the gems as bit-identically-replayable showcases. The load-bearing first piece
  is a reproducible SCORER; everything else is search plumbing. The metric set was pinned by the
  `emergent-scorer-design` 3-lens panel (the spec is folded into this ADR).
- **Decision:**
  - **New crate `crates/discovery`** (added to the workspace members) — **std + serde ONLY** (no `sim-core`, no
    `harness` dep): it scores a PLAIN `PerGenTrace` it is handed, so the scorer stays on the clean side of the
    capture seam (inv #1/#5). It is NOT a zero-dep BOUNDARY crate (it links serde, MIT/Apache-2.0, GPL-clean for
    trace I/O) → intentionally absent from `BOUNDARY_CRATES`; the GPL scan still covers its closure.
  - **D0 scorer** (`src/ecology.rs`): six INTEGER, RNG-free, basis-point metrics over the stable window — M1
    coexistence, M2 integer-Simpson evenness, M3 amp+turns **dynamism** (single-boom-capped), M4 FlowMatrix-aggregate
    trophic structure (edges/roles/octave-log flow), M5 saturating **events** (booms/crashes/takeovers/established-
    immigrations), M6 a **multiplicative survival GATE** (anti-instant-death) — combined `Q = (ΣWᵢmᵢ/WSUM)·m6` →
    `[0, 1_000_000]`. `InterestingnessScorer` trait (inv #5 pluggable); `DefaultScorer` id `"ecology-d0"`; a 12-dim
    integer fingerprint + `novelty_l1` + `final_score` (novelty applied as a save-time MULTIPLIER, gem persistence is
    D2). The lone `f64` is the fenced `q16` capture quantization; no RNG, no HashMap-iteration.
  - **D1 capture seam in `crates/harness`** (`src/capture.rs` `capture_trace`): drives a live `GeneSimEnv`
    (reset → step → `observe_all()` + `flow_matrix()` per gen) into a `PerGenTrace` — the harness (which already
    depends on sim-core) owns the engine touch; `discovery` stays clean. `harness → discovery` is the only new edge.
- **Determinism / hash (inv #3):** **HASH-NEUTRAL — pinned literal `0x47a0_3c8f_6701_f240` unchanged.** Capture
  READS only `observe_all()`/`flow_matrix()` (pure `&self`, zero `SimRng`, never folded into `hash_world`); proven by
  `harness/tests/trace_capture.rs` (a real predator/prey run scored both ways asserts captured-hash == plain-hash, and
  the pinned single-species config one-gen-at-a-time under the exact capture reads still hashes `0x47a0…`) +
  `per_gen_stats_preserves_determinism_hash`. The score path is integer end-to-end → byte-reproducible cross-platform.
- **Pinned `ScoreParams` (inv #7 — the tunable starting point; the struct lets every value change without code):**
  weights `[W1=14, W2=14, W3=22, W4=18, W5=18]` (M3 dynamism + M5 events = 40/86 → **drama outranks forced
  stability**, encoding memory `no-hardcoded-balance-open-system`; M6 the multiplicative gate that does NOT penalize
  END-state extinction — only EARLY total collapse). `SCALE=10_000`, `SCORE_SCALE=1_000_000`, `BURN_IN_BP=2000`,
  `PERSIST_BP=8000`, `RICH_CAP=6`, `TURN_TARGET=8`, `EDGE_TARGET=4`, `BOOM_K=3`, `CRASH_K=4`, `POP_FLOOR=5`,
  `CRASH_FROM=20`, `EVENT_SAT=6×SCALE`, `NOV_SAT=3×SCALE`, `NOV_FLOOR=4000`, `DEDUP_MIN=SCALE`, `FP_DIMS=12`.
- **Invariants:** **#1** std+serde, GPL-clean, the capture engine touch is in the harness not the scorer; **#2** the
  scorer only READS exported numbers (no genome/genotype→phenotype); **#3** integer/RNG-free/off-hash (above); **#4**
  headless; **#5** the metric set is pluggable behind `InterestingnessScorer`; **#6** config/operator level. Verified
  3/3 on every dimension; a 7-archetype synthetic oracle + a real grounded run assert the contract (live limit-cycle
  **A = 784_500** strictly beats frozen coexistence **F = 355_000**).
- **Consequences:** runs can now be SCORED reproducibly. FOLLOW-UP: D2 (the gradient-free → evolutionary search loop +
  the gem library / novelty dedup persistence), D3 (the surrogate "brute-force gradient" model), D4 (the autonomous
  night-batch + the showcase gallery), anchored on the `data/presets/primordial.json` starter.

---

## ADR-024 — Emergent-discovery D2a: the random-search gem loop (propose → run → score → save replayable gems)

- **Date:** 2026-06-24
- **Status:** Accepted (D2a — random search; the evolutionary proposer D2b + the surrogate D3 are later).
- **Context:** ADR-023 gave a reproducible SCORER (D0) + per-gen TRACE (D1). D2a makes the loop actually RUN: search
  the config space, score each run, and SAVE the gems — the autonomous "find the dramatic runs" step.
- **Decision:**
  - **`crates/discovery::search`** (still std + serde ONLY): `SearchConfig` (master_seed + per-species start counts +
    containment + temp_q/season — a DETERMINISTIC description of one run); a `SearchSpace` pinning the proposal ranges
    (Primordial roster: plant 200..=1200, ecoli 50..=600, bacillus 30..=400, bdellovibrio …, containment 0..=3, a temp
    range); a std-only DETERMINISTIC proposal sampler (`propose(search_seed, trial, field)` = a splitmix64 integer hash
    + Lemire range draw — **NO `rand` crate**, so discovery stays std+serde); a `Gem` record (config + score/quality/
    novelty/breakdown/fingerprint + `recorded_hash` + `build_id` + an integer-derived `caption`, serde); and a
    `GemLibrary::consider` that keeps top-K by `rank_key` (score desc, then `recorded_hash`/seed) and rejects
    near-duplicates via integer `novelty_l1` (`nn < DEDUP_MIN`).
  - **`crates/harness::discover`** (`discover(search_seed, trials, keep, gens, out_dir)`): per trial → `propose` a
    config → build a `GeneSimEnv` (`set_roster`/`set_environment`/`set_containment`) → `capture_trace` → `DefaultScorer`
    → `GemLibrary.consider`. Each KEPT gem is written to `data/runs/gems/<score>-<seed>.json` **only after**
    `record_episode → replay() == recorded_hash` (the reproducibility contract — a failed round-trip is DROPPED). A CLI
    subcommand `--discover --trials N --keep K --search-seed S --discover-gens G` prints a ranked summary. `data/runs/*`
    is gitignored (gems are generated artifacts; a curated showcase set lands in D4).
  - **`BUILD_ID = "ecology-d0@47a03c8f6701f240"`** anchors every stored gem to the pinned sim hash (inv #7) — a re-pin
    self-invalidates stored scores (cheaply recomputed by replay).
- **Determinism / hash (inv #3):** **the pinned literal `0x47a0_3c8f_6701_f240` is UNTOUCHED.** The search adds a new
  module + CLI and changes NO sim-path; the proposal sampler is a META-level splitmix RNG, distinct from the sim
  `ChaCha8Rng`. A dedicated test (`harness/tests/discover.rs::pinned_determinism_literal_is_unmoved_by_the_search_slice`)
  asserts both `run_headless` and the stepwise path still hash the literal. A full `discover()` run is byte-reproducible
  per `search_seed` (identical gem files); every saved gem round-trips. Verified 5/5 (std+serde, sim-hash-untouched,
  gems-round-trip, search-deterministic, novelty-dedup-real).
- **Invariants:** **#1** discovery std+serde (the engine touch — build/run/replay — is in the harness); **#2** scores
  read exported numbers only; **#3** integer/off-hash/reproducible (above); **#4** headless; **#5** the proposer/scorer
  are swappable; **#6** the search acts at the CONFIG/operator level (rosters + env), never per-organism.
- **Consequences:** the discovery loop now produces real, replayable gems. KNOWN (→ D2b): the Primordial space clusters
  in one fingerprint neighborhood (most configs score alike → dedup keeps ~1 distinct gem) — D2b WIDENS the space
  (broader count ranges / species mixes / scheduled mid-run edits) + adds the evolutionary proposer; the gem-staging dir
  is keyed by the run's content-addressed id (a same-master-seed collision is ~2⁻⁶⁴, negligible — tighten if D2b
  reuses seeds). FOLLOW-UP: D3 surrogate, D4 night-batch showcase gallery.

---

## ADR-025 — Emergent-discovery D2b: widened search space + the evolutionary proposer

- **Date:** 2026-06-24
- **Status:** Accepted (D2b — random + evolutionary search; the surrogate D3 + showcase D4 are later).
- **Context:** D2a (ADR-024) ran but the narrow 4-species Primordial space CLUSTERS — most configs land in one
  fingerprint neighborhood, so the novelty-dedup kept only ~1 distinct gem. To surface a DIVERSE gem library the
  search must (a) explore a wider, mixed-species space and (b) exploit the best finds, not just sample i.i.d.
- **Decision (all in `crates/discovery::search`, still std + serde ONLY — NO `rand` crate):**
  - **Widened `SearchSpace::default`:** 7 free-living species axes with a per-species PRESENCE knob `include_bp`
    so proposed rosters differ in the species MIX (not just counts): `default` always present (`include_bp=SCALE`,
    so a run is never empty), then `ecoli` 7000 · `bacillus` 6000 · `pseudomonas` 5500 · `staph` 5000 ·
    `aspergillus-niger` 4500 · `bdellovibrio` 4000 (descending presence bp), broader count ranges, containment
    0..=3, temp 0.15..=0.85.
  - **Deterministic std-only EVOLUTIONARY operators** (salted disjoint splitmix64 streams `MUTATE_SALT`/
    `CROSS_SALT`/`EVOLVE_SALT`, pure functions of `(search_seed, step, field)`): `mutate(parent)` (bounded ±
    count perturbation clamped to the axis, occasional presence flip, env tweak), `crossover(a, b)` (per-species
    pick count/presence from a parent via an ordered Vec key-union — no HashMap iteration), and `propose_evolved`
    (dispatch mutate vs crossover). `ensure_autotroph` guarantees a non-empty, in-bounds roster always.
  - **`crates/harness::discover_evolved(search_seed, pop_size, generations, keep, gens, out_dir)`:** generation 0
    = `pop_size` RANDOM configs; then each generation proposes `pop_size` NEW configs — an EXPLORE fraction
    `EVOLVE_EXPLORE_BP = 2500` (25%) fresh-random + the rest mutate/crossover of the CURRENT kept gems (elites) —
    folded into the same `GemLibrary` (top-K + novelty-dedup). The gem WRITE still goes through the shared
    `verify_and_write_library` (`record_episode → replay == recorded_hash` or DROP — the reproducibility contract
    is unchanged). CLI `--evolve-gens G` (G>0 → evolutionary; **G=0 reduces EXACTLY to the D2a random `discover`**)
    + `--pop-size P` (default 16).
- **Determinism / hash (inv #3):** **the pinned literal `0x47a0_3c8f_6701_f240` is UNTOUCHED** — the evolutionary
  search is meta-level (splitmix proposal RNG, distinct from the sim `ChaCha8Rng`); no sim-path change. A test
  (`discover_evolved.rs::pinned_determinism_literal_is_unmoved_by_the_evolutionary_slice`) asserts both `run_headless`
  and the stepwise path still hash the literal. A full `discover_evolved` run is byte-reproducible per `search_seed`
  (identical saved gems); every gem round-trips.
- **Invariants:** **#1** discovery std+serde, operators std-only splitmix (no `rand`, no engine dep); **#2** the
  operators carry no genotype→phenotype; **#3** integer/off-hash/reproducible (above); **#4** headless; **#5** the
  proposer is swappable; **#6** config/operator level. Verified 3/3 on every dimension; the diversity win is pinned
  by `evolutionary_keeps_more_distinct_gems_than_same_budget_random` (matched budget `pop*(gens+1)`, STRICT
  `evo_distinct > rnd_distinct`, both on the SAME widened space → the win is the explore/exploit machinery).
- **Consequences:** the search now escapes the single cluster and grows a DIVERSE gem set. FOLLOW-UP: D3 (the
  surrogate "brute-force gradient" model biasing the proposer), D4 (the autonomous night-batch + showcase gallery);
  at D3/D4 SCALE the flat-JSON gem dir + linear novelty scan is the trigger to add a behind-the-boundary sqlite-vec
  gem-index SIDECAR (the ADR-014 pattern — a derived index rebuildable from the source-of-truth JSON gems).

---

## ADR-026 — PERF-2: per-tick OrgId-keyed `BTreeMap`/`BTreeSet` → reused sorted-`Vec` (hash-neutral)

- **Status:** Accepted (2026-06-26). **Hash-neutral** — the pinned literal `0x47a0_3c8f_6701_f240` is byte-identical
  (`same_seed_same_hash` green; 180/180 sim-core tests incl. `--features determinism`). NOT a re-pin.
- **Context:** post-F5, the hot path still built a fistful of OrgId-keyed `BTreeMap`s + `BTreeSet`s FRESH every tick
  over the whole living set (`by_org`, `maint_energy`, `parent_debit` in lib.rs; `spent` in chem.rs; `pred_credit`,
  `symb_credit`, the `prey_debit`/`host_debit` struct maps, the collect maps `litterfall`/`toxin_mints`, and the
  `dead_set`/`despawn_set` membership sets in trophic.rs). The post-F5 baseline note had DEFERRED these as "would
  re-pin." Profiling re-opened it: the hot `items`/`rows` vectors are already sorted by `(cell, species, OrgId)`, so
  every map's iteration/lookup order is reproducible from a sorted `Vec` — the conversion can be byte-identical.
- **Decision:** replace each with a REUSED sorted-`Vec` scratch buffer held in a `Resource` (the PERF-1
  `mem::take` + `clear()` discipline). Two helpers in lib.rs: `sort_merge_org_i64` (sort by key + sum-merge dup keys
  — byte-identical to `entry().or_insert(0)+=v`) and `org_lookup` (`binary_search` == `BTreeMap::get`). By shape:
  i64 maps → `Vec<(u64,i64)>` + the helpers; collect-then-iterate maps (`litterfall`/`toxin_mints`) → row `Vec`
  sorted by `(cell,..)` then iterated (NO lookup — already the zero-lookup ideal); membership sets → sorted
  `Vec<Entity>` + `binary_search` (== `BTreeSet::contains`); struct-valued maps (`prey_debit` `PreyDebit{eaten,dead}`,
  `host_debit` `HostDebit{drawn}`) → `Vec<(u64,T)>` sorted by org with a struct-aware sum-merge + `binary_search`
  get/get_mut (the three-phase build→get_mut(dead)→get(apply) preserved). Two new scratch structs
  `PredationScratch` / `HostCouplingScratch` (trophic.rs), registered in `Simulation::new`.
- **Why hash-neutral (and why NOT the "even-better" zero-lookup everywhere):** the ECS-mutating apply passes keep
  using the arbitrary-order `q.iter_mut()` query (ECS table order is NOT canonical — the reason collect-then-apply
  exists), so a zero-lookup `Vec` indexed by items-position is NOT achievable there; `binary_search` is the correct
  ceiling. Only `litterfall`/`toxin_mints`, applied by iterating the buffer itself, are lookup-free.
- **Result:** tick_loop **−48 %** across the board (1 k 32.0 ms / 5 k 151.3 ms / 10 k 305.2 ms vs the PERF-1 baseline
  61.7 / 295.4 / 590.8 ms; ~1.64 M updates/s at 10 k, ≈1.9×), all p < 0.05 — the per-node heap alloc + pointer-chase
  of a per-tick `BTreeMap` over thousands of orgs was a large fraction of the plant-only tick.
  - **Back-to-back re-confirmation (2026-06-27, after rebasing PERF-2 onto PERF-1):** criterion `--baseline` on the
    same machine, PERF-1 (`ed558d7`) vs PERF-2 composed (`3886fc6`) → marginal **−47.4 % / −48.9 % / −47.8 %**
    (p < 0.05) at 1 k / 5 k / 10 k = 32.2 / 151.8 / 308.2 ms vs 61.4 / 297.3 / 590.9 ms. Note PERF-1's own bench
    (61.4 / 297.3 / 590.9) matches this "PERF-1 baseline" row: PERF-1's scratch-Vec hoist was perf-NEUTRAL on this
    bench (it eliminated allocations off the critical path), so the −48 % is genuinely PERF-2's marginal contribution,
    not PERF-1 + PERF-2 conflated. The recorded table numbers (32.0 / 151.3 / 305.2) sit within run-to-run noise of
    the fresh 32.2 / 151.8 / 308.2.
- **Coverage caveat — CLOSED (follow-up done, 2026-06-27):** the pinned `0x47a0…` config is plant-only and
  early-returns out of predation/host_coupling, so the `prey_debit`/`host_debit`/`pred_credit`/`symb_credit`/
  `despawn_set` conversions were not locked by it — only by construction-equivalence + the run-to-run
  `f6_predation_*` / `s5_host_coupling_*` tests. Now LOCKED by two GOLDEN-literal pins: `predation_roster_hash_is_pinned`
  (`0xd4eb_7676_531f_b2bf`, the f6 3-species predator roster — seed 57, 50 gens, 600) and
  `host_coupling_roster_hash_is_pinned` (`0xf723_26af_466e_bb64`, the s5 inoculate→couple run — seed 47). Any future
  change that perturbs those byte-paths now fails CI, exactly like `0x47a0…` guards the plant path. These are NEW pins
  on NEW configs — hash-neutral to `0x47a0…` (test-only addition, no sim-logic change).
- **Consequences:** supersedes the post-F5 "Deferred — would re-pin" note. `sort_merge_org_i64` / `org_lookup` are
  the reusable pattern for any future OrgId-keyed per-tick collect/apply map.

---

## Baseline benchmarks — perf threshold (SPEC §11, §10.7)

Reference platform: Apple M4 Max, native aarch64, `release` profile (`lto = "thin"`, `codegen-units = 1`).
Source: `cargo bench -p sim-core` (`crates/sim-core/benches/tick.rs`), run via `GATE_BENCH=1 tools/gate.sh`.
The perf gate (§10.7) fails on a regression **below the CURRENT baseline**. Re-baseline at each stage that
changes the hot path, in the same slice (this is anticipated — see the Stage 0 row note).

### Current baseline — post-F5 pipeline, after the PERF-2 BTreeMap→sorted-Vec pass (ADR-026, hash-neutral)
| Workload (entities × generations) | Median wall time | Throughput |
|---|---|---|
| 1 000 × 50  | **32.0 ms** | ~1.56 M organism-updates/s |
| 5 000 × 50  | **151.3 ms** | ~1.65 M organism-updates/s |
| 10 000 × 50 | **305.2 ms** | ~1.64 M organism-updates/s |

**Headline (current):** ~**1.64 M organism-updates/s** at 10 k entities (≈1.9× the post-PERF-1 row below).
The large slowdown vs the stale R1.1
row is the real cost of the post-F0b biology, NOT a regression: F3 replaced constant-N Wright-Fisher with a
variable-N energy-funded births/deaths chemostat (per-cell Monod uptake, largest-remainder apportionment over
co-located demanders, per-org `split_budget` convert, conserved-J ledger asserted every tick), F4 added the
decomposer mineralization loop + the measured `FlowMatrix`, and F5 added the toxin/kin/alarm diffusion field —
and the `entities_N` count is the SPAWN count; population then grows over the 50 generations, so each "tick"
processes well more than N orgs. (The R1.1 row is kept under Historical for the record.)

**Prior pass — PERF-1 (scratch-buffer reuse, the post-F5 row this PERF-2 baseline supersedes):** an allocation-elimination sweep that preserved the EXACT integer
sequence (`determinism_hash_is_pinned` = `0x47a0_3c8f_6701_f240`, byte-identical throughout). Changes, all
reusing scratch across ticks or hoisting a constant out of the hot loop — never touching iteration/accumulation
order or any value: (1) `fixed::apportion_into` / `split_budget_into` — buffer-reusing cores of `apportion` /
`split_budget`, called per-(cell,channel) and per-org, that write into caller-owned `out`/scratch (bit-identical
math); wired into `metabolism` pass-2/pass-3, `mineralize`, and `PoolProvenance::withdraw`. (2) `SolarLightCap`
— the static `ResourceField` light cap (`min(to_unit_u16(light)·CELL_CAP_SCALE, POOL_CAP)`) is constant, so it
is precomputed ONCE at reset instead of re-flooring an f64 per cell per tick in `solar_influx`. (3) Per-tick
`Vec` clones turned into reused buffers held in `MetabolismScratch` / `ReproScratch` / `ChemEmitScratch` (the
`items`/`rows`/`demand`/`granted` row vectors + the `frozen_light/nutrient/detritus/toxin/alarm` plane
snapshots, now `clear()`+refill / `extend_from_slice`), and the per-channel `src` clone in `diffuse_and_decay`
(reused `ChemField.src_buf`). Net: 1 k −13 %, 5 k −8 %, 10 k −6 % vs the pre-pass post-F5 numbers (70.9 / 321.2
/ 631.4 ms), all p < 0.05.

**PERF-2 (ADR-026) — DONE, hash-neutral (supersedes the prior "deferred — would re-pin" note):** the remaining
per-tick OrgId-keyed `BTreeMap`s (`by_org`, `maint_energy`, `parent_debit`, `spent`, `pred_credit`, `symb_credit`,
the `litterfall`/`toxin_mints` collect maps, the `prey_debit`/`host_debit` struct maps) and `BTreeSet`s
(`dead_set`, the two `despawn_set`s) were all swapped for REUSED sorted-`Vec` scratch buffers
(`sort_merge_org_i64` + `org_lookup`/`binary_search`). The "any mis-step moves the hash" worry did NOT
materialize — careful construction-equivalence (sorted-unique-key Vec ≡ BTreeMap iteration; binary_search ≡
`get`; sort-merge sum ≡ `entry().or_insert(0)+=`; sorted Vec + binary_search ≡ `BTreeSet::contains`) kept the
pinned literal `0x47a0_3c8f_6701_f240` byte-identical (180/180 sim-core tests green). Net vs the post-F5 PERF-1
row above: **1 k −48 %, 5 k −49 %, 10 k −48 %** (32.0 / 151.3 / 305.2 ms vs 61.7 / 295.4 / 590.8 ms), all
p < 0.05 — the BTreeMap per-node heap-alloc + pointer-chase was a large fraction of the plant-only tick.

### Historical — Stage 0 (slice S0): empty deterministic loop (no selection)
| 1 000 × 50 → **302.6 µs** · 5 000 × 50 → **1.438 ms** · 10 000 × 50 → **2.856 ms** (~175 M updates/s). |
| Superseded by the Stage 1 row above once real selection landed; kept for the record. |

---

## ADR-027 — Variant Lab D: the mid-run-EDIT search axis (auto-research gets the CRISPR-edit action, hash-neutral)

**Status:** Accepted (2026-06-28). Extends ADR-024 (D2a random search) + ADR-025 (D2b evolutionary proposer);
sits inside their envelope (ADR-025 already foreshadowed "scheduled mid-run edits within the widened search").

**Context.** The discovery search (D2a/D2b) probed only the INITIAL-CONFIG space — `score_config` ran
`capture_trace(.., &[])` with no journaled actions. The user's Variant Lab vision requires the brute-force
auto-research to ALSO wield the CRISPR-edit action: explore the (init-config + MID-RUN-EDIT) space so an
interesting *edited* lineage can be discovered + saved as a replayable gem, exactly like a player editing a
species. The per-species edit primitive already exists (Slice A: `Action::ApplyEdit(EditAction{ target, guide,
species })`).

**Decision.** A scheduled-edits axis on `SearchConfig`, threaded through the existing capture/replay seam:
- `SearchConfig.edits: Vec<EditGene>` — the LAST field, `#[serde(default, skip_serializing_if = "Vec::is_empty")]`
  → legacy/no-edit gems serialize + deserialize byte-identically (the surrogate eval-log JSON-prefix contract is
  intact). `EditGene { gen, species_index, target, guide }` is bare ints + an ACGT `String` (std+serde, inv #1/#5;
  no sim-core/genome dep).
- An `edit_budget: u8` knob on `SearchSpace`, **default 0**. When `edit_budget == 0`, `draw_edits` returns
  `Vec::new()` BEFORE drawing any word — so the default search, every existing discovery test, and the eval-log
  bytes are byte-identical, and edits enter ONLY when a caller opts in (`--edit-budget N`).
- `harness::discover::edits_to_actions` maps each `EditGene` onto the EXISTING `Action::ApplyEdit` (resolving
  `species_index` positionally against the same `env_config.roster` on both the capture and the verify side) —
  **no new sim Action**; the genotype→phenotype gate stays in sim-core (inv #2/#6). `verify_and_write_library`
  rebuilds the round-trip journal to MATCH `capture_trace`'s interleave (per gen: the scheduled `ApplyEdit`s, then
  `Advance(1)`), so an edited gem round-trips (`replay == recorded == gem.recorded_hash`) or is dropped.

**Two load-bearing determinism choices (the reason this is reproducible + hash-neutral):**
1. **q16 span-independent gen encoding** (`EDIT_GEN_Q16_DEN`): an edit's firing generation is drawn as a q16
   fraction of the run, not an absolute gen, so the schedule is stable + meaningful regardless of the `gens` the
   trial runs; `gen_abs` is recomputed by integer mul/div in `edits_to_actions` (no float).
2. **`EDIT_SALT` XOR stream-layering**: the edit draws use NEW field indices (`5 + 2N + 4k`) on a salt
   `EDIT_SALT = 0x4564_6974_5363_0004`, disjoint from the four existing operator salts (propose `0` / `MUTATE`
   `…0001` / `CROSS` `…0002` / `EVOLVE` `…0003`); the mutate-edit stream is `MUTATE_SALT ^ EDIT_SALT`. Adding the
   axis therefore perturbs NO existing count/presence/env draw — proven by `raising_budget_does_not_perturb_roster_or_env`.

**Consequences.** The pinned literal `0x47a0_3c8f_6701_f240` is UNMOVED (sim-core untouched; the single-plant
pinned config has no edits). The search is a strict superset: `edit_budget 0` ≡ the prior D2a/D2b behaviour
byte-for-byte; `edit_budget > 0` discovers reproducible edited gems. The D3 surrogate (steered loop) can later
steer this axis once it lands. Gate GREEN; 3-skeptic verify CONFIRMED (5/5 claims at 3/3).

---

## ADR-028 — OVERSIGHT in-game UI: the renderer immediate-commit path (ADR-017 S4/S5/S6 surface, hash-neutral)

**Status:** Accepted (2026-06-28). The renderer SURFACE of the ADR-017 layered-E. coli OVERSIGHT earned-edit loop
(economy core landed in prior slices — the harness `CreditLedger`, the `due_epoch` multi-fidelity firewall, the
`EcoliEditModifier` ripple via the F4 decomposer loop, the `oracle-fba` KO table accepted under ADR-018). This is
the first DECISIONS block for the loop's player-facing layer; the economy itself is designed in
`docs/llm/proposals/ecoli-oversight-gameloop-draft.md`.

**Context.** The OVERSIGHT economy existed only headless. This slice lets the player, in `--live`, EARN credit
(RNG-free accrual), REQUEST → PREVIEW (the FBA knockout result, read-only) → COMMIT an E. coli edit that ripples
through the F4 loop.

**Decision.**
- `godot-sim` gains thin marshalling `#[func]`s only — `oversight_state(&self)`, `preview_ecoli_edit(&self, …)`
  (read-only), `commit_ecoli_edit(&mut self, …)` — every economy/biology decision stays in the harness/core
  (inv #2): `edit_factor_q` / `commit_species_edit` (integer, no RNG, sim-core), `can_afford` / `try_spend`
  (harness `oversight`). GDScript moves only ints + the marshaled `VarDictionary` (the sole arithmetic is permille
  `/1000.0` display formatting).
- A COMMIT goes through `harness::commit_ecoli_edit`: `try_spend` (RNG-free credit check) → `alloc_req_id` →
  journal `RequestEcoliEdit` + `CommitEcoliImpact`. This pair is recorded into the same journal `save_session` /
  replay persist, so the loop is fully replay-reproducible.

**Why hash-neutral / replay-equal (inv #3).** `RequestEcoliEdit` draws ZERO `SimRng` (inert arm);
`CommitEcoliImpact` reads a COMMITTED integer (no oracle call on the hot path); credit accrual is RNG-free + is
never folded into `hash_world` (off-hash); `due_epoch` is a GENERATION COUNT (`epoch_of(gen)+EPOCH_LEAD` — no
`SystemTime`/`Instant` anywhere, so no wall-clock leak). The pinned literal `0x47a0_3c8f_6701_f240` is UNMOVED on
a no-commit run (`oversight_plumbing_is_hash_neutral`); a COMMITTED edit moves the hash DELIBERATELY (the player
acted) and replays byte-equal on a fresh oversight-less env (`renderer_committed_edit_is_replay_equal`) — exactly
like `apply_edit`/`inoculate`.

**Load-bearing divergence (recorded honestly; flagged for a follow-up).** The renderer applies the commit
IMMEDIATELY at the current generation (effect lands on the next `Advance`), whereas the headless
`OversightEpisode` buffers + splices the commit at the future `due_epoch` boundary. BOTH are internally
deterministic + replay-equal, but the same player intent yields different ecosystem TIMING across the two paths,
and the UI "due epoch N" marker label currently implies a deferral the renderer path does not perform. Accepted
for this slice (both paths are deterministic, hash-neutral); a follow-up should EITHER defer the renderer commit
to `due_epoch` OR relabel the UI to immediate-commit semantics. Related off-hash cosmetic note: credit-accrual
sampling granularity differs (renderer per `step(n)`, headless per-gen) — no determinism/replay impact.

**Consequences.** The player can earn → preview → commit E. coli edits in-game; hash-neutral to `0x47a0`. Gate
GREEN; 3-skeptic verify CONFIRMED (5/5 claims at 3/3). **Follow-up UX (tracked in QUEUE `oversight-ui-polish`):**
default the "growth ratio q" knob to `1000` (wild-type/no-op) instead of `0` (growth-lethal); align the
due-epoch marker label with the immediate-commit semantics; re-enable oversight in `load_session` so the ledger
resumes after a loaded session.

---

## ADR-029 — COLONIES: off-hash `dominant_variant_id` channel (GSS6) + heritable `Variant` tag, renderer-derived district polygons + size/zoom LOD pop

**Status:** Accepted (2026-06-29). S1 channel slice landed — gate GREEN; the pinned literal `0x47a0_3c8f_6701_f240`
is BYTE-IDENTICAL at `crates/sim-core/src/lib.rs:3544` (`determinism_hash_is_pinned`) + `:3708`
(`species_signatures_export_is_hash_neutral`), both pins green UNCHANGED (the inserted code shifted them down
~101 lines but touched neither literal). **NOT a re-pin.** 187/187 sim-core determinism tests pass. S2–S6 are
renderer-only and depend on this channel.

**Context.** The play map draws up to `MAX_DOTS_PER_CELL` per non-empty cell → unreadable "spam" at Field scope,
and the per-organism draw cost is the wall bigger maps hit ([[perf-bigger-maps-needs-structural-change]]). The core
already ships an off-hash per-cell `dominant_species_id` projection (GSS5, ADR-021) of the off-hash `Species` tag.
A "colony" is the same construction one level finer: group a contiguous population (incl. brush-created variations)
into one Cities-Skylines district polygon.

**Decision.**
1. Add an off-hash, heritable, spawn-assigned `Variant(u16)` component (default 0 = founding colony of the species),
   minted from a monotonic `NextVariantId` resource — modelled byte-for-byte on the off-hash `Species` tag +
   `NextOrgId`. Inherited by offspring exactly as `Species` is (through `ReproRow`/`Child`/spawn).
2. Project it to a `dominant_variant_id` snapshot channel (GSS6: magic GSS5→GSS6, `CHANNEL_COUNT` 13→14, appended
   LAST so offsets 0..12 never reorder) — the per-cell most-populous Variant ordinal, computed in `snapshot()` by an
   ordinal-sorted per-cell tally (no HashMap, lowest-id tiebreak, zero SimRng), exactly like `dominant_species_id`.
3. A CRISPR brush (`Action::ApplyEditRegion`, already journaled) mints one fresh `Variant` id and stamps it on the
   covered organisms — a 2-line extension of the existing covered loop, no new action, no new wire field, no new RNG
   draw. The disc becomes a nested district; the district keeps its identity as members disperse/reproduce.
   (`region_inoculate` may stamp a fresh id too. Note: the mint is per-EVENT, before the covered loop, so a
   zero-coverage brush still consumes one id — off-hash + harmless.)
4. The renderer (new `colonies.gd`, sibling under `organisms.gd`) derives colony GEOMETRY: deterministic
   connected-components over (`dominant_species_id`, `dominant_variant_id`) → marching-squares/hull contour → fill +
   outline + label. A size×zoom footprint ladder pops selected/large colonies open to the existing `organisms.gd`
   morph glyphs; plants are always-visible, pop first, and render as a soft canopy hull. *(S2–S6.)*

**Invariant audit.**
- inv #2 — core decides per-cell colony IDENTITY (a read-only projection of tags); renderer derives GEOMETRY
  (CC/contour/label = presentation, not biology). No genotype→phenotype in GDScript.
- inv #3 — `Variant`/`NextVariantId`/`dominant_variant_id` are NOT in `hash_world` (which omits `Species` too — the
  off-hash proof); assigned with zero SimRng; snapshot is downstream of the tick; `hash_world`+snapshot sort by
  `OrgId` so archetype order never reaches the hash. The pinned single-species-plant config issues zero
  `ApplyEditRegion` → all orgs stay `Variant(0)` → channel uniformly 0.0 → `0x47a0_3c8f_6701_f240` BYTE-IDENTICAL.
  NOT a re-pin. `actions.ndjson` stays byte-identical (ids are derived from event order, not journaled).
- inv #6 — the brush stays a regional operator action; only its display grouping is new.

**Consequences.**
- (+) de-spam (O(#colonies) draws vs O(cells×5)); the bigger-maps LOD draw lever.
- (+) brush edits are legible as nested districts that survive replay.
- (−) snapshot binary format bumps GSS5→GSS6 (render format, independent of `hash_world`); every 13-channel
  reader/gate moves to 14 in the same slice (`godot/snapshot.gd` + `tools/check_godot_snapshot.sh` +
  `livesim_smoke.gd`).
- (−) u16 variant-id ceiling (65 535 brush edits/run) to preserve exact u16-in-f32 round-trip; per-run; documented.

**Alternatives considered.**
- Lens C: derive `colony_id` by connected-components over (`dominant_species_id`, `allele_band`) with NO new core
  tag. REJECTED: loses district identity when the edited disc mixes/drifts; band-thrashing flicker; conflates
  identity with geometry and puts presentation geometry in the core. (Kept its render LOD ideas.)
- u32 variant id: REJECTED — silently loses precision in the f32 channel unless widened to two planes (overkill for
  a PoC).
- Hold variants in a parallel off-hash `Vec<u16>` side-table instead of an ECS component: fallback only (fiddlier
  across births/deaths); the component is provably outside the hashed set, so it is preferred.

---

## ADR-030 — Gem replay fidelity: resolution stays in core (`gem_edit_schedule`) + off-hash `Gem.gens_requested`

**Status:** Accepted (2026-06-28).

**Context.** Load-gem-replay lets the renderer reconstruct + play a discovered gem (config + scheduled CRISPR
edits) so the player WATCHES the scenario. The first attempt resolved the mid-run edits IN GDSCRIPT and diverged
from the search's `edits_to_actions`: it passed the bare search `target` instead of `loci[edit.target % loci.len()].id`
(81/147 gem edits failed `UnknownTargetLocus` — silent no-ops the gem had scored as *applied*), and it computed
`gen_abs` from `gem.gens` (the early-stop trace length) instead of the search horizon `gens_requested` (which was
never serialized). The renderer-only gate missed it (the `--gem` smoke reported *dispatched*, not *applied*); the
3-skeptic adversarial verify caught it (`replays_gem_config_and_edits` 0/3 → RED).

**Decision.**
1. The edit RESOLUTION lives in CORE, never re-derived in GDScript: a read-only `godot-sim`
   `gem_edit_schedule(gem_json) -> [{gen_abs, cas, target, guide, species}]` `#[func]` that **reuses
   `harness::edits_to_actions`** (the SAME `loci[edit.target % loci.len()].id` target, the SAME
   `gen_abs = edit.gen * gens_requested / 65536`, the SAME `species_index → SpeciesId`). The renderer only moves the
   resolved ints/strings into the existing `apply_edit` and fires each at its `gen_abs` (before that gen's `Advance`,
   matching `capture_trace`/`build_journal`). Keeps biology/resolution in core (inv #2); the v1 GDScript divergence
   cannot recur.
2. `Gem.gens_requested: u32` is serialized (the LAST field, `#[serde(default)]`) so the replay uses the search
   horizon, not the early-stop length. OFF-HASH metadata: `Gem` lives in gitignored `data/runs`, the field is never
   folded into the run `recorded_hash`, and old gems (no field) deserialize to `0` → the loader falls back to
   `gem.gens` (documented divergence for pre-fix gems). The pinned literal `0x47a0_3c8f_6701_f240` is UNMOVED.

**Consequences.** A discovered gem replays byte-faithfully to what the search scored (a correctly-resolved
real-locus edit can still legitimately fail the CRISPR PAM/on-target gate — that faithfully reproduces the captured
outcome, not a bug). Known coupling: the core resolver reads repo-root `data/species` while the renderer config
replay reads `res://data/species` — byte-identical via the staged mirror (gated by `check_godot_snapshot.sh`); a
divergence between the two roots would desync the replay. Follow-up: wire a gated headless `--gem` smoke (asserting
`applied==total>0` on a gem WITH edits) so edit fidelity is covered by CI, not only the manual smoke + the 3 core
tests. Gate GREEN; 3-skeptic verify CONFIRMED (4/4 at 3/3).

---

## ADR-031 — Starter-map library: committed gen-1 + gen-N-checkpoint content from the auto-research

**Status:** Accepted (2026-06-29). The capstone of the auto-research → playable-content loop (curated gems →
`proposals/starter-candidates.json` → committed starters).

**Context.** The discovery search produces round-trip-verified gems (config + scheduled edits). To make them
playable + browsable, a `promote` tool (`crates/harness/src/promote.rs`) turns a curated gem into a **committed**
starter under `data/presets/starters/`, surfaced by an RCT-style selector (`godot/gallery.gd`).

**Decision — two reproducibility tiers:**
1. **GEN-1 (`<slug>.json`)** — a fresh config (roster + env + containment) + provenance (`source_hash`, `source_seed`).
   The gem's edits are **dropped** (a gen-1 starter is pristine — the player starts the discovered *community*, not a
   mid-run edited state). Inert data; loaded via the Load Starter path.
2. **GEN-N CHECKPOINT (`<slug>/`)** — the gem replayed to gen N via the EXISTING `record_episode` so the scheduled
   edits are RECORDED in the session journal (`seed.json` + `actions.ndjson`) — a developed state with the
   interventions on the scrub-back timeline; loaded via `load_session`. Round-trip-verified (`record == replay`)
   BEFORE writing.

**Hash-neutral.** The promote tool is meta-level (pure config + the existing `record_episode`/`replay`/`save_journal`,
no `SimRng`/`HashMap`); the committed data is inert; the gallery is renderer-only (inv #2). Pinned literal
`0x47a0_3c8f_6701_f240` unmoved (sim-core 184/184). New on-disk format under `data/presets/starters/` + `index.json`,
staged into `res://` (recursive `run.sh` + byte-gated).

**Known trap (tracked → `starter-promote-hardening`).** `promote_gen1` copies `source_hash = gem.recorded_hash` but
drops the gem's edits **without enforcing the source gem is edit-free**. This is correct **today only because CRISPR
edits are currently hash-neutral** (identity-gated — `committed_neutral_edit_does_not_move_the_run_hash`); the 6
shipped gen-1 starters are all edit-free / no-op-edit (verified: each replays to its `source_hash`). **When edits
become hash-active, a gen-1 starter promoted from an edited gem would silently stop replaying to its `source_hash`.**
The fix (queued): reject firing-edit gems in `promote_gen1`, OR recompute the gen-1 `source_hash` from an edit-free
replay; also store `gens` (+ an edit flag) in the gen-1 doc so it is self-contained re-verifiable. Gate GREEN;
3-skeptic verify CONFIRMED (5/5 at 3/3); committed library empirically replay-verified.

---

## ADR-032 — Scenario GIF preview: renderer `--shot` capture + in-process pure-Rust GIF assembly

**Status:** Accepted (2026-06-29). The CAPTURE + ASSEMBLE half of the scenario preview, on the off-hash Stage-1
KEY-EVENT schedule (`crates/harness/src/keyframe.rs`). The `.gif` consumer (gallery preview) already exists
(ADR-031: "the GIF if present, else a live replay").

**Context.** A discovered gem replays a *story* (boom/crash/takeover, scheduled edits, immigration). The
`keyframe` detector picks the KEY generations to snapshot (the SAME ecology events the D0 scorer's M5 rewards). To
turn those into a browsable animated preview next to a starter we need (a) one rendered frame per key gen and (b)
an animated-GIF assembly — without crossing any invariant.

**Decision.**
1. **CAPTURE = the renderer's `--shot`, reused via the discovery-load-gem-replay loader** (`tools/make_starter_gif.sh`).
   For each key gen the script REPLAYS the gem (`godot … --gem <abs> --steps N --shot frame.png`, INCLUDING its
   mid-run edits) and shoots ONE PNG. **macOS-safe** (the load-bearing capture discipline): a `$(godot…)` PIPE
   capture HANGS on macOS (a child holds the stdout pipe past Godot's exit), so frames are captured to a FILE under
   `timeout`, never a pipe (mirrors `check_godot_snapshot.sh`). `--shot` needs a GPU/display → WINDOWED, not
   `--headless`; on a no-display box the script SKIPs cleanly (exit 0), like the UI gate.
2. **A minimal renderer hook (inv #2 preserved):** `--gem --shot --steps N` advances the freshly-loaded gem N gens
   firing each due CORE-resolved edit + scheduled immigration (the SAME `_process` interleave) before the shot, so a
   per-keyframe frame is DEVELOPED incl. edits. It only drives existing core surfaces (`_live.step` +
   `_fire_due_gem_edits` + `_fire_due_immigration`); NO genotype→phenotype/biology in GDScript. Each shot is a
   separate process → a deterministic pure function of `(gem, N)`.
3. **ASSEMBLE = the in-process, GPL-clean (inv #1) `gif` encoder** (`crates/harness/src/gifenc.rs` + the
   `--keyframes`/`--assemble-gif` CLI). PNGs are decoded with `png`, nearest-neighbour downscaled to a small
   thumbnail (default longest side 480px), NeuQuant-quantized (`color_quant`, `gif`'s default feature), and written
   as a LOOPING GIF (default 30cs/frame → a ~3.6s 12-frame loop, inside the readable ~2-4s window). The slice's
   documented fallback (an external `imagemagick`/`ffmpeg`) would have to be a SUBPROCESS at the boundary; the
   pure-Rust path is light enough that we never take it. **Pinned (inv #7): `gif = 0.13`, `png = 0.17`** (both
   MIT/Apache, image-rs; their closure — `color_quant`/`weezl`/`miniz_oxide`/`flate2`/`fdeflate`/`adler2`/
   `crc32fast`/`simd-adler32`/`bitflags` — is GPL-free, license gate GREEN). The detached `crates/godot-sim` (which
   deps `harness`) links them too; its Cargo.lock was refreshed.
4. **Index hook:** the preview lands at `data/presets/starters/<slug>.gif` — next to the committed `<slug>.json` —
   so `gallery.gd` finds it at `res://data/presets/starters/<slug>.gif` (it already reads exactly that path). It is
   staged into `res://` by the SAME recursive `cp -R data/presets/.` run.sh + the byte-gate already do; no new
   staging. The `.gif` is a GENERATED artifact (`.gitignore`d, re-buildable from the gem), never committed; the
   index (`*.json` scan) ignores it.

**Hash-neutral (inv #3).** The whole pipeline is off-hash: `--keyframes` runs only the proven hash-neutral trace
capture; `gifenc` is pure post-processing of inert PNG bytes; the renderer hook only steps the existing
deterministic core. Pinned literal `0x47a0_3c8f_6701_f240` unmoved (sim-core 184/184, harness 101/101 incl. 5 new
gifenc tests). The headless smoke (`gifenc::tests::encode_writes_a_valid_looping_multiframe_gif`) asserts a valid,
non-empty, >1-frame GIF without a GPU; the full pipeline was empirically run end-to-end (6 real `--shot` frames of a
gem → a 480×404 6-frame looping GIF). Gate GREEN.

**Consequences / known gaps.** Godot 4 has no GIF-from-buffer decoder, so the gallery's IN-ENGINE animated playback
of the `.gif` is best-effort (it falls back to a live replay) — the `.gif` is still a valid artifact for external
tooling / a future Godot GIF loader. The capture is environment-gated (needs a GPU/display); CI without one SKIPs,
and the gate's coverage of the assembly is the pure-Rust `gifenc` smoke. Real captured frames are colour-rich → a
6-frame preview is ~300KB even downscaled; a future tighter palette / fewer frames / smaller `--gif-max-dim` would
shrink it.

---

## ADR-033 — Discovery steering target: the drama-weighted `D` (M3+M5 ≈ 78%) + clean steer/curate separation (D3-B.2)

**Status:** Accepted (2026-06-30). The first brute-force batch showed **M1 (coexistence) SATURATES** — once most
runs coexist, M1 stops discriminating, so the raw quality `Q` (which spends ~46% of its weight on M1/M2) no longer
separates a *dramatic* run from a *placid* one. This slice defines the steering target the surrogate (D3-B.3/B.4)
will predict. It is **off-hash, pure-integer scorer math** (`crates/discovery/src/surrogate.rs`); the pinned literal
`0x47a0_3c8f_6701_f240` is byte-identical and `cargo tree -p discovery` stays `std`+`serde` only (inv #5).

**Context.** `discovery` already computes a 6-metric `ScoreVec.breakdown:[u16; 6]` (M1 coexistence · M2 evenness ·
M3 dynamism · M4 trophic · M5 events · M6 survival-gate) and curates gems by `final_score = Q × novelty`. To make an
*autonomous emergent-run search* hunt the INTERESTING runs (limit-cycles / cascades = high M3 dynamism + M5 events),
the surrogate needs a target that rewards drama, not stable coexistence.

**Decision.**
1. **`DramaWeights`** — a serde `Serialize/Deserialize` struct (modelled byte-for-byte on `ScoreParams`, retune-
   without-code, inv #7) with `w1..w5: u64` + a `wsum()` (M6 excluded), a `version: u32` (`#[serde(default)]`) +
   `DRAMA_WEIGHTS_VERSION = 1` re-pin self-invalidation anchor. **PINNED default `{w1=8, w2=4, w3=40, w4=8, w5=32}`**
   (sum 92; `w3+w5 = 72/92 = 78%` of the weight on dynamism + events, vs ~46% in `Q`).
2. **`drama_target(breakdown: &[u16; 6], &DramaWeights) -> u64`** = `((Σ wᵢMᵢ for i∈1..5)/wsum().max(1)) · M6/SCALE`
   — EXACTLY the `Q` combine shape (`ecology.rs:70-71`) with the drama weights. Pure integer (zero f64), no RNG, no
   HashMap; M6 stays the unchanged multiplicative instant-death gate (M6→0 crushes `D` to 0). Strictly monotone
   non-decreasing in every metric, strictly increasing in M3 & M5 over any non-trivial change (gate open).
3. **CLEAN STEER/CURATE SEPARATION (load-bearing).** `D` is a NEW, SEPARATE target. `Q` / `final_score` /
   `final_score_with` / `ScoreParams` / the gem-curation path are **UNCHANGED** (byte-identical, tests pass). The
   surrogate STEERS the search by predicting `D`; the library still CURATES gems by `final_score` (Q × novelty). A
   test proves the divergence: `D` ranks a dynamic run above a placid-but-coexisting one where `Q` ranks them
   opposite.

**Invariant audit.** inv #3 — off-hash, pure-integer, deterministic (no f64/RNG/HashMap); the pinned literal is
unmoved (the target reads only the inert `[u16; 6]` breakdown; `discovery` has no `sim-core`/`harness` dependency).
inv #5 — science stays behind the existing scorer surface; `discovery` adds no dependency (`std`+`serde`). inv #7 —
the weights are a pinned, versioned, ADR-recorded tunable.

**Scope / sequencing of the human sign-off.** This slice only **DEFINES** the target — it changes **no search
behaviour** and is **not yet wired into the loop** (`discover_evolved` is untouched; the steered sibling lands in
D3-B.4). The drama-weighted default directly encodes the standing user memory `no-hardcoded-balance-open-system`
("steer toward living dynamics, not forced stability") and the design judge picked it; it is also fully reversible
(swap the steering target in one place). The broader steering-target sign-off flagged in `surrogate-model-spec.md`
(open-question #1: drama `D` vs raw `Q`) bites where steering actually changes emergent output — **wired into D3-B.4
(steered loop) and gated as a dependency of D4 (batch-showcase)** per `QUEUE.md`; that behavioural decision will be
surfaced to the human before D4 generates the committed showcase. Pinning the constants here does not pre-empt it.

**Alternatives considered.** Raw `Q` as the steering target: REJECTED — M1 saturation makes it blind to drama
(the empirical motivation). Replacing `Q` with `D` for curation too: REJECTED — curation wants the
coexistence/quality criterion; conflating steer + curate loses the "hunt drama, keep a quality library" property.
A float target / matrix-inversion fit: REJECTED — cross-platform non-determinism (inv #3); integer GD lands in D3-B.3.

---

## ADR-034 — Discovery surrogate model: `RidgeInt` (integer ridge regression, fixed-point GD) + the pluggable `Surrogate` trait (D3-B.3)

**Status:** Accepted (2026-06-30). The model the steered loop (D3-B.4) fits on the eval log to predict the
drama target `D` (ADR-033). Off-hash, pure-integer (`crates/discovery/src/surrogate.rs`); the pinned literal
`0x47a0_3c8f_6701_f240` is byte-identical and `cargo tree -p discovery` stays `std`+`serde` only (inv #5).

**Context.** The discovery regime is **tens-to-hundreds of evals over 28 features** → variance-bound, so the
lowest-variance model (a linear regressor) wins; the hand-crafted interaction features (predator×prey,
temp-extremity) supply the only nonlinearity drama needs. A float fit (matrix inversion / float GD) is
cross-platform non-deterministic (inv #3), so the model + its training must be **pure integer**.

**Decision.**
1. **`Surrogate` trait (inv #5 seam)** — `fn fit(&mut self, x:&[FeatureVec], y:&[u64], seed:u64)` ·
   `fn predict(&self, &FeatureVec)->u64` · `fn id(&self)->&'static str` · `fn min_samples(&self)->usize`.
   Object-safe; impls are swappable without touching the search. **`NullSurrogate`** = the base case (`fit` no-op,
   `predict` constant 0, `min_samples()==usize::MAX` so a steered run COLD-STARTS to passthrough = byte-identical
   to `discover_evolved`). **`RidgeInt`** = the default. The **`BoostStumpInt`** upgrade (when the log exceeds
   ~300 rows) and any heavy ML (XGBoost/LightGBM at a **subprocess boundary** crate, never linked — inv #1) stay
   behind the same trait, deferred.
2. **`RidgeInt`** — integer ridge LINEAR regression. `θ` (length `FEAT_DIMS=28`) is `i64` on **`THETA_SHIFT=16`**;
   predict is a pure-integer dot product `(θ·x) >> THETA_SHIFT` clamped to `[0, SCALE]`. fit: **sort the rows once**
   by `(y, features)` (→ row-order-independent: the batch gradient sums are commutative integer sums), then run a
   **PINNED `N_ITERS=2000`** fixed loop (no float early-stop) of fixed-point gradient descent on the ridge MSE —
   every dot-product / gradient sum uses **i128 accumulators** (overflow-safe), the per-iteration data step is
   `Σ rⱼ·xⱼᵢ / (n · 2^LR_SHIFT)` with **`LR_SHIFT=11`**, plus a decoupled L2 decay `θᵢ −= θᵢ / 2^RIDGE_LAMBDA_SHIFT`
   (`RIDGE_LAMBDA_SHIFT=8`, non-bias). **Zero f64** on train or predict. Serde, with `RIDGE_MIN_SAMPLES=FEAT_DIMS`
   and a **`RIDGE_BUILD_ID = "ridgeint-v1@dims28-shift16-iters2000"`** self-invalidation anchor (travels with a
   serialized model; a re-pin bumps the id so a stale model self-detects — like `ENCODER_ID` /
   `DramaWeights.version`).

**Invariant audit.** inv #3 — off-hash, pure-integer, deterministic + row-order-independent (sort-once + i128
commutative sums; no f64/RNG/HashMap); the pinned literal is byte-identical (`discovery` has no `sim-core` dep; the
diff touches only `surrogate.rs` + the `lib.rs` re-export). inv #5 — the science is behind the `Surrogate` trait;
`discovery` adds no dependency (no `ndarray`/`nalgebra`/`linfa`/`smartcore`/heavy-ML/GPL — those reach `harness` only
via `bio→crispr`, never `discovery`). inv #7 — the GD constants are pinned + the `RIDGE_BUILD_ID` anchor records them.

**Scope / sequencing.** This slice **DEFINES** the model — it changes **no search behaviour** and is **not wired
into the loop** (`discover_evolved` untouched; `search.rs` not in the diff). The steered sibling lands in D3-B.4;
per ADR-033, the behavioural model-choice + steering-target sign-off (spec open-questions) is surfaced to the human
before D4 generates the committed showcase. The model is fully reversible behind the trait.

**Known provisional / follow-ups (non-blocking).** `LR_SHIFT=11` is a single global learning-rate shift that learns
heterogeneous binary/one-hot features slowly (spec open-question #4) — acceptable here because the planted-signal
test converges and `RIDGE_BUILD_ID` self-invalidates a stale model on any re-pin; a real-eval-log convergence pass
(possibly a per-feature scale or more iters) is a D3-B.4-adjacent retune. The final `θ` write-back narrows
`i128→i64` by a deterministic two's-complement cast (the accumulators are i128-guarded and `θ` stays within range
under the pinned dynamics); a `saturating` cast / `debug_assert` is a cheap future hardening.

**Alternatives considered.** Float GD / closed-form matrix inversion: REJECTED — cross-platform non-determinism
(inv #3). A nonlinear model now (GBT / NN): REJECTED for the variance-bound small-N regime — deferred behind the
trait as `BoostStumpInt`, with heavy ML kept at a subprocess boundary (inv #1). Linking a Rust ML crate
(`linfa`/`smartcore`): REJECTED — drags a heavy dep tree into the in-process `discovery` crate (inv #5); the
hand-rolled integer regressor is ~200 lines and determinism-exact.
