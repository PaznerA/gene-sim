#!/usr/bin/env bash
# tools/install_godot.sh — install Godot 4 (thin UI, GDScript; no .NET) and headless-smoke it (SPEC §W3).
#
# INVARIANT #4 (build order): the UI is built LAST — only after the core runs headless + deterministic
# (Stages 0–3 done). INVARIANT #2: GDScript reads snapshots only; no biology in the renderer.
# INVARIANT #7: pin the Godot minor in docs/llm/DECISIONS.md.
set -euo pipefail

# Pinned minor (ADR/DECISIONS). Installed via Homebrew cask (cask symlinks the CLI to `godot`).
GODOT_PIN="${GODOT_PIN:-4.6}"  # repinned 4.7→4.6 for stable gdext api-4-6 (ADR-010, live-sim epic)

if ! command -v godot >/dev/null 2>&1; then
  echo ">> installing Godot via Homebrew cask…"
  brew install --cask godot
fi

GODOT_BIN="$(command -v godot || true)"
[ -n "$GODOT_BIN" ] || { echo "ERROR: godot not on PATH after install" >&2; exit 1; }

echo ">> godot: $GODOT_BIN"
godot --version
case "$(godot --version | head -1)" in
  ${GODOT_PIN}.*) : ;;  # matches pinned minor
  *) echo "WARNING: installed Godot does not match pinned minor $GODOT_PIN — update DECISIONS.md or repin." >&2 ;;
esac

# Headless smoke (no window): boot the project and quit. Run from repo root.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
echo ">> headless smoke: godot --headless --path godot --quit"
godot --headless --path "$ROOT/godot" --quit
echo ">> Godot OK. Record the exact version in docs/llm/DECISIONS.md (invariant #7)."
