#!/usr/bin/env bash
# tools/run_batch.sh — N parallel seeded, deterministic sim runs (SPEC §W7).
#
# Derives each run's seed from a single master seed (invariant #3): run `i` uses derive_seed(MASTER, i),
# computed inside the harness from `--master-seed MASTER --run-index i`. Runs are independent processes,
# each with its own derived seed, so the whole batch reproduces bit-for-bit when re-run with the same args.
# Each run writes its per-generation columnar stats to data/runs/<id>/per_gen.csv (--per-gen-stats); the
# Parquet aggregation step (scripts/) consumes those files later.
#
# Usage:
#   tools/run_batch.sh [MASTER] [RUNS] [GENS]
#   tools/run_batch.sh            # MASTER=42 RUNS=64 GENS=200
#   tools/run_batch.sh 42 8 50    # 8 runs off master seed 42, 50 generations each
set -euo pipefail

# Run from the repo root regardless of CWD.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# The harness shell may not have Cargo on PATH (SNIPPETS gotcha).
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

MASTER="${1:-42}"
RUNS="${2:-64}"
GENS="${3:-200}"

# Build the release binary ONCE up front, then invoke the built binary in parallel (avoids cargo
# rebuild/lock contention across the parallel jobs).
cargo build --release -p harness
BIN="$ROOT/target/release/harness"

JOBS="$(sysctl -n hw.ncpu)"
echo "run_batch: master=$MASTER runs=$RUNS generations=$GENS jobs=$JOBS"

# Run indices 0..RUNS-1, each as its own process with a derived seed, parallel over all cores.
seq 0 "$((RUNS - 1))" | xargs -P "$JOBS" -I{} \
  "$BIN" --master-seed "$MASTER" --run-index {} --generations "$GENS" --per-gen-stats

echo "run_batch: DONE — $RUNS runs (master=$MASTER, generations=$GENS); per_gen.csv under data/runs/"
