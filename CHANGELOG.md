# Changelog

All notable changes per slice. One slice = one entry. Format loosely follows Keep a Changelog.

## [Unreleased]

### R1.0 — terrain/soil substrate: hash-neutral static SoilField (feat, roadmap R1; multi-agent designed)
Multi-agent designed (3 scoping lenses → adversarial vetting against determinism/ADR-005/perf/snapshot →
synthesis) + human sign-off. First slice of the terrain epic — **substrate only, provably hash-neutral**:
- `crates/sim-core/src/soil.rs`: a static `SoilField` (moisture / nutrients / pH, each `[0,1]`) generated
  once in `Simulation::reset` from `derive_seed` (value-noise over a 5×5 lattice, multiply-add only) — **zero
  `SimRng` draws**, never folded into `hash_world`. Plus an `EnvironmentModifier` trait (invariant-#5 seam) +
  `LinearTraitMatchModifier` default, present but **unwired** (coupling is R1.1+).
- Snapshot gains **3 read-only soil channels**: `CHANNEL_COUNT` 3→6, magic **GSS1→GSS2** (loud bad-magic on a
  stale reader). `godot/snapshot.gd` is **parse-only**; the click-detail panel now shows per-cell soil values
  (no shader/overlay — "Godot LAST" respected).
- **Determinism proven:** a new test pins the exact pre-soil hash literal (`0xc530…7ab1`); matching it on the
  with-soil build proves soil is hash-neutral (guards the `check_determinism.sh` silent-change gap). Perf
  within criterion noise (no re-baseline; soil gen is off the hot loop). ADR-008 + a `derive_seed` stream registry.

### UI/controls + visual polish round (A+C; feat/refinement, Stage 4) — multi-agent designed
Designed + adversarially vetted by a multi-agent **workflow** (parallel design → invariant-#2/Godot-4.7-API
review → synthesized gated plan), then implemented serially (one slice → headless `--check` → `tools/gate.sh`
9/9 → windowed `--shot` visual check → commit). All read-only presentation (invariant #2); the determinism
hash is unchanged throughout.
- **S1 / C1 — plant polish** (`lsystem.gd`): leaves render as teardrop polygons oriented along the live tip
  heading; fecundity-driven flowers (petal ring + centre); ground line + 16-gon shadow under each base. All
  geometry precomputed in `build()` so the headless gate catches malformed polygons; `bounds()` unchanged.
- **S2 / A1 — specimen UX** (`main.gd`): a top-right panel — specimen selector (`OptionButton`) + a 5-trait
  readout (ProgressBar + value + **delta-vs-baseline** arrow ▲/▼/=). Focusing brightens the chosen plant,
  dims the rest, and frames the camera. Tab cycles; `--focus <i>` for deterministic `--shot`.
- **S3 / A2 — ecosystem controls** (`main.gd`): a second control-bar row — playback-speed slider (runtime
  `_frame_seconds`), zoom-scope toggle buttons (Field/Patch/Cells, synced to the camera), and a generation
  scrubber (bidirectional, `set_value_no_signal` + a re-entrancy guard). Step/scrubber disable in the
  specimen view; window margin bumped so the two-row bar is fully on-screen.
- **S4 / C2 — ecosystem polish** (`organisms.gd`, `main.gd`, `data_layer.gdshader`): softer organism markers
  (halo + core); richer grass (per-pixel blade streaks); a screen-space edge **vignette** (CanvasLayer 1
  below the UI at layer 2; hidden in the specimen view); and an overlay **alpha-gamma** curve in the shader
  (smoother heat — the `inferno(v)` colour mapping stays byte-identical, only alpha is shaped).

### S4.5 — L-system plant morphology + UI controls (feat, Stage 4) — **Stage 4 COMPLETE**
- **Core export** (`harness --specimens <DIR>` → `specimens.json`): the species-genome **trait vector**
  (baseline) plus one per demo CRISPR edit, each expressed by the core's `WeightedSumMap` GP map via a
  separate `GeneSimEnv` (its own seeded RNG — never the hashed run, so **no determinism-hash impact**,
  inv. #3). Any edit outcome (Applied *or* Failed) mutates the genome, so every specimen's traits differ
  from baseline — genotype→phenotype stays in the core (inv. #2).
- **L-system renderer** (`godot/lsystem.gd`): a parametric bracketed turtle-graphics plant (ABOP grammar)
  drawn from **numeric params only** — pure presentation, zero biology. `main.gd::_plant_params_from_traits`
  maps each trait → a visual param (growth→size/reach, reflectance→spread+leaf hue, drought→taper+tip colour,
  fecundity→leaf size, kill-switch→jitter). The genome→trait math is the core's; trait→appearance is the
  renderer's job (SPEC "L-system rule params").
- **Specimen view** (key `V` / the View button): renders baseline + edited plants side by side with captions
  — an edit **visibly** stunts (growth knockdown) or greens-and-grows (kill-switch/reflectance) the plant.
- **UI control bar:** view toggle (Ecosystem ⇄ Specimen), play/pause, step ◀/▶, and a data-layer dropdown —
  all change *view* state only (no biology). Keyboard shortcuts still work and stay in sync.
- The gate's headless `--check` now also builds the L-system specimens (catches GDScript errors in CI); the
  gate generates `specimens.json` for the check. Full gate green; determinism hash unchanged. ADR-007.

### S4.3/S4.4 visual polish (refinement, Stage 4)
- **Heatmap palette:** the data-layer shader now uses an *inferno* ramp (indigo→purple→red→orange→yellow)
  that contrasts with the green field instead of the muddy blue→cyan over grass.
- **Organisms** (`organisms.gd`): markers get a white specular core + darker rim and a palette off the grass
  green (cyan→magenta→red by allele_freq); fitter cells render slightly larger — far more legible.
- **Grass** (`main.gd`): terrain shade comes from a coarse block (grassy patches, not per-tile checker noise)
  with an occasional single-cell speckle and a darker soil tone.
- **HUD:** the status line sits in a translucent panel; a new bottom-left **legend** shows the active layer
  name + the colormap gradient (low → high). All read-only presentation (invariant #2); gates unaffected.

### S4.4 — data-layer shaders + zoom scopes (feat, Stage 4)
- `godot/data_layer.gdshader` (canvas_item): samples the per-cell data texture the core produced
  (R=density, G=allele_freq, B=fitness via `snapshot.gd::to_data_image`) and maps the channel chosen by a
  `layer` uniform through a heat colormap on the GPU — replacing the S4.3 CPU `_heat` loop. INVARIANT #2
  intact: the shader only **visualises** values the core already computed.
- **≥2 toggleable data layers:** `D` cycles off → density → allele_freq → fitness (the shader `layer`
  uniform); the overlay `Sprite2D` uses NEAREST filtering so each texel is one crisp cell.
- **Viewport zoom scopes:** mouse-wheel = continuous zoom; keys `1`/`2`/`3` jump to scope presets
  (field ×1 / patch ×2.6 / cells ×6); arrows pan. HUD shows the live layer + scope + magnification. The
  zoomed "cells" scope makes individual organism dots and per-cell data legible.
- `--shot` gains `--layer <0..3>` and `--zoom <f>` so each layer/scope can be captured for visual review.
  Verified by windowed screenshots of the allele_freq, fitness and zoomed-density views; the headless
  `--check` render smoke (gate 9/9) now also builds the `ShaderMaterial` path. Cargo gates + determinism
  hash unaffected. (Renderer architecture: ADR-006.)

### S4.3 — 2D ecosystem view: live run render from snapshots (feat, Stage 4)
- `godot/main.gd` now builds a **2D ecosystem view of one scope** in code (all read-only — invariant #2):
  a tiled **grass field** (`TileMapLayer` from a procedurally-generated shade atlas), a per-cell **data
  overlay** (`Sprite2D` heat texture: density / allele_freq / fitness), an **organism dot layer**
  (`godot/organisms.gd`: per-cell markers, hue=allele_freq, brightness=fitness, count∝density — hash-jittered
  scatter is presentation only, not a spatial model), a framing `Camera2D`, and a HUD (gen / pop / grid / layer).
- **Live run playback:** `--run <dir>` loads every `snap_*.bin` ordered by generation and auto-advances on a
  timer (loops); with no args + a display it auto-discovers the newest `data/runs/<id>/` holding snapshots.
  Keys: Space pause · D cycle overlay (off/density/allele/fitness) · `,`/`.` step. The gen-0→gen-60 render
  visibly tracks selection (more amber organisms + warmer overlay as allele_freq shifts).
- **Verification harness:** windowed `--shot <png> [--gen N]` captures the real viewport to PNG (human/agent
  eyeballing); headless `--check` builds the scene and prints `render scene OK` (no GPU). The Godot gate
  (`tools/check_godot_snapshot.sh`, step 9/9) now runs **both** the S4.2 reader check and the S4.3 render
  smoke — catching GDScript parse/logic errors in CI. Fixed a `:=` type-inference parse error (untyped
  `Array` index → `Variant`). Determinism hash unchanged; cargo gates unaffected. See ADR-006.

### S4.2 — snapshot reader: Rust→GDScript render bridge (feat, Stage 4)
- `crates/sim-core/src/snapshot.rs`: `GridSnapshot` — a **derived, read-only** per-cell grid
  (`density` / `allele_freq` / `fitness`, each `[0,1]` row-major) produced by `Simulation::snapshot(w,h)`.
  Placement is a pure function of `OrgId` (splitmix, no RNG draw, no mutation) → byte-identical for a fixed
  `(seed, generation, grid)` and **cannot** change the determinism hash (invariant #3). `std`-only binary
  format `"GSS1"` (LE header + 3 channel-major `f32` planes); round-trip + read-only tests in-crate.
- `harness --snapshots <DIR> --grid WxH`: writes `snap_<gen>.bin` per epoch + final, off the hash path (additive).
- `godot/snapshot.gd` (**read-only**, invariant #2): parses `GSS1` bytes → channels + `to_data_image()`
  (RGBF data texture for the S4.4 shader). `godot/main.gd --snap <file>` loads one headless and reports
  `WxH, gen, population, cells, channels`.
- **Headless robustness fix:** dropped the `class_name Snapshot` global (only registered by an editor import
  pass, so unresolved under a fresh `--headless` run) in favour of `preload` + a self-preload const — the
  reader now parses cleanly with no `.godot/` cache.
- New gate **9/9** `tools/check_godot_snapshot.sh`: generates a snapshot with the headless core and asserts
  the Godot reader reports `snapshot OK`; SKIPs when godot is absent (mirrors the slim oracle gate). Enforces
  invariant #4 for the first UI feature and locks in the headless fix. Determinism hash unchanged.

### S4.1 — Godot UI skeleton + headless smoke (chore, Stage 4; human-signed-off 🛑)
- `godot/` thin 2D project (Godot **4.7**, GL-compatibility): `project.godot`, `Main.tscn`, `main.gd`. The
  script is **read-only** — boots, prints version, exits under headless (invariant #2: no biology in GDScript).
- `tools/install_godot.sh` (SPEC §W3): brew-cask install + version check + `godot --headless --path godot --quit`
  smoke. Godot pinned 4.7 in DECISIONS (commit `5b4e0cb0`). Build-order gate satisfied — core is headless +
  deterministic through Stage 3 (invariant #4). UI-only slice; cargo gates unaffected, verified via the Godot
  headless smoke (`UI booted … headless smoke OK`).

### S3.3 — parallel batch runner + columnar Parquet stats (feat, Stage 3)
- `harness --per-gen-stats`: drives the stepwise `Simulation` and writes `data/runs/<run_id>/per_gen.csv`
  (run_index, generation, population_size, allele_freq + 5 trait columns), additive — final stats hash
  unchanged (proven). `run_id` for `--run-index` now keyed `_i{index}` so parallel jobs don't collide.
- `tools/run_batch.sh [MASTER] [RUNS] [GENS]` (SPEC §W7): builds release once, runs `target/release/harness`
  in parallel via `xargs -P $(nproc)` over derived seeds. **Two batches → byte-identical per_gen.csv** (reproducible).
- `scripts/aggregate_parquet.py` (pyarrow): globs `data/runs/*/per_gen.csv` → one columnar **Parquet**
  (pinned schema, lossless concat). Verified: 8 runs → 400 rows × 9 cols.
- `pyarrow 24.0.0` pinned (`scripts/requirements.txt` + DECISIONS row; Apache-2.0, analysis-only, never linked).
  Determinism hash unchanged (`fde0e0b6…`). Loop: implementer (Rust+shell) + orchestrator (Python) → gate
  (GREEN) → reviewer (send-back for the pyarrow pin → recorded → APPROVE).

### S3.2 — replay logs: seed.json + actions.ndjson (feat, Stage 3)
- `crates/harness/src/replay.rs`: `record_episode(config, seed, actions, dir)` writes `data/runs/<run_id>/`
  `seed.json` (master seed + config + pinned tool versions, SPEC §5) + `actions.ndjson` (one `Action`/line);
  `replay(dir)` re-runs and returns the final stats hash. Record & replay share one private `run_episode`, so
  **replay is bit-identical by construction** (SPEC §6). Deterministic `run_id` (no wall-clock).
- serde plumbing: `genome::LocusId` (`#[serde(transparent)]` u32), `crispr::GuideSequence` (hand-rolled serde —
  deserialize routes through `GuideSequence::new`, so a non-ACGT guide in a log fails to load), `Action`/
  `EditAction` derive serde. `serde_json` added (workspace dep, MIT/Apache; DECISIONS row).
- Determinism hash unchanged (`fde0e0b6…`). Tests: record→replay bit-identical, malformed-guide rejected,
  action_count mismatch rejected, serde round-trips. Loop: implementer → gate (GREEN) → reviewer (send-back
  for the `serde_json` pin → recorded → APPROVE).

### S3.1 — gym-like environment (reset/step/seed) (feat, Stage 3)
- `crates/sim-core`: public stepwise `Simulation` handle (`reset`/`step`/`observe`/`species_genome`/
  `with_genome_and_rng`) + public `Observation { generation, population_size, allele_freq, phenotype }`.
  `run_headless` reimplemented on top of it — **bit-identical** (determinism hash unchanged `fde0e0b6…`).
- `crates/harness` (now lib+bin): `Env` trait (`reset/step/seed`) + `GeneSimEnv`; `Action { Advance(u64),
  ApplyEdit(EditAction) }` — **species/operator-granular only** (invariant #6; per-organism actions
  unrepresentable). `ApplyEdit` runs `crispr::apply_edit` on the species genome and re-expresses phenotype.
- Determinism (inv. #3): one ChaCha8Rng seeded once in `reset`, threaded through step + edit via
  `std::mem::replace` (stream position preserved — no re-seed/clone). reward = `allele_freq` ∈ [0,1].
- Tests: stepwise==one-shot, observe-is-pure, edit-changes-phenotype, reset/step/seed cycles, replay
  determinism (+proptest). Loop: implementer → gate (GREEN) → reviewer (APPROVE).

### S2.4 + S2.5 — golden oracle gate + license gate (feat, Stage 2; **Stage 2 complete**)
- **S2.4** golden oracle gate (SPEC §10.6): `data/golden/slim_case1.json` records the stats for a pinned case
  (seed 1234 + the produce_trees params, SLiM v5.2). `slim_analyze.py --check` compares a fresh run to the
  golden (integer fields exact, floats within rel-tol 1e-6); `tools/check_slim_oracle.sh` drives it and skips
  gracefully if slim/.venv/golden are absent. Wired into `tools/gate.sh` as gate 7/8. Verified: passes on a
  fresh run, fails on a tampered golden. This pins the genetics to SLiM v5.2 (re-record + ADR on a version bump).
- **S2.5** license gate — already delivered in the dev-loop hardening (`scripts/check_license.sh`, gate 8/8):
  SPDX-OR-aware GPL detector + `oracle-slim` depless assertion. Marked done; no new work.
- `tools/gate.sh` is now an 8-gate suite (added the oracle gate); the `gate` skill lists it.

### S2.3 — tskit `.trees` analysis (feat, Stage 2)
- `scripts/slim_analyze.py` (tskit): reads a SLiM `.trees` → JSON stats (num_samples/individuals/trees/sites/
  mutations, segregating sites, mean+max derived-allele freq ∈ [0,1], nucleotide diversity). Stats come from
  the genealogy, not file bytes (provenance timestamps differ).
- `crates/oracle-slim/examples/produce_trees.rs`: runs the S2.2 driver → writes `data/runs/slim_demo/out.trees`
  → prints path; chains S2.2 → S2.3 (`cargo run -p oracle-slim --example produce_trees <seed>`).
- **Verified SLiM genetics are reproducible** for a fixed seed (identical stats twice; different seed differs)
  — de-risks the S2.4 golden gate.
- Python stack pinned in `scripts/requirements.txt` (`.venv`, gitignored): tskit 1.0.3 / pyslim 1.1.1 /
  numpy 2.4.6 (MIT/MIT/BSD) + msprime 1.4.2 (**GPL-3, standalone-analysis-only — never linked**, invariant #1
  unaffected; same pattern as the SLiM subprocess). DECISIONS rows added.

### S2.2 — oracle-slim SLiM subprocess driver (feat, Stage 2)
- `crates/oracle-slim`: **dependency-free** (std-only) driver — `SlimParams` → `write_model` generates a
  self-contained SLiM 5 Eidos model (params baked via `defineConstant`, `initializeTreeSeq()`, final
  `<gen> late()` → `treeSeqOutput` + `simulationFinished`) → `run_model` shells out
  `Command::new(slim).arg("-seed").arg(seed).arg(model)` and returns the `.trees` path. `SlimError` carries
  SLiM's stderr; `resolve_slim_bin` = `SLIM_BIN` → `~/.local/bin/slim` → PATH.
- **Invariant #1 verified (adversarial review):** zero deps (`cargo tree` shows the crate alone), no FFI/
  `#[link]`/`build.rs`/linkage — `slim` is invoked as a subprocess only, never linked. Seed passed in
  (caller derives via `sim-core::derive_seed`); oracle-slim adds no entropy.
- Tests: model-generation unit tests (no slim needed) + an integration test that actually runs slim
  (fixed seed → non-empty `.trees`) and **skips gracefully** when slim is absent. Does not byte-compare
  `.trees` (SLiM provenance timestamps differ). Loop: implementer → gate (GREEN) → reviewer (APPROVE).

### S2.1 — build SLiM from source, pinned (chore, Stage 2; human-signed-off 🛑)
- `tools/install_slim.sh`: clones MesserLab/SLiM, checks out the pinned tag (`v5.2`), CMake Release build,
  symlinks the CLI to `~/.local/bin/slim`. GPL-subprocess-only contract documented at the top (inv. #1).
- Built + installed **SLiM v5.2** (commit `f11de0d`); `slim -version` confirmed. Recorded in DECISIONS
  (SLiM row flipped to installed). Invariant #1 verified: license gate green, `oracle-slim` still depless,
  no GPL crate in the workspace tree (SLiM is purely an external binary — never linked).

### S1.5 — genotype→phenotype map + selection (feat, Stage 1; **Stage 1 complete**)
- `crates/sim-core/gp.rs`: `Trait`/`Phenotype`/`GenotypePhenotypeMap` (TAXONOMY §2) + `WeightedSumMap` (transparent
  weighted sum of genome param unit-scalars → traits, clamped [0,1]). Pure/deterministic; trait boundary (inv. #5).
- Selection wired into the tick loop: per-organism `Genotype∈[0,1]` (seeded), constant-N **Wright-Fisher**
  resampling ∝ fitness (`0.05 + base_growth·genotype`), drawn from the single `SimRng` in `OrgId` order (inv. #3;
  ordered cumulative table + binary search; BTreeMap write-back). `allele_freq` (mean genotype) in `RunStats`,
  folded into the hash, surfaced by the harness. No extinction (constant N).
- Determinism hash updated `3393…`→`fde0e0b61b9e23e6` (expected; gate compares two runs, still GREEN).
- Perf re-baselined at Stage 1 exit (~175 M→~19 M organism-updates/s at 10k; selection added — DECISIONS table).
- ADR-005 (selection model). Tests: express-deterministic, selection-responds-to-trait (directional allele_freq),
  proptest allele_freq+traits ∈ [0,1], same-seed-same-stats. Loop: implementer → gate (GREEN incl. bench) →
  reviewer APPROVE. Follow-ups F1/F2 tracked in TASKS.

### S1.4 — gated edit application (feat, Stage 1)
- `crates/crispr`: `apply_edit(genome, edit, variants, on, off, thresholds, rng)` — the core CRISPR mechanic
  (SPEC §4): resolve cas+locus → find PAM → score (on/off) → gate. Pass ⇒ mutate the target Parameter
  (magnitude from on-eff); fail ⇒ realistic off-target perturbations on *other* loci. `Edit`,
  `EditThresholds {min_on_target, max_off_target}` (default 0.5/5), `EditFailure`, `EditOutcome {Applied|Failed}`.
- Determinism (inv. #3): the passed-in `&mut ChaCha8Rng` is the ONLY randomness source (same `rng_unit` as
  sim-core); ordered-Vec selection, no HashMap. Generic over the S1.3 score traits (inv. #5 preserved).
- §10.4 property gates: `genome.is_valid()` always holds after a valid-input edit (every mutation clamps);
  forced-fail edits never return `Applied` and never touch the target Parameter. 30 unit + 5 proptests.
- Dep edge: `rand_chacha` added to crispr (already workspace-pinned; no new crate, no DECISIONS change).
  Loop: implementer → gate (GREEN) → reviewer (adversarial APPROVE).

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
