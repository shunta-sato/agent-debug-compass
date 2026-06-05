#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RESULT_ROOT="${ADC_E2E_RESULT_ROOT:-$ROOT_DIR/e2e-results/local}"
ADC_STATE_ROOT="${ADC_HOME:-$RESULT_ROOT/state}"
ADC_BIN="${ADC_BIN:-cargo run -q -p adc --}"
ADC_TARGETD_BIN="${ADC_TARGETD_BIN:-cargo run -q -p adc-targetd --}"
ADC_WORKLOAD_BIN="${ADC_WORKLOAD_BIN:-cargo run -q -p adc-workload --}"
MCP_BIN="${ADC_MCP_BIN:-cargo run -q -p adc-mcp --}"
PROFILE_DIR="$RESULT_ROOT/profiles"
KMSG_FIXTURE="$RESULT_ROOT/kmsg_fixture.log"
APP_LOG_FIXTURE="$RESULT_ROOT/app.log"
DOMAIN_EVENTS_FIXTURE="$RESULT_ROOT/domain_events.jsonl"
CONFIG_FIXTURE="$RESULT_ROOT/config.env"
OTLP_FIXTURE="$RESULT_ROOT/otlp_metrics.json"
JOURNALD_FIXTURE="$RESULT_ROOT/journald.jsonl"
PERFETTO_FIXTURE="$RESULT_ROOT/perfetto_trace.json"
FAKE_BIN_DIR="$RESULT_ROOT/fake-bin"
FAKE_SSH_REMOTE_ROOT="$RESULT_ROOT/fake-ssh-remotes"
FLEET_INVENTORY="$RESULT_ROOT/fleet_targets.yaml"
DISCOVERY_NEIGHBORS="$RESULT_ROOT/discovery_neighbors.txt"

mkdir -p "$RESULT_ROOT" "$ADC_STATE_ROOT" "$PROFILE_DIR" "$FAKE_BIN_DIR" "$FAKE_SSH_REMOTE_ROOT"

write_report() {
  local test_id="$1"
  local status="$2"
  local reason="$3"
  local dir="$RESULT_ROOT/$test_id"
  mkdir -p "$dir"
  cat >"$dir/assertion_report.json" <<JSON
{
  "test_id": "$test_id",
  "status": "$status",
  "reason": "$reason"
}
JSON
}

run_shell() {
  local test_id="$1"
  shift
  local dir="$RESULT_ROOT/$test_id"
  mkdir -p "$dir"
  bash -lc "$*" >"$dir/stdout.log" 2>"$dir/stderr.log"
}

daemon_run_id_from_stdout() {
  local test_id="$1"
  grep 'R-DAEMON-' "$RESULT_ROOT/$test_id/stdout.log" \
    | tail -n 1 \
    | sed 's/.*"\(R-DAEMON-[^"]*\)".*/\1/'
}

assert_v2_run_layout() {
  local run_id="$1"
  local actual
  local expected
  actual="$(mktemp)"
  expected="$(mktemp)"
  find "$ADC_STATE_ROOT/runs/$run_id" -mindepth 1 -maxdepth 1 -printf '%f\n' \
    | grep -v '^recorder_freeze_decision\.json$' \
    | sort >"$actual"
  cat >"$expected" <<'TXT'
evidence_index.yaml
manifest.json
overhead_report.json
raw
timeline.jsonl
windows
TXT
  diff -u "$expected" "$actual"
  rm -f "$actual" "$expected"
}

assert_daemon_trigger_run() {
  local test_id="$1"
  local trigger="$2"
  local source="$3"
  local run_id
  run_id="$(daemon_run_id_from_stdout "$test_id")"
  test -n "$run_id"
  test -f "$ADC_STATE_ROOT/runs/$run_id/manifest.json"
  assert_v2_run_layout "$run_id"
  grep -q "$trigger" "$ADC_STATE_ROOT/runs/$run_id/evidence_index.yaml"
  grep -q "\"source\":\"$source\"" "$ADC_STATE_ROOT/runs/$run_id/timeline.jsonl"
}

RUN_ID="R-E2E-LOCAL"
COMPARE_BEFORE_ID="R-E2E-COMPARE-BEFORE"
COMPARE_AFTER_ID="R-E2E-COMPARE-AFTER"
CAPTURE_ID="R-E2E-CAPTURE"
rm -rf \
  "$ADC_STATE_ROOT/runs/$RUN_ID" \
  "$ADC_STATE_ROOT/runs/$CAPTURE_ID" \
  "$ADC_STATE_ROOT/runs/$COMPARE_BEFORE_ID" \
  "$ADC_STATE_ROOT/runs/$COMPARE_AFTER_ID"

cat >"$FAKE_BIN_DIR/ssh" <<SCRIPT
#!/usr/bin/env bash
set -euo pipefail
while [[ "\${1:-}" == "-o" ]]; do
  shift 2
done
if [[ "\${1:-}" == "-p" ]]; then
  shift 2
fi
dest="\${1:?missing ssh destination}"
shift
host="\${dest#*@}"
if [[ "\$host" == "denied.local" ]]; then
  printf 'Permission denied (publickey).\\n' >&2
  exit 255
fi
if [[ "\${1:-}" == "adc-mcp" ]]; then
  shift
  mkdir -p "$FAKE_SSH_REMOTE_ROOT/\$host"
  cd "$ROOT_DIR"
  ADC_HOME="$FAKE_SSH_REMOTE_ROOT/\$host" cargo run -q -p adc-mcp -- "\$@"
  exit 0
fi
printf 'unexpected fake ssh command: %s\\n' "\$*" >&2
exit 127
SCRIPT
chmod +x "$FAKE_BIN_DIR/ssh"

cat >"$FAKE_BIN_DIR/systemctl" <<'SCRIPT'
#!/usr/bin/env bash
if [[ "${1:-}" == "show" ]]; then
  cat <<'OUT'
Id=e2e-service.service
LoadState=loaded
ActiveState=active
SubState=running
MainPID=999999
FragmentPath=/usr/lib/systemd/system/e2e-service.service
OUT
  exit 0
fi
exit 1
SCRIPT
chmod +x "$FAKE_BIN_DIR/systemctl"

cat >"$FAKE_BIN_DIR/journalctl" <<'SCRIPT'
#!/usr/bin/env bash
cat <<'OUT'
2026-05-27T00:01:00+09:00 host e2e-service[1]: startup complete
2026-05-27T00:02:00+09:00 host e2e-service[2]: warning queue depth high
2026-05-27T00:03:00+09:00 host e2e-service[3]: error timeout request_id=e2e-001
OUT
SCRIPT
chmod +x "$FAKE_BIN_DIR/journalctl"

cat >"$FLEET_INVENTORY" <<'YAML'
targets:
  - id: e2e-local
    transport: local
  - id: e2e-ssh
    transport: mcp_stdio_over_ssh
    host: fake-pi5.local
  - id: e2e-denied
    transport: mcp_stdio_over_ssh
    host: denied.local
YAML

cat >"$DISCOVERY_NEIGHBORS" <<'TXT'
198.51.100.10 dev eth0 lladdr aa:bb:cc:dd:ee:01 REACHABLE
198.51.100.11 dev eth0 FAILED
203.0.113.12 dev eth0 lladdr aa:bb:cc:dd:ee:02 REACHABLE
TXT

cat >"$APP_LOG_FIXTURE" <<'TXT'
info startup complete
warn queue depth high
error timeout request_id=e2e-001
TXT

cat >"$DOMAIN_EVENTS_FIXTURE" <<'TXT'
{"event_type":"queue_backlog","queue_depth":99}
{"event_type":"sensor_frame_gap","frame_id":"42","gap_ms":120}
TXT

cat >"$CONFIG_FIXTURE" <<'TXT'
retry_backoff_ms=0
token=e2e-secret-token
TXT

cat >"$OTLP_FIXTURE" <<'TXT'
{"resourceMetrics":[{"scopeMetrics":[{"metrics":[{"name":"queue.depth"},{"name":"request.errors"}]}]}]}
TXT

cat >"$JOURNALD_FIXTURE" <<'TXT'
{"MESSAGE":"timeout","PRIORITY":"4"}
{"MESSAGE":"worker recovered","PRIORITY":"6"}
TXT

cat >"$PERFETTO_FIXTURE" <<'TXT'
{"traceEvents":[{"name":"frame_gap","ph":"i"},{"name":"request","ph":"X"}]}
TXT

cat >"$PROFILE_DIR/pi5_basic.yaml" <<YAML
profile: pi5_basic
sampling:
  interval_ms: 1000
always_on:
  collectors: [cpu, memory, network]
budgets:
  max_daemon_cpu_percent: 3
  max_memory_mb: 128
  max_artifact_mb_per_run: 512
YAML

cat >"$PROFILE_DIR/e2e_cpu.yaml" <<YAML
profile: e2e_cpu
sampling:
  interval_ms: 50
always_on:
  collectors: [cpu]
budgets:
  max_daemon_cpu_percent: 10
  max_memory_mb: 128
  max_artifact_mb_per_run: 64
triggers:
  - name: cpu_sustained_high
    type: threshold_duration
    signal: cpu.total_percent
    op: ">="
    value: 0
    duration_sec: 0
YAML

cat >"$PROFILE_DIR/e2e_memory.yaml" <<YAML
profile: e2e_memory
sampling:
  interval_ms: 50
always_on:
  collectors: [memory]
budgets:
  max_daemon_cpu_percent: 10
  max_memory_mb: 128
  max_artifact_mb_per_run: 64
triggers:
  - name: memory_available_observed
    type: threshold
    signal: memory.available_percent
    op: "<="
    value: 100
YAML

cat >"$PROFILE_DIR/e2e_kmsg.yaml" <<YAML
profile: e2e_kmsg
sampling:
  interval_ms: 50
always_on:
  collectors: [kmsg]
budgets:
  max_daemon_cpu_percent: 10
  max_memory_mb: 128
  max_artifact_mb_per_run: 64
triggers:
  - name: kmsg_warning_pattern
    type: kmsg_pattern
    severity_at_least: warning
    patterns: [warning, timeout]
YAML

cat >"$PROFILE_DIR/e2e_network.yaml" <<YAML
profile: e2e_network
sampling:
  interval_ms: 50
always_on:
  collectors: [network]
budgets:
  max_daemon_cpu_percent: 10
  max_memory_mb: 128
  max_artifact_mb_per_run: 64
triggers:
  - name: network_delta_observed
    type: delta
    signal: network.total_delta_bytes
    op: ">="
    value: 0
YAML

run_shell E2E-001 "ADC_HOME='$ADC_STATE_ROOT' $ADC_TARGETD_BIN --service-once"
write_report E2E-001 passed "service-once initialized daemon state; systemd enable/start is documented target smoke"

run_shell E2E-002 "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN snapshot --run-id '$RUN_ID'"
test -f "$ADC_STATE_ROOT/runs/$RUN_ID/manifest.json"
assert_v2_run_layout "$RUN_ID"
test -f "$ADC_STATE_ROOT/runs/$RUN_ID/evidence_index.yaml"
test -f "$ADC_STATE_ROOT/runs/$RUN_ID/timeline.jsonl"
test -f "$ADC_STATE_ROOT/runs/$RUN_ID/raw/system.json"
write_report E2E-002 passed "snapshot bundle contains manifest, evidence index, timeline, and raw refs"

run_shell E2E-002A "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN capture --run-id '$CAPTURE_ID' --duration-ms 160 --interval-ms 40"
test -f "$ADC_STATE_ROOT/runs/$CAPTURE_ID/manifest.json"
assert_v2_run_layout "$CAPTURE_ID"
test -f "$ADC_STATE_ROOT/runs/$CAPTURE_ID/evidence_index.yaml"
test -f "$ADC_STATE_ROOT/runs/$CAPTURE_ID/timeline.jsonl"
test -f "$ADC_STATE_ROOT/runs/$CAPTURE_ID/windows/W001.yaml"
test -f "$ADC_STATE_ROOT/runs/$CAPTURE_ID/raw/samples.jsonl"
test -f "$ADC_STATE_ROOT/runs/$CAPTURE_ID/raw/cpu.jsonl"
test "$(wc -l < "$ADC_STATE_ROOT/runs/$CAPTURE_ID/raw/samples.jsonl")" -ge 2
test "$(wc -l < "$ADC_STATE_ROOT/runs/$CAPTURE_ID/raw/cpu.jsonl")" -ge 2
grep -q "capture_mode: capture" "$ADC_STATE_ROOT/runs/$CAPTURE_ID/evidence_index.yaml"
grep -q "trigger_reason: manual_capture" "$ADC_STATE_ROOT/runs/$CAPTURE_ID/windows/W001.yaml"
ADC_HOME="$ADC_STATE_ROOT" bash -lc "$ADC_BIN evidence series --run-id '$CAPTURE_ID' --source cpu --limit 5" \
  >"$RESULT_ROOT/E2E-002A/search.stdout.log" 2>"$RESULT_ROOT/E2E-002A/search.stderr.log"
grep -q '"source": "cpu"' "$RESULT_ROOT/E2E-002A/search.stdout.log"
write_report E2E-002A passed "bounded capture produced multi-sample timeline, window, evidence, raw refs, and searchable events"

run_shell E2E-002B "PATH='$FAKE_BIN_DIR':\$PATH ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN observe --run-id R-E2E-OBSERVE --duration-ms 160 --interval-ms 40 --log-file '$APP_LOG_FIXTURE' --domain-events-file '$DOMAIN_EVENTS_FIXTURE' --config-file '$CONFIG_FIXTURE' --service-name e2e-service --otlp-file '$OTLP_FIXTURE' --journald-jsonl-file '$JOURNALD_FIXTURE' --perfetto-file '$PERFETTO_FIXTURE' && ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN agent-context --run-id R-E2E-OBSERVE --format json > '$RESULT_ROOT/E2E-002B/agent_context.json' && ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN investigate route-packs > '$RESULT_ROOT/E2E-002B/route_packs.json' && PATH='$FAKE_BIN_DIR':\$PATH ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN investigate bug --run-id R-E2E-OBSERVE --symptom 'latency timeout' --service-name e2e-service --journal-lines 3 > '$RESULT_ROOT/E2E-002B/symptom_context.json' && PATH='$FAKE_BIN_DIR':\$PATH ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN investigate start --run-id R-E2E-OBSERVE --service-name e2e-service --journal-lines 3 > '$RESULT_ROOT/E2E-002B/investigation_start.json' && PATH='$FAKE_BIN_DIR':\$PATH ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN investigate continue --run-id R-E2E-OBSERVE --service-name e2e-service --step-id IR001 > '$RESULT_ROOT/E2E-002B/investigation_continue.json' && PATH='$FAKE_BIN_DIR':\$PATH ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN investigate continue --run-id R-E2E-OBSERVE --service-name e2e-service --step-id IR002 --session-id S-route-R-E2E-OBSERVE-IR001 > '$RESULT_ROOT/E2E-002B/investigation_continue_ir002.json' && ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN investigate session --run-id R-E2E-OBSERVE --session-id S-route-R-E2E-OBSERVE-IR001 > '$RESULT_ROOT/E2E-002B/investigation_session.json' && ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN investigate cleanup-sessions --run-id R-E2E-OBSERVE --max-sessions 0 --dry-run > '$RESULT_ROOT/E2E-002B/investigation_session_cleanup.json' && ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN agent-context --run-id R-E2E-OBSERVE --format openmetrics > '$RESULT_ROOT/E2E-002B/agent_context.prom' && ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN agent-context --run-id R-E2E-OBSERVE --format otlp-json > '$RESULT_ROOT/E2E-002B/agent_context.otlp.json' && ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN agent-context --run-id R-E2E-OBSERVE --format journald-jsonl > '$RESULT_ROOT/E2E-002B/agent_context.journald.jsonl' && ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN agent-context --run-id R-E2E-OBSERVE --format perfetto-json > '$RESULT_ROOT/E2E-002B/agent_context.perfetto.json'"
test -f "$ADC_STATE_ROOT/runs/R-E2E-OBSERVE/agent_context.md"
test -f "$ADC_STATE_ROOT/runs/R-E2E-OBSERVE/raw/app.log"
test -f "$ADC_STATE_ROOT/runs/R-E2E-OBSERVE/raw/domain_events.jsonl"
test -f "$ADC_STATE_ROOT/runs/R-E2E-OBSERVE/raw/config_redacted.txt"
test -f "$ADC_STATE_ROOT/runs/R-E2E-OBSERVE/raw/process_snapshot.json"
test -f "$ADC_STATE_ROOT/runs/R-E2E-OBSERVE/raw/fd_thread_snapshot.json"
test -f "$ADC_STATE_ROOT/runs/R-E2E-OBSERVE/raw/kernel_probe_snapshot.json"
test -f "$ADC_STATE_ROOT/runs/R-E2E-OBSERVE/raw/otlp_metrics.json"
test -f "$ADC_STATE_ROOT/runs/R-E2E-OBSERVE/raw/journald.jsonl"
test -f "$ADC_STATE_ROOT/runs/R-E2E-OBSERVE/raw/perfetto_trace.json"
grep -q '"schema_version": "obs.agent_context.v1"' "$RESULT_ROOT/E2E-002B/agent_context.json"
grep -q '"log_error_slice"' "$RESULT_ROOT/E2E-002B/agent_context.json"
grep -q '"domain_event_count"' "$RESULT_ROOT/E2E-002B/agent_context.json"
grep -q '"process_snapshot"' "$RESULT_ROOT/E2E-002B/agent_context.json"
grep -q '"fd_thread_snapshot"' "$RESULT_ROOT/E2E-002B/agent_context.json"
grep -q '"kernel_optional_probe_snapshot"' "$RESULT_ROOT/E2E-002B/agent_context.json"
grep -q '"otlp_metric_count"' "$RESULT_ROOT/E2E-002B/agent_context.json"
grep -q '"journald_entry_count"' "$RESULT_ROOT/E2E-002B/agent_context.json"
grep -q '"perfetto_event_count"' "$RESULT_ROOT/E2E-002B/agent_context.json"
grep -q '"service_state"' "$RESULT_ROOT/E2E-002B/agent_context.json"
grep -q '"availability": "available"' "$RESULT_ROOT/E2E-002B/agent_context.json"
grep -q '"schema_version": "obs.agent_playbook.v1"' "$RESULT_ROOT/E2E-002B/agent_context.json"
grep -q '"schema_version": "obs.investigation_route.v1"' "$RESULT_ROOT/E2E-002B/agent_context.json"
grep -q '"schema_version": "obs.investigation_start.v1"' "$RESULT_ROOT/E2E-002B/investigation_start.json"
grep -q '"schema_version": "obs.symptom_context.v1"' "$RESULT_ROOT/E2E-002B/symptom_context.json"
grep -q '"normalized": "latency_timeout"' "$RESULT_ROOT/E2E-002B/symptom_context.json"
grep -q '"schema_version": "obs.compiled_route.v1"' "$RESULT_ROOT/E2E-002B/symptom_context.json"
grep -q '"domain": "latency_timeouts"' "$RESULT_ROOT/E2E-002B/symptom_context.json"
grep -q '"fact_id": "resource.cpu_busy_percent"' "$RESULT_ROOT/E2E-002B/symptom_context.json"
grep -q '"next_safe_probes"' "$RESULT_ROOT/E2E-002B/symptom_context.json"
grep -q '"schema_version": "obs.investigation_continue.v1"' "$RESULT_ROOT/E2E-002B/investigation_continue.json"
grep -q '"schema_version": "obs.route_pack_registry.v1"' "$RESULT_ROOT/E2E-002B/route_packs.json"
grep -q '"domain": "network_degradation"' "$RESULT_ROOT/E2E-002B/route_packs.json"
grep -q '"domain": "thermal_power_edge"' "$RESULT_ROOT/E2E-002B/route_packs.json"
grep -q '"cause_neutral": true' "$RESULT_ROOT/E2E-002B/route_packs.json"
grep -q '"predicate"' "$RESULT_ROOT/E2E-002B/investigation_start.json"
grep -q '"opened_service_state"' "$RESULT_ROOT/E2E-002B/investigation_continue.json"
grep -q '"facts"' "$RESULT_ROOT/E2E-002B/investigation_continue.json"
grep -q '"fact_id": "service.availability"' "$RESULT_ROOT/E2E-002B/investigation_continue.json"
grep -q '"fact_id": "port.availability"' "$RESULT_ROOT/E2E-002B/investigation_continue.json"
grep -q '"branch_evaluations"' "$RESULT_ROOT/E2E-002B/investigation_continue.json"
grep -q '"missing_fact_ids"' "$RESULT_ROOT/E2E-002B/investigation_continue.json"
grep -q '"next_actions"' "$RESULT_ROOT/E2E-002B/investigation_continue.json"
grep -q '"schema_version": "obs.investigation_continue.v1"' "$RESULT_ROOT/E2E-002B/investigation_continue_ir002.json"
grep -q '"label": "log"' "$RESULT_ROOT/E2E-002B/investigation_continue_ir002.json"
grep -q '"fact_id": "signal.signal_line_count"' "$RESULT_ROOT/E2E-002B/investigation_continue_ir002.json"
grep -q '"schema_version": "obs.investigation_session_state.v1"' "$RESULT_ROOT/E2E-002B/investigation_session.json"
grep -q '"completed_steps"' "$RESULT_ROOT/E2E-002B/investigation_session.json"
grep -q '"IR002"' "$RESULT_ROOT/E2E-002B/investigation_session.json"
grep -q '"compact_summary"' "$RESULT_ROOT/E2E-002B/investigation_session.json"
grep -q '"schema_version": "obs.investigation_session_cleanup.v1"' "$RESULT_ROOT/E2E-002B/investigation_session_cleanup.json"
grep -q '"dry_run": true' "$RESULT_ROOT/E2E-002B/investigation_session_cleanup.json"
grep -q '"deleted": false' "$RESULT_ROOT/E2E-002B/investigation_session_cleanup.json"
grep -q '"service_name": "e2e-service"' "$RESULT_ROOT/E2E-002B/investigation_start.json"
test -f "$ADC_STATE_ROOT/runs/R-E2E-OBSERVE/investigation_route.json"
test -f "$ADC_STATE_ROOT/runs/R-E2E-OBSERVE/symptom_context.json"
test -f "$ADC_STATE_ROOT/runs/R-E2E-OBSERVE/compiled_route.json"
test -f "$ADC_STATE_ROOT/runs/R-E2E-OBSERVE/fact_gap_report.json"
test -d "$ADC_STATE_ROOT/runs/R-E2E-OBSERVE/investigation_sessions"
test -f "$ADC_STATE_ROOT/runs/R-E2E-OBSERVE/investigation_sessions/S-route-R-E2E-OBSERVE-IR001.state.json"
grep -q '"cause_neutral": true' "$RESULT_ROOT/E2E-002B/agent_context.json"
grep -q 'adc_agent_context_info' "$RESULT_ROOT/E2E-002B/agent_context.prom"
grep -q 'resourceMetrics' "$RESULT_ROOT/E2E-002B/agent_context.otlp.json"
grep -q 'ADC_RUN_ID' "$RESULT_ROOT/E2E-002B/agent_context.journald.jsonl"
grep -q 'traceEvents' "$RESULT_ROOT/E2E-002B/agent_context.perfetto.json"
if grep -q 'e2e-secret-token' "$ADC_STATE_ROOT/runs/R-E2E-OBSERVE/raw/config_redacted.txt" "$RESULT_ROOT/E2E-002B/agent_context.json" "$RESULT_ROOT/E2E-002B/investigation_start.json" "$RESULT_ROOT/E2E-002B/investigation_continue.json"; then
  echo "secret leaked into agent context artifacts" >&2
  exit 1
fi
ADC_HOME="$ADC_STATE_ROOT" bash -lc "$ADC_BIN investigate cleanup-sessions --run-id R-E2E-OBSERVE --max-age-days 0 --execute" \
  >"$RESULT_ROOT/E2E-002B/investigation_session_cleanup_execute.json" 2>"$RESULT_ROOT/E2E-002B/investigation_session_cleanup_execute.stderr.log"
grep -q '"dry_run": false' "$RESULT_ROOT/E2E-002B/investigation_session_cleanup_execute.json"
grep -q '"deleted": true' "$RESULT_ROOT/E2E-002B/investigation_session_cleanup_execute.json"
test ! -f "$ADC_STATE_ROOT/runs/R-E2E-OBSERVE/investigation_sessions/S-route-R-E2E-OBSERVE-IR001.state.json"
write_report E2E-002B passed "observe generated Agent context plus symptom-first compiled context with typed route facts, route packs, executable session cleanup, continuation session, playbook, logs, domain events, redacted config, runtime snapshots, advanced probes, interop imports, and interop exports"

run_shell E2E-003 "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN capabilities"
write_report E2E-003 passed "capability map command completed"

run_shell E2E-004 "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN arm --profile e2e_cpu >/dev/null && ($ADC_WORKLOAD_BIN cpu-spike --duration-ms 1000 >/dev/null 2>&1 &) && ADC_PROFILE_DIR='$PROFILE_DIR' ADC_HOME='$ADC_STATE_ROOT' $ADC_TARGETD_BIN --service-for-ms 1500"
assert_daemon_trigger_run E2E-004 cpu_sustained_high cpu
write_report E2E-004 passed "daemon-integrated CPU workload produced a trigger window"

run_shell E2E-005 "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN arm --profile e2e_memory >/dev/null && ($ADC_WORKLOAD_BIN memory-pressure --mb 8 --duration-ms 1000 >/dev/null 2>&1 &) && ADC_PROFILE_DIR='$PROFILE_DIR' ADC_HOME='$ADC_STATE_ROOT' $ADC_TARGETD_BIN --service-for-ms 1500"
assert_daemon_trigger_run E2E-005 memory_available_observed memory
write_report E2E-005 passed "daemon-integrated memory workload produced a memory observation window"

run_shell E2E-006 "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN arm --profile e2e_kmsg >/dev/null && $ADC_WORKLOAD_BIN kmsg-mock --message 'warning: synthetic timeout' --output '$KMSG_FIXTURE' >/dev/null && ADC_KMSG_FIXTURE='$KMSG_FIXTURE' ADC_PROFILE_DIR='$PROFILE_DIR' ADC_HOME='$ADC_STATE_ROOT' $ADC_TARGETD_BIN --service-for-ms 1500"
assert_daemon_trigger_run E2E-006 kmsg_warning_pattern kmsg
write_report E2E-006 passed "daemon-integrated kmsg mock workload produced a warning trigger window"

run_shell E2E-007 "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN arm --profile e2e_network >/dev/null && ($ADC_WORKLOAD_BIN network-loopback --bytes 65536 >/dev/null 2>&1 &) && ADC_PROFILE_DIR='$PROFILE_DIR' ADC_HOME='$ADC_STATE_ROOT' $ADC_TARGETD_BIN --service-for-ms 1500"
assert_daemon_trigger_run E2E-007 network_delta_observed network
write_report E2E-007 passed "daemon-integrated loopback workload produced a network observation window"

run_shell E2E-008 "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN evidence search --run-id '$RUN_ID' --limit 5"
write_report E2E-008 passed "bounded timeline search completed"

run_shell E2E-009 "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN evidence get --run-id '$RUN_ID'"
write_report E2E-009 passed "evidence index retrieval completed with raw refs"

run_shell E2E-010 "ADC_HOME='$ADC_STATE_ROOT' $MCP_BIN --tool-list-json"
write_report E2E-010 passed "MCP tool list completed without arbitrary shell tool"

run_shell E2E-011 "ADC_PROFILE_DIR='$PROFILE_DIR' ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN snapshot --run-id '$COMPARE_BEFORE_ID' >/dev/null && $ADC_WORKLOAD_BIN cpu-spike --duration-ms 100 >/dev/null && ADC_PROFILE_DIR='$PROFILE_DIR' ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN snapshot --run-id '$COMPARE_AFTER_ID' >/dev/null"
test -f "$ADC_STATE_ROOT/runs/$COMPARE_BEFORE_ID/manifest.json"
test -f "$ADC_STATE_ROOT/runs/$COMPARE_AFTER_ID/manifest.json"
write_report E2E-011 passed "explicit before/after snapshots captured a workload without ADC command execution"

test -f "$ADC_STATE_ROOT/runs/$RUN_ID/overhead_report.json"
mkdir -p "$RESULT_ROOT/E2E-012"
cp "$ADC_STATE_ROOT/runs/$RUN_ID/overhead_report.json" "$RESULT_ROOT/E2E-012/overhead_report.json"
write_report E2E-012 passed "overhead report generated"

if "$ROOT_DIR/kernel/adc_sensor_probe/tests/selftest.sh" --build-only; then
  mkdir -p "$RESULT_ROOT/E2E-014"
  cp "$ROOT_DIR/kernel/adc_sensor_probe/test-results/assertion_report.json" "$RESULT_ROOT/E2E-014/assertion_report.json"
else
  write_report E2E-014 skipped "KO build-only harness failed; inspect kernel/adc_sensor_probe/test-results"
fi

write_report E2E-013 skipped "ftrace/perf capture is target-privileged smoke; capability map is generated"
write_report E2E-015 skipped "safe kprobe smoke requires explicit symbol and root approval"

run_shell E2E-016 "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN compare --before '$COMPARE_BEFORE_ID' --after '$COMPARE_AFTER_ID'"
write_report E2E-016 passed "before/after comparison produced bounded metric deltas and raw refs"

run_shell E2E-017 "ADC_HOME='$ADC_STATE_ROOT' $ADC_TARGETD_BIN --service-once"
grep -q "$COMPARE_AFTER_ID" "$RESULT_ROOT/E2E-017/stdout.log"
write_report E2E-017 passed "service restart recovered existing run manifests"

run_shell E2E-018 "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN evidence get --run-id '$RUN_ID' && ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN evidence window --run-id '$RUN_ID' --window-id W001 && ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN evidence series --run-id '$RUN_ID' --source cpu --limit 5 && ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN compare --before '$COMPARE_BEFORE_ID' --after '$COMPARE_AFTER_ID'"
write_report E2E-018 passed "CLI workflow simulation retrieved evidence, window, series, and comparison without raw dump"

run_shell E2E-019 "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN fleet discover --cidr 198.51.100.0/24 --neighbors-file '$DISCOVERY_NEIGHBORS'"
grep -q 'target-198-51-100-10' "$RESULT_ROOT/E2E-019/stdout.log"
grep -q 'neighbor 198.51.100.11 is not currently reachable' "$RESULT_ROOT/E2E-019/stdout.log"
write_report E2E-019 passed "same-network discovery returned bounded candidate targets and data_quality"

run_shell E2E-020 "PATH='$FAKE_BIN_DIR':\$PATH ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN fleet snapshot --inventory '$FLEET_INVENTORY' --fleet-run-id F-E2E-SNAPSHOT"
test -f "$ADC_STATE_ROOT/fleet_runs/F-E2E-SNAPSHOT/fleet_evidence.yaml"
test -f "$ADC_STATE_ROOT/fleet_runs/F-E2E-SNAPSHOT/targets/e2e-ssh/evidence_index.yaml"
grep -q 'target_id: e2e-ssh' "$ADC_STATE_ROOT/fleet_runs/F-E2E-SNAPSHOT/targets/e2e-ssh/evidence_index.yaml"
grep -q 'permission_denied' "$ADC_STATE_ROOT/fleet_runs/F-E2E-SNAPSHOT/fleet_evidence.yaml"
write_report E2E-020 passed "fake SSH fleet snapshot ingested bounded evidence and recorded denied target data_quality"

run_shell E2E-021 "PATH='$FAKE_BIN_DIR':\$PATH ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN fleet capture --inventory '$FLEET_INVENTORY' --fleet-run-id F-E2E-CAPTURE --duration-ms 120 --interval-ms 40"
test -f "$ADC_STATE_ROOT/fleet_runs/F-E2E-CAPTURE/fleet_evidence.yaml"
test -f "$ADC_STATE_ROOT/fleet_runs/F-E2E-CAPTURE/targets/e2e-ssh/evidence_index.yaml"
grep -q 'capture_mode: capture' "$ADC_STATE_ROOT/fleet_runs/F-E2E-CAPTURE/targets/e2e-ssh/evidence_index.yaml"
grep -q 'target_id: e2e-ssh' "$ADC_STATE_ROOT/fleet_runs/F-E2E-CAPTURE/targets/e2e-ssh/evidence_index.yaml"
grep -q 'permission_denied' "$ADC_STATE_ROOT/fleet_runs/F-E2E-CAPTURE/fleet_evidence.yaml"
write_report E2E-021 passed "fake target MCP-over-SSH fleet capture ingested bounded evidence and preserved partial-success data_quality"

run_shell E2E-022 "PATH='$FAKE_BIN_DIR':\$PATH ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN fleet preflight --inventory '$FLEET_INVENTORY' && PATH='$FAKE_BIN_DIR':\$PATH ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN fleet observe --inventory '$FLEET_INVENTORY' --fleet-run-id F-E2E-OBSERVE --duration-ms 120 --interval-ms 40 && ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN agent-context --fleet-run-id F-E2E-OBSERVE --format json > '$RESULT_ROOT/E2E-022/fleet_agent_context.json'"
grep -q '"schema_version": "obs.agent_context.fleet.v1"' "$RESULT_ROOT/E2E-022/fleet_agent_context.json"
grep -q '"captured_count": 2' "$RESULT_ROOT/E2E-022/fleet_agent_context.json"
grep -q '"failed_count": 1' "$RESULT_ROOT/E2E-022/fleet_agent_context.json"
grep -q 'permission_denied' "$RESULT_ROOT/E2E-022/fleet_agent_context.json"
write_report E2E-022 passed "fleet observe and fleet Agent context preserved partial-success remediation evidence"

run_shell E2E-022A "PATH='$FAKE_BIN_DIR':\$PATH ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN fleet investigate service e2e-service --inventory '$FLEET_INVENTORY' --fleet-run-id F-E2E-OBSERVE --journal-lines 3 > '$RESULT_ROOT/E2E-022A/fleet_service.json' && PATH='$FAKE_BIN_DIR':\$PATH ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN investigate start --fleet-run-id F-E2E-OBSERVE --service-name e2e-service --inventory '$FLEET_INVENTORY' --journal-lines 3 > '$RESULT_ROOT/E2E-022A/investigation_start.json'"
grep -q '"schema_version": "obs.fleet_service_investigation.v1"' "$RESULT_ROOT/E2E-022A/fleet_service.json"
grep -q '"captured_count": 1' "$RESULT_ROOT/E2E-022A/fleet_service.json"
grep -q '"failed_count": 2' "$RESULT_ROOT/E2E-022A/fleet_service.json"
grep -q '"availability": "unavailable"' "$RESULT_ROOT/E2E-022A/fleet_service.json"
grep -q 'e2e-ssh' "$RESULT_ROOT/E2E-022A/fleet_service.json"
grep -q 'e2e-denied' "$RESULT_ROOT/E2E-022A/fleet_service.json"
test -f "$ADC_STATE_ROOT/fleet_runs/F-E2E-OBSERVE/service_investigation.json"
test -f "$ADC_STATE_ROOT/fleet_runs/F-E2E-OBSERVE/targets/e2e-local/service_investigation.json"
grep -q '"schema_version": "obs.investigation_start.v1"' "$RESULT_ROOT/E2E-022A/investigation_start.json"
grep -q '"schema_version": "obs.investigation_route.v1"' "$RESULT_ROOT/E2E-022A/investigation_start.json"
grep -q '"fleet_semantic_diff"' "$RESULT_ROOT/E2E-022A/investigation_start.json"
grep -q 'Compare service investigation packs' "$RESULT_ROOT/E2E-022A/investigation_start.json"
test -f "$ADC_STATE_ROOT/fleet_runs/F-E2E-OBSERVE/fleet_semantic_diff.json"
grep -q '"schema_version": "obs.fleet_semantic_diff.v1"' "$ADC_STATE_ROOT/fleet_runs/F-E2E-OBSERVE/fleet_semantic_diff.json"
grep -q '"service.sub_state"' "$ADC_STATE_ROOT/fleet_runs/F-E2E-OBSERVE/fleet_semantic_diff.json"
grep -q '"journal.severity_buckets"' "$ADC_STATE_ROOT/fleet_runs/F-E2E-OBSERVE/fleet_semantic_diff.json"
grep -q '"data_quality.class"' "$ADC_STATE_ROOT/fleet_runs/F-E2E-OBSERVE/fleet_semantic_diff.json"
grep -q '"quality_class_by_target"' "$ADC_STATE_ROOT/fleet_runs/F-E2E-OBSERVE/fleet_semantic_diff.json"
write_report E2E-022A passed "fleet service investigation returned per-target packs, semantic diff, investigation route, and partial-success data_quality"

run_shell E2E-023 "$ROOT_DIR/scripts/install/tests/install-target-mcp-binaries-test.sh"
write_report E2E-023 passed "rootless target MCP bootstrap script passed fake transport tests"

run_shell E2E-024 "$ROOT_DIR/scripts/e2e/tests/run-target-mcp-fleet-smoke-test.sh"
write_report E2E-024 passed "target MCP fleet smoke runner passed skip and fake binary tests"

run_shell E2E-025 "ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN fleet init && ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN fleet enroll --target-id e2e-managed-local --transport local --profile pi5_basic --tag e2e && ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN fleet targets > '$RESULT_ROOT/E2E-025/targets.json' && ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN fleet snapshot --selector tag=e2e --fleet-run-id F-E2E-MANAGED-SNAPSHOT"
grep -q '"target_id": "e2e-managed-local"' "$RESULT_ROOT/E2E-025/targets.json"
test -f "$ADC_STATE_ROOT/runs/F-E2E-MANAGED-SNAPSHOT-e2e-managed-local/evidence_index.yaml"
write_report E2E-025 passed "managed fleet registry selector snapshot worked without a manual inventory file"

run_shell E2E-026 "set -euo pipefail; token='$RESULT_ROOT/E2E-026/managed.token'; echo e2e-managed-token > \"\$token\"; port=39245; PATH='$FAKE_BIN_DIR':\$PATH ADC_HOME='$ADC_STATE_ROOT' $MCP_BIN --target-mode --managed-listen 127.0.0.1:\$port --managed-token-file \"\$token\" & server_pid=\$!; trap 'kill \$server_pid 2>/dev/null || true; wait \$server_pid 2>/dev/null || true' EXIT; for _ in 1 2 3 4 5 6 7 8 9 10; do if (: >/dev/tcp/127.0.0.1/\$port) 2>/dev/null; then break; fi; sleep 0.2; done; PATH='$FAKE_BIN_DIR':\$PATH ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN fleet enroll --target-id e2e-managed-mcp --transport managed_mcp --host 127.0.0.1 --port \$port --auth-token-file \"\$token\" --tag e2e-managed-mcp; PATH='$FAKE_BIN_DIR':\$PATH ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN fleet preflight --selector tag=e2e-managed-mcp > '$RESULT_ROOT/E2E-026/preflight.json'; PATH='$FAKE_BIN_DIR':\$PATH ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN fleet snapshot --selector tag=e2e-managed-mcp --fleet-run-id F-E2E-MANAGED-MCP > '$RESULT_ROOT/E2E-026/snapshot.json'; PATH='$FAKE_BIN_DIR':\$PATH ADC_HOME='$ADC_STATE_ROOT' $ADC_BIN fleet investigate service e2e-service --selector tag=e2e-managed-mcp --fleet-run-id F-E2E-MANAGED-SERVICE > '$RESULT_ROOT/E2E-026/service.json'"
grep -q '"ready_count": 1' "$RESULT_ROOT/E2E-026/preflight.json"
grep -q '"captured_count": 1' "$RESULT_ROOT/E2E-026/snapshot.json"
grep -q '"transport": "managed_mcp"' "$RESULT_ROOT/E2E-026/snapshot.json"
grep -q '"captured_count": 1' "$RESULT_ROOT/E2E-026/service.json"
grep -q '"schema_version": "obs.fleet_service_investigation.v1"' "$RESULT_ROOT/E2E-026/service.json"
write_report E2E-026 passed "authenticated managed_mcp fleet snapshot and service investigation worked without SSH"

run_shell E2E-027 "cargo build -q -p adc-mcp -p adc && '$ROOT_DIR/scripts/e2e/tests/run-managed-mcp-mtls-smoke.sh' '$RESULT_ROOT/E2E-027' '$ADC_STATE_ROOT' '$ROOT_DIR/target/debug/adc-mcp' '$ROOT_DIR/target/debug/adc'"
write_report E2E-027 passed "managed_mcp mutual TLS fleet snapshot worked without SSH"

run_shell E2E-028 "'$ROOT_DIR/scripts/e2e/tests/run-managed-mcp-enrollment-kit-test.sh' '$RESULT_ROOT/E2E-028' '$ADC_STATE_ROOT' '$ADC_BIN'"
write_report E2E-028 passed "managed_mcp enrollment kit generated target credentials and enrolled the controller registry"

run_shell E2E-029 "$ROOT_DIR/scripts/install/tests/provision-managed-mcp-target-test.sh"
write_report E2E-029 passed "managed_mcp remote provisioner passed dry-run and fake SSH/SCP rootless service tests"

write_report PERF-001 skipped "30-minute idle overhead measurement is target-duration smoke; overhead report is generated in E2E-012"
write_report PERF-002 skipped "Level 0 collector overhead measurement requires target workload isolation"
write_report PERF-003 skipped "high-fidelity ftrace/perf overhead requires privileged target capture"
write_report PERF-004 skipped "event storm resilience requires target workload generator"
write_report SEC-001 passed "MCP tool list contains no arbitrary shell tool"
write_report SEC-002 passed "privileged helper allowlist is covered by integration tests"
write_report SEC-003 passed "MCP initial responses expose bounded evidence/window/search surfaces"
write_report SEC-004 skipped "KO runtime unload requires explicit root target smoke"

if [[ "${ADC_E2E_IMPORT_TARGET_SMOKE:-0}" == "1" ]]; then
  "$ROOT_DIR/scripts/e2e/merge-target-smoke.sh" --result-root "$RESULT_ROOT"
fi

cat >"$RESULT_ROOT/summary.json" <<JSON
{
  "result_root": "$RESULT_ROOT",
  "state_root": "$ADC_STATE_ROOT",
  "run_id": "$RUN_ID",
  "capture_id": "$CAPTURE_ID",
  "observe_id": "R-E2E-OBSERVE",
  "compare_before_id": "$COMPARE_BEFORE_ID",
  "compare_after_id": "$COMPARE_AFTER_ID",
  "fleet_snapshot_id": "F-E2E-SNAPSHOT",
  "fleet_capture_id": "F-E2E-CAPTURE",
  "fleet_observe_id": "F-E2E-OBSERVE"
}
JSON
