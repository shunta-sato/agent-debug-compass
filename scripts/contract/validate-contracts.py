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


def contains_root_cause_claim(value: Any) -> bool:
    if not isinstance(value, str):
        return False
    text = " ".join(value.lower().split())
    return bool(
        re.search(r"\b(root cause|root-cause|cause is|caused by|is the cause)\b", text)
    )


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
