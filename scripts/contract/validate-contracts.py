#!/usr/bin/env python3
"""Minimal contract fixture validator, not a complete JSON Schema implementation."""

import argparse
import json
import sys
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--schema-dir", required=True)
    parser.add_argument("--fixture-dir", required=True)
    args = parser.parse_args()

    schema_dir = Path(args.schema_dir)
    fixture_dir = Path(args.fixture_dir)
    schemas = {
        path.name.removesuffix(".schema.json"): load_json(path)
        for path in sorted(schema_dir.glob("*.schema.json"))
    }
    if not schemas:
        print("no schemas found", file=sys.stderr)
        return 1

    fixtures = sorted(fixture_dir.glob("*.json"))
    if not fixtures:
        print("no golden fixtures found", file=sys.stderr)
        return 1

    errors = []
    for fixture_path in fixtures:
        fixture = load_json(fixture_path)
        schema_id = fixture_path.name.removesuffix(".min.json")
        schema = schemas.get(schema_id)
        if schema is None:
            errors.append(f"{fixture_path}: no schema named {schema_id}")
            continue
        validate_value(fixture, schema, fixture_path.name, errors)

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print(json.dumps({"schema_count": len(schemas), "fixture_count": len(fixtures)}, indent=2))
    return 0


def load_json(path: Path):
    with path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def validate_value(value, schema, path, errors):
    expected_type = schema.get("type")
    if expected_type and not type_matches(value, expected_type):
        errors.append(f"{path}: expected type {expected_type}")
        return

    enum = schema.get("enum")
    if enum is not None and value not in enum:
        errors.append(f"{path}: value {value!r} is not in enum {enum!r}")

    if expected_type == "object":
        required = schema.get("required", [])
        for key in required:
            if key not in value:
                errors.append(f"{path}: missing required field {key}")
        properties = schema.get("properties", {})
        for key, child in properties.items():
            if key in value:
                validate_value(value[key], child, f"{path}.{key}", errors)
    elif expected_type == "array":
        item_schema = schema.get("items")
        if item_schema:
            for index, item in enumerate(value):
                validate_value(item, item_schema, f"{path}[{index}]", errors)


def type_matches(value, expected_type):
    if isinstance(expected_type, list):
        return any(type_matches(value, item) for item in expected_type)
    return {
        "object": lambda item: isinstance(item, dict),
        "array": lambda item: isinstance(item, list),
        "string": lambda item: isinstance(item, str),
        "boolean": lambda item: isinstance(item, bool),
        "integer": lambda item: isinstance(item, int) and not isinstance(item, bool),
        "number": lambda item: isinstance(item, (int, float)) and not isinstance(item, bool),
    }.get(expected_type, lambda _item: True)(value)


if __name__ == "__main__":
    raise SystemExit(main())
