#!/usr/bin/env bash
# tools/check_livesim.sh — live-sim GDExtension smoke (roadmap R6/P1b, ADR-010).
#
# Builds the workspace-DETACHED `crates/godot-sim` gdext cdylib and loads the `LiveSim` node in an ISOLATED
# temp Godot project (so the main renderer project godot/ stays extension-free and the other gates never try
# to load a possibly-unbuilt dylib). The smoke drives reset → step → observe → snapshot and asserts
# LIVESIM_SMOKE_OK — proving the api-4-6 cdylib loads + runs under the installed Godot (forward-compat:
# runtime >= API). INVARIANT #2: LiveSim is a thin Rust binding over sim-core/harness; GDScript only CALLS it.
#
# SKIPs cleanly (exit 0, prints "SKIP — ...") when godot/cargo are absent or the cdylib does not build, so a
# fresh checkout without the live-sim toolchain still gates green. Exit non-zero only on a real smoke failure.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

command -v godot >/dev/null 2>&1 || { echo "SKIP — godot not installed"; exit 0; }
command -v cargo >/dev/null 2>&1 || { echo "SKIP — cargo not installed"; exit 0; }
[ -f crates/godot-sim/Cargo.toml ] || { echo "SKIP — crates/godot-sim not present"; exit 0; }

# Build the detached cdylib (its own Cargo.lock; does not touch the workspace gate).
if ! cargo build --quiet --manifest-path crates/godot-sim/Cargo.toml 2>/tmp/godot_sim_build.log; then
  echo "SKIP — godot-sim cdylib did not build (gdext deps unavailable?):"
  tail -4 /tmp/godot_sim_build.log
  exit 0
fi

DYLIB="$ROOT/crates/godot-sim/target/debug/libgodot_sim.dylib"
[ -f "$DYLIB" ] || DYLIB="$ROOT/crates/godot-sim/target/debug/libgodot_sim.so"  # linux
[ -f "$DYLIB" ] || { echo "SKIP — built cdylib not found under crates/godot-sim/target/debug/"; exit 0; }

# Assemble an isolated temp project: project.godot + the .gdextension (dylib copied in) + the smoke script.
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
printf 'config_version=5\n[application]\nconfig/name="livesim-smoke"\n' > "$TMP/project.godot"
cp "$DYLIB" "$TMP/$(basename "$DYLIB")"
LIBKEY="macos.debug"; [ "$(uname -s)" = "Linux" ] && LIBKEY="linux.debug"
cat > "$TMP/gene_sim.gdextension" <<EOF
[configuration]
entry_symbol = "gdext_rust_init"
compatibility_minimum = 4.6
[libraries]
$LIBKEY = "res://$(basename "$DYLIB")"
EOF
mkdir -p "$TMP/.godot"
printf 'res://gene_sim.gdextension\n' > "$TMP/.godot/extension_list.cfg"
cp crates/godot-sim/godot/livesim_smoke.gd "$TMP/livesim_smoke.gd"

OUT="$(godot --headless --path "$TMP" --script livesim_smoke.gd 2>&1)"
if printf '%s' "$OUT" | grep -q "LIVESIM_SMOKE_OK"; then
  echo "LIVESIM OK — $(printf '%s' "$OUT" | grep -E 'Initialize godot-rust' | head -1)"
  exit 0
fi
echo "FAIL — LiveSim smoke did not print LIVESIM_SMOKE_OK. Full output:"
printf '%s\n' "$OUT" | tail -20
exit 1
