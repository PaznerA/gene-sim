#!/usr/bin/env bash
# tools/gate.sh — the single robust gate runner (SPEC §10). Runs ALL gates, prints PASS/FAIL/SKIP/N-A
# per item, and exits non-zero if ANY gate FAILED. This is the deterministic backbone of the dev loop:
# humans and agents run the exact same command. "Any red = STOP THE LINE."
#
# Hard, non-negotiable gates: determinism (inv. #3), oracle golden (Stage 2+), and license (inv. #1).
#
# Usage:
#   tools/gate.sh                 # full gate; perf bench SKIPPED (it's slow) unless GATE_BENCH=1
#   GATE_BENCH=1 tools/gate.sh    # also run the criterion perf bench — use at stage exits (§11)
set -uo pipefail   # NOT -e: we run every gate, collect results, then exit non-zero if any failed.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# The harness shell may not have Cargo on PATH (SNIPPETS gotcha).
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

FAILED=0
SUMMARY=""

record() { # record <STATUS> <label>
  SUMMARY="${SUMMARY}"$'\n'"  $1	$2"
  [ "$1" = "FAIL" ] && FAILED=1
  return 0
}
step() { printf '\n\033[1m── %s\033[0m\n' "$1"; }

step "1/8  cargo fmt --check"
if cargo fmt --check; then record PASS "fmt"; else record FAIL "fmt"; fi

step "2/8  cargo clippy --workspace --all-targets -- -D warnings"
if cargo clippy --workspace --all-targets -- -D warnings; then record PASS "clippy"; else record FAIL "clippy"; fi

step "3/8  cargo test --workspace"
if cargo test --workspace; then record PASS "test"; else record FAIL "test"; fi

step "4/8  ./tools/check_determinism.sh   (HARD — inv. #3)"
if ./tools/check_determinism.sh; then record PASS "determinism"; else record FAIL "determinism [HARD]"; fi

step "5/8  cargo test --workspace --features proptest"
if cargo test --workspace --features proptest; then record PASS "proptest"; else record FAIL "proptest"; fi

step "6/8  cargo bench -p sim-core   (perf §11)"
if [ "${GATE_BENCH:-0}" = "1" ]; then
  if cargo bench -p sim-core; then record PASS "bench"; else record FAIL "bench"; fi
else
  echo "SKIPPED — set GATE_BENCH=1 to run (slow). Perf is enforced at stage exits; baseline in docs/llm/DECISIONS.md."
  record SKIP "bench (GATE_BENCH=1 to run)"
fi

step "7/8  ./tools/check_slim_oracle.sh   (oracle golden §10.6; skips if slim/.venv absent)"
if [ -x ./tools/check_slim_oracle.sh ]; then
  ORACLE_OUT="$(./tools/check_slim_oracle.sh 2>&1)"; ORACLE_RC=$?
  printf '%s\n' "$ORACLE_OUT"
  if [ "$ORACLE_RC" != "0" ]; then
    record FAIL "oracle [HARD]"
  elif printf '%s' "$ORACLE_OUT" | grep -q "SKIP"; then
    record SKIP "oracle (slim/.venv absent)"
  else
    record PASS "oracle"
  fi
else
  echo "N/A — tools/check_slim_oracle.sh not present."
  record "N/A" "oracle"
fi

step "8/8  ./scripts/check_license.sh   (HARD — inv. #1)"
if [ -x ./scripts/check_license.sh ]; then
  if ./scripts/check_license.sh; then record PASS "license"; else record FAIL "license [HARD]"; fi
else
  echo "N/A — scripts/check_license.sh not present yet (lands in Stage 2 / S2.5)."
  record "N/A" "license"
fi

printf '\n\033[1m==== GATE SUMMARY ====\033[0m\n'
printf '%s\n' "$SUMMARY"
if [ "$FAILED" = "1" ]; then
  printf '\033[31m\nGATE: RED — STOP THE LINE. Fix or revert; do not commit.\033[0m\n'
  exit 1
fi
printf '\033[32m\nGATE: GREEN.\033[0m\n'
