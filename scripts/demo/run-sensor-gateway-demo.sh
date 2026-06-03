#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RESULT_ROOT="${ADC_DEMO_RESULT_ROOT:-$ROOT_DIR/demo-results/sensor-gateway}"
ADC_STATE_ROOT=""
PROFILE_DIR=""
REPORT_DIR=""
KMSG_FIXTURE=""

default_bin_cmd() {
  local bin="$1"
  local crate="$2"
  if [[ -x "$ROOT_DIR/bin/$bin" ]]; then
    printf '%s\n' "$ROOT_DIR/bin/$bin"
  else
    printf 'cargo run -q -p %s --\n' "$crate"
  fi
}

ADC_BIN="${ADC_BIN:-$(default_bin_cmd adc adc)}"
ADC_TARGETD_BIN="${ADC_TARGETD_BIN:-$(default_bin_cmd adc-targetd adc-targetd)}"
ADC_DEMO_BIN="${ADC_DEMO_BIN:-$(default_bin_cmd adc-demo-sensor-gateway adc-demo-sensor-gateway)}"
MCP_BIN="${ADC_MCP_BIN:-$(default_bin_cmd adc-mcp adc-mcp)}"

MODE="run"
QUICK=0

usage() {
  cat <<'USAGE'
Usage:
  scripts/demo/run-sensor-gateway-demo.sh [--quick] [--result-root DIR]
  scripts/demo/run-sensor-gateway-demo.sh --dry-run [--result-root DIR]

Options:
  --quick          Use very short scenario durations for CI/script smoke tests.
  --result-root   Write demo outputs under DIR.
  --dry-run       Print planned commands without running them.
  -h, --help      Show this help.

Outputs:
  agent_context.md
  reports/*.json
  state/runs/* artifacts
USAGE
}

log() {
  printf '[run-sensor-gateway-demo.sh] %s\n' "$*"
}

die() {
  printf '[run-sensor-gateway-demo.sh] ERROR: %s\n' "$*" >&2
  exit 1
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --quick)
        QUICK=1
        ;;
      --result-root)
        RESULT_ROOT="${2:?missing --result-root value}"
        shift
        ;;
      --dry-run)
        MODE="dry-run"
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        die "unknown argument: $1"
        ;;
    esac
    shift
  done
}

set_paths() {
  ADC_STATE_ROOT="$RESULT_ROOT/state"
  PROFILE_DIR="$RESULT_ROOT/profiles"
  REPORT_DIR="$RESULT_ROOT/reports"
  KMSG_FIXTURE="$RESULT_ROOT/kmsg_fixture.log"
}

duration_ms() {
  local normal="$1"
  local quick="$2"
  if [[ "$QUICK" -eq 1 ]]; then
    printf '%s\n' "$quick"
  else
    printf '%s\n' "$normal"
  fi
}

packet_attempts() {
  if [[ "$QUICK" -eq 1 ]]; then
    printf '3\n'
  else
    printf '200\n'
  fi
}

retained_kb() {
  if [[ "$QUICK" -eq 1 ]]; then
    printf '64\n'
  else
    printf '8192\n'
  fi
}

print_plan() {
  log "mode: $MODE"
  log "result_root: $RESULT_ROOT"
  log "state_root: $ADC_STATE_ROOT"
  log "profile: demo_sensor_gateway"
  log "scenario: baseline"
  log "scenario: retry-storm"
  log "scenario: memory-leak"
  log "agent context: $RESULT_ROOT/agent_context.md"
}

validate_result_root() {
  local normalized
  normalized="$(realpath -m "$RESULT_ROOT")"
  case "$normalized" in
    /|"$ROOT_DIR"|"$HOME"|.)
      die "refusing unsafe result root: $RESULT_ROOT"
      ;;
  esac
}

write_profile() {
  mkdir -p "$PROFILE_DIR" "$REPORT_DIR" "$ADC_STATE_ROOT"
  cat >"$PROFILE_DIR/demo_sensor_gateway.yaml" <<YAML
profile: demo_sensor_gateway
sampling:
  interval_ms: 50
always_on:
  collectors: [cpu, memory, network, kmsg]
budgets:
  max_daemon_cpu_percent: 10
  max_memory_mb: 256
  max_artifact_mb_per_run: 128
triggers:
  - name: demo_retry_warning
    type: kmsg_pattern
    severity_at_least: warning
    patterns: [warning, retry, storm]
  - name: demo_memory_pressure_observed
    type: threshold
    signal: memory.available_percent
    op: "<="
    value: 100
  - name: demo_network_delta_observed
    type: delta
    signal: network.total_delta_bytes
    op: ">="
    value: 0
YAML
}

run_shell() {
  local name="$1"
  shift
  local stdout="$REPORT_DIR/$name.stdout.log"
  local stderr="$REPORT_DIR/$name.stderr.log"
  log "+ $*"
  bash -lc "$*" >"$stdout" 2>"$stderr"
}

run_shell_to() {
  local name="$1"
  local output="$2"
  shift 2
  local stderr="$REPORT_DIR/$name.stderr.log"
  log "+ $* > $output"
  bash -lc "$*" >"$output" 2>"$stderr"
}

extract_daemon_run_id() {
  grep -o 'R-DAEMON-[^"]*' "$REPORT_DIR/daemon-retry.stdout.log" | tail -n 1 || true
}

append_excerpt() {
  local title="$1"
  local lang="$2"
  local path="$3"
  {
    printf '\n## %s\n\n' "$title"
    printf '```%s\n' "$lang"
    sed -n '1,120p' "$path"
    printf '```\n'
  } >>"$RESULT_ROOT/agent_context.md"
}

write_agent_context() {
  local daemon_run_id="$1"
  cat >"$RESULT_ROOT/agent_context.md" <<MD
# Agent Context: Sensor Gateway Bugs

This context is intentionally bounded. It gives an Agent evidence/window/series/compare output first and leaves raw artifacts as references.

## Scenario Runs

- baseline observed run: R-DEMO-BASELINE-after
- retry-storm observed run: R-DEMO-RETRY-after
- memory-leak observed run: R-DEMO-MEMORY-after
- daemon trigger run: $daemon_run_id

## raw_refs

- baseline app events: $ADC_STATE_ROOT/runs/R-DEMO-BASELINE-after/raw/app_events.jsonl
- retry app events: $ADC_STATE_ROOT/runs/R-DEMO-RETRY-after/raw/app_events.jsonl
- memory app events: $ADC_STATE_ROOT/runs/R-DEMO-MEMORY-after/raw/app_events.jsonl
- kmsg fixture: $KMSG_FIXTURE

MD
  append_excerpt "obs.get_evidence_index equivalent" "yaml" "$REPORT_DIR/retry.evidence.yaml"
  append_excerpt "obs.get_window equivalent" "yaml" "$REPORT_DIR/retry.window.yaml"
  append_excerpt "obs.get_signal_series equivalent" "json" "$REPORT_DIR/retry.series.json"
  append_excerpt "obs.compare_runs retry vs baseline" "json" "$REPORT_DIR/retry_vs_baseline.compare.json"
  append_excerpt "obs.compare_runs memory vs baseline" "json" "$REPORT_DIR/memory_vs_baseline.compare.json"
  append_excerpt "obs.get_agent_context" "markdown" "$REPORT_DIR/daemon.agent_context.md"
  append_excerpt "OpenMetrics summary adapter" "text" "$REPORT_DIR/daemon.agent_context.prom"
  append_excerpt "MCP bounded surface" "json" "$REPORT_DIR/mcp_tool_list.json"
}

run_demo() {
  validate_result_root
  rm -rf -- "$RESULT_ROOT"
  mkdir -p "$RESULT_ROOT" "$ADC_STATE_ROOT" "$PROFILE_DIR" "$REPORT_DIR"
  write_profile

  local baseline_ms retry_ms memory_ms attempts retained
  baseline_ms="$(duration_ms 250 1)"
  retry_ms="$(duration_ms 500 1)"
  memory_ms="$(duration_ms 500 1)"
  attempts="$(packet_attempts)"
  retained="$(retained_kb)"

  mkdir -p \
    "$ADC_STATE_ROOT/runs/R-DEMO-BASELINE-after/raw" \
    "$ADC_STATE_ROOT/runs/R-DEMO-RETRY-after/raw" \
    "$ADC_STATE_ROOT/runs/R-DEMO-MEMORY-after/raw"

  run_shell_to baseline-before "$REPORT_DIR/baseline.before.json" \
    "ADC_PROFILE_DIR='$PROFILE_DIR' ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN snapshot --run-id R-DEMO-BASELINE-before"
  run_shell baseline-workload \
    "$ADC_DEMO_BIN baseline --duration-ms '$baseline_ms' --events-jsonl '$ADC_STATE_ROOT/runs/R-DEMO-BASELINE-after/raw/app_events.jsonl'"
  run_shell_to baseline-after "$REPORT_DIR/baseline.after.json" \
    "ADC_PROFILE_DIR='$PROFILE_DIR' ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN snapshot --run-id R-DEMO-BASELINE-after"

  run_shell_to retry-before "$REPORT_DIR/retry.before.json" \
    "ADC_PROFILE_DIR='$PROFILE_DIR' ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN snapshot --run-id R-DEMO-RETRY-before"
  run_shell retry-workload \
    "$ADC_DEMO_BIN retry-storm --duration-ms '$retry_ms' --packet-attempts '$attempts' --events-jsonl '$ADC_STATE_ROOT/runs/R-DEMO-RETRY-after/raw/app_events.jsonl' --kmsg-fixture '$KMSG_FIXTURE'"
  run_shell_to retry-after "$REPORT_DIR/retry.after.json" \
    "ADC_PROFILE_DIR='$PROFILE_DIR' ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN snapshot --run-id R-DEMO-RETRY-after"

  run_shell_to memory-before "$REPORT_DIR/memory.before.json" \
    "ADC_PROFILE_DIR='$PROFILE_DIR' ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN snapshot --run-id R-DEMO-MEMORY-before"
  run_shell memory-workload \
    "$ADC_DEMO_BIN memory-leak --duration-ms '$memory_ms' --retained-kb '$retained' --events-jsonl '$ADC_STATE_ROOT/runs/R-DEMO-MEMORY-after/raw/app_events.jsonl'"
  run_shell_to memory-after "$REPORT_DIR/memory.after.json" \
    "ADC_PROFILE_DIR='$PROFILE_DIR' ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN snapshot --run-id R-DEMO-MEMORY-after"

  run_shell arm-retry \
    "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN arm --profile demo_sensor_gateway"
  run_shell daemon-retry \
    "ADC_KMSG_FIXTURE='$KMSG_FIXTURE' ADC_PROFILE_DIR='$PROFILE_DIR' ADC_HOME='$ADC_STATE_ROOT' $ADC_TARGETD_BIN --service-for-ms 500"

  local daemon_run_id
  daemon_run_id="$(extract_daemon_run_id)"
  [[ -n "$daemon_run_id" ]] || die "failed to find daemon trigger run id"

  run_shell_to retry-evidence "$REPORT_DIR/retry.evidence.yaml" \
    "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN evidence get --run-id '$daemon_run_id'"
  run_shell_to retry-window "$REPORT_DIR/retry.window.yaml" \
    "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN evidence window --run-id '$daemon_run_id' --window-id W001"
  run_shell_to retry-series "$REPORT_DIR/retry.series.json" \
    "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN evidence series --run-id '$daemon_run_id' --source cpu --limit 10"
  run_shell_to retry-compare "$REPORT_DIR/retry_vs_baseline.compare.json" \
    "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN compare --before R-DEMO-BASELINE-after --after R-DEMO-RETRY-after"
  run_shell_to memory-compare "$REPORT_DIR/memory_vs_baseline.compare.json" \
    "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN compare --before R-DEMO-BASELINE-after --after R-DEMO-MEMORY-after"
  run_shell_to mcp-tools "$REPORT_DIR/mcp_tool_list.json" \
    "ADC_HOME='$ADC_STATE_ROOT' $MCP_BIN --tool-list-json"
  run_shell_to daemon-agent-context "$REPORT_DIR/daemon.agent_context.md" \
    "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN agent-context --run-id '$daemon_run_id'"
  run_shell_to daemon-agent-context-metrics "$REPORT_DIR/daemon.agent_context.prom" \
    "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN agent-context --run-id '$daemon_run_id' --format openmetrics"

  write_agent_context "$daemon_run_id"
  cat >"$RESULT_ROOT/summary.json" <<JSON
{
  "result_root": "$RESULT_ROOT",
  "state_root": "$ADC_STATE_ROOT",
  "profile_dir": "$PROFILE_DIR",
  "agent_context": "$RESULT_ROOT/agent_context.md",
  "retry_trigger_run_id": "$daemon_run_id"
}
JSON
  log "agent context: $RESULT_ROOT/agent_context.md"
}

main() {
  parse_args "$@"
  set_paths
  print_plan
  if [[ "$MODE" == "dry-run" ]]; then
    return 0
  fi
  run_demo
}

main "$@"
