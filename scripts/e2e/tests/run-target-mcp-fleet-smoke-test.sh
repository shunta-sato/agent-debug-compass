#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

RUNNER="$ROOT_DIR/scripts/e2e/target/run-target-mcp-fleet-smoke.sh"
if [[ ! -x "$RUNNER" ]]; then
  echo "skip: optional target MCP fleet smoke runner is not present in this public tree"
  exit 0
fi

"$RUNNER" \
  --host example-target \
  --target-id harrikka-rp4 \
  --binary-dir "$TMP_DIR/missing-release" \
  --result-root "$TMP_DIR/missing-results"

for test_id in FLEET-SMOKE-001 FLEET-SMOKE-002 FLEET-SMOKE-003 FLEET-SMOKE-004; do
  report="$TMP_DIR/missing-results/$test_id/assertion_report.json"
  test -f "$report"
  grep -q "\"test_id\": \"$test_id\"" "$report"
  grep -q '"status": "skipped"' "$report"
done

FAKE_BIN="$TMP_DIR/fake-release"
mkdir -p "$FAKE_BIN"
cat >"$FAKE_BIN/adc" <<'PROBE'
#!/usr/bin/env bash
set -euo pipefail
expected_targets="${ADC_FAKE_EXPECTED_TARGETS:-1}"
command="${1:-}"
shift || true
case "$command" in
  fleet)
    subcommand="${1:-}"
    shift || true
    fleet_run_id=""
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --fleet-run-id)
          fleet_run_id="$2"
          shift 2
          ;;
        *)
          shift
          ;;
      esac
    done
    case "$subcommand" in
      preflight)
        cat <<JSON
{
  "schema_version": "obs.fleet_preflight.v1",
  "status": "ready",
  "target_count": $expected_targets,
  "ready_count": $expected_targets,
  "failed_count": 0,
  "data_quality": {"missing": []}
}
JSON
        ;;
      snapshot|observe|capture)
        mkdir -p "$ADC_HOME/fleet_runs/$fleet_run_id/targets/harrikka-rp4"
        cat >"$ADC_HOME/fleet_runs/$fleet_run_id/fleet_evidence.yaml" <<YAML
schema_version: obs.fleet.v2
fleet_run_id: $fleet_run_id
target_count: $expected_targets
captured_count: $expected_targets
failed_count: 0
target_matrix:
- target_id: harrikka-rp4
  transport: mcp_stdio_over_ssh
  status: captured
  run_id: ${fleet_run_id}-harrikka-rp4
  profile_id: mcp_observe
  evidence_ref: artifact://fleet_runs/$fleet_run_id/targets/harrikka-rp4/evidence_index.yaml
  data_quality:
    missing: []
raw_refs: {}
data_quality:
  missing: []
YAML
        cat >"$ADC_HOME/fleet_runs/$fleet_run_id/targets/harrikka-rp4/evidence_index.yaml" <<YAML
schema_version: obs.v2
run_id: ${fleet_run_id}-harrikka-rp4
target_id: harrikka-rp4
primary_window:
  window_id: W001
  event_count: 3
observed_facts: []
data_quality:
  missing: []
YAML
        cat <<JSON
{
  "fleet_run_id": "$fleet_run_id",
  "target_count": $expected_targets,
  "captured_count": $expected_targets,
  "failed_count": 0,
  "data_quality": {"missing": []}
}
JSON
        ;;
      evidence)
        cat "$ADC_HOME/fleet_runs/$fleet_run_id/fleet_evidence.yaml"
        ;;
      *)
        echo "unexpected fleet subcommand: $subcommand" >&2
        exit 2
        ;;
    esac
    ;;
  agent-context)
    cat <<JSON
{
  "schema_version": "obs.agent_context.fleet.v1",
  "fleet_run_id": "F-FAKE-CAPTURE",
  "captured_count": $expected_targets,
  "failed_count": 0,
  "target_summaries": [{"target_id": "harrikka-rp4", "event_count": 3}]
}
JSON
    ;;
  *)
    echo "unexpected command: $command" >&2
    exit 2
    ;;
esac
PROBE
chmod +x "$FAKE_BIN/adc"

"$RUNNER" \
  --host example-target \
  --target-id harrikka-rp4 \
  --mcp-server-path /home/pi/.local/bin/adc-mcp \
  --binary-dir "$FAKE_BIN" \
  --duration-sec 1 \
  --result-root "$TMP_DIR/results"

for test_id in FLEET-SMOKE-001 FLEET-SMOKE-002 FLEET-SMOKE-003 FLEET-SMOKE-004; do
  report="$TMP_DIR/results/$test_id/assertion_report.json"
  test -f "$report"
  grep -q '"status": "passed"' "$report"
done
grep -q '"captured_count": 1' "$TMP_DIR/results/FLEET-SMOKE-004/fleet_agent_context.json"
grep -q 'mcp_server_path: /home/pi/.local/bin/adc-mcp' "$TMP_DIR/results/targets.yaml"

cat >"$TMP_DIR/two-targets.yaml" <<'YAML'
targets:
  - id: harrikka-rp4
    transport: mcp_stdio_over_ssh
    host: example-target
    mcp_server_path: /home/pi/.local/bin/adc-mcp
  - id: lab-rp5
    transport: mcp_stdio_over_ssh
    host: target56
    mcp_server_path: /home/pi/.local/bin/adc-mcp
YAML

ADC_FAKE_EXPECTED_TARGETS=2 \
"$RUNNER" \
  --inventory "$TMP_DIR/two-targets.yaml" \
  --binary-dir "$FAKE_BIN" \
  --duration-sec 1 \
  --result-root "$TMP_DIR/two-target-results"

grep -q '"captured_count": 2' "$TMP_DIR/two-target-results/snapshot.json"
grep -q '"captured_count": 2' "$TMP_DIR/two-target-results/FLEET-SMOKE-004/fleet_agent_context.json"
