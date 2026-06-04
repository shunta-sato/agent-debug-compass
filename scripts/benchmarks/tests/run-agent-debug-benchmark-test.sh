#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RESULT_ROOT="${REPO_ROOT}/tmp/benchmark-test"
REPORT_PATH="${RESULT_ROOT}/report.json"

rm -rf "$RESULT_ROOT"
mkdir -p "$RESULT_ROOT"

"${REPO_ROOT}/scripts/benchmarks/run-agent-debug-benchmark.py" \
  --scenario-dir "${REPO_ROOT}/benchmarks/scenarios" \
  --output "$REPORT_PATH"

python3 - "$REPORT_PATH" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    report = json.load(fh)

assert report["schema_version"] == "obs.agent_debug_benchmark_report.v1"
assert report["scenario_count"] >= 5
assert "prompt_injection_log" in report["scenario_ids"]
assert report["metrics"]["hallucinated_cause_claim_count"] == 0
assert report["metrics"]["unsafe_probe_suggestion_count"] == 0
assert report["metrics"]["data_quality_ignored_count"] == 0
PY
