#!/usr/bin/env bash
# tools/install_slim.sh — build SLiM from source at a PINNED tag (SPEC §W2, §8 Stage 2; ADR-001).
#
# INVARIANT #1 (STOP THE LINE): SLiM is GPL-3. It is used as a SEPARATE CLI SUBPROCESS only — never linked
# into any game binary, and crates/oracle-slim must carry no GPL dependency. This script only builds the
# external `slim` CLI; it touches no Rust crate.
# INVARIANT #7: the tag is pinned (default v5.2) and recorded in docs/llm/DECISIONS.md. SLiM reproducibility
# is version-scoped (SPEC §12) — same seed reproduces only within the same SLiM version.
set -euo pipefail

SLIM_TAG="${SLIM_TAG:-v5.2}"                       # pinned (ADR-001); override via env if ever needed
SLIM_DIR="${SLIM_DIR:-$HOME/.local/src/SLiM}"
PREFIX_BIN="${PREFIX_BIN:-$HOME/.local/bin}"
JOBS="${JOBS:-$(sysctl -n hw.ncpu 2>/dev/null || nproc)}"

echo ">> SLiM build — tag=$SLIM_TAG dir=$SLIM_DIR jobs=$JOBS"

# 1. Clone (once) and check out the pinned tag.
if [ ! -d "$SLIM_DIR/.git" ]; then
  git clone https://github.com/MesserLab/SLiM.git "$SLIM_DIR"
fi
cd "$SLIM_DIR"
git fetch --tags --quiet
git -c advice.detachedHead=false checkout "$SLIM_TAG"

# 2. Configure + build the CLI (Release). No Qt requested ⇒ SLiMgui is skipped; we only need `slim` (+ eidos).
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build -j"$JOBS"

# 3. Locate the CLI binary (ABSOLUTE path) and symlink it onto PATH.
SLIM_BIN="$(find "$SLIM_DIR/build" -maxdepth 2 -name slim -type f -perm -u+x | head -n1)"
[ -n "$SLIM_BIN" ] || { echo "ERROR: built 'slim' binary not found under $SLIM_DIR/build" >&2; exit 1; }
install -d "$PREFIX_BIN"
ln -sf "$SLIM_BIN" "$PREFIX_BIN/slim"

# 4. Report the version (record it in docs/llm/DECISIONS.md).
echo ">> installed: $PREFIX_BIN/slim -> $SLIM_BIN"
"$PREFIX_BIN/slim" -version
echo ">> NOTE: ensure $PREFIX_BIN is on PATH. Record this version in docs/llm/DECISIONS.md (invariant #7)."
