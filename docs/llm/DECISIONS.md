# DECISIONS — ADR log & pinned versions

> Append-only. Each ADR: Context · Decision · Consequences. Load-bearing choices only.
> Invariant #7 (SPEC §2.1.7): SLiM tag, Godot minor, Bevy version, Rust toolchain — all pinned here.
> Cross-version reproducibility is **not** guaranteed; the determinism gate runs on one pinned platform/build.

## Pinned versions (the reproducibility contract — SPEC §2.1.7, §6)

| Component | Pinned version | Status | Notes |
|---|---|---|---|
| Reference platform | macOS (Darwin 25.3) / Apple Silicon **M4 Max**, 14 cores | active | The single determinism reference platform (SPEC §6). |
| Rust toolchain | **stable 1.96.0** (`ac68faa20`, 2026-05-25) | installed | Native aarch64-apple-darwin. `rust-toolchain.toml` pins it in-repo. |
| `bevy_ecs` | **0.18** (resolved `0.18.x` — see Cargo.lock) | installed (Stage 0) | ECS only, **no render plugins** (SPEC §2.2). |
| `rand_chacha` | **0.9** (`ChaCha8Rng`) | installed (Stage 0) | The one portable, reproducible RNG (invariant #3). |
| SLiM | **tag `v5.2`** (latest stable v5.x) | NOT yet built — Stage 2 | Built from source via `tools/install_slim.sh`; confirm `slim -version` then. **GPL-3, subprocess only.** |
| Crisflash | latest release | NOT yet built — Stage 2+ | Off-target oracle (CPU). Optional realism. |
| crisprScore | (Bioconductor) | optional — not on critical path | On-target realism only (SPEC §2.2). |
| Godot | **4.x** (pin the exact minor at install) | NOT yet installed — Stage 4 | Thin 2D UI, built LAST; `tools/install_godot.sh`. |

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
   **`bevy_ecs` 0.18**, **`rand_chacha` 0.9**. Deferred-but-pinned: **SLiM `v5.2`** (Stage 2),
   **Godot 4.x exact minor** (Stage 4), **Crisflash** (Stage 2+).

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
