#!/usr/bin/env bash
set -euo pipefail

RESULT_DIR="${1:?missing result dir}"
ADC_STATE_ROOT="${2:?missing ADC_HOME}"
ADC_BIN="${3:?missing adc command}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"

mkdir -p "$RESULT_DIR"

KIT_PATH="$("$ROOT_DIR/scripts/install/create-managed-mcp-enrollment-kit.sh" \
  --kit-dir "$RESULT_DIR/kit" \
  --target-id e2e-kit-target \
  --host e2e-kit.local \
  --port 39250 \
  --tag e2e-kit)"

test -s "$KIT_PATH"
ADC_HOME="$ADC_STATE_ROOT" bash -lc "$ADC_BIN fleet enroll-kit --kit '$KIT_PATH'" \
  >"$RESULT_DIR/enroll.json"
ADC_HOME="$ADC_STATE_ROOT" bash -lc "$ADC_BIN fleet targets" \
  >"$RESULT_DIR/targets.json"

grep -q '"target_id": "e2e-kit-target"' "$RESULT_DIR/enroll.json"
grep -q '"transport": "managed_mcp"' "$RESULT_DIR/enroll.json"
grep -q '"enrollment_mode": "kit"' "$RESULT_DIR/enroll.json"
grep -q '"target_id": "e2e-kit-target"' "$RESULT_DIR/targets.json"
grep -q '"auth_token_file": "' "$RESULT_DIR/targets.json"
grep -q '"tls_client_cert_file": "' "$RESULT_DIR/targets.json"
