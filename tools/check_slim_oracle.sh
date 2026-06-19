#!/usr/bin/env bash
# Stage 2 oracle gate (SPEC §10.6): a pinned seed → SLiM tree-sequence stats within tolerance of a golden
# file (data/golden/slim_case1.json). This pins the genetics to SLiM v5.2 (invariant #7) — if SLiM drifts
# (e.g. a version bump), this gate fails and the golden must be re-recorded with an ADR.
#
# Skips gracefully (exit 0 with a SKIP notice) if slim / the .venv / the golden are unavailable, so
# tools/gate.sh stays green on machines without the Stage 2 oracle installed.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

SEED="${SLIM_GOLDEN_SEED:-1234}"          # the produce_trees example bakes the rest of the params
GOLDEN="${SLIM_GOLDEN:-data/golden/slim_case1.json}"
PY="$ROOT/.venv/bin/python"
SLIM_RESOLVED="${SLIM_BIN:-$HOME/.local/bin/slim}"

# --- graceful prerequisites check ---
command -v cargo >/dev/null 2>&1 || { echo "oracle gate SKIP: cargo not found"; exit 0; }
[ -x "$PY" ]      || { echo "oracle gate SKIP: .venv missing (.venv/bin/pip install -r scripts/requirements.txt)"; exit 0; }
{ [ -x "$SLIM_RESOLVED" ] || command -v slim >/dev/null 2>&1; } || { echo "oracle gate SKIP: slim binary not found (tools/install_slim.sh)"; exit 0; }
[ -f "$GOLDEN" ]  || { echo "oracle gate SKIP: golden $GOLDEN missing"; exit 0; }

# --- run: produce a .trees for the pinned seed, then compare its stats to the golden within tolerance ---
TREES="$(cargo run -q -p oracle-slim --example produce_trees "$SEED")" \
  || { echo "oracle gate FAIL: produce_trees errored" >&2; exit 1; }

"$PY" scripts/slim_analyze.py "$TREES" --check "$GOLDEN"
