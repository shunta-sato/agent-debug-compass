#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
HOST="target55"
BINARY_DIR="$ROOT_DIR/target/debug"
RESULT_ROOT="$ROOT_DIR/tmp/target55-resource-discipline-smoke"
REMOTE_ROOT="/tmp/adc-pr10-resource-discipline-${USER:-user}-$$"
KEEP_REMOTE=0

usage() {
  cat <<'USAGE'
Usage: run-target55-resource-discipline-smoke.sh [options]

Options:
  --host HOST          SSH host alias to use. Default: target55.
  --binary-dir DIR     Directory containing adc and adc-targetd. Default: target/debug.
  --result-root DIR    Local directory for smoke outputs. Default: tmp/target55-resource-discipline-smoke.
  --remote-root DIR    Remote temporary root. Default: /tmp/adc-pr10-resource-discipline-$USER-$$.
  --keep-remote        Do not remove the remote temporary root after the smoke.
  --help               Show this help.

This smoke is intentionally rootless. It does not run arbitrary shell tools
through ADC; SSH is used only to deploy binaries and execute the fixed smoke
sequence on the target.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --host)
      HOST="${2:?missing --host value}"
      shift 2
      ;;
    --binary-dir)
      BINARY_DIR="${2:?missing --binary-dir value}"
      shift 2
      ;;
    --result-root)
      RESULT_ROOT="${2:?missing --result-root value}"
      shift 2
      ;;
    --remote-root)
      REMOTE_ROOT="${2:?missing --remote-root value}"
      shift 2
      ;;
    --keep-remote)
      KEEP_REMOTE=1
      shift
      ;;
    --help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

ADC_BIN="$BINARY_DIR/adc"
TARGETD_BIN="$BINARY_DIR/adc-targetd"
if [[ ! -x "$ADC_BIN" ]]; then
  echo "missing executable adc at $ADC_BIN" >&2
  exit 1
fi
if [[ ! -x "$TARGETD_BIN" ]]; then
  echo "missing executable adc-targetd at $TARGETD_BIN" >&2
  exit 1
fi

rm -rf "$RESULT_ROOT"
mkdir -p "$RESULT_ROOT"

ssh -o BatchMode=yes -o ConnectTimeout=10 "$HOST" \
  'uname -a; id; command -v python3 || true; ls /sys/class/power_supply 2>/dev/null || true' \
  >"$RESULT_ROOT/target_identity.txt"

ssh "$HOST" "rm -rf '$REMOTE_ROOT' && mkdir -p '$REMOTE_ROOT/bin' '$REMOTE_ROOT/profiles' '$REMOTE_ROOT/results'"
scp "$ADC_BIN" "$TARGETD_BIN" "$HOST:$REMOTE_ROOT/bin/" >/dev/null
ssh "$HOST" "chmod 0755 '$REMOTE_ROOT/bin/adc' '$REMOTE_ROOT/bin/adc-targetd'"

ssh "$HOST" "cat > '$REMOTE_ROOT/profiles/recorder_network_memory.yaml'" <<'YAML'
profile: recorder_network_memory
sampling:
  interval_ms: 10
always_on:
  collectors: [memory, network]
budgets:
  max_daemon_cpu_percent: 3
  max_memory_mb: 128
  max_artifact_mb_per_run: 16
triggers: []
YAML

ssh "$HOST" "set -euo pipefail
export PATH='$REMOTE_ROOT/bin':\$PATH

ADC_HOME='$REMOTE_ROOT/no-trigger' ADC_PROFILE_DIR='$REMOTE_ROOT/profiles' adc arm --profile recorder_network_memory > '$REMOTE_ROOT/results/no_trigger_arm.json'
ADC_HOME='$REMOTE_ROOT/no-trigger' ADC_PROFILE_DIR='$REMOTE_ROOT/profiles' adc-targetd --service-for-ms 250 > '$REMOTE_ROOT/results/no_trigger_summary.json'
ADC_HOME='$REMOTE_ROOT/no-trigger' ADC_PROFILE_DIR='$REMOTE_ROOT/profiles' adc recorder status > '$REMOTE_ROOT/results/no_trigger_status.json'

ADC_HOME='$REMOTE_ROOT/battery-low' ADC_PROFILE_DIR='$REMOTE_ROOT/profiles' adc arm --profile recorder_network_memory > '$REMOTE_ROOT/results/battery_arm.json'
ADC_HOME='$REMOTE_ROOT/battery-low' ADC_PROFILE_DIR='$REMOTE_ROOT/profiles' adc recorder mark --marker-id marker-target55-battery-low --symptom 'target55 battery low resource discipline marker' > '$REMOTE_ROOT/results/battery_marker.json'
ADC_RECORDER_POWER_MODE=battery_low ADC_HOME='$REMOTE_ROOT/battery-low' ADC_PROFILE_DIR='$REMOTE_ROOT/profiles' adc-targetd --service-for-ms 350 > '$REMOTE_ROOT/results/battery_summary.json'
ADC_HOME='$REMOTE_ROOT/battery-low' ADC_PROFILE_DIR='$REMOTE_ROOT/profiles' adc recorder status > '$REMOTE_ROOT/results/battery_status.json'
ADC_HOME='$REMOTE_ROOT/battery-low' ADC_PROFILE_DIR='$REMOTE_ROOT/profiles' adc recorder incidents > '$REMOTE_ROOT/results/battery_incidents.json'
ADC_HOME='$REMOTE_ROOT/battery-low' ADC_PROFILE_DIR='$REMOTE_ROOT/profiles' adc recorder incident get --incident-id INC-marker-target55-battery-low > '$REMOTE_ROOT/results/battery_incident_resolution.json'
ADC_HOME='$REMOTE_ROOT/battery-low' ADC_PROFILE_DIR='$REMOTE_ROOT/profiles' adc investigate ref --ref artifact://recorder/incidents/INC-marker-target55-battery-low/coverage.json --limit 220 > '$REMOTE_ROOT/results/battery_coverage_ref.json'
ADC_HOME='$REMOTE_ROOT/battery-low' ADC_PROFILE_DIR='$REMOTE_ROOT/profiles' adc investigate ref --ref artifact://recorder/incidents/INC-marker-target55-battery-low/loss_report.json --limit 80 > '$REMOTE_ROOT/results/battery_loss_report_ref.json'
"

scp -r "$HOST:$REMOTE_ROOT/results/." "$RESULT_ROOT/" >/dev/null

python3 - "$RESULT_ROOT" <<'PY'
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])

def load(name):
    with (root / name).open("r", encoding="utf-8") as fh:
        return json.load(fh)

no_trigger_status = load("no_trigger_status.json")
resource = no_trigger_status["resource_status"]
assert resource["schema_version"] == "obs.recorder_resource_status.v1"
assert resource["continuous_ring_disk_write_bytes"] == 0
assert resource["frozen_artifact_write_bytes"] == 0
assert resource["network_upload_bytes"] == 0

battery_status = load("battery_status.json")
battery_resource = battery_status["resource_status"]
assert battery_resource["policy_mode"] == "battery_low"
assert battery_resource["continuous_ring_disk_write_bytes"] == 0
assert battery_resource["data_quality"]["throttled"] is True
assert any(
    "network.counters" in decision.get("affected_signals", [])
    for decision in battery_resource.get("degradation_decisions", [])
)

resolution = load("battery_incident_resolution.json")
assert resolution["coverage_ref"].endswith("/coverage.json")
assert resolution["loss_report_ref"].endswith("/loss_report.json")

coverage_ref = load("battery_coverage_ref.json")
coverage_text = json.dumps(coverage_ref)
assert "obs.recorder_observation_coverage.v1" in coverage_text
assert "battery_low_policy" in coverage_text

loss_ref = load("battery_loss_report_ref.json")
loss_text = json.dumps(loss_ref)
assert "obs.loss_report.v1" in loss_text
assert "battery_low_policy" in loss_text

summary = {
    "schema_version": "adc.target55_resource_discipline_smoke.v1",
    "target": "target55",
    "passed": True,
    "no_trigger_continuous_ring_disk_write_bytes": resource["continuous_ring_disk_write_bytes"],
    "battery_low_policy_mode": battery_resource["policy_mode"],
    "coverage_ref": resolution["coverage_ref"],
    "loss_report_ref": resolution["loss_report_ref"],
}
(root / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
print(json.dumps(summary, indent=2))
PY

if [[ "$KEEP_REMOTE" -eq 0 ]]; then
  ssh "$HOST" "rm -rf '$REMOTE_ROOT'"
else
  echo "remote smoke root retained: $HOST:$REMOTE_ROOT" >"$RESULT_ROOT/remote_retained.txt"
fi
