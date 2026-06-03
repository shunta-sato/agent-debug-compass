#!/usr/bin/env bash
set -euo pipefail

DEFAULT_TARGET_ROOT="/var/tmp/agent-debug-compass-target-smoke/${USER:-unknown}"
TARGET_ROOT="${ADC_TARGET_SMOKE_RESULT_ROOT:-$DEFAULT_TARGET_ROOT}"
RESULT_ROOT="${ADC_E2E_RESULT_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/e2e-results/local}"
CASES=(E2E-013 E2E-014 E2E-015 SEC-004)

usage() {
  cat <<'USAGE'
Usage: merge-target-smoke.sh [--target-root DIR] [--result-root DIR]

Imports explicit target/root smoke assertion reports into a local E2E result root.
The script only imports reports that already exist under TARGET_ROOT.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target-root)
      TARGET_ROOT="${2:?missing --target-root value}"
      shift 2
      ;;
    --result-root)
      RESULT_ROOT="${2:?missing --result-root value}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

json_string_field() {
  local field="$1"
  local file="$2"
  sed -n "s/^[[:space:]]*\"$field\"[[:space:]]*:[[:space:]]*\"\\([^\"]*\\)\".*/\\1/p" "$file" | head -n 1
}

json_escape() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//$'\n'/ }"
  printf '%s' "$value"
}

mkdir -p "$RESULT_ROOT"

for test_id in "${CASES[@]}"; do
  report="$TARGET_ROOT/$test_id/assertion_report.json"
  [[ -f "$report" ]] || continue

  status="$(json_string_field status "$report")"
  reason="$(json_string_field reason "$report")"
  [[ -n "$status" ]] || status="failed"
  [[ -n "$reason" ]] || reason="target smoke report did not include a reason"

  out_dir="$RESULT_ROOT/$test_id"
  mkdir -p "$out_dir"
  cat >"$out_dir/assertion_report.json" <<JSON
{
  "test_id": "$(json_escape "$test_id")",
  "status": "$(json_escape "$status")",
  "reason": "$(json_escape "$reason")",
  "source": "target_smoke",
  "target_report": "$(json_escape "$report")"
}
JSON
done
