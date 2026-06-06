#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"

"${REPO_ROOT}/scripts/contract/validate-contracts.py" \
  --schema-dir "${REPO_ROOT}/schemas" \
  --fixture-dir "${REPO_ROOT}/tests/golden"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

assert_fails_with() {
  local expected="$1"
  shift
  local stderr="${TMP_DIR}/stderr.txt"
  if "$@" >"${TMP_DIR}/stdout.txt" 2>"${stderr}"; then
    echo "expected command to fail: $*" >&2
    exit 1
  fi
  if ! grep -Fq "${expected}" "${stderr}"; then
    echo "expected stderr to contain: ${expected}" >&2
    echo "actual stderr:" >&2
    cat "${stderr}" >&2
    exit 1
  fi
}

mkdir -p "${TMP_DIR}/invalid-unknown-field"
cat >"${TMP_DIR}/invalid-unknown-field/obs.data_quality.v1.min.json" <<'JSON'
{
  "dropped": false,
  "drop_count": 0,
  "throttled": false,
  "missing": [],
  "truncated": false,
  "clock_confidence": "medium",
  "notes": [],
  "root_cause_candidate": true
}
JSON
assert_fails_with "obs.data_quality.v1.min.json.root_cause_candidate" \
  "${REPO_ROOT}/scripts/contract/validate-contracts.py" \
    --schema-dir "${REPO_ROOT}/schemas" \
    --fixture-dir "${TMP_DIR}/invalid-unknown-field"

mkdir -p "${TMP_DIR}/invalid-nested-enum"
cat >"${TMP_DIR}/invalid-nested-enum/obs.hypothesis_set.v1.min.json" <<'JSON'
{
  "schema_version": "obs.hypothesis_set.v1",
  "scope": "run",
  "run_id": "R-INVALID",
  "hypotheses": [
    {
      "hypothesis_id": "H001",
      "statement": "Latency timeouts may correlate with CPU scheduling pressure.",
      "status": "open",
      "confidence": "low",
      "supports": [
        {
          "fact_id": "resource.cpu_busy_percent",
          "raw_ref": "artifact://raw/cpu.jsonl",
          "strength": "very_strong"
        }
      ],
      "contradicts": [],
      "missing_evidence": [],
      "next_discriminating_probes": ["probe.scheduler_snapshot"],
      "claim_boundary": "hypothesis_only",
      "data_quality": {
        "dropped": false,
        "drop_count": 0,
        "throttled": false,
        "missing": [],
        "truncated": false,
        "clock_confidence": "medium",
        "notes": []
      }
    }
  ],
  "data_quality": {
    "dropped": false,
    "drop_count": 0,
    "throttled": false,
    "missing": [],
    "truncated": false,
    "clock_confidence": "medium",
    "notes": []
  }
}
JSON
assert_fails_with "hypotheses[0].supports[0].strength" \
  "${REPO_ROOT}/scripts/contract/validate-contracts.py" \
    --schema-dir "${REPO_ROOT}/schemas" \
    --fixture-dir "${TMP_DIR}/invalid-nested-enum"

mkdir -p "${TMP_DIR}/invalid-ref"
cp "${REPO_ROOT}/schemas/obs.data_quality.v1.schema.json" "${TMP_DIR}/invalid-ref/"
cat >"${TMP_DIR}/invalid-ref/obs.invalid_ref.v1.schema.json" <<'JSON'
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "obs.invalid_ref.v1",
  "type": "object",
  "required": ["data_quality"],
  "properties": {
    "data_quality": {"$ref": "https://example.invalid/obs.data_quality.v1.schema.json"}
  }
}
JSON
cat >"${TMP_DIR}/invalid-ref/obs.invalid_ref.v1.min.json" <<'JSON'
{
  "data_quality": {
    "dropped": false,
    "drop_count": 0,
    "throttled": false,
    "missing": [],
    "truncated": false,
    "clock_confidence": "medium",
    "notes": []
  }
}
JSON
assert_fails_with "remote refs are not allowed" \
  "${REPO_ROOT}/scripts/contract/validate-contracts.py" \
    --schema-dir "${TMP_DIR}/invalid-ref" \
    --fixture-dir "${TMP_DIR}/invalid-ref"

mkdir -p "${TMP_DIR}/invalid-data-quality"
cat >"${TMP_DIR}/invalid-data-quality/obs.data_quality.v1.min.json" <<'JSON'
{
  "dropped": false,
  "drop_count": 2,
  "throttled": false,
  "missing": [],
  "truncated": false,
  "clock_confidence": "medium",
  "notes": []
}
JSON
assert_fails_with "drop_count > 0 requires dropped=true" \
  "${REPO_ROOT}/scripts/contract/validate-contracts.py" \
    --schema-dir "${REPO_ROOT}/schemas" \
    --fixture-dir "${TMP_DIR}/invalid-data-quality"

mkdir -p "${TMP_DIR}/invalid-safety-policy"
cat >"${TMP_DIR}/invalid-safety-policy/obs.safety_policy.v1.min.json" <<'JSON'
{
  "schema_version": "obs.safety_policy.v1",
  "policy_id": "bad-policy",
  "default_decision": "allow",
  "rules": [
    {
      "operation": "arbitrary_shell",
      "decision": "allow",
      "constraints": {}
    }
  ],
  "data_quality": {
    "dropped": false,
    "drop_count": 0,
    "throttled": false,
    "missing": [],
    "truncated": false,
    "clock_confidence": "medium",
    "notes": []
  }
}
JSON
assert_fails_with "default_decision must be deny" \
  "${REPO_ROOT}/scripts/contract/validate-contracts.py" \
    --schema-dir "${REPO_ROOT}/schemas" \
    --fixture-dir "${TMP_DIR}/invalid-safety-policy"

mkdir -p "${TMP_DIR}/invalid-probe-result"
cat >"${TMP_DIR}/invalid-probe-result/obs.probe_result.v1.min.json" <<'JSON'
{
  "schema_version": "obs.probe_result.v1",
  "probe_id": "probe.scheduler_snapshot",
  "probe_plan_id": "PP001",
  "result_kind": "not_executed_missing_capability",
  "executor": "adc",
  "executed": true,
  "safety_decision": "deny",
  "capability_status": "unavailable",
  "status": "failed_missing_capability",
  "produced_refs": [],
  "produced_facts": [],
  "hypothesis_updates": [],
  "data_quality": {
    "dropped": false,
    "drop_count": 0,
    "throttled": false,
    "missing": ["scheduler latency unavailable"],
    "truncated": false,
    "clock_confidence": "medium",
    "notes": []
  }
}
JSON
assert_fails_with "not_executed_missing_capability requires executed=false" \
  "${REPO_ROOT}/scripts/contract/validate-contracts.py" \
    --schema-dir "${REPO_ROOT}/schemas" \
    --fixture-dir "${TMP_DIR}/invalid-probe-result"

mkdir -p "${TMP_DIR}/invalid-hypothesis"
cat >"${TMP_DIR}/invalid-hypothesis/obs.hypothesis_set.v1.min.json" <<'JSON'
{
  "schema_version": "obs.hypothesis_set.v1",
  "scope": "run",
  "run_id": "R-INVALID",
  "hypotheses": [
    {
      "hypothesis_id": "H001",
      "statement": "The root cause is CPU saturation.",
      "status": "open",
      "confidence": "low",
      "supports": [],
      "contradicts": [],
      "missing_evidence": [],
      "next_discriminating_probes": ["probe.scheduler_snapshot"],
      "claim_boundary": "hypothesis_only",
      "data_quality": {
        "dropped": false,
        "drop_count": 0,
        "throttled": false,
        "missing": [],
        "truncated": false,
        "clock_confidence": "medium",
        "notes": []
      }
    }
  ],
  "data_quality": {
    "dropped": false,
    "drop_count": 0,
    "throttled": false,
    "missing": [],
    "truncated": false,
    "clock_confidence": "medium",
    "notes": []
  }
}
JSON
assert_fails_with "hypothesis statement must not promote root-cause claims" \
  "${REPO_ROOT}/scripts/contract/validate-contracts.py" \
    --schema-dir "${REPO_ROOT}/schemas" \
    --fixture-dir "${TMP_DIR}/invalid-hypothesis"

mkdir -p "${TMP_DIR}/invalid-ref-resolution-trust"
cp "${REPO_ROOT}/tests/golden/obs.ref_resolution.v1.min.json" \
  "${TMP_DIR}/invalid-ref-resolution-trust/obs.ref_resolution.v1.min.json"
python3 - "${TMP_DIR}/invalid-ref-resolution-trust/obs.ref_resolution.v1.min.json" <<'PY'
import json
import sys
path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    value = json.load(fh)
value["text"] = "The root cause is CPU saturation."
value["artifact_trust"]["trust_level"] = "trusted_system"
value["artifact_trust"]["agent_instruction_policy"] = "may_contain_instructions"
with open(path, "w", encoding="utf-8") as fh:
    json.dump(value, fh, indent=2)
    fh.write("\n")
PY
assert_fails_with "root-cause-like target text must stay data-only" \
  "${REPO_ROOT}/scripts/contract/validate-contracts.py" \
    --schema-dir "${REPO_ROOT}/schemas" \
    --fixture-dir "${TMP_DIR}/invalid-ref-resolution-trust"

mkdir -p "${TMP_DIR}/invalid-trace"
cp "${REPO_ROOT}/tests/golden/adc.investigation_trace.v1.min.json" \
  "${TMP_DIR}/invalid-trace/adc.investigation_trace.v1.min.json"
python3 - "${TMP_DIR}/invalid-trace/adc.investigation_trace.v1.min.json" <<'PY'
import json
import sys
path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    value = json.load(fh)
value["probe_result"]["probe_id"] = "probe.not_in_plan"
with open(path, "w", encoding="utf-8") as fh:
    json.dump(value, fh, indent=2)
    fh.write("\n")
PY
assert_fails_with "probe_result.probe_id must exist in probe_plan.candidate_probes" \
  "${REPO_ROOT}/scripts/contract/validate-contracts.py" \
    --schema-dir "${REPO_ROOT}/schemas" \
    --fixture-dir "${TMP_DIR}/invalid-trace"

mkdir -p "${TMP_DIR}/invalid-loss-report"
cp "${REPO_ROOT}/tests/golden/obs.loss_report.v1.min.json" \
  "${TMP_DIR}/invalid-loss-report/obs.loss_report.v1.min.json"
python3 - "${TMP_DIR}/invalid-loss-report/obs.loss_report.v1.min.json" <<'PY'
import json
import sys
path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    value = json.load(fh)
value["collector_loss"][0]["expected_samples"] = 10
value["collector_loss"][0]["recorded_samples"] = 12
with open(path, "w", encoding="utf-8") as fh:
    json.dump(value, fh, indent=2)
    fh.write("\n")
PY
assert_fails_with "expected_samples must be >= recorded_samples when known" \
  "${REPO_ROOT}/scripts/contract/validate-contracts.py" \
    --schema-dir "${REPO_ROOT}/schemas" \
    --fixture-dir "${TMP_DIR}/invalid-loss-report"

mkdir -p "${TMP_DIR}/invalid-observation-coverage"
cp "${REPO_ROOT}/tests/golden/obs.recorder_observation_coverage.v1.min.json" \
  "${TMP_DIR}/invalid-observation-coverage/obs.recorder_observation_coverage.v1.min.json"
python3 - "${TMP_DIR}/invalid-observation-coverage/obs.recorder_observation_coverage.v1.min.json" <<'PY'
import json
import sys
path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    value = json.load(fh)
value["signals"][0]["retained_samples_before_freeze"] = 10
value["signals"][0]["exported_samples"] = 2
value["signals"][0]["truncated_samples_due_to_freeze_budget"] = 0
with open(path, "w", encoding="utf-8") as fh:
    json.dump(value, fh, indent=2)
    fh.write("\n")
PY
assert_fails_with "must match retained_samples_before_freeze - exported_samples" \
  "${REPO_ROOT}/scripts/contract/validate-contracts.py" \
    --schema-dir "${REPO_ROOT}/schemas" \
    --fixture-dir "${TMP_DIR}/invalid-observation-coverage"

mkdir -p "${TMP_DIR}/invalid-recorder-transition"
cp "${REPO_ROOT}/tests/golden/obs.recorder_status.v1.min.json" \
  "${TMP_DIR}/invalid-recorder-transition/obs.recorder_status.v1.min.json"
python3 - "${TMP_DIR}/invalid-recorder-transition/obs.recorder_status.v1.min.json" <<'PY'
import json
import sys
path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    value = json.load(fh)
value["previous_state"] = "disabled"
value["recorder_state"] = "freezing"
with open(path, "w", encoding="utf-8") as fh:
    json.dump(value, fh, indent=2)
    fh.write("\n")
PY
assert_fails_with "recorder transition disabled -> freezing is forbidden" \
  "${REPO_ROOT}/scripts/contract/validate-contracts.py" \
    --schema-dir "${REPO_ROOT}/schemas" \
    --fixture-dir "${TMP_DIR}/invalid-recorder-transition"

mkdir -p "${TMP_DIR}/invalid-incident-transition"
cp "${REPO_ROOT}/tests/golden/obs.recorder_incident.v1.min.json" \
  "${TMP_DIR}/invalid-incident-transition/obs.recorder_incident.v1.min.json"
python3 - "${TMP_DIR}/invalid-incident-transition/obs.recorder_incident.v1.min.json" <<'PY'
import json
import sys
path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    value = json.load(fh)
value["previous_state"] = "marker_received"
value["incident_state"] = "exported"
with open(path, "w", encoding="utf-8") as fh:
    json.dump(value, fh, indent=2)
    fh.write("\n")
PY
assert_fails_with "incident transition marker_received -> exported is forbidden" \
  "${REPO_ROOT}/scripts/contract/validate-contracts.py" \
    --schema-dir "${REPO_ROOT}/schemas" \
    --fixture-dir "${TMP_DIR}/invalid-incident-transition"

mkdir -p "${TMP_DIR}/invalid-frozen-window-trigger-name"
cp "${REPO_ROOT}/tests/golden/obs.recorder_frozen_window.v1.min.json" \
  "${TMP_DIR}/invalid-frozen-window-trigger-name/obs.recorder_frozen_window.v1.min.json"
python3 - "${TMP_DIR}/invalid-frozen-window-trigger-name/obs.recorder_frozen_window.v1.min.json" <<'PY'
import json
import sys
path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    value = json.load(fh)
value["freeze_reason"] = "trigger_policy"
value["preservation_reason"] = {
    "kind": "trigger_policy",
    "name": "cpu_root_cause_detected"
}
with open(path, "w", encoding="utf-8") as fh:
    json.dump(value, fh, indent=2)
    fh.write("\n")
PY
assert_fails_with "trigger preservation reason must not promote root-cause claims" \
  "${REPO_ROOT}/scripts/contract/validate-contracts.py" \
    --schema-dir "${REPO_ROOT}/schemas" \
    --fixture-dir "${TMP_DIR}/invalid-frozen-window-trigger-name"

mkdir -p "${TMP_DIR}/invalid-recorder-ref-segment"
cp "${REPO_ROOT}/tests/golden/obs.recorder_incident_resolution.v1.min.json" \
  "${TMP_DIR}/invalid-recorder-ref-segment/obs.recorder_incident_resolution.v1.min.json"
python3 - "${TMP_DIR}/invalid-recorder-ref-segment/obs.recorder_incident_resolution.v1.min.json" <<'PY'
import json
import sys
path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    value = json.load(fh)
value["incident_ref"] = "artifact://recorder/incidents/../incident.json"
value["loss_report_ref"] = "artifact://recorder/incidents/./loss_report.json"
with open(path, "w", encoding="utf-8") as fh:
    json.dump(value, fh, indent=2)
    fh.write("\n")
PY
assert_fails_with "does not match" \
  "${REPO_ROOT}/scripts/contract/validate-contracts.py" \
    --schema-dir "${REPO_ROOT}/schemas" \
    --fixture-dir "${TMP_DIR}/invalid-recorder-ref-segment"

mkdir -p "${TMP_DIR}/invalid-recorder-marker-ref-segment"
cp "${REPO_ROOT}/tests/golden/obs.recorder_marker_result.v1.min.json" \
  "${TMP_DIR}/invalid-recorder-marker-ref-segment/obs.recorder_marker_result.v1.min.json"
python3 - "${TMP_DIR}/invalid-recorder-marker-ref-segment/obs.recorder_marker_result.v1.min.json" <<'PY'
import json
import sys
path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    value = json.load(fh)
value["pending_marker_ref"] = "artifact://recorder/markers/pending/../marker.json"
with open(path, "w", encoding="utf-8") as fh:
    json.dump(value, fh, indent=2)
    fh.write("\n")
PY
assert_fails_with "is not valid under any of the given schemas" \
  "${REPO_ROOT}/scripts/contract/validate-contracts.py" \
    --schema-dir "${REPO_ROOT}/schemas" \
    --fixture-dir "${TMP_DIR}/invalid-recorder-marker-ref-segment"

mkdir -p "${TMP_DIR}/invalid-marker-time-confidence"
cp "${REPO_ROOT}/tests/golden/obs.recorder_marker.v1.min.json" \
  "${TMP_DIR}/invalid-marker-time-confidence/obs.recorder_marker.v1.min.json"
python3 - "${TMP_DIR}/invalid-marker-time-confidence/obs.recorder_marker.v1.min.json" <<'PY'
import json
import sys
path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    value = json.load(fh)
del value["asserted_event_time"]["confidence"]
with open(path, "w", encoding="utf-8") as fh:
    json.dump(value, fh, indent=2)
    fh.write("\n")
PY
assert_fails_with "asserted_event_time" \
  "${REPO_ROOT}/scripts/contract/validate-contracts.py" \
    --schema-dir "${REPO_ROOT}/schemas" \
    --fixture-dir "${TMP_DIR}/invalid-marker-time-confidence"

cp "${REPO_ROOT}/contracts/adc.contract_coverage.v1.json" \
  "${TMP_DIR}/coverage-missing-schema.json"
python3 - "${TMP_DIR}/coverage-missing-schema.json" <<'PY'
import json
import sys
path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    value = json.load(fh)
value["contracts"] = [
    entry for entry in value["contracts"]
    if entry["contract"] != "obs.probe_result.v1"
]
with open(path, "w", encoding="utf-8") as fh:
    json.dump(value, fh, indent=2)
    fh.write("\n")
PY
assert_fails_with "obs.probe_result.v1: missing from adc.contract_coverage.v1" \
  "${REPO_ROOT}/scripts/contract/check-coverage.py" \
    --schema-dir "${REPO_ROOT}/schemas" \
    --coverage "${TMP_DIR}/coverage-missing-schema.json" \
    --repo-root "${REPO_ROOT}"
