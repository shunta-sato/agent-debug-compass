#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STAMP="$(date +%Y%m%d-%H%M%S)"
RESULT_ROOT="${ADC_AGENT_QUALITY_RESULT_ROOT:-$ROOT_DIR/e2e-results/agent-quality-dogfood-$STAMP}"
STATE_ROOT="${ADC_AGENT_QUALITY_STATE_ROOT:-$RESULT_ROOT/state}"
REPORT_DIR="$RESULT_ROOT/reports"
FIXTURE_DIR="$RESULT_ROOT/fixtures"
FAKE_BIN_DIR="$RESULT_ROOT/fake-bin"
INVENTORY_PATH="${ADC_AGENT_QUALITY_INVENTORY:-$FIXTURE_DIR/partial-targets.yaml}"
THRESHOLD="${ADC_AGENT_QUALITY_THRESHOLD:-95}"
LOCAL_RUN_ID="${ADC_AGENT_QUALITY_LOCAL_RUN_ID:-R-AGENT-QUALITY-LOCAL}"
FLEET_RUN_ID="${ADC_AGENT_QUALITY_FLEET_RUN_ID:-F-AGENT-QUALITY-FLEET}"
SERVICE_NAME="${ADC_AGENT_QUALITY_SERVICE_NAME:-dogfood-agent.service}"
FLEET_SERVICE_NAME="${ADC_AGENT_QUALITY_FLEET_SERVICE_NAME:-ssh}"

if [[ -n "${ADC_AGENT_QUALITY_ADC_BIN:-}" ]]; then
  ADC_BIN="$ADC_AGENT_QUALITY_ADC_BIN"
elif [[ -x "$ROOT_DIR/bin/adc" ]]; then
  ADC_BIN="$ROOT_DIR/bin/adc"
elif [[ -x "$ROOT_DIR/dist/adc-targetd-0.1.0-aarch64-linux/bin/adc" ]]; then
  ADC_BIN="$ROOT_DIR/dist/adc-targetd-0.1.0-aarch64-linux/bin/adc"
elif [[ -x "$ROOT_DIR/target/release/adc" ]]; then
  ADC_BIN="$ROOT_DIR/target/release/adc"
else
  cargo build -q -p adc
  ADC_BIN="$ROOT_DIR/target/debug/adc"
fi

mkdir -p "$RESULT_ROOT" "$STATE_ROOT" "$REPORT_DIR" "$FIXTURE_DIR" "$FAKE_BIN_DIR"

if [[ -z "${ADC_AGENT_QUALITY_INVENTORY:-}" ]]; then
  install -m 0600 /dev/null "$FIXTURE_DIR/managed-mcp.token"
  printf 'agent-quality-managed-token\n' >"$FIXTURE_DIR/managed-mcp.token"
  cat >"$INVENTORY_PATH" <<YAML
targets:
  - id: local-self
    transport: local
  - id: example-unreachable-managed
    transport: managed_mcp
    host: 127.0.0.1
    port: 1
    auth_token_file: $FIXTURE_DIR/managed-mcp.token
YAML
fi

COMMAND_FAILURES=()

json_escape() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//$'\n'/ }"
  printf '%s' "$value"
}

write_command_status() {
  local name="$1"
  local status="$2"
  cat >"$REPORT_DIR/$name.status.json" <<JSON
{
  "name": "$(json_escape "$name")",
  "exit_code": $status
}
JSON
}

run_capture() {
  local name="$1"
  local output="$2"
  shift 2
  local status=0
  if "$@" >"$output" 2>"$REPORT_DIR/$name.stderr.log"; then
    status=0
  else
    status=$?
    COMMAND_FAILURES+=("$name:$status")
  fi
  write_command_status "$name" "$status"
  return 0
}

cat >"$FAKE_BIN_DIR/systemctl" <<'SCRIPT'
#!/usr/bin/env bash
if [[ "${1:-}" == "show" ]]; then
  cat <<'OUT'
Id=dogfood-agent.service
LoadState=loaded
ActiveState=active
SubState=running
MainPID=424242
FragmentPath=/usr/lib/systemd/system/dogfood-agent.service
OUT
  exit 0
fi
exit 1
SCRIPT
chmod +x "$FAKE_BIN_DIR/systemctl"

cat >"$FAKE_BIN_DIR/journalctl" <<'SCRIPT'
#!/usr/bin/env bash
cat <<'OUT'
2026-05-28T09:00:00+09:00 pi dogfood-agent[1]: startup complete
2026-05-28T09:00:01+09:00 pi dogfood-agent[2]: warn queue depth high
2026-05-28T09:00:02+09:00 pi dogfood-agent[3]: error timeout request_id=agent-quality-001
OUT
SCRIPT
chmod +x "$FAKE_BIN_DIR/journalctl"

cat >"$FIXTURE_DIR/app.log" <<'TXT'
info startup complete
warn queue depth high
error timeout request_id=agent-quality-001
TXT

cat >"$FIXTURE_DIR/domain_events.jsonl" <<'TXT'
{"event_type":"queue_backlog","queue_depth":42}
{"event_type":"request_timeout","elapsed_ms":1500}
TXT

cat >"$FIXTURE_DIR/config.env" <<'TXT'
retry_backoff_ms=0
token=agent-quality-secret
TXT

run_capture \
  local_observe \
  "$REPORT_DIR/local_observe.json" \
  env PATH="$FAKE_BIN_DIR:$PATH" ADC_HOME="$STATE_ROOT" "$ADC_BIN" observe \
    --run-id "$LOCAL_RUN_ID" \
    --duration-ms 180 \
    --interval-ms 40 \
    --log-file "$FIXTURE_DIR/app.log" \
    --domain-events-file "$FIXTURE_DIR/domain_events.jsonl" \
    --config-file "$FIXTURE_DIR/config.env" \
    --service-name "$SERVICE_NAME"

run_capture \
  route_packs \
  "$REPORT_DIR/route_packs.json" \
  env ADC_HOME="$STATE_ROOT" "$ADC_BIN" investigate route-packs

run_capture \
  local_start \
  "$REPORT_DIR/local_start.json" \
  env PATH="$FAKE_BIN_DIR:$PATH" ADC_HOME="$STATE_ROOT" "$ADC_BIN" investigate start \
    --run-id "$LOCAL_RUN_ID" \
    --service-name "$SERVICE_NAME" \
    --journal-lines 3

run_capture \
  local_symptom_latency \
  "$REPORT_DIR/local_symptom_latency.json" \
  env PATH="$FAKE_BIN_DIR:$PATH" ADC_HOME="$STATE_ROOT" "$ADC_BIN" investigate bug \
    --run-id "$LOCAL_RUN_ID" \
    --symptom "latency timeout" \
    --service-name "$SERVICE_NAME" \
    --journal-lines 3

run_capture \
  local_symptom_memory \
  "$REPORT_DIR/local_symptom_memory.json" \
  env PATH="$FAKE_BIN_DIR:$PATH" ADC_HOME="$STATE_ROOT" "$ADC_BIN" investigate bug \
    --run-id "$LOCAL_RUN_ID" \
    --symptom "memory pressure" \
    --service-name "$SERVICE_NAME" \
    --journal-lines 3

run_capture \
  local_symptom_network \
  "$REPORT_DIR/local_symptom_network.json" \
  env PATH="$FAKE_BIN_DIR:$PATH" ADC_HOME="$STATE_ROOT" "$ADC_BIN" investigate bug \
    --run-id "$LOCAL_RUN_ID" \
    --symptom "packet loss" \
    --service-name "$SERVICE_NAME" \
    --journal-lines 3

run_capture \
  local_symptom_thermal \
  "$REPORT_DIR/local_symptom_thermal.json" \
  env PATH="$FAKE_BIN_DIR:$PATH" ADC_HOME="$STATE_ROOT" "$ADC_BIN" investigate bug \
    --run-id "$LOCAL_RUN_ID" \
    --symptom "thermal throttling" \
    --service-name "$SERVICE_NAME" \
    --journal-lines 3

run_capture \
  local_symptom_config \
  "$REPORT_DIR/local_symptom_config.json" \
  env PATH="$FAKE_BIN_DIR:$PATH" ADC_HOME="$STATE_ROOT" "$ADC_BIN" investigate bug \
    --run-id "$LOCAL_RUN_ID" \
    --symptom "config drift" \
    --service-name "$SERVICE_NAME" \
    --journal-lines 3

run_capture \
  local_symptom_unknown \
  "$REPORT_DIR/local_symptom_unknown.json" \
  env PATH="$FAKE_BIN_DIR:$PATH" ADC_HOME="$STATE_ROOT" "$ADC_BIN" investigate bug \
    --run-id "$LOCAL_RUN_ID" \
    --symptom "unclear failure" \
    --service-name "$SERVICE_NAME" \
    --journal-lines 3

run_capture \
  local_continue_ir001 \
  "$REPORT_DIR/local_continue_ir001.json" \
  env PATH="$FAKE_BIN_DIR:$PATH" ADC_HOME="$STATE_ROOT" "$ADC_BIN" investigate continue \
    --run-id "$LOCAL_RUN_ID" \
    --service-name "$SERVICE_NAME" \
    --step-id IR001

run_capture \
  local_continue_ir002 \
  "$REPORT_DIR/local_continue_ir002.json" \
  env PATH="$FAKE_BIN_DIR:$PATH" ADC_HOME="$STATE_ROOT" "$ADC_BIN" investigate continue \
    --run-id "$LOCAL_RUN_ID" \
    --service-name "$SERVICE_NAME" \
    --step-id IR002 \
    --session-id "S-route-$LOCAL_RUN_ID-IR001"

run_capture \
  local_session \
  "$REPORT_DIR/local_session.json" \
  env ADC_HOME="$STATE_ROOT" "$ADC_BIN" investigate session \
    --run-id "$LOCAL_RUN_ID" \
    --session-id "S-route-$LOCAL_RUN_ID-IR001"

run_capture \
  local_cleanup_dry_run \
  "$REPORT_DIR/local_cleanup_dry_run.json" \
  env ADC_HOME="$STATE_ROOT" "$ADC_BIN" investigate cleanup-sessions \
    --run-id "$LOCAL_RUN_ID" \
    --max-sessions 0 \
    --dry-run

run_capture \
  local_cleanup_execute \
  "$REPORT_DIR/local_cleanup_execute.json" \
  env ADC_HOME="$STATE_ROOT" "$ADC_BIN" investigate cleanup-sessions \
    --run-id "$LOCAL_RUN_ID" \
    --max-age-days 0 \
    --execute

FLEET_ENABLED=0
if [[ -f "$INVENTORY_PATH" ]]; then
  FLEET_ENABLED=1
  run_capture \
    fleet_preflight \
    "$REPORT_DIR/fleet_preflight.json" \
    env ADC_HOME="$STATE_ROOT" "$ADC_BIN" fleet preflight \
      --inventory "$INVENTORY_PATH"

  run_capture \
    fleet_snapshot \
    "$REPORT_DIR/fleet_snapshot.json" \
    env ADC_HOME="$STATE_ROOT" "$ADC_BIN" fleet snapshot \
      --inventory "$INVENTORY_PATH" \
      --fleet-run-id "$FLEET_RUN_ID"

  run_capture \
    fleet_service \
    "$REPORT_DIR/fleet_service.json" \
    env ADC_HOME="$STATE_ROOT" "$ADC_BIN" fleet investigate service "$FLEET_SERVICE_NAME" \
      --inventory "$INVENTORY_PATH" \
      --fleet-run-id "$FLEET_RUN_ID" \
      --journal-lines 3

  run_capture \
    fleet_start \
    "$REPORT_DIR/fleet_start.json" \
    env ADC_HOME="$STATE_ROOT" "$ADC_BIN" investigate start \
      --fleet-run-id "$FLEET_RUN_ID" \
      --service-name "$FLEET_SERVICE_NAME" \
      --inventory "$INVENTORY_PATH" \
      --journal-lines 3

  run_capture \
    fleet_symptom_latency \
    "$REPORT_DIR/fleet_symptom_latency.json" \
    env ADC_HOME="$STATE_ROOT" "$ADC_BIN" investigate bug \
      --fleet-run-id "$FLEET_RUN_ID" \
      --symptom "latency timeout" \
      --service-name "$FLEET_SERVICE_NAME" \
      --inventory "$INVENTORY_PATH" \
      --journal-lines 3

  run_capture \
    fleet_continue_ir002 \
    "$REPORT_DIR/fleet_continue_ir002.json" \
    env ADC_HOME="$STATE_ROOT" "$ADC_BIN" investigate continue \
      --fleet-run-id "$FLEET_RUN_ID" \
      --service-name "$FLEET_SERVICE_NAME" \
      --step-id IR002

  run_capture \
    fleet_session \
    "$REPORT_DIR/fleet_session.json" \
    env ADC_HOME="$STATE_ROOT" "$ADC_BIN" investigate session \
      --fleet-run-id "$FLEET_RUN_ID" \
      --session-id "S-route-$FLEET_RUN_ID-IR002"
fi

python3 - "$RESULT_ROOT" "$REPORT_DIR" "$STATE_ROOT" "$THRESHOLD" "$FLEET_ENABLED" "$INVENTORY_PATH" "${COMMAND_FAILURES[*]:-}" <<'PY'
import json
import re
import sys
from datetime import date
from pathlib import Path

result_root = Path(sys.argv[1])
report_dir = Path(sys.argv[2])
state_root = Path(sys.argv[3])
threshold = int(sys.argv[4])
fleet_enabled = sys.argv[5] == "1"
inventory_path = sys.argv[6]
command_failures = [item for item in sys.argv[7].split() if item]

dimensions = []

def read_json(name):
    path = report_dir / name
    if not path.exists():
        return None
    try:
        return json.loads(path.read_text())
    except Exception:
        return None

def raw_text(paths):
    chunks = []
    for path in paths:
        if path.exists() and path.is_file():
            try:
                chunks.append(path.read_text(errors="replace"))
            except Exception:
                pass
    return "\n".join(chunks)

def add(name, points, passed, detail, artifacts=None):
    dimensions.append({
        "name": name,
        "points": points,
        "earned": points if passed else 0,
        "status": "passed" if passed else "failed",
        "detail": detail,
        "artifacts": [str(path) for path in (artifacts or [])],
    })

route_packs = read_json("route_packs.json") or {}
packs = route_packs.get("packs") or []
domains = {pack.get("domain") for pack in packs}
required_domains = {
    "service_health",
    "latency_timeouts",
    "memory_growth",
    "cpu_saturation",
    "network_degradation",
    "disk_io_pressure",
    "config_deploy_drift",
    "thermal_power_edge",
}
add(
    "route-pack registry breadth",
    12,
    route_packs.get("schema_version") == "obs.route_pack_registry.v1"
    and required_domains.issubset(domains)
    and all(pack.get("cause_neutral") is True for pack in packs),
    f"domains={sorted(domains)}",
    [report_dir / "route_packs.json"],
)

local_start = read_json("local_start.json") or {}
route = local_start.get("investigation_route") or local_start.get("route") or {}
branch_conditions = []
for step in route.get("steps") or []:
    branch_conditions.extend(step.get("branch_conditions") or [])
has_predicates = any("predicate" in condition for condition in branch_conditions)
add(
    "typed branch predicates",
    10,
    has_predicates,
    f"predicate_conditions={sum(1 for condition in branch_conditions if 'predicate' in condition)}",
    [report_dir / "local_start.json"],
)

symptom_expectations = {
    "local_symptom_latency.json": ("latency_timeout", "latency_timeouts"),
    "local_symptom_memory.json": ("memory_growth", "memory_growth"),
    "local_symptom_network.json": ("network_degradation", "network_degradation"),
    "local_symptom_thermal.json": ("thermal_power", "thermal_power_edge"),
    "local_symptom_config.json": ("config_drift", "config_deploy_drift"),
    "local_symptom_unknown.json": ("unknown", "service_health"),
}
symptom_results = {}
for file_name, (normalized, domain) in symptom_expectations.items():
    context = read_json(file_name) or {}
    selected_domains = {
        pack.get("domain")
        for pack in ((context.get("compiled_route") or {}).get("selected_packs") or [])
    }
    fact_ids = {fact.get("fact_id") for fact in context.get("facts") or []}
    symptom_results[file_name] = {
        "schema_ok": context.get("schema_version") == "obs.symptom_context.v1",
        "normalized": (context.get("symptom") or {}).get("normalized"),
        "expected_normalized": normalized,
        "selected_domains": sorted(str(item) for item in selected_domains if item),
        "expected_domain": domain,
        "fact_count": len(fact_ids),
        "missing_fact_count": len(context.get("missing_fact_ids") or []),
        "next_safe_probe_count": len(context.get("next_safe_probes") or []),
        "returned_bytes": (context.get("budget") or {}).get("returned_bytes"),
    }
symptom_ok = all(
    item["schema_ok"]
    and item["normalized"] == item["expected_normalized"]
    and item["expected_domain"] in item["selected_domains"]
    and item["fact_count"] > 0
    and item["next_safe_probe_count"] > 0
    and isinstance(item["returned_bytes"], int)
    and item["returned_bytes"] <= 65536
    for item in symptom_results.values()
)
add(
    "symptom-first context compiler",
    15,
    symptom_ok,
    f"scenarios={symptom_results}",
    [report_dir / name for name in symptom_expectations],
)

direct_shell_comparison = {
    "schema_version": "obs.direct_shell_comparison.v1",
    "scenario": "latency_timeout_local",
    "agent_entrypoint": "adc investigate bug --symptom 'latency timeout'",
    "agent_step_count": 1,
    "manual_shell_step_count": 9,
    "manual_shell_baseline": [
        "systemctl show service state",
        "journalctl bounded warning/error scan",
        "tail/grep application log",
        "cat /proc/stat twice and compute CPU delta",
        "cat /proc/meminfo",
        "cat /proc/net/dev",
        "read /sys/class/thermal zones",
        "redact and inspect config",
        "assemble refs, gaps, and next probe plan",
    ],
    "agent_output_schema": (read_json("local_symptom_latency.json") or {}).get("schema_version"),
    "agent_returned_bytes": ((read_json("local_symptom_latency.json") or {}).get("budget") or {}).get("returned_bytes"),
}
add(
    "direct-shell comparison",
    10,
    direct_shell_comparison["agent_output_schema"] == "obs.symptom_context.v1"
    and direct_shell_comparison["agent_step_count"] < direct_shell_comparison["manual_shell_step_count"]
    and isinstance(direct_shell_comparison["agent_returned_bytes"], int)
    and direct_shell_comparison["agent_returned_bytes"] <= 65536,
    (
        f"agent_steps={direct_shell_comparison['agent_step_count']} "
        f"manual_steps={direct_shell_comparison['manual_shell_step_count']} "
        f"bytes={direct_shell_comparison['agent_returned_bytes']}"
    ),
    [report_dir / "local_symptom_latency.json"],
)

local_continue = read_json("local_continue_ir001.json") or {}
opened_refs = local_continue.get("opened_refs") or []
facts = [fact for opened in opened_refs for fact in opened.get("facts") or []]
fact_ids = {fact.get("fact_id") for fact in facts}
branch_evaluations = local_continue.get("branch_evaluations") or []
has_missing_fact_ids = all(isinstance(evaluation.get("missing_fact_ids"), list) for evaluation in branch_evaluations)
add(
    "typed fact extraction",
    14,
    {"service.availability", "port.availability"}.issubset(fact_ids) and has_missing_fact_ids,
    f"fact_ids={sorted(str(fact_id) for fact_id in fact_ids if fact_id)} branch_evaluations={len(branch_evaluations)}",
    [report_dir / "local_continue_ir001.json"],
)

matched_facts = []
for evaluation in branch_evaluations:
    matched_facts.extend(evaluation.get("matched_facts") or [])
substring_confused = any("port.availability" in fact and "available" in fact and "unavailable" in fact for fact in matched_facts)
service_available_matched = any('service.availability="available"' in fact for fact in matched_facts)
add(
    "availability ambiguity regression",
    10,
    service_available_matched and not substring_confused,
    f"matched_facts={matched_facts[:4]}",
    [report_dir / "local_continue_ir001.json"],
)

local_continue_ir002 = read_json("local_continue_ir002.json") or {}
ir002_facts = [
    fact
    for opened in local_continue_ir002.get("opened_refs") or []
    for fact in opened.get("facts") or []
]
ir002_fact_ids = {fact.get("fact_id") for fact in ir002_facts}
add(
    "log/domain route continuation",
    8,
    "signal.signal_line_count" in ir002_fact_ids and "signal.has_signal_words" in ir002_fact_ids,
    f"fact_ids={sorted(str(fact_id) for fact_id in ir002_fact_ids if fact_id)}",
    [report_dir / "local_continue_ir002.json"],
)

local_session = read_json("local_session.json") or {}
compact_summary = local_session.get("compact_summary") or []
completed_steps = local_session.get("completed_steps") or []
add(
    "fresh-agent session resume",
    10,
    bool(compact_summary) and len(completed_steps) >= 2,
    f"compact_summary={compact_summary[:2]} completed_steps={len(completed_steps)}",
    [report_dir / "local_session.json"],
)

cleanup_dry = read_json("local_cleanup_dry_run.json") or {}
cleanup_exec = read_json("local_cleanup_execute.json") or {}
dry_candidates = cleanup_dry.get("candidates") or []
exec_candidates = cleanup_exec.get("candidates") or []
add(
    "session cleanup dry-run then execute",
    8,
    cleanup_dry.get("dry_run") is True
    and all(candidate.get("deleted") is False for candidate in dry_candidates)
    and cleanup_exec.get("dry_run") is False
    and any(candidate.get("deleted") is True for candidate in exec_candidates),
    f"dry_candidates={len(dry_candidates)} exec_deleted={cleanup_exec.get('deleted_count')}",
    [report_dir / "local_cleanup_dry_run.json", report_dir / "local_cleanup_execute.json"],
)

budget_files = [
    report_dir / "local_continue_ir001.json",
    report_dir / "local_continue_ir002.json",
    report_dir / "local_session.json",
]
budget = {
    "local_continue_ir001": (read_json("local_continue_ir001.json") or {}).get("budget", {}).get("returned_bytes"),
    "local_continue_ir002": (read_json("local_continue_ir002.json") or {}).get("budget", {}).get("returned_bytes"),
    "local_session": (read_json("local_session.json") or {}).get("budget", {}).get("returned_bytes"),
}
add(
    "bounded local agent packets",
    8,
    all(isinstance(value, int) and value <= 12288 for value in budget.values()),
    f"returned_bytes={budget}",
    budget_files,
)

scan_paths = list(report_dir.glob("*.json")) + list((state_root / "runs").glob("**/*.json")) + list((state_root / "fleet_runs").glob("**/*.json"))
scan = raw_text(scan_paths)
unsafe_regex = re.compile(r"\b(root[_ -]?cause|likely[_ -]?cause|blame|remediation[_ -]?engine)\b", re.IGNORECASE)
add(
    "safety and privacy scan",
    8,
    "agent-quality-secret" not in scan and not unsafe_regex.search(scan),
    "secret and cause-inference terms were absent from generated JSON artifacts",
    scan_paths[:20],
)

if fleet_enabled:
    fleet_preflight = read_json("fleet_preflight.json") or {}
    fleet_snapshot = read_json("fleet_snapshot.json") or {}
    fleet_service = read_json("fleet_service.json") or {}
    fleet_start = read_json("fleet_start.json") or {}
    fleet_continue = read_json("fleet_continue_ir002.json") or {}
    fleet_session = read_json("fleet_session.json") or {}
    semantic_diff_path = state_root / "fleet_runs" / "F-AGENT-QUALITY-FLEET" / "fleet_semantic_diff.json"
    semantic_diff = json.loads(semantic_diff_path.read_text()) if semantic_diff_path.exists() else {}
    field_names = {field.get("field") for field in semantic_diff.get("field_diffs") or []}
    fleet_ok = (
        fleet_preflight.get("ready_count", 0) >= 1
        and fleet_snapshot.get("captured_count", 0) >= 1
        and fleet_service.get("captured_count", 0) >= 1
        and fleet_service.get("failed_count", 0) >= 1
        and (fleet_start.get("investigation_route") or fleet_start.get("route") or {}).get("schema_version") == "obs.investigation_route.v1"
        and isinstance(fleet_continue.get("branch_evaluations"), list)
        and bool(fleet_session.get("compact_summary"))
        and "data_quality.class" in field_names
    )
    fleet_symptom = read_json("fleet_symptom_latency.json") or {}
    fleet_symptom_ok = (
        fleet_symptom.get("schema_version") == "obs.symptom_context.v1"
        and (fleet_symptom.get("target_summary") or {}).get("failed_count", 0) >= 1
        and (fleet_symptom.get("compiled_route") or {}).get("schema_version") == "obs.compiled_route.v1"
    )
    add(
        "managed MCP and degraded fleet",
        12,
        fleet_ok and fleet_symptom_ok,
        (
            f"inventory={inventory_path} ready={fleet_preflight.get('ready_count')} "
            f"captured={fleet_service.get('captured_count')} failed={fleet_service.get('failed_count')} "
            f"semantic_fields={sorted(str(field) for field in field_names if field)[:8]} "
            f"symptom_context={fleet_symptom.get('schema_version')}"
        ),
        [
            report_dir / "fleet_preflight.json",
            report_dir / "fleet_service.json",
            report_dir / "fleet_symptom_latency.json",
            semantic_diff_path,
        ],
    )
else:
    add(
        "managed MCP and degraded fleet",
        12,
        False,
        f"inventory not found: {inventory_path}",
        [],
    )

add(
    "command execution health",
    0,
    not command_failures,
    f"failures={command_failures}",
    [report_dir / f"{name.split(':')[0]}.status.json" for name in command_failures],
)

score = sum(item["earned"] for item in dimensions)
max_score = sum(item["points"] for item in dimensions)
passed = score >= threshold and not command_failures and all(item["status"] == "passed" for item in dimensions if item["points"] > 0)
scorecard = {
    "schema_version": "obs.agent_quality_scorecard.v2",
    "score": score,
    "max_score": max_score,
    "threshold": threshold,
    "passed": passed,
    "result_root": str(result_root),
    "state_root": str(state_root),
    "fleet_enabled": fleet_enabled,
    "inventory_path": inventory_path,
    "command_failures": command_failures,
    "symptom_results": symptom_results,
    "direct_shell_comparison": direct_shell_comparison,
    "dimensions": dimensions,
}
(result_root / "quality_scorecard.json").write_text(json.dumps(scorecard, indent=2, sort_keys=True) + "\n")

lines = [
    "# Strict Dogfood Report - Agent Investigation Quality v2",
    "",
    f"Date: {date.today().isoformat()}",
    f"Result root: `{result_root}`",
    f"Score: {score}/{max_score}",
    f"Threshold: {threshold}",
    f"Verdict: {'PASS' if passed else 'FAIL'}",
    "",
    "## Dimensions",
    "",
]
for item in dimensions:
    lines.append(f"- {item['status'].upper()} {item['name']}: {item['earned']}/{item['points']} - {item['detail']}")
lines.extend([
    "",
    "## Hard Critique",
    "",
    "This dogfood is intentionally strict. It passes only if symptom-first context compilation, typed route decisions, bounded packets, stale-session cleanup, direct-shell comparison, and managed/degraded fleet evidence remain useful without cause inference.",
])
(result_root / "STRICT_DOGFOOD_REPORT.md").write_text("\n".join(lines) + "\n")

if not passed:
    sys.exit(1)
PY

cat >"$RESULT_ROOT/summary.json" <<JSON
{
  "schema_version": "obs.agent_quality_dogfood.v2",
  "result_root": "$(json_escape "$RESULT_ROOT")",
  "state_root": "$(json_escape "$STATE_ROOT")",
  "report": "$(json_escape "$RESULT_ROOT/STRICT_DOGFOOD_REPORT.md")",
  "scorecard": "$(json_escape "$RESULT_ROOT/quality_scorecard.json")",
  "adc_bin": "$(json_escape "$ADC_BIN")",
  "fleet_inventory": "$(json_escape "$INVENTORY_PATH")"
}
JSON

printf 'agent quality dogfood result: %s\n' "$RESULT_ROOT"
