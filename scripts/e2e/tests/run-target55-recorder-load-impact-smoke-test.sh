#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RUNNER="$ROOT_DIR/scripts/e2e/target/run-target55-recorder-load-impact-smoke.sh"

"$RUNNER" --help >/tmp/adc-target55-load-impact-help.txt
grep -q "load-impact" /tmp/adc-target55-load-impact-help.txt
grep -q -- "--profile-interval-ms" /tmp/adc-target55-load-impact-help.txt
grep -q -- "--evaluation-mode" /tmp/adc-target55-load-impact-help.txt
grep -q "production_safe" /tmp/adc-target55-load-impact-help.txt
grep -q "high_frequency_stress" /tmp/adc-target55-load-impact-help.txt
grep -q "deployability" /tmp/adc-target55-load-impact-help.txt
grep -q "deployability_passed" "$RUNNER"
grep -q "resource_violation" "$RUNNER"
grep -q "max_production_adc_targetd_cpu_ratio" "$RUNNER"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

if "$RUNNER" --binary-dir "$TMP_DIR/missing" --result-root "$TMP_DIR/results" >/tmp/adc-target55-load-impact-missing.out 2>/tmp/adc-target55-load-impact-missing.err; then
  echo "runner should fail when binaries are missing" >&2
  exit 1
fi
grep -q "missing executable adc" /tmp/adc-target55-load-impact-missing.err
