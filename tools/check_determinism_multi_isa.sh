#!/usr/bin/env bash
# tools/check_determinism_multi_isa.sh — cross-ISA determinism gate (ADR-013 §"Cross-platform determinism
# gate"; the HARD precondition for phase F3). The single-host gate (tools/check_determinism.sh) only proves
# run==run ON ONE ARCH — it would silently bless a reproducible-but-arch-DIVERGENT hash. The "real gate" runs
# the SAME pinned harness invocation on BOTH x86_64 AND aarch64 and asserts the two hashes are BYTE-IDENTICAL.
#
# That cross-arch comparison is owned by the CI matrix (.github/workflows/ci.yml jobs `determinism-multi-isa`
# + `assert-isa-match`) where two real runners (ubuntu-latest = x86_64, ubuntu-24.04-arm = aarch64) execute
# the binary natively — emulation/cross-compile would not prove the runtime float behaviour.
#
# LOCALLY there is normally only one arch reachable, so — exactly like the bench gate — this script NO-OP-SKIPs
# (exit 0, prints "SKIP — ...") rather than failing. It still does the useful local half: it RECORDS this
# host's hash for the pinned invocation and, if a second-arch hash has been deposited (env CROSS_ISA_REF_HASH,
# or a file named by CROSS_ISA_REF_FILE), it asserts byte-equality against it. This lets a developer with two
# machines wire a real local check; CI is the authoritative gate.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

command -v cargo >/dev/null 2>&1 || { echo "SKIP — cargo not installed"; exit 0; }

# Pinned (seed, generations, entities) — MUST match crates/sim-core determinism_hash_is_pinned and the
# ISA_* env in .github/workflows/ci.yml. At F3 this is re-tuned to fire births/deaths/contention.
SEED="${ISA_SEED:-13679457532755275413}"
GENERATIONS="${ISA_GENERATIONS:-50}"
ENTITIES="${ISA_ENTITIES:-1000}"

ARCH="$(uname -m)"
HASH="$(cargo run -q --release -p harness -- \
  --seed "$SEED" --generations "$GENERATIONS" --entities "$ENTITIES" --hash-only)" || {
  echo "SKIP — harness did not build/run on this host"; exit 0; }

echo "this host: arch=$ARCH hash=$HASH (seed=$SEED generations=$GENERATIONS entities=$ENTITIES)"

# A reference hash from the OTHER arch may be supplied locally for a real cross-arch assert.
REF=""
if [ -n "${CROSS_ISA_REF_HASH:-}" ]; then
  REF="$CROSS_ISA_REF_HASH"
elif [ -n "${CROSS_ISA_REF_FILE:-}" ] && [ -f "${CROSS_ISA_REF_FILE}" ]; then
  REF="$(cat "${CROSS_ISA_REF_FILE}")"
fi

if [ -z "$REF" ]; then
  echo "SKIP — no second-arch reference hash available locally (set CROSS_ISA_REF_HASH or CROSS_ISA_REF_FILE)."
  echo "       The authoritative cross-ISA byte-equality assertion runs in CI (assert-isa-match job)."
  exit 0
fi

if [ "$HASH" != "$REF" ]; then
  echo "CROSS-ISA DETERMINISM FAIL: this host ($ARCH: $HASH) != reference ($REF)" >&2
  exit 1
fi
echo "CROSS-ISA OK: $ARCH hash == reference == $HASH (byte-identical)"
