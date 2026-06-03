#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

TARGET_ROOT="$TMP_DIR/target"
RESULT_ROOT="$TMP_DIR/local"
mkdir -p "$TARGET_ROOT/E2E-013" "$RESULT_ROOT"

cat >"$TARGET_ROOT/E2E-013/assertion_report.json" <<'JSON'
{
  "test_id": "E2E-013",
  "status": "passed",
  "reason": "ftrace trace_marker=yes, perf=yes"
}
JSON

"$ROOT_DIR/scripts/e2e/merge-target-smoke.sh" \
  --target-root "$TARGET_ROOT" \
  --result-root "$RESULT_ROOT"

REPORT="$RESULT_ROOT/E2E-013/assertion_report.json"
test -f "$REPORT"
grep -q '"status": "passed"' "$REPORT"
grep -q '"source": "target_smoke"' "$REPORT"
grep -q '"target_report":' "$REPORT"
