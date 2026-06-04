#!/usr/bin/env python3
"""Check that every public ADC schema is represented in the coverage manifest."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


PATH_FIELDS = (
    "static_fixtures",
    "generated_cli",
    "generated_mcp",
    "invariant_tests",
    "adversarial_tests",
    "trace_fixtures",
)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--schema-dir", required=True)
    parser.add_argument("--coverage", required=True)
    parser.add_argument("--repo-root", default=str(Path.cwd()))
    args = parser.parse_args()

    repo_root = Path(args.repo_root)
    schema_dir = Path(args.schema_dir)
    coverage_path = Path(args.coverage)
    manifest = load_json(coverage_path)
    entries = manifest.get("contracts", [])
    errors: list[str] = []

    by_contract = {}
    for index, entry in enumerate(entries):
        contract = entry.get("contract")
        if not isinstance(contract, str):
            errors.append(f"contracts[{index}].contract: missing contract id")
            continue
        if contract in by_contract:
            errors.append(f"{contract}: duplicate coverage entry")
        by_contract[contract] = entry
        validate_entry_paths(repo_root, contract, entry, errors)
        validate_coverage_levels(contract, entry, errors)

    public_schemas = {
        path.name.removesuffix(".schema.json")
        for path in schema_dir.glob("*.schema.json")
        if path.name.startswith(("obs.", "adc."))
    }
    for schema_id in sorted(public_schemas):
        entry = by_contract.get(schema_id)
        if entry is None:
            errors.append(f"{schema_id}: missing from adc.contract_coverage.v1")
        elif entry.get("schema") != f"schemas/{schema_id}.schema.json":
            errors.append(f"{schema_id}: schema path does not match schema id")

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print(json.dumps({"covered_contract_count": len(public_schemas)}, indent=2))
    return 0


def load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def validate_entry_paths(
    repo_root: Path,
    contract: str,
    entry: dict[str, Any],
    errors: list[str],
) -> None:
    schema_path = entry.get("schema")
    if not isinstance(schema_path, str) or not (repo_root / schema_path).is_file():
        errors.append(f"{contract}.schema: path does not exist")
    for field in PATH_FIELDS:
        for path_text in entry.get(field, []):
            if not isinstance(path_text, str):
                errors.append(f"{contract}.{field}: path must be string")
                continue
            if not (repo_root / path_text).is_file():
                errors.append(f"{contract}.{field}: path does not exist: {path_text}")


def validate_coverage_levels(contract: str, entry: dict[str, Any], errors: list[str]) -> None:
    levels = set(entry.get("coverage_levels", []))
    if "deferred" in levels and not entry.get("deferred_reason"):
        errors.append(f"{contract}.deferred_reason: required when coverage is deferred")
    for level, field in [
        ("static", "static_fixtures"),
        ("generated_cli", "generated_cli"),
        ("generated_mcp", "generated_mcp"),
        ("invariant", "invariant_tests"),
        ("adversarial", "adversarial_tests"),
        ("trace", "trace_fixtures"),
    ]:
        if level in levels and not entry.get(field):
            errors.append(f"{contract}.{field}: required by coverage level {level}")


if __name__ == "__main__":
    raise SystemExit(main())
