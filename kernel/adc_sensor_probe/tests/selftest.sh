#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MODULE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
RESULT_DIR="${ADC_E2E_RESULT_DIR:-$MODULE_DIR/test-results}"
KDIR="${KDIR:-/lib/modules/$(uname -r)/build}"
MODE="${1:---build-only}"

mkdir -p "$RESULT_DIR"

write_report() {
  local status="$1"
  local reason="$2"
  cat >"$RESULT_DIR/assertion_report.json" <<JSON
{
  "test_id": "KO-SELFTEST",
  "status": "$status",
  "reason": "$reason",
  "module": "adc_sensor_probe"
}
JSON
}

if [[ ! -d "$KDIR" ]]; then
  write_report "skipped" "kernel build directory not found: $KDIR"
  exit 0
fi

make -C "$MODULE_DIR" KDIR="$KDIR" >"$RESULT_DIR/build.log" 2>&1

if [[ "$MODE" != "--load" ]]; then
  write_report "passed" "build-only self-test completed"
  exit 0
fi

if [[ "$(id -u)" != "0" ]]; then
  write_report "skipped" "module load requires root"
  exit 0
fi

insmod "$MODULE_DIR/adc_sensor_probe.ko"
trap 'rmmod adc_sensor_probe || true' EXIT
cat /dev/adc_sensor_probe >"$RESULT_DIR/selftest_event.jsonl"
rmmod adc_sensor_probe
trap - EXIT
write_report "passed" "module load/read/unload self-test completed"
