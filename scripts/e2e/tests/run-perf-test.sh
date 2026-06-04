#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

RUNNER="$ROOT_DIR/scripts/e2e/target/run-perf.sh"
if [[ ! -x "$RUNNER" ]]; then
  echo "skip: optional perf smoke runner is not present in this public tree"
  exit 0
fi

"$RUNNER" \
  --duration-sec 1 \
  --result-root "$TMP_DIR/results" \
  --binary-dir "$TMP_DIR/missing-release"

for test_id in PERF-001 PERF-002 PERF-003 PERF-004; do
  report="$TMP_DIR/results/$test_id/assertion_report.json"
  test -f "$report"
  grep -q "\"test_id\": \"$test_id\"" "$report"
  grep -q '"status": "skipped"' "$report"
done
