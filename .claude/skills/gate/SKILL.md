---
name: gate
description: Run all PoC test gates via tools/gate.sh and block on any failure (STOP THE LINE). Use before every commit and at stage exits.
---
Run the single gate runner:

```bash
tools/gate.sh                 # full gate; perf bench skipped (slow) unless GATE_BENCH=1
GATE_BENCH=1 tools/gate.sh    # also run the criterion perf bench — use at stage exits (§11)
```

It runs, in order, and reports PASS/FAIL/SKIP/N-A per item (SPEC §10):
1. `cargo fmt --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. `./tools/check_determinism.sh`            — same seed twice → identical hash (**HARD**, inv. #3)
5. `cargo test --workspace --features proptest`   — invariant property tests
6. `cargo bench -p sim-core`                 — perf not regressed below baseline (§11; opt-in via GATE_BENCH=1)
7. `./tools/check_slim_oracle.sh`            — Stage 2 oracle: pinned seed → SLiM stats within tol of the golden (§10.6; skips if slim/.venv absent)
8. `./scripts/check_license.sh`              — no GPL crate in the dep tree; oracle-slim shells out only (**HARD**, inv. #1)

`tools/gate.sh` exits non-zero if ANY gate FAILED. **Any red = STOP THE LINE.** Do not proceed to commit.
