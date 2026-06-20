#!/usr/bin/env bash
# tools/check_godot_snapshot.sh — S4.2 UI gate: the read-only Godot snapshot reader parses a snapshot
# written by the Rust core, headlessly (invariant #4: every feature is tested with no renderer state).
#
# It generates a tiny deterministic snapshot with the headless harness, then has `godot --headless` parse
# it and assert it reports "snapshot OK". This guards the headless `class_name`/global-cache regression
# fixed in S4.2 (a bare `Snapshot` global is unresolved without an editor import pass; the reader must
# `preload` instead). SKIPs cleanly when godot is absent — mirroring the slim oracle gate.
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

OUT="$(godot --headless --path godot -- --snap "$SNAP" 2>&1)"
if printf '%s' "$OUT" | grep -q "snapshot OK"; then
  echo "GODOT READER OK — $(printf '%s' "$OUT" | grep 'snapshot OK')"
  exit 0
fi
echo "FAIL — Godot reader did not report 'snapshot OK'. Full output:"
printf '%s\n' "$OUT"
exit 1
