#!/usr/bin/env bash
# scripts/check_license.sh — licensing gate (SPEC §10.8, HARD — invariant #1).
# Two assertions over the *resolved* dependency tree (Cargo.lock):
#   (A) NO GPL-licensed crate anywhere in the tree (incl. LGPL/AGPL/-or-later). GPL tools must be
#       subprocess-only (crates/oracle-slim shells out to `slim`; it never links GPL).
#   (B) crates/oracle-slim carries ZERO normal/build dependencies — belt-and-suspenders for (A).
#
# Started as the S2.5 deliverable but promoted to Stage 0 machinery so invariant #1 is guarded from
# the first commit. MPL-2.0 (e.g. godot-rust later) is permissive copyleft and is NOT flagged.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

command -v jq >/dev/null 2>&1 || { echo "check_license: jq is required (brew install jq)" >&2; exit 2; }

META="$(cargo metadata --format-version 1 --locked 2>/dev/null)" \
  || { echo "check_license: 'cargo metadata' failed" >&2; exit 2; }

# (A) GPL scan over third-party crates (source != null skips our own path crates).
# SPDX-OR-aware: a crate is only a problem if EVERY OR-alternative is GPL-family (incl. LGPL/AGPL),
# i.e. there is no GPL-free license we can choose. "MIT OR Apache-2.0 OR LGPL-2.1-or-later" is fine
# (we pick MIT/Apache); "GPL-3.0-or-later" or "MIT AND GPL-3.0" is flagged.
GPL="$(printf '%s' "$META" | jq -r '
  .packages[]
  | select(.source != null)
  | . as $p
  | ($p.license // "") as $lic
  | select($lic != "")
  | ($lic | ascii_upcase | gsub("/"; " OR ") | [splits(" OR ")] | map(gsub("^ +| +$"; ""))) as $alts
  | select($alts | all(test("GPL")))
  | "\($p.name) \($p.version)  [\($lic)]"')"

if [ -n "$GPL" ]; then
  echo "LICENSE FAIL (invariant #1): GPL-licensed crate(s) found in the dependency tree:" >&2
  printf '  - %s\n' "$GPL" >&2
  echo "  → GPL tools must be SUBPROCESS-ONLY. Remove the link; shell out from crates/oracle-slim instead." >&2
  exit 1
fi

# (B) oracle-slim must have no normal/build deps (dev-deps for its own tests are allowed).
OS_DEPS="$(printf '%s' "$META" | jq -r '
  .packages[]
  | select(.name == "oracle-slim")
  | [ .dependencies[] | select(.kind == null or .kind == "build") ]
  | length')"

if [ "${OS_DEPS:-0}" != "0" ]; then
  echo "LICENSE FAIL (invariant #1): crates/oracle-slim has ${OS_DEPS} normal/build dependency(ies); it must carry NONE." >&2
  printf '%s' "$META" | jq -r '
    .packages[] | select(.name=="oracle-slim")
    | .dependencies[] | select(.kind == null or .kind == "build") | "  - \(.name)"' >&2
  exit 1
fi

echo "LICENSE OK: no GPL crate in the dependency tree; crates/oracle-slim is dependency-free (shells out only)."
