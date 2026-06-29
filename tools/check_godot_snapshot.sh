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

# SP-4 res:// codex mirror — the SAME discipline as the species mirror above. data/codex/ is the committed
# source of truth; godot/data/codex/ is the generated, gitignored mirror codex.gd reads via FileAccess in dev
# AND the exported PCK. Stage it + assert byte-equality (RED on drift), so the codex can never silently rot.
# This is the staging fix that BLOCKED SP-4 (the codex JSON was never staged into the mirror → codex.gd found
# nothing → deferral). The headless --check below now exercises the REAL content path.
mkdir -p godot/data/codex && cp data/codex/*.json godot/data/codex/
if ! diff -rq data/codex godot/data/codex >/dev/null 2>&1; then
  echo "FAIL — godot/data/codex is not byte-equal to data/codex (SP-4 mirror drift):"
  diff -rq data/codex godot/data/codex || true
  exit 1
fi
echo "CODEX MIRROR OK — godot/data/codex == data/codex ($(ls data/codex | wc -l | tr -d ' ') files)"

# Item 3 res:// presets mirror — the SAME discipline as the species/codex mirrors. data/presets/ is the committed
# source of truth (the "Load Starter" presets main_menu.gd reads via FileAccess); godot/data/presets/ is the
# generated, gitignored mirror. Stage it + assert byte-equality (RED on drift) so a preset can never silently rot.
# STARTER-MAP PROMOTE adds data/presets/starters/ (gen-1 starter docs + index.json + gen-N checkpoint session
# subdirs), so stage the WHOLE tree recursively (cp -R …/.) and let `diff -rq` recurse — the starter library +
# index are byte-gated against the canonical dir exactly like the top-level presets.
mkdir -p godot/data/presets && cp -R data/presets/. godot/data/presets/
if ! diff -rq data/presets godot/data/presets >/dev/null 2>&1; then
  echo "FAIL — godot/data/presets is not byte-equal to data/presets (Item 3 mirror drift):"
  diff -rq data/presets godot/data/presets || true
  exit 1
fi
echo "PRESETS MIRROR OK — godot/data/presets == data/presets ($(ls data/presets | wc -l | tr -d ' ') files)"

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

# macOS headless Godot can leave a child process holding the stdout PIPE so a `$(godot …)` command substitution
# never returns even after Godot has finished its work and exited — the gate appears to hang forever. Capture to a
# FILE under a `timeout` instead (a file has no pipe to wait on; a genuine hang is bounded by GODOT_TIMEOUT and
# reported as a missing success marker). Linux CI is unaffected — Godot finishes in seconds and the timeout is just
# a safety net. The callers below grep the captured file exactly as they greped the old `$OUT`/`$ROUT` strings.
GODOT_TIMEOUT="${GODOT_TIMEOUT:-180}"
run_godot() {  # run_godot <out-file> <args-after-`--path godot`...>; never aborts the script (callers grep the file)
  local out="$1"; shift
  timeout "$GODOT_TIMEOUT" godot --headless --path godot "$@" > "$out" 2>&1 || true
}

# 1. S4.2 reader.
run_godot "$TMP/reader_out.log" -- --snap "$SNAP"
OUT="$(cat "$TMP/reader_out.log")"
if ! printf '%s' "$OUT" | grep -q "snapshot OK"; then
  echo "FAIL — Godot reader did not report 'snapshot OK'. Full output:"
  printf '%s\n' "$OUT"
  exit 1
fi
echo "GODOT READER OK — $(printf '%s' "$OUT" | grep 'snapshot OK')"
# GSS6 channel-count contract: a silent channel-count regression (e.g. a stale 13-channel reader, or a
# producer that drops the dominant_variant_id colony plane) goes RED here. main.gd prints `channels=%d` from the header.
printf '%s' "$OUT" | grep -q "channels=14" || { echo "FAIL — expected channels=14 (GSS6 + dominant_variant_id colony plane), got:"; printf '%s\n' "$OUT"; exit 1; }

# 2. S4.3 render scene (headless build smoke) — ISOMETRIC (default, P3) AND orthographic (--ortho opt-out).
# SP-4: --check now ALSO builds every baked species' glyph via the key-led factory (prints glyphs=N) and
# exercises the codex-enriched inspect join (prints codex=OK), so a parse error / malformed polygon in ANY
# morphotype body OR a garbled codex.json / broken join goes RED here — the inv-#4 guarantee the deferred
# SP-4 lacked. Expect glyphs=13 (the 12 baked species + 1 unknown-key fallback) and codex=OK.
for mode in "" "--ortho"; do
  run_godot "$TMP/check_out.log" -- --run "$TMP" --check $mode
  ROUT="$(cat "$TMP/check_out.log")"
  if ! printf '%s' "$ROUT" | grep -q "render scene OK"; then
    echo "FAIL — Godot render scene (${mode:-iso-default}) did not report 'render scene OK'. Full output:"
    printf '%s\n' "$ROUT"
    exit 1
  fi
  printf '%s' "$ROUT" | grep -q "glyphs=13" || { echo "FAIL — expected glyphs=13 (12 baked species + unknown-key fallback), got:"; printf '%s\n' "$ROUT"; exit 1; }
  printf '%s' "$ROUT" | grep -q "codex=OK" || { echo "FAIL — codex inspect join did not resolve (codex=OK expected). Output:"; printf '%s\n' "$ROUT"; exit 1; }
  echo "GODOT RENDER OK (${mode:-iso-default}) — $(printf '%s' "$ROUT" | grep 'render scene OK')"
done
echo "CODEX INSPECT OK — every species glyph built headlessly + the codex inspect join resolved"

# 3. ADR-029 S4 colony render-surface proof (renderer-only, no display): a brushed child Variant nested in its
# parent territory makes the parent a hole-cut FRAME (filled area == outer MINUS the child hole), the district
# tracks its members across a move (the heritable S1 tag), the registry names it, and the selected-pop override +
# its anti-re-spam budget are present. A geometry regression (hole not cut, district lost on move) goes RED here.
run_godot "$TMP/colony_s4_out.log" --script colony_s4_test.gd
COUT="$(cat "$TMP/colony_s4_out.log")"
if ! printf '%s' "$COUT" | grep -q "COLONY_S4_TEST_OK"; then
  echo "FAIL — ADR-029 S4 colony render-surface test did not pass. Full output:"
  printf '%s\n' "$COUT"
  exit 1
fi
echo "COLONY S4 OK — $(printf '%s' "$COUT" | grep 'HOLE_CUT_OK')"

# 4. ADR-029 S5 plant-realism render-surface proof (renderer-only, no display): a sub-MIN_COLONY_CELLS PLANT
# colony renders as a DISTRICT (never a haze speck) while a tiny microbe colony stays haze (microbe unchanged); a
# plant district is a soft canopy hull with MORE contour points than an equal-shape microbe hard district; every
# non-empty plant cell lands in a colony (>=1-colony guarantee); the plant ghost-fill floor > the microbe
# GHOST_FILL_FACTOR. A plant-realism regression (plant decaying to haze, the hull losing its extra smoothing, an
# unlabeled plant cell, the ghost floor inverting) goes RED here.
run_godot "$TMP/colony_s5_out.log" --script colony_s5_test.gd
SOUT="$(cat "$TMP/colony_s5_out.log")"
if ! printf '%s' "$SOUT" | grep -q "COLONY_S5_TEST_OK"; then
  echo "FAIL — ADR-029 S5 plant-realism colony render-surface test did not pass. Full output:"
  printf '%s\n' "$SOUT"
  exit 1
fi
echo "COLONY S5 OK — $(printf '%s' "$SOUT" | grep 'CANOPY_OK')"
exit 0
