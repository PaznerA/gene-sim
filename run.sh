#!/usr/bin/env bash
# Local launcher for the gene-sim game (macOS/Linux). Builds the LiveSim cdylib, then runs the windowed live
# sim — handling the two friction points: the cdylib must be built (the dev launcher loads it from
# crates/godot-sim/target/debug/) and game flags must come AFTER a `--` separator (Godot eats the ones before).
#
# Usage:
#   ./run.sh                       # build + run live sandbox (the pre-run menu lets you toggle features)
#   ./run.sh --mission             # pass game flags through (anything after `./run.sh`)
#   ./run.sh --no-menu --ortho     # skip the menu, flat view
#   GENE_SIM_CONFIG=my.config ./run.sh   # use a specific config file
#   GODOT=/path/to/godot ./run.sh        # use a specific Godot binary
#
# Optional config file (default: ./run.config, see run.config.example): a sourced bash file that sets a
# FLAGS=(...) array of default game flags. CLI args to run.sh are appended AFTER the config's FLAGS, so they win.
set -euo pipefail
cd "$(dirname "$0")"

GODOT="${GODOT:-godot}"
CONFIG="${GENE_SIM_CONFIG:-run.config}"

FLAGS=()
if [[ -f "$CONFIG" ]]; then
  echo "» config: $CONFIG"
  # shellcheck disable=SC1090
  source "$CONFIG"
fi

echo "» building LiveSim cdylib (debug)…"
cargo build --manifest-path crates/godot-sim/Cargo.toml

if ! command -v "$GODOT" >/dev/null 2>&1; then
  echo "✗ Godot binary '$GODOT' not found on PATH. Install Godot 4.6 or set GODOT=/path/to/godot." >&2
  exit 1
fi

# Engine args before `--`; game args (FLAGS from config + CLI args) after it. --live is always on for run.sh.
echo "» launching: $GODOT --path godot -- --live ${FLAGS[*]:-} $*"
exec "$GODOT" --path godot -- --live "${FLAGS[@]}" "$@"
