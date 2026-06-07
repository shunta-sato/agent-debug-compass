#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RUNNER="$ROOT_DIR/scripts/e2e/target/run-target55-recorder-load-impact-smoke.sh"

"$RUNNER" --help >/tmp/adc-target55-load-impact-help.txt
grep -q "load-impact" /tmp/adc-target55-load-impact-help.txt

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

if "$RUNNER" --binary-dir "$TMP_DIR/missing" --result-root "$TMP_DIR/results" >/tmp/adc-target55-load-impact-missing.out 2>/tmp/adc-target55-load-impact-missing.err; then
  echo "runner should fail when binaries are missing" >&2
  exit 1
fi
grep -q "missing executable adc" /tmp/adc-target55-load-impact-missing.err
