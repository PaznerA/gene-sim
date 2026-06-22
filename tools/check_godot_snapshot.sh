#!/usr/bin/env bash
# tools/check_godot_snapshot.sh — Godot UI gate (invariant #4: every feature tested with no renderer state).
# Two headless checks against snapshots written by the Rust core:
#   1. S4.2 reader  — `--snap <file>` parses one snapshot and reports "snapshot OK".
#   2. S4.3/S4.5    — `--run <dir> --check` builds the full ecosystem scene (terrain TileMap, data overlay,
#                     organism layer, HUD) AND the S4.5 L-system specimen plants (from specimens.json) and
#                     reports "render scene OK" — proving both render paths compile and construct without a
#                     GPU (catches GDScript parse/logic errors in CI).
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

# ADR-017 res:// species mirror: data/species/ is the single source of truth; godot/data/species/ is a generated,
# gitignored mirror the renderer reads via FileAccess(res://data/species/…) in dev AND in the exported PCK. Stage
# it here (the same copy run.sh/CI do) and assert it is byte-equal to the canonical dir — RED on drift, so the
# mirror can never silently rot vs the Rust-side truth (which the harness tests pin via CARGO_MANIFEST_DIR).
mkdir -p godot/data/species && cp data/species/*.json godot/data/species/
if ! diff -rq data/species godot/data/species >/dev/null 2>&1; then
  echo "FAIL — godot/data/species is not byte-equal to data/species (ADR-017 mirror drift):"
  diff -rq data/species godot/data/species || true
  exit 1
fi
echo "SPECIES MIRROR OK — godot/data/species == data/species ($(ls data/species | wc -l | tr -d ' ') files)"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# Write a small, deterministic snapshot + specimen trait vectors via the headless core. snapshot() draws no
# RNG and the per-cell grid / specimens are pure functions of (seed, generation, grid) — reproducible &
# read-only (inv. #3). The specimens drive the S4.5 L-system view exercised by the render check below.
if ! cargo run -q -p harness -- --seed 7 --generations 10 --snapshots "$TMP" --grid 16x16 --specimens "$TMP" >/dev/null 2>&1; then
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
# GSS4 channel-count contract: a silent channel-count regression (e.g. a stale 9-channel reader, or a
# producer that drops the 3 chem planes) goes RED here. main.gd prints `channels=%d` from the file header.
printf '%s' "$OUT" | grep -q "channels=12" || { echo "FAIL — expected channels=12 (GSS4 chem planes), got:"; printf '%s\n' "$OUT"; exit 1; }

# 2. S4.3 render scene (headless build smoke) — ISOMETRIC (default, P3) AND orthographic (--ortho opt-out).
for mode in "" "--ortho"; do
  ROUT="$(godot --headless --path godot -- --run "$TMP" --check $mode 2>&1)"
  if ! printf '%s' "$ROUT" | grep -q "render scene OK"; then
    echo "FAIL — Godot render scene (${mode:-iso-default}) did not report 'render scene OK'. Full output:"
    printf '%s\n' "$ROUT"
    exit 1
  fi
  echo "GODOT RENDER OK (${mode:-iso-default}) — $(printf '%s' "$ROUT" | grep 'render scene OK')"
done
exit 0
