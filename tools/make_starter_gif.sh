#!/usr/bin/env bash
# tools/make_starter_gif.sh — CAPTURE + ASSEMBLE the SCENARIO GIF PREVIEW for a discovered gem, on the off-hash
# Stage-1 KEY-EVENT schedule (the renderer-side half of the preview; the schedule itself is harness::keyframe).
#
# Pipeline:
#   1. SCHEDULE — `harness --keyframes <gem>` prints the KEY generations (boom/crash/takeover/edit/immigrate +
#      start/context/final anchors) the clip should snapshot. The detector is the SAME ecology event logic the D0
#      scorer's M5 rewards, so the GIF keys off exactly the events that made the run interesting.
#   2. CAPTURE — for each key gen, REPLAY the gem (the discovery-load-gem-replay loader: `godot … --gem <abs>`,
#      INCLUDING its mid-run CRISPR edits) advanced to that gen and shoot ONE frame to a PNG via the renderer's
#      `--shot`. macOS-SAFE: a `$(godot…)` PIPE-capture HANGS on macOS (a child holds the stdout pipe even after
#      Godot exits), so we capture to a FILE under `timeout` and grep the file — never a pipe (mirrors
#      tools/check_godot_snapshot.sh). `--shot` needs a GPU/display, so this is the WINDOWED path (NOT --headless).
#   3. ASSEMBLE — `harness --assemble-gif` encodes the PNGs into a looping animated GIF via the IN-PROCESS MIT
#      `gif` encoder (inv #1 — GPL stays at the process boundary; the encoder is LINKED + pure-Rust, never a GPL
#      imagemagick/ffmpeg subprocess). The `.gif` lands next to the starter (`data/presets/starters/<slug>.gif`)
#      so the RCT selector can show it; it is staged into res:// by the SAME recursive `cp -R data/presets/.` that
#      run.sh + check_godot_snapshot.sh already do (the byte-gate). `<slug>.gif` is a GENERATED artifact (gitignored).
#
# Usage:
#   tools/make_starter_gif.sh <gem.json> <slug> [out_dir]
# Exit 0 = a GIF was produced OR a clean SKIP (godot absent / no GPU-display to render --shot — like the UI gate).
# Exit non-zero = a real failure (bad gem, encoder error).
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

GEM="${1:-}"
SLUG="${2:-}"
OUT_DIR="${3:-data/presets/starters}"
if [ -z "$GEM" ] || [ -z "$SLUG" ]; then
  echo "usage: tools/make_starter_gif.sh <gem.json> <slug> [out_dir]" >&2
  exit 2
fi
[ -f "$GEM" ] || { echo "FAIL — gem not found: $GEM" >&2; exit 2; }
# --gem wants an ABSOLUTE path (data/runs is gitignored, NOT under res://); resolve it.
GEM_ABS="$(cd "$(dirname "$GEM")" && pwd)/$(basename "$GEM")"

GODOT="${GODOT:-godot}"
command -v "$GODOT" >/dev/null 2>&1 || { echo "SKIP — godot not installed (pin: see tools/install_godot.sh)"; exit 0; }

# 1. SCHEDULE — the off-hash key-event gens (stdout is the clean machine-readable gen list; field 1 = gen).
echo "» keyframe schedule for $GEM"
GENS="$(cargo run -q -p harness -- --keyframes "$GEM" 2>/dev/null | awk '{print $1}')"
if [ -z "$GENS" ]; then
  echo "SKIP — no keyframes (the gem's roster may no longer resolve, or the run died at gen 0)"; exit 0
fi
echo "  gens: $(echo "$GENS" | tr '\n' ' ')"

# Build the LiveSim cdylib the renderer loads + stage the res:// data mirrors (the SAME steps run.sh does), so a
# windowed `--gem` replay finds its species/codex/preset JSON. Quiet unless it fails.
echo "» building LiveSim cdylib + staging data"
cargo build -q --manifest-path crates/godot-sim/Cargo.toml || { echo "FAIL — cdylib build failed" >&2; exit 1; }
mkdir -p godot/data/species godot/data/codex godot/data/presets
cp data/species/*.json godot/data/species/ 2>/dev/null || true
cp data/codex/*.json   godot/data/codex/   2>/dev/null || true
cp -R data/presets/.   godot/data/presets/ 2>/dev/null || true

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# macOS-SAFE renderer shot: WINDOWED (--shot needs a GPU), captured to a FILE under `timeout` (never a pipe).
GODOT_TIMEOUT="${GODOT_TIMEOUT:-180}"
shoot() {  # shoot <gen> <out.png> — replay the gem to <gen> (incl. its edits) and save one frame; returns 0 on "shot OK"
  local gen="$1" out="$2" log="$3"
  timeout "$GODOT_TIMEOUT" "$GODOT" --path godot -- \
    --live --no-menu --gem "$GEM_ABS" --steps "$gen" --shot "$out" > "$log" 2>&1 || true
  grep -q "shot OK" "$log"
}

# 2. CAPTURE — one PNG per key gen, named frame_<zero-padded-gen>.png (name order == gen order for the assembler).
echo "» capturing $(echo "$GENS" | wc -l | tr -d ' ') frame(s) (windowed --shot; macOS-safe file capture)"
n=0
for g in $GENS; do
  png="$TMP/$(printf 'frame_%05d.png' "$g")"
  if shoot "$g" "$png" "$TMP/shot_${g}.log"; then
    n=$((n+1))
  else
    # The FIRST failure that is "no viewport image" means no GPU/display (headless box) → clean SKIP, like the UI gate.
    if grep -q "no viewport image" "$TMP/shot_${g}.log"; then
      echo "SKIP — godot has no GPU/display to render --shot (headless box); the in-process encoder is gate-tested"
      echo "       (gifenc unit smoke). Run on a machine with a display to capture real frames."
      exit 0
    fi
    echo "FAIL — shot failed at gen $g:" >&2; cat "$TMP/shot_${g}.log" >&2; exit 1
  fi
done
[ "$n" -gt 1 ] || { echo "SKIP — captured $n frame(s) (<2); nothing to animate"; exit 0; }

# 3. ASSEMBLE — encode the captured frames into the looping preview next to the starter.
mkdir -p "$OUT_DIR"
OUT="$OUT_DIR/$SLUG.gif"
echo "» assembling $OUT"
cargo run -q -p harness -- --assemble-gif "$OUT" --frames "$TMP" || { echo "FAIL — GIF assemble failed" >&2; exit 1; }

# Validate: exists, non-empty, GIF magic.
[ -s "$OUT" ] || { echo "FAIL — $OUT is empty" >&2; exit 1; }
if [ "$(head -c 3 "$OUT")" != "GIF" ]; then
  echo "FAIL — $OUT is not a GIF" >&2; exit 1
fi
echo "GIF OK — $OUT ($(wc -c < "$OUT" | tr -d ' ') bytes, $n frames)"
exit 0
