#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

"$ROOT_DIR/scripts/e2e/target/run-pi5-release-smoke.sh" \
  --duration-sec 1 \
  --result-root "$TMP_DIR/results" \
  --binary-dir "$TMP_DIR/missing-release"

for test_id in PI5-SMOKE-001 PI5-SMOKE-002 PI5-SMOKE-003 PI5-SMOKE-004; do
  report="$TMP_DIR/results/$test_id/assertion_report.json"
  test -f "$report"
  grep -q "\"test_id\": \"$test_id\"" "$report"
  grep -q '"status": "skipped"' "$report"
done
