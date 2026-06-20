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
| Godot | **4.7** (4.7.stable.official, commit `5b4e0cb0`) | **installed (Stage 4, S4.1)** | Thin 2D UI, built LAST (inv. #4); `tools/install_godot.sh`; `godot` on PATH (brew cask). GDScript reads snapshots only (inv. #2). |

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

## Baseline benchmarks — perf threshold (SPEC §11, §10.7)

Reference platform: Apple M4 Max, native aarch64, `release` profile (`lto = "thin"`, `codegen-units = 1`).
Source: `cargo bench -p sim-core` (`crates/sim-core/benches/tick.rs`), run via `GATE_BENCH=1 tools/gate.sh`.
The perf gate (§10.7) fails on a regression **below the CURRENT baseline**. Re-baseline at each stage that
changes the hot path, in the same slice (this is anticipated — see the Stage 0 row note).

### Current baseline — Stage 1 exit (slice S1.5): empty loop + Wright-Fisher selection
| Workload (entities × generations) | Median wall time | Throughput |
|---|---|---|
| 1 000 × 50  | **1.645 ms** | ~30 M organism-updates/s |
| 5 000 × 50  | **10.48 ms** | ~24 M organism-updates/s |
| 10 000 × 50 | **25.97 ms** | ~19 M organism-updates/s |

**Headline (current):** ~**19 M organism-updates/s** at 10 k entities (10 k advance 50 generations of
*real selection* in ~26 ms ⇒ ~1.9 k generations/s). The ~9× drop vs the Stage 0 row is **expected**: S1.5
added the per-generation Wright-Fisher selection step (cumulative-fitness sampling + write-back) where Stage 0
had an empty loop. Still far from the SPEC §11 trigger to move to GPU / cohorts. Known cheap win (tracked in
TASKS follow-ups): the selection write-back uses a `BTreeMap` (O(N log N) + allocation); a `Vec` indexed by the
contiguous `OrgId` would be O(N) — would lift this baseline materially when done.

### Historical — Stage 0 (slice S0): empty deterministic loop (no selection)
| 1 000 × 50 → **302.6 µs** · 5 000 × 50 → **1.438 ms** · 10 000 × 50 → **2.856 ms** (~175 M updates/s). |
| Superseded by the Stage 1 row above once real selection landed; kept for the record. |
