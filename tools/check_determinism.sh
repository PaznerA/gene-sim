#!/usr/bin/env bash
# Determinism gate (SPEC §W8, §10.3) — HARD, NON-NEGOTIABLE (invariant #3).
# Runs the same seed twice and asserts an identical hash. Build-scoped (SPEC §6).
set -euo pipefail

# Run from the repo root regardless of CWD.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# The harness shell may not have Cargo on PATH (SNIPPETS gotcha).
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

SEED="${SEED:-1234}"
GENERATIONS="${GENERATIONS:-300}"

A="$(cargo run -q --release -p harness -- --seed "$SEED" --generations "$GENERATIONS" --hash-only)"
B="$(cargo run -q --release -p harness -- --seed "$SEED" --generations "$GENERATIONS" --hash-only)"

if [ "$A" != "$B" ]; then
  echo "DETERMINISM FAIL: $A != $B" >&2
  exit 1
fi
echo "DETERMINISM OK (seed=$SEED generations=$GENERATIONS hash=$A)"
