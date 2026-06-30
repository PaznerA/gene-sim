# Contributing to gene-sim

> gene-sim is a 2D, deterministic CRISPR-ecosystem simulator (PoC). **Headless Rust sim core first, Godot UI last.**
> This guide is the practical entry point; the authoritative sources are
> [docs/llm/SPEC.md](docs/llm/SPEC.md) (invariants + architecture),
> [docs/llm/DECISIONS.md](docs/llm/DECISIONS.md) (ADRs + pinned versions),
> [docs/llm/LOOP.md](docs/llm/LOOP.md) (the per-slice runbook), and [CLAUDE.md](CLAUDE.md) (the condensed
> session context). Read the invariants below before your first change.

## 1. The 7 invariants — STOP THE LINE if violated (SPEC §2.1)

Violating one is a "stop the line" event: **halt, surface to a maintainer, do not work around it.**

1. **GPL stays at the process boundary.** SLiM (GPL-3) and any other GPL tool are invoked as **separate CLI
   subprocesses only** (`crates/oracle-slim` shells out) — never linked into a shipped binary.
2. **Genome lives in the sim core; render is read-only.** Genotype→phenotype logic exists only in
   `crates/genome` / `crates/sim-core`. `godot/` consumes snapshots and **never computes biology** (no genome
   logic in GDScript, ever).
3. **Determinism.** One master seed derives all sub-seeds. Same seed + same build + same platform → identical
   bytes. Use `rand_chacha::ChaCha8Rng` threaded explicitly — **never** thread-local/global RNG, **never**
   iterate a `HashMap` in sim logic (use ordered/indexed collections). The pinned hash is
   `0x47a0_3c8f_6701_f240`; see §4.
4. **Headless-first.** Every sim feature works and is tested with no renderer attached before any UI work.
5. **Science is pluggable behind a trait.** On-/off-target scoring (and the discovery surrogate) sit behind
   Rust traits; swapping an impl must not touch core logic. `crates/discovery` stays `std` + `serde` only.
6. **Agent granularity ceiling.** AI agents act at the operator/species level, not per-organism.
7. **Versions are pinned.** SLiM tag, Godot minor, Bevy, Rust toolchain, every new crate — pinned and recorded
   in `DECISIONS.md`. Load-bearing constants (RNG params, model hyperparameters, channel counts) are pinned too.

## 2. Build & run

```sh
cargo build --workspace          # the headless sim + harness + science crates
cargo test  --workspace          # the headless test suite
bash run.sh                      # build the Godot cdylib + stage data + launch the renderer (UI is LAST)
```

The renderer (`crates/godot-sim` + `godot/`) is a **detached** workspace — it is not part of the headless
build/gate and is never linked into the deterministic core.

## 3. The one gate — `tools/gate.sh`

Everything is gated by a single command. **Never merge on red.**

```sh
bash tools/gate.sh               # full gate (perf bench skipped — it's slow)
GATE_BENCH=1 bash tools/gate.sh  # also run the criterion perf bench (use at stage exits)
```

It runs 10 steps and prints `PASS` / `FAIL` / `SKIP` / `N/A` per item, exiting non-zero if any **failed**:

| # | Gate | Notes |
|---|------|-------|
| 1 | `cargo fmt --check` | |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | warnings are errors |
| 3 | `cargo test --workspace` | |
| 4 | `tools/check_determinism.sh` | **HARD (inv #3)** — same seed twice → identical hash |
| 4b | `tools/check_determinism_multi_isa.sh` | cross-ISA byte-equality; SKIPs locally, the **CI matrix is authoritative** |
| 5 | `cargo test --workspace --features proptest` | property tests |
| 6 | `cargo bench -p sim-core` | SKIPPED unless `GATE_BENCH=1` |
| 7 | `tools/check_slim_oracle.sh` | oracle golden; skips if `slim`/`.venv` absent |
| 8 | `scripts/check_license.sh` | **HARD (inv #1)** — no GPL crate linked |
| 9 | `tools/check_godot_snapshot.sh` | UI headless snapshot reader; skips if `godot` absent |
| 10 | `tools/check_livesim.sh` | live-sim GDExtension smoke; skips if `godot`/cdylib absent |

A `SKIP` (a tool isn't installed locally) is not a failure; CI runs the full matrix. The **hard** gates
(determinism, license, oracle golden) must never be skipped where the inputs exist.

## 4. Determinism discipline (inv #3)

The headless core hashes to a **pinned literal `0x47a0_3c8f_6701_f240`** (asserted in
`crates/sim-core/src/lib.rs`). Most changes must leave it **byte-identical**:

- **Hash-neutral** — the default. Off-hash work (renderer, tooling, the discovery scorer/surrogate, snapshot
  channels that are not folded into `hash_world`) must keep the literal unchanged. If your change moves it
  unexpectedly, that is a STOP-THE-LINE event: **halt and surface it — do not silently re-pin.**
- **Re-pin** — a *deliberate*, designed change to the simulation that moves the hash. It is owned by an ADR,
  moves the literal in the same commit, and is validated by the cross-ISA CI matrix (4b). Mark such items 🔁.

When in doubt, run `cargo test -p sim-core --features determinism` and confirm `determinism_hash_is_pinned`.

## 5. The per-slice loop (SPEC §7.2 — see [LOOP.md](docs/llm/LOOP.md))

A **slice** is the smallest vertical change that leaves the build green and demonstrably advances the bar.

1. **LOAD** — read the invariants + the current slice in [docs/llm/TASKS.md](docs/llm/TASKS.md) +
   relevant ADRs.
2. **PLAN** — restate the goal + acceptance criteria. If the slice is large **or touches an invariant** →
   **stop and get sign-off first.**
3. **IMPLEMENT** — code **and** tests together, fewest crates touched; respect the invariants.
4. **GATE** — `bash tools/gate.sh`. Any red → fix or revert.
5. **REFLECT** — a load-bearing decision → append an ADR to `DECISIONS.md`; update `CHANGELOG.md`.
6. **COMMIT** — one slice = one commit / PR (§6).
7. **CLOSE** — mark the slice done; emit a short summary.

## 6. Branches, commits, and PRs

- **Never commit directly to `main`.** Branch first, gate green, then merge.

  ```sh
  git switch -c feat/<short-slug>
  # … implement + tests …
  bash tools/gate.sh                       # must be green
  git commit                               # conventional commit (below)
  git switch main
  git merge --no-ff feat/<short-slug>      # one slice = one merge commit
  ```

- **One slice = one commit / one merge.** Keep the merge a clean unit (code + tests + the CHANGELOG/ADR/docs
  for that slice). Stage the slice's files explicitly — don't sweep in unrelated working-tree files.
- **Conventional commits**: `feat(scope): …`, `fix(scope): …`, `docs(scope): …`, `refactor(scope): …`,
  `test(scope): …`, `chore(scope): …`. The subject says *what changed*; the body says *why* + the invariant
  audit (e.g. "hash-neutral — pinned literal byte-identical; renderer-only").
- For AI-assisted commits, keep the `Co-Authored-By:` trailer crediting the assistant.

## 7. ADRs — `docs/llm/DECISIONS.md` (append-only)

Any **load-bearing** decision gets an ADR: a new pinned version/crate, a re-pin, a snapshot-format bump, a new
public contract, an architectural choice, or anything that resolves a trade-off a future reader would question.
ADRs are numbered sequentially and **append-only** (never edit an accepted ADR's decision — supersede it with a
new one). A reviewer will **send a slice back** if it makes a load-bearing change without its ADR. Trivial,
already-recorded, or purely-mechanical changes do not need one.

## 8. Review & invariant audit

Every change is reviewed against the 7 invariants and the licensing rule. A useful self-check before opening a
PR: (1) gate green? (2) pinned literal byte-identical (or a deliberate, ADR-owned re-pin)? (3) no genome logic
in `godot/`? (4) no new GPL/heavy dependency linked into a shipped crate? (5) tests cover the new behaviour?
(6) CHANGELOG + any ADR written? If all six hold, you're ready.

## 9. Licensing

The project license is **TBD** (`README.md`) — gene-sim is a pre-release PoC, and invariant #1 (GPL only at the
subprocess boundary) deliberately preserves the freedom to choose a closed/commercial license later. The crate
metadata is currently `MIT OR Apache-2.0` (`Cargo.toml`). By contributing you agree your contributions may be
released under the project's eventual license. Practically, this means: **do not link a GPL (or other
copyleft) crate into a shipped binary** — keep such tools behind the subprocess boundary (inv #1), and the
`scripts/check_license.sh` gate enforces it.
