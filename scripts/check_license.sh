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
gpl_filter() {  # reads `cargo metadata` JSON on stdin, prints "name version [license]" for GPL-only crates
  jq -r '
    .packages[]
    | select(.source != null)
    | . as $p
    | ($p.license // "") as $lic
    | select($lic != "")
    | ($lic | ascii_upcase | gsub("/"; " OR ") | [splits(" OR ")] | map(gsub("^ +| +$"; ""))) as $alts
    | select($alts | all(test("GPL")))
    | "\($p.name) \($p.version)  [\($lic)]"'
}

GPL="$(printf '%s' "$META" | gpl_filter)"

# Also scan the workspace-DETACHED godot-sim crate: its gdext cdylib (godot 0.5.3 et al.) is built + shipped
# by release.yml, so its dependency closure must be GPL-scanned too even though the root metadata excludes it.
# (gdext is MPL-2.0 — permissive, not flagged; this just closes the blind spot for any future transitive dep.)
if [ -f crates/godot-sim/Cargo.toml ]; then
  GS_META="$(cargo metadata --format-version 1 --locked --manifest-path crates/godot-sim/Cargo.toml 2>/dev/null)" \
    || { echo "check_license: 'cargo metadata' failed for godot-sim" >&2; exit 2; }
  GPL="$(printf '%s\n%s' "$GPL" "$(printf '%s' "$GS_META" | gpl_filter)" | sed '/^[[:space:]]*$/d')"
fi

if [ -n "$GPL" ]; then
  echo "LICENSE FAIL (invariant #1): GPL-licensed crate(s) found in the dependency tree:" >&2
  printf '  - %s\n' "$GPL" >&2
  echo "  → GPL tools must be SUBPROCESS-ONLY. Remove the link; shell out from crates/oracle-slim instead." >&2
  exit 1
fi

# (B) Every PROCESS-BOUNDARY crate must have no normal/build deps (dev-deps for its own tests are allowed) —
# belt-and-suspenders for (A): a dependency-free crate cannot link a GPL (or any) library, only shell out. Add
# new boundary subprocess crates to this list as they land (ADR-017: oracle-fba for the E. coli FBA KO-table,
# relations-index for the vector-DB sidecar). Crates not yet present are skipped (logged, not failed).
BOUNDARY_CRATES="oracle-slim oracle-fba relations-index"
boundary_fail=0
for crate in $BOUNDARY_CRATES; do
  present="$(printf '%s' "$META" | jq -r --arg c "$crate" '[.packages[] | select(.name == $c)] | length')"
  if [ "${present:-0}" = "0" ]; then
    echo "LICENSE: boundary crate '$crate' not present yet — skipping (enforced once the crate lands)."
    continue
  fi
  deps="$(printf '%s' "$META" | jq -r --arg c "$crate" '
    .packages[] | select(.name == $c)
    | [ .dependencies[] | select(.kind == null or .kind == "build") ] | length')"
  if [ "${deps:-0}" != "0" ]; then
    echo "LICENSE FAIL (invariant #1): crates/$crate has ${deps} normal/build dependency(ies); a process-boundary crate must carry NONE." >&2
    printf '%s' "$META" | jq -r --arg c "$crate" '
      .packages[] | select(.name==$c)
      | .dependencies[] | select(.kind == null or .kind == "build") | "  - \(.name)"' >&2
    boundary_fail=1
  fi
done
[ "$boundary_fail" = "0" ] || exit 1

echo "LICENSE OK: no GPL crate in the dependency tree; all present process-boundary crates are dependency-free (shell out only)."
