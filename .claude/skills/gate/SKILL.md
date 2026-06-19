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
