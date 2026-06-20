#!/usr/bin/env bash
# tools/check_godot_snapshot.sh — Godot UI gate (invariant #4: every feature tested with no renderer state).
# Two headless checks against snapshots written by the Rust core:
#   1. S4.2 reader  — `--snap <file>` parses one snapshot and reports "snapshot OK".
#   2. S4.3 render  — `--run <dir> --check` builds the full ecosystem scene (terrain TileMap, data overlay,
#                     organism layer, HUD) and reports "render scene OK" — proving the render path compiles
#                     and constructs without a GPU (catches GDScript parse/logic errors in CI).
# Both guard the headless `class_name`/global-cache trap (a bare global is unresolved without an editor
# import pass; the scripts `preload` instead). SKIPs cleanly when godot is absent — like the slim oracle gate.
#
# Exit: 0 = PASS or SKIP (prints "SKIP — ..."), non-zero = FAIL.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# The harness shell may not have Cargo on PATH (SNIPPETS gotcha).
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

command -v godot >/dev/null 2>&1 || { echo "SKIP — godot not installed (pin: see tools/install_godot.sh)"; exit 0; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# Write a small, deterministic snapshot via the headless core. snapshot() draws no RNG and the per-cell
# grid is a pure function of (seed, generation, grid) — so this is reproducible and read-only (inv. #3).
if ! cargo run -q -p harness -- --seed 7 --generations 10 --snapshots "$TMP" --grid 16x16 >/dev/null 2>&1; then
  echo "FAIL — harness could not write a snapshot"
  exit 1
fi

SNAP="$(ls "$TMP"/snap_*.bin 2>/dev/null | sort | tail -1)"
[ -n "$SNAP" ] && [ -f "$SNAP" ] || { echo "FAIL — harness wrote no snap_*.bin into $TMP"; exit 1; }

# 1. S4.2 reader.
OUT="$(godot --headless --path godot -- --snap "$SNAP" 2>&1)"
if ! printf '%s' "$OUT" | grep -q "snapshot OK"; then
  echo "FAIL — Godot reader did not report 'snapshot OK'. Full output:"
  printf '%s\n' "$OUT"
  exit 1
fi
echo "GODOT READER OK — $(printf '%s' "$OUT" | grep 'snapshot OK')"

# 2. S4.3 render scene (headless build smoke).
ROUT="$(godot --headless --path godot -- --run "$TMP" --check 2>&1)"
if ! printf '%s' "$ROUT" | grep -q "render scene OK"; then
  echo "FAIL — Godot render scene did not report 'render scene OK'. Full output:"
  printf '%s\n' "$ROUT"
  exit 1
fi
echo "GODOT RENDER OK — $(printf '%s' "$ROUT" | grep 'render scene OK')"
exit 0
