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

step "1/10  cargo fmt --check"
if cargo fmt --check; then record PASS "fmt"; else record FAIL "fmt"; fi

step "2/10  cargo clippy --workspace --all-targets -- -D warnings"
if cargo clippy --workspace --all-targets -- -D warnings; then record PASS "clippy"; else record FAIL "clippy"; fi

step "3/10  cargo test --workspace"
if cargo test --workspace; then record PASS "test"; else record FAIL "test"; fi

step "4/10  ./tools/check_determinism.sh   (HARD — inv. #3)"
if ./tools/check_determinism.sh; then record PASS "determinism"; else record FAIL "determinism [HARD]"; fi

step "4b/10  ./tools/check_determinism_multi_isa.sh   (cross-ISA inv. #3; SKIPs locally — CI matrix is authoritative)"
# The real cross-ISA byte-equality assertion lives in the CI matrix (.github/workflows/ci.yml: determinism-
# multi-isa + assert-isa-match). Locally only one arch is reachable, so this records this host's hash and
# no-op-SKIPs (like the bench gate) unless a second-arch reference hash is supplied. ADR-013 F3 precondition.
if [ -x ./tools/check_determinism_multi_isa.sh ]; then
  MISA_OUT="$(./tools/check_determinism_multi_isa.sh 2>&1)"; MISA_RC=$?
  printf '%s\n' "$MISA_OUT"
  if [ "$MISA_RC" != "0" ]; then
    record FAIL "determinism-multi-isa"
  elif printf '%s' "$MISA_OUT" | grep -q "SKIP"; then
    record SKIP "determinism-multi-isa (single arch local; CI matrix is the gate)"
  else
    record PASS "determinism-multi-isa"
  fi
else
  echo "N/A — tools/check_determinism_multi_isa.sh not present."
  record "N/A" "determinism-multi-isa"
fi

step "5/10  cargo test --workspace --features proptest"
if cargo test --workspace --features proptest; then record PASS "proptest"; else record FAIL "proptest"; fi

step "6/10  cargo bench -p sim-core   (perf §11)"
if [ "${GATE_BENCH:-0}" = "1" ]; then
  if cargo bench -p sim-core; then record PASS "bench"; else record FAIL "bench"; fi
else
  echo "SKIPPED — set GATE_BENCH=1 to run (slow). Perf is enforced at stage exits; baseline in docs/llm/DECISIONS.md."
  record SKIP "bench (GATE_BENCH=1 to run)"
fi

step "7/10  ./tools/check_slim_oracle.sh   (oracle golden §10.6; skips if slim/.venv absent)"
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

step "8/10  ./scripts/check_license.sh   (HARD — inv. #1)"
if [ -x ./scripts/check_license.sh ]; then
  if ./scripts/check_license.sh; then record PASS "license"; else record FAIL "license [HARD]"; fi
else
  echo "N/A — scripts/check_license.sh not present yet (lands in Stage 2 / S2.5)."
  record "N/A" "license"
fi

step "9/10  ./tools/check_godot_snapshot.sh   (UI headless reader §S4.2; skips if godot absent)"
if [ -x ./tools/check_godot_snapshot.sh ]; then
  GODOT_OUT="$(./tools/check_godot_snapshot.sh 2>&1)"; GODOT_RC=$?
  printf '%s\n' "$GODOT_OUT"
  if [ "$GODOT_RC" != "0" ]; then
    record FAIL "godot-reader"
  elif printf '%s' "$GODOT_OUT" | grep -q "SKIP"; then
    record SKIP "godot-reader (godot absent)"
  else
    record PASS "godot-reader"
  fi
else
  echo "N/A — tools/check_godot_snapshot.sh not present."
  record "N/A" "godot-reader"
fi

step "10/10  ./tools/check_livesim.sh   (live-sim GDExtension smoke §R6/P1b; skips if godot/cdylib absent)"
if [ -x ./tools/check_livesim.sh ]; then
  LIVE_OUT="$(./tools/check_livesim.sh 2>&1)"; LIVE_RC=$?
  printf '%s\n' "$LIVE_OUT"
  if [ "$LIVE_RC" != "0" ]; then
    record FAIL "livesim"
  elif printf '%s' "$LIVE_OUT" | grep -q "SKIP"; then
    record SKIP "livesim (godot/cdylib absent)"
  else
    record PASS "livesim"
  fi
else
  echo "N/A — tools/check_livesim.sh not present."
  record "N/A" "livesim"
fi

printf '\n\033[1m==== GATE SUMMARY ====\033[0m\n'
printf '%s\n' "$SUMMARY"
if [ "$FAILED" = "1" ]; then
  printf '\033[31m\nGATE: RED — STOP THE LINE. Fix or revert; do not commit.\033[0m\n'
  exit 1
fi
printf '\033[32m\nGATE: GREEN.\033[0m\n'
