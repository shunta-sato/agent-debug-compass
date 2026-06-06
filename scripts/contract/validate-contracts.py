#!/usr/bin/env python3
"""Validate ADC contract fixtures against the local JSON Schema registry.

This uses jsonschema's Draft 2020-12 validator for schema validation while
constraining refs to the checked-in local schema directory. Remote refs,
absolute paths, parent traversal, and deep JSON pointer fragments are rejected
before validation.
"""

from __future__ import annotations

import argparse
import copy
import json
import re
import sys
from pathlib import Path
from typing import Any

try:
    from jsonschema import Draft202012Validator, ValidationError
except ModuleNotFoundError as exc:  # pragma: no cover - exercised manually
    print(
        "missing contract validation dependency; install with "
        "`python3 -m pip install -r scripts/contract/requirements.txt`",
        file=sys.stderr,
    )
    raise SystemExit(1) from exc


ALLOWED_REF_SUFFIXES = ("", "#/")
SCHEMA_SUFFIX = ".schema.json"
FIXTURE_SUFFIXES = (
    ".min.json",
    ".generated.json",
    ".trace.json",
    ".valid.json",
)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--schema-dir", required=True)
    parser.add_argument(
        "--fixture-dir",
        required=True,
        action="append",
        help="Fixture directory. May be passed more than once.",
    )
    args = parser.parse_args()

    schema_dir = Path(args.schema_dir)
    fixture_dirs = [Path(path) for path in args.fixture_dir]
    raw_schemas = load_schemas(schema_dir)
    if not raw_schemas:
        print("no schemas found", file=sys.stderr)
        return 1

    errors: list[str] = []
    schemas: dict[str, dict[str, Any]] = {}
    for schema_id, schema in raw_schemas.items():
        try:
            resolved = resolve_local_refs(schema, schema_dir, raw_schemas)
            Draft202012Validator.check_schema(resolved)
            schemas[schema_id] = resolved
        except ContractValidationError as err:
            errors.append(f"{schema_id}: {err}")
        except Exception as err:
            errors.append(f"{schema_id}: invalid schema: {err}")

    fixtures = fixture_paths(fixture_dirs)
    if not fixtures:
        print("no contract fixtures found", file=sys.stderr)
        return 1

    for fixture_path in fixtures:
        fixture = load_json(fixture_path)
        schema_id = infer_schema_id(fixture_path, fixture)
        schema = schemas.get(schema_id)
        if schema is None:
            errors.append(f"{fixture_path}: no schema named {schema_id}")
            continue
        validator = Draft202012Validator(schema)
        for error in sorted(validator.iter_errors(fixture), key=error_sort_key):
            errors.append(format_validation_error(fixture_path, error))
        validate_semantic_invariants(schema_id, fixture, fixture_path.name, errors)

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print(
        json.dumps(
            {"schema_count": len(raw_schemas), "fixture_count": len(fixtures)},
            indent=2,
        )
    )
    return 0


def load_schemas(schema_dir: Path) -> dict[str, dict[str, Any]]:
    return {
        path.name.removesuffix(SCHEMA_SUFFIX): load_json(path)
        for path in sorted(schema_dir.glob(f"*{SCHEMA_SUFFIX}"))
    }


def fixture_paths(fixture_dirs: list[Path]) -> list[Path]:
    fixtures: list[Path] = []
    for fixture_dir in fixture_dirs:
        fixtures.extend(sorted(fixture_dir.glob("*.json")))
    return fixtures


def load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


class ContractValidationError(Exception):
    pass


def resolve_local_refs(
    value: Any,
    schema_dir: Path,
    raw_schemas: dict[str, dict[str, Any]],
) -> Any:
    if isinstance(value, dict):
        if "$ref" in value:
            ref = value["$ref"]
            if len(value) != 1:
                raise ContractValidationError(
                    f"$ref object must not mix sibling keys: {ref}"
                )
            schema_id = schema_id_for_ref(ref, schema_dir)
            referenced = raw_schemas.get(schema_id)
            if referenced is None:
                raise ContractValidationError(f"local ref target not found: {ref}")
            return resolve_local_refs(copy.deepcopy(referenced), schema_dir, raw_schemas)
        return {
            key: resolve_local_refs(child, schema_dir, raw_schemas)
            for key, child in value.items()
        }
    if isinstance(value, list):
        return [resolve_local_refs(item, schema_dir, raw_schemas) for item in value]
    return value


def schema_id_for_ref(ref: str, schema_dir: Path) -> str:
    if re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*://", ref):
        raise ContractValidationError(f"remote refs are not allowed: {ref}")
    if ref.startswith("/"):
        raise ContractValidationError(f"absolute refs are not allowed: {ref}")
    if ".." in Path(ref.split("#", 1)[0]).parts:
        raise ContractValidationError(f"parent traversal refs are not allowed: {ref}")

    path_text, fragment = split_ref(ref)
    if fragment not in ALLOWED_REF_SUFFIXES:
        raise ContractValidationError(f"deep JSON pointer refs are not supported: {ref}")
    normalized = path_text.removeprefix("./")
    if not normalized.endswith(SCHEMA_SUFFIX):
        raise ContractValidationError(f"local refs must target *.schema.json: {ref}")
    target = (schema_dir / normalized).resolve()
    schema_root = schema_dir.resolve()
    if not target.is_relative_to(schema_root):
        raise ContractValidationError(f"refs outside schemas are not allowed: {ref}")
    return Path(normalized).name.removesuffix(SCHEMA_SUFFIX)


def split_ref(ref: str) -> tuple[str, str]:
    if "#" not in ref:
        return ref, ""
    path_text, fragment = ref.split("#", 1)
    return path_text, f"#{fragment}"


def infer_schema_id(fixture_path: Path, fixture: Any) -> str:
    if isinstance(fixture, dict):
        schema_version = fixture.get("schema_version")
        if isinstance(schema_version, str):
            return schema_version
    name = fixture_path.name
    for suffix in FIXTURE_SUFFIXES:
        if name.endswith(suffix):
            return name.removesuffix(suffix)
    return name.removesuffix(".json")


def error_sort_key(error: ValidationError) -> tuple[str, str]:
    return (path_suffix(error), error.message)


def format_validation_error(fixture_path: Path, error: ValidationError) -> str:
    path = f"{fixture_path.name}{path_suffix(error)}"
    return f"{path}: {error.message}"


def path_suffix(error: ValidationError) -> str:
    parts: list[str] = []
    for part in error.absolute_path:
        if isinstance(part, int):
            parts.append(f"[{part}]")
        else:
            parts.append(f".{part}")
    if error.validator == "additionalProperties":
        unexpected = unexpected_additional_property(error.message)
        if unexpected:
            parts.append(f".{unexpected}")
    return "".join(parts)


def unexpected_additional_property(message: str) -> str | None:
    match = re.search(r"'([^']+)' was unexpected", message)
    if match:
        return match.group(1)
    return None


def validate_semantic_invariants(
    schema_id: str,
    fixture: Any,
    path: str,
    errors: list[str],
) -> None:
    validate_data_quality_values(fixture, path, errors)
    if schema_id == "obs.artifact_trust.v1":
        validate_artifact_trust(fixture, path, errors)
    elif schema_id == "obs.hypothesis_set.v1":
        validate_hypothesis_set(fixture, path, errors)
    elif schema_id == "obs.probe_plan.v1":
        validate_probe_plan(fixture, path, errors)
    elif schema_id == "obs.probe_result.v1":
        validate_probe_result(fixture, path, errors)
    elif schema_id == "obs.safety_policy.v1":
        validate_safety_policy(fixture, path, errors)
    elif schema_id == "obs.ref_resolution.v1":
        validate_ref_resolution(fixture, path, errors)
    elif schema_id == "obs.loss_report.v1":
        validate_loss_report(fixture, path, errors)
    elif schema_id == "obs.recorder_observation_coverage.v1":
        validate_recorder_observation_coverage(fixture, path, errors)
    elif schema_id == "obs.recorder_status.v1":
        validate_recorder_status(fixture, path, errors)
    elif schema_id == "obs.recorder_marker.v1":
        validate_recorder_marker(fixture, path, errors)
    elif schema_id == "obs.recorder_incident.v1":
        validate_recorder_incident(fixture, path, errors)
    elif schema_id == "obs.recorder_frozen_window.v1":
        validate_recorder_frozen_window(fixture, path, errors)
    elif schema_id == "adc.investigation_trace.v1":
        validate_investigation_trace(fixture, path, errors)


def validate_data_quality_values(value: Any, path: str, errors: list[str]) -> None:
    if isinstance(value, dict):
        if is_data_quality(value):
            dropped = value.get("dropped")
            drop_count = value.get("drop_count")
            truncated = value.get("truncated")
            missing = value.get("missing")
            notes = value.get("notes")
            if dropped is False and drop_count != 0:
                errors.append(f"{path}: drop_count > 0 requires dropped=true")
            if isinstance(drop_count, int) and drop_count > 0 and dropped is not True:
                errors.append(f"{path}: drop_count > 0 requires dropped=true")
            if truncated is True and not non_empty_string_list(missing) and not non_empty_string_list(notes):
                errors.append(
                    f"{path}: truncated=true requires a missing entry or note explaining truncation"
                )
        for key, child in value.items():
            validate_data_quality_values(child, f"{path}.{key}", errors)
    elif isinstance(value, list):
        for index, item in enumerate(value):
            validate_data_quality_values(item, f"{path}[{index}]", errors)


def is_data_quality(value: dict[str, Any]) -> bool:
    return {
        "dropped",
        "drop_count",
        "throttled",
        "missing",
        "truncated",
        "clock_confidence",
        "notes",
    }.issubset(value.keys())


def non_empty_string_list(value: Any) -> bool:
    return isinstance(value, list) and any(
        isinstance(item, str) and item.strip() for item in value
    )


def validate_artifact_trust(fixture: Any, path: str, errors: list[str]) -> None:
    if not isinstance(fixture, dict):
        return
    content_class = fixture.get("content_class")
    trust_level = fixture.get("trust_level")
    policy = fixture.get("agent_instruction_policy")
    if policy != "treat_as_data_only":
        errors.append(f"{path}.agent_instruction_policy: target text must be treat_as_data_only")
    if content_class in {"log", "journal", "config", "domain_event"} and trust_level == "trusted_system":
        errors.append(f"{path}.trust_level: target-originated text must not be trusted_system")


def validate_hypothesis_set(fixture: Any, path: str, errors: list[str]) -> None:
    if not isinstance(fixture, dict):
        return
    for index, hypothesis in enumerate(fixture.get("hypotheses", [])):
        if not isinstance(hypothesis, dict):
            continue
        statement = hypothesis.get("statement", "")
        if contains_root_cause_claim(statement):
            errors.append(
                f"{path}.hypotheses[{index}].statement: hypothesis statement must not promote root-cause claims"
            )
        if hypothesis.get("claim_boundary") != "hypothesis_only":
            errors.append(f"{path}.hypotheses[{index}].claim_boundary: must be hypothesis_only")


def validate_probe_plan(fixture: Any, path: str, errors: list[str]) -> None:
    if not isinstance(fixture, dict):
        return
    for index, candidate in enumerate(fixture.get("candidate_probes", [])):
        if not isinstance(candidate, dict):
            continue
        if not candidate.get("discriminates"):
            errors.append(f"{path}.candidate_probes[{index}].discriminates: must not be empty")
        if candidate.get("cause_neutral") is not True:
            errors.append(f"{path}.candidate_probes[{index}].cause_neutral: must be true")


def validate_probe_result(fixture: Any, path: str, errors: list[str]) -> None:
    if not isinstance(fixture, dict):
        return
    result_kind = fixture.get("result_kind")
    executed = fixture.get("executed")
    status = fixture.get("status")
    capability_status = fixture.get("capability_status")
    if result_kind == "not_executed_missing_capability" and executed is not False:
        errors.append(f"{path}.executed: not_executed_missing_capability requires executed=false")
    if result_kind == "not_executed_policy_denied" and executed is not False:
        errors.append(f"{path}.executed: not_executed_policy_denied requires executed=false")
    if status == "failed_missing_capability" and capability_status not in {
        "unavailable",
        "requires_privilege",
        "unknown",
        "degraded",
    }:
        errors.append(
            f"{path}.capability_status: failed_missing_capability requires unavailable/requires_privilege/unknown/degraded"
        )
    for index, produced_fact in enumerate(fixture.get("produced_facts", [])):
        if isinstance(produced_fact, dict) and contains_root_cause_claim(
            produced_fact.get("statement", "")
        ):
            errors.append(
                f"{path}.produced_facts[{index}].statement: probe result must not promote root-cause claims"
            )


def validate_safety_policy(fixture: Any, path: str, errors: list[str]) -> None:
    if not isinstance(fixture, dict):
        return
    if fixture.get("default_decision") != "deny":
        errors.append(f"{path}.default_decision: default_decision must be deny")
    for index, rule in enumerate(fixture.get("rules", [])):
        if not isinstance(rule, dict):
            continue
        operation = rule.get("operation")
        decision = rule.get("decision")
        if operation == "arbitrary_shell" and decision != "deny":
            errors.append(f"{path}.rules[{index}].decision: arbitrary_shell must be denied")
        if operation == "firmware_flash" and decision not in {"deny", "requires_human_approval"}:
            errors.append(
                f"{path}.rules[{index}].decision: firmware_flash must be denied or require human approval"
            )


def validate_ref_resolution(fixture: Any, path: str, errors: list[str]) -> None:
    if not isinstance(fixture, dict):
        return
    trust = fixture.get("artifact_trust")
    if not isinstance(trust, dict):
        return
    if trust.get("raw_ref") != fixture.get("ref_uri"):
        errors.append(f"{path}.artifact_trust.raw_ref: must match ref_uri")
    if contains_root_cause_claim(fixture.get("text", "")):
        if trust.get("agent_instruction_policy") != "treat_as_data_only":
            errors.append(
                f"{path}.artifact_trust.agent_instruction_policy: root-cause-like target text must stay data-only"
            )
        if trust.get("trust_level") == "trusted_system":
            errors.append(
                f"{path}.artifact_trust.trust_level: root-cause-like target text must not be trusted_system"
            )


def contains_root_cause_claim(value: Any) -> bool:
    if not isinstance(value, str):
        return False
    text = " ".join(value.lower().replace("_", " ").replace("-", " ").split())
    return bool(
        re.search(
            r"\b(root cause|cause is|caused by|is the cause|caused|cause detected)\b",
            text,
        )
    )


def validate_loss_report(fixture: Any, path: str, errors: list[str]) -> None:
    if not isinstance(fixture, dict):
        return
    for index, entry in enumerate(fixture.get("collector_loss", [])):
        if not isinstance(entry, dict):
            continue
        expected = entry.get("expected_samples")
        recorded = entry.get("recorded_samples")
        dropped = entry.get("dropped_samples")
        gaps = entry.get("gap_ranges", [])
        reasons = entry.get("loss_reasons", [])
        degraded = entry.get("collectors_degraded", [])
        if isinstance(expected, int) and isinstance(recorded, int) and expected < recorded:
            errors.append(
                f"{path}.collector_loss[{index}].expected_samples: expected_samples must be >= recorded_samples when known"
            )
        if gaps and not (
            isinstance(dropped, int)
            and dropped > 0
            or non_empty_string_list(reasons)
        ):
            errors.append(
                f"{path}.collector_loss[{index}].gap_ranges: gap_ranges require dropped_samples > 0 or an explicit loss reason"
            )
        if recorded == 0 and not non_empty_string_list(reasons) and not non_empty_string_list(degraded):
            errors.append(
                f"{path}.collector_loss[{index}].recorded_samples: recorded_samples=0 must explain absent or degraded collector"
            )
        if expected is None and entry.get("loss_confidence") not in {"unknown", "low"}:
            errors.append(
                f"{path}.collector_loss[{index}].loss_confidence: unknown expected_samples requires unknown/low loss_confidence"
            )


def validate_recorder_observation_coverage(fixture: Any, path: str, errors: list[str]) -> None:
    if not isinstance(fixture, dict):
        return
    expected_signals = fixture.get("expected_signals", [])
    if not isinstance(expected_signals, list):
        expected_signals = []
    signal_ids = {
        signal.get("signal_id")
        for signal in expected_signals
        if isinstance(signal, dict)
    }
    for index, signal in enumerate(fixture.get("signals", [])):
        if not isinstance(signal, dict):
            continue
        signal_id = signal.get("signal_id")
        if signal.get("expected") is True and signal_id not in signal_ids:
            errors.append(
                f"{path}.signals[{index}].signal_id: expected coverage signal must appear in expected_signals"
            )
        retained = signal.get("retained_samples_before_freeze")
        exported = signal.get("exported_samples")
        truncated = signal.get("truncated_samples_due_to_freeze_budget")
        if isinstance(retained, int) and isinstance(exported, int):
            if exported > retained:
                errors.append(
                    f"{path}.signals[{index}].exported_samples: exported_samples must be <= retained_samples_before_freeze"
                )
            if isinstance(truncated, int) and truncated != max(retained - exported, 0):
                errors.append(
                    f"{path}.signals[{index}].truncated_samples_due_to_freeze_budget: must match retained_samples_before_freeze - exported_samples"
                )
        if signal.get("expected_samples_basis") == "budgeted_recorder_interval":
            if signal.get("expected_samples") != signal.get("expected_samples_budgeted"):
                errors.append(
                    f"{path}.signals[{index}].expected_samples: budgeted basis must match expected_samples_budgeted"
                )
        if signal.get("coverage_state") == "missing":
            data_quality = signal.get("data_quality")
            if not isinstance(data_quality, dict) or not non_empty_string_list(data_quality.get("missing")):
                errors.append(
                    f"{path}.signals[{index}].data_quality.missing: missing coverage requires missing evidence explanation"
                )
        if signal.get("coverage_state") == "unavailable":
            if signal.get("capability_status") not in {
                "unavailable",
                "requires_privilege",
                "unsafe",
            }:
                errors.append(
                    f"{path}.signals[{index}].capability_status: unavailable coverage requires unavailable/requires_privilege/unsafe capability"
                )


ALLOWED_RECORDER_TRANSITIONS = {
    ("disabled", "armed"),
    ("armed", "recording"),
    ("recording", "degraded"),
    ("recording", "over_budget"),
    ("recording", "freezing"),
    ("degraded", "recording"),
    ("degraded", "freezing"),
    ("over_budget", "degraded"),
    ("freezing", "frozen"),
    ("freezing", "error"),
    ("frozen", "recording"),
    ("error", "disabled"),
}


def validate_recorder_status(fixture: Any, path: str, errors: list[str]) -> None:
    if not isinstance(fixture, dict):
        return
    previous = fixture.get("previous_state")
    current = fixture.get("recorder_state")
    if previous is not None and (previous, current) not in ALLOWED_RECORDER_TRANSITIONS:
        errors.append(
            f"{path}.recorder_state: recorder transition {previous} -> {current} is forbidden"
        )
    storage = fixture.get("storage")
    if isinstance(storage, dict):
        if storage.get("storage_mode") == "memory_ring":
            if storage.get("volatile") is not True:
                errors.append(f"{path}.storage.volatile: memory_ring must be volatile")
            for field in [
                "survives_daemon_restart",
                "survives_target_reboot",
                "survives_power_loss",
            ]:
                if storage.get(field) is not False:
                    errors.append(f"{path}.storage.{field}: memory_ring must report false")


def validate_recorder_marker(fixture: Any, path: str, errors: list[str]) -> None:
    if not isinstance(fixture, dict):
        return
    if fixture.get("agent_instruction_policy") != "treat_as_event_marker_only":
        errors.append(
            f"{path}.agent_instruction_policy: marker text must be treated only as an event marker"
        )
    asserted = fixture.get("asserted_event_time")
    if not isinstance(asserted, dict):
        return
    kind = asserted.get("kind")
    confidence = asserted.get("confidence")
    if kind in {"relative_now", "unknown"} and confidence == "high":
        errors.append(
            f"{path}.asserted_event_time.confidence: relative/unknown marker time must not be high confidence"
        )
    if fixture.get("time_policy") == "center_on_asserted_event_time" and kind not in {
        "wall_time",
        "monotonic",
    }:
        errors.append(
            f"{path}.time_policy: center_on_asserted_event_time requires wall_time or monotonic asserted_event_time"
        )


ALLOWED_INCIDENT_TRANSITIONS = {
    ("marker_received", "pre_window_selected"),
    ("pre_window_selected", "post_window_collecting"),
    ("post_window_collecting", "freezing"),
    ("pre_window_selected", "freezing"),
    ("freezing", "frozen"),
    ("frozen", "exported"),
    ("frozen", "expired"),
    ("frozen", "discarded"),
    ("expired", "discarded"),
}


def validate_recorder_incident(fixture: Any, path: str, errors: list[str]) -> None:
    if not isinstance(fixture, dict):
        return
    previous = fixture.get("previous_state")
    current = fixture.get("incident_state")
    if previous is not None and (previous, current) not in ALLOWED_INCIDENT_TRANSITIONS:
        errors.append(
            f"{path}.incident_state: incident transition {previous} -> {current} is forbidden"
        )
    if current in {"frozen", "exported"}:
        if not fixture.get("frozen_window_ref"):
            errors.append(f"{path}.frozen_window_ref: frozen/exported incident requires frozen_window_ref")
        if not fixture.get("loss_report_ref"):
            errors.append(f"{path}.loss_report_ref: frozen/exported incident requires loss_report_ref")


def validate_recorder_frozen_window(fixture: Any, path: str, errors: list[str]) -> None:
    if not isinstance(fixture, dict):
        return
    persistence = fixture.get("persistence")
    if isinstance(persistence, dict):
        if persistence.get("persistence_mode") != "bounded_artifact_bundle":
            errors.append(
                f"{path}.persistence.persistence_mode: frozen incident must be a bounded artifact bundle"
            )
        bounded_by = persistence.get("bounded_by", [])
        for required in ["max_freeze_bytes", "max_frozen_incidents"]:
            if required not in bounded_by:
                errors.append(f"{path}.persistence.bounded_by: missing {required}")
    reason = fixture.get("preservation_reason")
    if isinstance(reason, dict) and reason.get("kind") == "trigger_policy":
        if contains_root_cause_claim(reason.get("name", "")):
            errors.append(
                f"{path}.preservation_reason.name: trigger preservation reason must not promote root-cause claims"
            )
    if not isinstance(fixture.get("loss_report"), dict):
        errors.append(f"{path}.loss_report: frozen window must include loss_report")


def validate_investigation_trace(fixture: Any, path: str, errors: list[str]) -> None:
    if not isinstance(fixture, dict):
        return
    hypothesis_set = fixture.get("hypothesis_set")
    probe_plan = fixture.get("probe_plan")
    probe_result = fixture.get("probe_result")
    continuation = fixture.get("investigation_continue")
    if not isinstance(hypothesis_set, dict) or not isinstance(probe_plan, dict) or not isinstance(probe_result, dict):
        return

    hypotheses = {
        hypothesis.get("hypothesis_id")
        for hypothesis in hypothesis_set.get("hypotheses", [])
        if isinstance(hypothesis, dict)
    }
    candidates = {
        candidate.get("probe_id"): candidate
        for candidate in probe_plan.get("candidate_probes", [])
        if isinstance(candidate, dict)
    }
    result_probe_id = probe_result.get("probe_id")
    candidate = candidates.get(result_probe_id)
    if candidate is None:
        errors.append(
            f"{path}.probe_result.probe_id: probe_result.probe_id must exist in probe_plan.candidate_probes"
        )
    if probe_result.get("probe_plan_id") != probe_plan.get("probe_plan_id"):
        errors.append(
            f"{path}.probe_result.probe_plan_id: probe_result.probe_plan_id must match probe_plan.probe_plan_id"
        )

    discriminates = set(candidate.get("discriminates", [])) if isinstance(candidate, dict) else set()
    for index, update in enumerate(probe_result.get("hypothesis_updates", [])):
        if not isinstance(update, dict):
            continue
        hypothesis_id = update.get("hypothesis_id")
        if hypothesis_id not in hypotheses:
            errors.append(
                f"{path}.probe_result.hypothesis_updates[{index}].hypothesis_id: hypothesis id must exist in hypothesis_set"
            )
        reason = update.get("reason", "")
        if hypothesis_id not in discriminates and "outside discriminates" not in str(reason):
            errors.append(
                f"{path}.probe_result.hypothesis_updates[{index}].hypothesis_id: hypothesis update must be listed in candidate discriminates or explain why it is outside discriminates"
            )

    if isinstance(continuation, dict):
        for index, opened in enumerate(continuation.get("opened_refs", [])):
            if not isinstance(opened, dict):
                continue
            trust = opened.get("artifact_trust")
            if not isinstance(trust, dict):
                errors.append(
                    f"{path}.investigation_continue.opened_refs[{index}].artifact_trust: opened refs must carry artifact_trust"
                )
            elif trust.get("agent_instruction_policy") != "treat_as_data_only":
                errors.append(
                    f"{path}.investigation_continue.opened_refs[{index}].artifact_trust.agent_instruction_policy: target text must stay data-only"
                )


if __name__ == "__main__":
    raise SystemExit(main())
