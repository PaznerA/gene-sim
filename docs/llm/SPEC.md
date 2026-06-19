# gene-sim — CRISPR Ecosystem Simulator (PoC)
## Development Spec for Claude Code loop-based iterative development

> **Working codename:** `gene-sim` (rename freely; keep the slug consistent across repo, crates, docs).
> **Audience:** Claude Code agents (primary) + the human maintainer (secondary).
> **Status:** PoC. This file is the north star + the iteration protocol. Prose sections define *intent*; the model derives implementation. The Workflows section defines *exact, repeatable procedures*.
> **Canonical location:** `docs/llm/SPEC.md`. Companion context files live alongside it (see §7).

---

## 0. How to read this file

1. This is the single source of truth for *what we are building* and *how we iterate*.
2. **Invariants (§2.1) are non-negotiable.** Violating one is a "stop the line" event — halt, surface to the human, do not work around it.
3. Build order is fixed: **headless sim core first, Godot UI last.** Do not start the renderer until the core runs headless and deterministic.
4. Reuse > reinvent. Before writing any subsystem from scratch, check §3 for the FOSS component already chosen. If you think reinventing is justified, write an ADR (§7) and stop for human sign-off.
5. The science layer is **pluggable and staged**: start lightweight in-core, swap in heavy real tools (Cas-OFFinder, crisprScore, SLiM) as realism upgrades. Never block a slice on a heavyweight dependency you can stub.

---

## 1. Vision & scope

### 1.1 One-paragraph vision
A 2D, data-layer-driven simulation game (Plague Inc loop × Cities: Skylines info-overlays) where a player — or an LLM agent — picks a CRISPR "scissors" (Cas variant) and a target site on a **parametric genome**, applies an edit, and watches a population of organisms evolve across a bounded ecosystem (one field / forest / pond), surfacing **emergent systems and behavior**. The CRISPR mechanic uses **real science** (real PAM rules, on-/off-target effects, real gene/ontology data). The simulation core is **headless-first and deterministic**, so hundreds of seeded runs can be driven programmatically for emergent-behavior discovery. A biosafety layer models real **daisy-chain gene-drive containment** ("kill switch").

### 1.2 PoC must demonstrate (the bar)
- Load a parametric genome from data (not hardcoded species).
- Select a Cas variant + target locus + guide; apply an edit as a typed parameter mutation, with a realistic on-target efficiency score and off-target hit count gating the result.
- Evolve a population forward over N generations on a bounded grid; selection acts on genome-derived phenotype.
- Run the same seed twice → **bit-identical** output (determinism gate).
- Run M parallel seeded instances headless via a clean API; dump per-generation stats for analysis.
- Render one ecosystem scope in 2D with ≥2 toggleable data layers and zoom — built **after** the core works.

### 1.3 Explicit non-goals for the PoC
- No Unreal, no 3D, no Nanite, no shaders beyond simple 2D data-overlay sampling.
- No per-organism RL agents (agents act at species/operator granularity — see invariant §2.1.6).
- No ML guide-design models on the critical path (optional realism upgrade only).
- No commercial release decision — but keep the licensing invariant clean so the option stays open.
- No cross-platform bit-determinism guarantee (PoC determinism is same-build/same-platform — see §6).

---

## 2. Architecture

```
                 ┌──────────────────────────────────────────────────────────┐
                 │                    HEADLESS SIM CORE (Rust)               │
   AI / batch    │  crates/genome  ─ parametric genome data + ops (rust-bio) │
  ┌───────────┐  │  crates/crispr  ─ PAM finding, edit apply, score plugins  │
  │ harness   │◄─┤  crates/sim-core ─ Bevy ECS tick loop, deterministic RNG  │
  │ (gym-like)│  │  crates/oracle-slim ─ SLiM SUBPROCESS driver (never linked)│
  └───────────┘  └───────────────────────────┬──────────────────────────────┘
        ▲                                     │  bulk state snapshots (read-only)
        │ reset()/step()/seed                 ▼
        │                          ┌────────────────────────┐
   parallel seeded runs            │  godot/ (thin 2D UI)   │  built LAST
                                   │  reads snapshots,      │
                                   │  data-layer shaders    │
                                   └────────────────────────┘
        external realism oracles (subprocess, optional):
        Cas-OFFinder / Crisflash (off-target) · CHOPCHOP / crisprScore (on-target) · SLiM (.trees → tskit)
```

### 2.1 Invariants (STOP THE LINE if violated)

1. **GPL stays at the process boundary.** SLiM (GPL-3) and any other GPL tool are invoked as **separate CLI subprocesses only**. Never link GPL code into the game binary. `crates/oracle-slim` shells out; it must not depend on any GPL crate. (This preserves licensing freedom for a future closed/commercial release.)
2. **Genome lives in the sim core, render is read-only.** The genotype→phenotype logic exists only in `crates/genome` / `crates/sim-core`. `godot/` consumes snapshots and never computes biology. No genome logic in GDScript. Ever.
3. **Determinism.** One master seed per run derives all sub-seeds (sim-core RNG + SLiM `-seed`). Same seed + same build + same platform → identical bytes. Use a portable, reproducible RNG (`rand_chacha::ChaCha8Rng`), never thread-local/global RNG, never iterate `HashMap` in sim logic (use ordered/indexed collections).
4. **Headless-first.** Every sim feature must work and be tested with no renderer attached before any UI work touches it.
5. **Science is pluggable behind a trait.** On-target and off-target scoring sit behind Rust traits with a lightweight in-core default impl and optional subprocess-backed "realistic" impls. Swapping impls must not touch sim-core logic.
6. **Agent granularity ceiling.** AI agents act at the *operator/species* level, not per-organism. (PettingZoo-style envs degrade well past ~10k named agents; per-organism agents would blow this up.) Individual organisms are ECS entities, not RL agents.
7. **Versions are pinned.** SLiM tag, Godot minor version, Bevy version, Rust toolchain — all pinned and recorded in `docs/llm/DECISIONS.md`. Reproducibility across SLiM versions is not guaranteed.

### 2.2 Component choices (the reuse map)

| Concern | Chosen FOSS | License | How it's used |
|---|---|---|---|
| Sequence ops, PAM finding, FM-index | `rust-bio` | MIT | Linked into `crates/genome` / `crates/crispr` |
| Real-time ecosystem sim core | `bevy_ecs` (ECS only, no render) | MIT/Apache-2.0 | The core crate, headless |
| Population-genetics oracle | **SLiM** (`slim` CLI) | GPL-3 | **Subprocess only**; outputs `.trees` → tskit for analysis |
| Off-target scoring (realism) | Cas-OFFinder (OpenCL) or **Crisflash** (C, CPU) | open | Subprocess; Crisflash preferred on Apple Silicon (no OpenCL) |
| On-target scoring (realism) | CHOPCHOP / crisprScore (R/Bioconductor) | open | Subprocess; **optional**, not on critical path |
| 2D render + data-layer UI | **Godot 4** (GDScript) | MIT | Thin layer, reads snapshots, built last |
| AI/batch harness | Gymnasium/PettingZoo **API shape** (not the training stack) | MIT | `reset/step/seed`, parallel seeded runs |
| Ontology seed | Sequence Ontology + Gene Ontology (`go-basic.obo`) + NCBI Taxonomy | open | Parsed into the in-game ontology; LLM extends, schema-validated |
| Plant/tree morphology (2D) | L-system lib (L-Py or a permissive port) | per lib | Genome params drive production rules → visible morphology |
| Reference genomes | Ensembl REST/FTP, UCSC, NCBI E-utilities | open | Downloaded once, cached offline under `data/genomes/` |

---

## 3. Repository layout

```
gene-sim/
├─ CLAUDE.md                       # entry context for Claude Code (invariants + loop pointer)
├─ Cargo.toml                      # Rust workspace
├─ .claude/
│  ├─ skills/                      # CURRENT format (slash + autonomous). .claude/commands/ is legacy.
│  │  ├─ iterate/SKILL.md          # the per-slice loop
│  │  ├─ gate/SKILL.md             # run all test gates
│  │  └─ slice-done/SKILL.md       # close a slice (ADR + changelog + commit)
│  └─ agents/                      # subagents (Task-tool, context-isolated)
│     ├─ planner.md
│     ├─ implementer.md
│     ├─ gatekeeper.md
│     └─ reviewer.md
├─ docs/llm/                       # persistent LLM context (see §7)
│  ├─ SPEC.md                      # THIS FILE
│  ├─ TASKS.md                     # backlog + current slice + acceptance criteria
│  ├─ DECISIONS.md                 # ADR log (load-bearing choices, pinned versions)
│  ├─ TAXONOMY.md                  # canonical genome + ontology model
│  ├─ GLOSSARY.md                  # domain terms (bio + game), keep both languages
│  └─ SNIPPETS.md                  # reusable patterns, gotchas
├─ crates/
│  ├─ genome/                      # parametric genome data model + ops (rust-bio)
│  ├─ crispr/                      # Cas variant table, PAM finding, edit apply, Score traits
│  ├─ sim-core/                    # Bevy ECS headless tick loop, deterministic RNG, phenotype
│  ├─ harness/                     # gym-like API + parallel batch runner + replay
│  └─ oracle-slim/                 # SLiM subprocess driver (NO GPL deps)
├─ tools/
│  ├─ install_slim.sh
│  ├─ install_godot.sh
│  ├─ install_crispr.sh            # optional realism oracles
│  ├─ run_batch.sh                 # N parallel seeded runs
│  └─ check_determinism.sh         # same-seed-twice hash compare
├─ data/
│  ├─ genomes/                     # cached reference FASTA (gitignored, fetched once)
│  ├─ ontology/                    # SO / GO / NCBI-tax (OBO)
│  ├─ runs/<run_id>/              # seed.json, actions.ndjson, snapshots/, slim/*.trees
│  └─ golden/                      # golden files for determinism + oracle gates
├─ godot/                          # thin 2D UI project (GDScript) — built LAST
├─ benches/                        # criterion perf benches
└─ scripts/                        # python glue (tskit analysis, ontology parsing)
```

---

## 4. Data model — parametric genome

Prose (the model derives the concrete types; keep the canonical version in `docs/llm/TAXONOMY.md`):

- A **Genome** is an ordered set of **Loci**. A Locus has: a stable id, a DNA-ish sequence (for PAM/edit realism), a list of typed **Parameters** (numeric/enum/bool with ranges), and **ontology tags** (Sequence-Ontology feature type + Gene-Ontology function references). Loci are data, not code — new locus *kinds* are new ontology nodes, not new Rust enums.
- A **GenotypePhenotypeMap** turns Parameters into **Traits** (growth rate, reflectance, drought tolerance, fecundity, kill-switch linkage, …) via a transparent function (start: weighted-sum / simple GRN; later: optional indirect encoding). Traits feed selection in the sim and morphology in the renderer (via L-system rule params).
- An **Edit** is `(CasVariant, target_locus, guide_sequence)`. Applying it:
  1. Validate the guide against the locus: find PAM for the Cas variant, compute on-target efficiency (Score trait), compute off-target hits (Score trait).
  2. If it passes gating thresholds, mutate the target Parameter(s) (and/or add an ontology node for a novel modifier). Otherwise: partial/failed edit with realistic consequences (off-target side effects = unintended Parameter perturbations elsewhere).
- **CasVariant** is a data row: PAM pattern, cut offset, editing window, edit type (DSB / base-edit / prime). Seed table (hand-encoded from literature): SpCas9 (NGG), SaCas9 (NNGRRT), Cas12a (TTTV, staggered), PAM-relaxed (NG / SpRY), base/prime editors. Keep it in `data/` as a table, not in code.
- **On-the-fly ontology extension:** the LLM may add new ontology nodes (subclasses of existing SO/GO terms) and new modifier functions at runtime. They are validated against a fixed JSON schema and the ontology graph **before** admission. This is the only place new "genes" enter the system — the safe extension boundary.

---

## 5. Storage & persistence

- **Config & run logs are human-readable + git-friendly:** RON or JSON. A run is fully described by `seed.json` (master seed + derived seeds + pinned versions) and `actions.ndjson` (ordered edit/operator actions). Replaying `seed + actions` on the same build reproduces the run exactly — this *is* the determinism contract artifact.
- **Sim→render snapshots are compact binary:** `bincode` or MessagePack, one snapshot per render tick or per epoch. The renderer reads these in bulk; it never asks the core for per-entity data across a boundary in a hot loop.
- **Batch analytics are columnar:** per-generation population stats (allele frequencies, fitness, trait distributions) across many runs written as Parquet for fast cross-run analysis of emergent behavior.
- **SLiM outputs:** `.trees` tree-sequence files land in `data/runs/<run_id>/slim/`, analyzed via tskit/pyslim in `scripts/`.
- **Golden files:** `data/golden/` holds reference hashes/outputs for the determinism gate and SLiM-oracle gate (pinned seed → known allele frequency within tolerance).
- `data/genomes/` and large caches are **gitignored**; a fetch script (Workflow W4) downloads reference genomes once.

---

## 6. Determinism contract

- **Scope (PoC):** same source build + same platform + same master seed ⇒ identical `actions.ndjson` replay output and identical stats hash. **Cross-platform bitwise determinism is explicitly out of scope** (floating-point + arch differences make it a hard problem). The determinism gate therefore runs on one pinned reference platform/build.
- **Rules:** single seeded `ChaCha8Rng` threaded through the sim (no global/thread RNG); deterministic system ordering in Bevy (fixed timestep, explicit ordering, no reliance on `HashMap` iteration order in sim logic — use `IndexMap`/sorted keys); SLiM invoked with an explicit `-seed` derived from the master seed.
- **Gate:** `tools/check_determinism.sh` runs a fixed seed twice and asserts identical output hash. This is a hard merge gate (§10).

---

## 7. Iterative development loop (the Claude Code protocol)

### 7.1 Persistent context (read at the start of every slice)
- `docs/llm/SPEC.md` — invariants + architecture (this file).
- `docs/llm/TASKS.md` — backlog, the *current* slice, and its acceptance criteria. The loop reads the top unstarted slice from here.
- `docs/llm/DECISIONS.md` — ADRs and pinned versions. Append-only.
- `docs/llm/TAXONOMY.md` — canonical genome/ontology model (the data-model source of truth).
- `CLAUDE.md` — short, points here, restates the invariants so they're always in session context.

### 7.2 The per-slice loop
A "slice" is the smallest vertical change that leaves the build green and demonstrably advances the bar (§1.2).

```
1. LOAD     read SPEC invariants + TASKS top slice + DECISIONS.
2. PLAN     restate the slice goal + acceptance criteria in TASKS.md.
            If the slice is >~1 day OR touches an invariant (§2.1) → STOP, ask the human.
3. IMPLEMENT  code AND tests together, fewest crates touched.
              Respect invariants: no GPL linking, no genome logic in godot/, seeded RNG only.
4. GATE     run /gate. Any red → fix or revert. Never proceed on red.
5. REFLECT  if a load-bearing choice was made → append an ADR to DECISIONS.md. Update CHANGELOG.
6. COMMIT   conventional commit; one slice = one commit/PR.
7. CLOSE    mark slice done in TASKS.md, emit a 3-line summary.
            Default: STOP for human review. With an explicit --auto flag: continue to next slice.
```

### 7.3 Multi-agent split (context isolation)
Subagents live in `.claude/agents/` and are spawned via the Task tool. Keep each one's context clean and single-purpose.

- **planner** — decomposes a goal into vertical slices; writes acceptance criteria into `TASKS.md`; flags invariant-touching work for human sign-off. No code.
- **implementer** — implements exactly one slice: code + tests, minimal surface. Knows the invariants; refuses to link GPL or put biology in the renderer.
- **gatekeeper** — runs `/gate`, reports pass/fail per gate, blocks the slice on any red. Authority to reject. No code.
- **reviewer** — checks the diff against SPEC invariants and the licensing rule (no GPL crate in the dependency tree; `oracle-slim` only shells out). Approves or sends back.

Handoff artifacts are files: `TASKS.md` entries (planner→implementer), the diff/PR (implementer→gatekeeper→reviewer). The main session orchestrates; subagents do the isolated work.

### 7.4 `.claude/skills/iterate/SKILL.md` (the loop, invokable as `/iterate`)
```markdown
---
name: iterate
description: Run one vertical development slice end to end (plan → implement → gate → reflect → commit).
invocation: user
---
Execute the per-slice loop from docs/llm/SPEC.md §7.2 on the top unstarted slice in docs/llm/TASKS.md.
Hard rules (docs/llm/SPEC.md §2.1): GPL stays at the subprocess boundary; no genome logic in godot/;
seeded ChaCha8 RNG only; AI agents at species granularity; pin versions.
If the slice exceeds ~1 day or touches an invariant, STOP and ask the human before writing code.
After IMPLEMENT, you MUST run the gate skill and pass before committing.
End with a 3-line summary and stop, unless the human passed --auto.
```

### 7.5 `.claude/skills/gate/SKILL.md` (invokable as `/gate`)
```markdown
---
name: gate
description: Run all PoC test gates; block on any failure.
invocation: user
---
Run, in order, and report PASS/FAIL per item (see docs/llm/SPEC.md §10):
1. cargo fmt --check
2. cargo clippy --workspace -- -D warnings
3. cargo test --workspace
4. ./tools/check_determinism.sh            # same seed twice → identical hash
5. cargo test --workspace --features proptest   # invariant property tests
6. cargo bench -p sim-core                 # perf threshold not regressed (§11)
7. ./scripts/check_license.sh              # no GPL crate in `cargo tree`; oracle-slim only shells out
Any FAIL = STOP THE LINE. Do not proceed to commit.
```

### 7.6 `.claude/skills/slice-done/SKILL.md`
```markdown
---
name: slice-done
description: Close a completed, gated slice (ADR if needed, changelog, conventional commit, mark done).
invocation: user
---
Preconditions: /gate is fully green.
1. If a load-bearing decision was made, append an ADR to docs/llm/DECISIONS.md (context, decision, consequences).
2. Update CHANGELOG.
3. Conventional commit (feat/fix/docs/refactor/test/chore), one slice per commit.
4. Mark the slice done in docs/llm/TASKS.md; surface a 3-line summary.
```

> Note: `.claude/skills/<name>/SKILL.md` is the current Claude Code format (slash-invokable as `/name`, plus autonomous invocation). The older `.claude/commands/*.md` form is legacy but still works; prefer skills. Verify exact frontmatter fields against current Claude Code docs.

---

## 8. Stage plan (each stage has a hard exit gate)

| Stage | Deliverable | Exit gate (Definition of Done) |
|---|---|---|
| **0 — Headless core** | Rust/Bevy ECS crate: parametric `Genome`, tick loop, seeded RNG, CLI runs N seeded instances headless and dumps stats. No graphics. | `cargo run -p harness -- --seed 42 --runs 8` produces stats; **determinism gate green**; entity-count bench recorded as baseline. |
| **1 — CRISPR mechanic** | `crates/crispr`: Cas-variant table, PAM finding (rust-bio), `Score` traits with in-core default impls; an Edit mutates a Parameter, gated by on-target eff + off-target count. | Edit applies & is reproducible; failed-edit path produces realistic off-target perturbation; unit + property tests green. |
| **2 — Genetics realism** | `crates/oracle-slim`: translate an in-game edit into an Eidos model, run `slim` (subprocess), read back allele freqs/fitness via `.trees`/tskit. | Pinned seed → allele freq within tolerance of golden file; **no GPL crate in dep tree**; determinism preserved. |
| **3 — AI harness** | `crates/harness`: gym-like `reset/step/seed`; hundreds of parallel deterministic runs; action+seed replay logs. | M parallel runs reproduce; replay of a logged run is bit-identical; species-granularity actions only. |
| **4 — Godot UI (LAST)** | `godot/`: 2D ecosystem view, ≥2 toggleable data layers (TileMap + data-texture shader), zoom scopes; reads snapshots. L-system morphology for visible plant change. | UI renders a live run from snapshots; **zero biology logic in GDScript**; layers + zoom work. |
| **5 — Ontology + LLM modifiers** | Load SO/GO/NCBI-tax; schema-validated extension API for LLM-generated ontology nodes/modifiers; daisy-chain kill-switch containment modeled. | LLM-added node passes schema + graph validation before admission; kill-switch dilutes ~50%/gen and self-exhausts in sim. |

---

## 9. Workflows (concrete, runnable)

> macOS / Apple Silicon assumed (Mac Studio M4 Max). Adjust `sysctl -n hw.ncpu` → `nproc` on Linux. Record every pinned version in `docs/llm/DECISIONS.md`.

### W1 — Bootstrap
```bash
# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable && cargo --version
# Workspace + crates
cargo new --lib crates/genome && cargo new --lib crates/crispr
cargo new --lib crates/sim-core && cargo new --bin crates/harness
cargo new --lib crates/oracle-slim
# Add to root Cargo.toml [workspace] members. Pin bevy_ecs, rand_chacha, rust-bio, serde, ron, bincode.
mkdir -p docs/llm data/{genomes,ontology,runs,golden} tools scripts benches godot .claude/{skills,agents}
```

### W2 — Install SLiM (subprocess oracle, GPL-3, never linked)
```bash
# tools/install_slim.sh
set -euo pipefail
SLIM_DIR="${SLIM_DIR:-$HOME/.local/src/SLiM}"
[ -d "$SLIM_DIR/.git" ] || git clone https://github.com/MesserLab/SLiM.git "$SLIM_DIR"
cd "$SLIM_DIR"
git fetch --tags
git tag -l 'v*'                         # pick the latest stable v5.x tag, then:
git checkout "${SLIM_TAG:?export SLIM_TAG=<stable tag, e.g. a v5.x release>}"
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build -j"$(sysctl -n hw.ncpu)"
SLIM_BIN="$(find build -maxdepth 2 -name slim -type f | head -n1)"   # locate CLI binary
install -d "$HOME/.local/bin" && ln -sf "$SLIM_BIN" "$HOME/.local/bin/slim"
slim -version    # record the version in docs/llm/DECISIONS.md
# Alt (quicker, less reproducible): conda install -c conda-forge slim
# Optional OpenMP build: see SLiM manual for the parallel CMake flag (not needed for PoC).
```

### W3 — Install Godot 4 (thin UI, GDScript — no .NET; heavy logic stays in Rust)
```bash
# tools/install_godot.sh
brew install --cask godot          # or download a pinned 4.x from godotengine.org
godot --version                    # pin the minor version in docs/llm/DECISIONS.md
godot --headless --quit            # headless smoke test (no window)
```

### W4 — Reference data (fetch once, cache offline)
```bash
# Genomes: download a small model-organism reference FASTA from Ensembl/UCSC into data/genomes/ (gitignored).
# Ontology: fetch go-basic.obo, the Sequence Ontology .obo, and NCBI Taxonomy dump into data/ontology/.
# Parse with scripts/parse_ontology.py (obonet) → an in-game ontology graph.
```

### W5 — Install optional CRISPR realism oracles (Stage 2+, not on critical path)
```bash
# tools/install_crispr.sh
# Stage 1 needs NOTHING external (in-core rust-bio PAM + heuristic on-target + naive off-target).
# Realism upgrades:
#   off-target: Crisflash (C, CPU — preferred on Apple Silicon; Apple deprecated OpenCL)
#               or Cas-OFFinder (OpenCL) run inside a Linux container.
#   on-target:  CHOPCHOP / crisprScore (R + Bioconductor) via subprocess. Heavy deps — keep optional.
```

### W6 — Build & run the headless core
```bash
cargo build --workspace
cargo run -p harness -- --seed 42 --runs 1 --generations 200   # single deterministic run → data/runs/<id>/
```

### W7 — Run N parallel deterministic sims
```bash
# tools/run_batch.sh  (derives per-run seeds from a master seed; runs in parallel)
MASTER="${1:-42}"; RUNS="${2:-64}"
seq 0 $((RUNS-1)) | xargs -P "$(sysctl -n hw.ncpu)" -I{} \
  cargo run --release -p harness -- --master-seed "$MASTER" --run-index {} --generations 500
# Aggregate per-generation stats (Parquet) for emergent-behavior analysis in scripts/.
```

### W8 — Determinism check
```bash
# tools/check_determinism.sh
set -euo pipefail
A="$(cargo run --release -p harness -- --seed 1234 --generations 300 --hash-only)"
B="$(cargo run --release -p harness -- --seed 1234 --generations 300 --hash-only)"
[ "$A" = "$B" ] || { echo "DETERMINISM FAIL: $A != $B"; exit 1; }
echo "DETERMINISM OK ($A)"
```

### W9 — SLiM oracle integration (Stage 2)
```bash
# crates/oracle-slim generates an Eidos model + runs: slim -seed <derived> -d <params> model.slim
# Output: data/runs/<id>/slim/out.trees  → analyze with scripts/slim_analyze.py (tskit/pyslim).
# Gate: pinned seed → allele frequency within tolerance of data/golden/<case>.json
```

### W10 — Godot UI bridge (Stage 4, LAST)
```bash
# godot/ reads data/runs/<id>/snapshots/*.bin (bincode) in bulk per render tick.
# Data layers = a per-cell data texture (channels: density, allele freq, fitness, edit penetrance)
#               sampled in a 2D shader on a TileMap; viewport zoom switches scope.
# RULE: GDScript only reads/plays snapshots. No genome math here.
```

---

## 10. Testing gates (what blocks a merge / blocks the next slice)

1. **Format/lint** — `cargo fmt --check`, `cargo clippy -- -D warnings`. Hard fail.
2. **Unit** — `cargo test --workspace`. Genome ops, PAM finding, edit application, score traits.
3. **Determinism** — `tools/check_determinism.sh`. Same seed twice → identical hash. **Hard, non-negotiable.**
4. **Property** — proptest invariants: allele freq ∈ [0,1]; an edit never yields an invalid genome; failed edits never silently succeed; ontology nodes always validate against schema before admission.
5. **Integration** — a full vertical slice run (edit → evolve → expected stat range).
6. **Oracle (Stage 2+)** — SLiM pinned seed → allele freq within tolerance of a golden file.
7. **Performance** — `cargo bench` (criterion): entity-count × tick-rate must not regress below the recorded threshold (§11). Regression = fail.
8. **License** — `scripts/check_license.sh`: assert no GPL crate appears in `cargo tree`; assert `oracle-slim` only invokes `slim` via subprocess. **Hard, non-negotiable.**

Green = all pass. Any red = stop the line.

---

## 11. Benchmarks & thresholds that change the plan

- Record a **baseline entity count × tick rate** at end of Stage 0. The perf gate (§10.7) enforces no regression below it.
- If Bevy headless can't hit the target entity count at target tick rate → move the hot path to GPU (JAX/`vmap`-style batched sim) **or** coarsen organisms into population cohorts (SLiM carries genetics, ECS carries only spatial/visible agents).
- If SLiM subprocess latency dominates batch throughput → precompute/cache edit→outcome tables, or call SLiM only at epoch boundaries, not per tick.
- If the harness hits the ~10k named-agent ceiling → keep RL/agent granularity at operator/species level (invariant §2.1.6), never per-organism.
- If GPL-3 becomes unacceptable for a planned release → keep SLiM strictly optional/external and fall back to a permissive pop-gen path (msprime/tskit are MIT, or a clean reimplementation of just the needed math) for the shipped core.

---

## 12. Risks & caveats

- **SLiM reproducibility is version-scoped** — same seed reproduces only within the same SLiM version. Pin the tag (invariant §2.1.7).
- **GPL-3 is genuinely constraining** — the subprocess pattern is the standard mitigation (as used by stdpopsim), but warrants a legal check before any commercial release.
- **Some CRISPR ML scores are dependency-heavy** (Python-2-era Azimuth/DeepCpf1 need conda envs; some are Windows-unavailable). Prefer maintained scores (RuleSet3/DeepHF) + CFD; treat exotic scores as optional realism only.
- **Apple Silicon + OpenCL** — Apple deprecated OpenCL; prefer Crisflash (CPU) for off-targets, or run Cas-OFFinder in a Linux container.
- **Cross-platform bit-determinism is out of scope** for the PoC (FP/arch). Run the determinism gate on one pinned platform.
- **`biology.digital` "CRISPOR enterprise" content is unreliable marketing** — use the genuine academic CRISPOR (Haeussler/Tefor) and its published methods.

---

## 13. Future vision (brief, not PoC)

- **Rendering:** swap/augment the thin Godot 2D layer with richer shaders; later, an optional Unreal/Nanite "presentation layer" reading the *same* sim snapshots — the sim core never moves into the renderer.
- **Continuous zoom** from genome → cell → organism → ecosystem (Cities-Skylines-grade info views).
- **Real-data import:** pull live gene/ontology data; let players edit real model-organism genomes.
- **Emergence research mode:** large parallel seeded sweeps mined for emergent systems; the harness becomes a discovery tool.
- **Wet-lab tie-in (long horizon):** the same edit-modeling could inform real contained-fermentation strain design — the PoC's CRISPR scoring and containment models are the seed of that bridge.

---

*End of SPEC. Keep this file authoritative; when reality diverges, update SPEC + DECISIONS in the same slice that causes the divergence.*
