#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RESULT_ROOT="$(mktemp -d)"
trap 'rm -rf "$RESULT_ROOT"' EXIT

DRY_RUN_OUTPUT="$("$ROOT_DIR/scripts/demo/run-sensor-gateway-demo.sh" --dry-run --result-root "$RESULT_ROOT" 2>&1)"
grep -q 'mode: dry-run' <<<"$DRY_RUN_OUTPUT"
grep -q 'demo_sensor_gateway' <<<"$DRY_RUN_OUTPUT"
grep -q 'retry-storm' <<<"$DRY_RUN_OUTPUT"
grep -q 'memory-leak' <<<"$DRY_RUN_OUTPUT"

if "$ROOT_DIR/scripts/demo/run-sensor-gateway-demo.sh" --quick --result-root / >/tmp/adc-demo-root-guard.out 2>/tmp/adc-demo-root-guard.err; then
  echo "expected root result path to be rejected" >&2
  exit 1
fi
grep -q 'refusing unsafe result root' /tmp/adc-demo-root-guard.err

"$ROOT_DIR/scripts/demo/run-sensor-gateway-demo.sh" --quick --result-root "$RESULT_ROOT"

test -f "$RESULT_ROOT/agent_context.md"
test -f "$RESULT_ROOT/state/runs/R-DEMO-BASELINE-after/manifest.json"
test -f "$RESULT_ROOT/state/runs/R-DEMO-RETRY-after/manifest.json"
test -f "$RESULT_ROOT/state/runs/R-DEMO-MEMORY-after/manifest.json"
test -f "$RESULT_ROOT/state/runs/R-DEMO-BASELINE-after/raw/app_events.jsonl"
test -f "$RESULT_ROOT/state/runs/R-DEMO-RETRY-after/raw/app_events.jsonl"
test -f "$RESULT_ROOT/state/runs/R-DEMO-MEMORY-after/raw/app_events.jsonl"
test -f "$RESULT_ROOT/reports/retry_vs_baseline.compare.json"
test -f "$RESULT_ROOT/reports/memory_vs_baseline.compare.json"
test -f "$RESULT_ROOT/reports/daemon.agent_context.md"
test -f "$RESULT_ROOT/reports/daemon.agent_context.prom"
grep -q 'Agent Context' "$RESULT_ROOT/agent_context.md"
grep -q 'obs.get_agent_context' "$RESULT_ROOT/agent_context.md"
grep -q 'adc_agent_context_info' "$RESULT_ROOT/reports/daemon.agent_context.prom"
grep -q 'raw_refs' "$RESULT_ROOT/agent_context.md"
grep -q 'R-DEMO-RETRY' "$RESULT_ROOT/agent_context.md"
grep -q 'R-DEMO-MEMORY' "$RESULT_ROOT/agent_context.md"
