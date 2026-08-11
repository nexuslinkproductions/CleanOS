#!/usr/bin/env bash
# CleanOS verification: builds the binary and runs the live diagnostics.
# The diagnostic checks live in `cleanos doctor` (src/doctor.rs): probe
# errors, sockets, system-path leaks, reap_safe discipline, redaction, JSON
# roundtrip, and bench tolerance. This script adds the schema parse check.
# Fixture-based unit tests are deliberately absent; they encode the same
# assumptions as the code and verify nothing.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
REPO="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${CLEANOS_BIN:-$REPO/target/debug/cleanos}"
FAILURES=0

check() {
  local name="$1" ok="$2" detail="$3"
  if [ "$ok" = "1" ]; then echo "PASS  $name"; else echo "FAIL  $name  [$detail]"; FAILURES=$((FAILURES+1)); fi
}

echo "== CleanOS verification ($(date +%H:%M:%S)) =="

if cargo build --manifest-path "$REPO/Cargo.toml" >/dev/null 2>&1; then
  check "cargo build" 1 ""
else
  check "cargo build" 0 "build failed"
  echo "verify aborted"; exit 1
fi

"$BIN" doctor --bench
check "cleanos doctor (live diagnostics + bench tolerance)" "$([ $? -eq 0 ] && echo 1 || echo 0)" "doctor rc=$?"

for s in "$REPO"/schemas/*.json; do
  python3 -c "import json; json.load(open('$s'))" 2>/dev/null
  check "schema parses: $(basename "$s")" "$([ $? -eq 0 ] && echo 1 || echo 0)" ""
done

echo
if [ "$FAILURES" -eq 0 ]; then echo "VERIFY OK"; else echo "VERIFY FAILED: $FAILURES check(s)"; fi
exit "$FAILURES"
