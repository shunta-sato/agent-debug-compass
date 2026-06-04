#!/usr/bin/env python3
"""Normalize generated contract fixtures so local paths and clocks do not drift."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Any


AUTO_ID_PATTERNS = (
    (re.compile(r"R-SYMPTOM-\d+"), "R-SYMPTOM-GENERATED"),
    (re.compile(r"S-IR-[A-Za-z0-9_.:-]+"), "S-IR-GENERATED"),
)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture-dir", required=True)
    parser.add_argument("--repo-root", default=str(Path.cwd()))
    args = parser.parse_args()

    fixture_dir = Path(args.fixture_dir)
    repo_root = Path(args.repo_root).resolve()
    for path in sorted(fixture_dir.glob("*.json")):
        value = load_json(path)
        normalized = normalize_value(value, repo_root)
        with path.open("w", encoding="utf-8") as fh:
            json.dump(normalized, fh, indent=2, sort_keys=True)
            fh.write("\n")
    return 0


def load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def normalize_value(value: Any, repo_root: Path) -> Any:
    if isinstance(value, dict):
        output = {}
        for key, child in value.items():
            if key in {"generated_at_unix_ms", "created_at_unix_ms", "timestamp_unix_ms"}:
                output[key] = 0
            else:
                output[key] = normalize_value(child, repo_root)
        return output
    if isinstance(value, list):
        return [normalize_value(item, repo_root) for item in value]
    if isinstance(value, str):
        return normalize_string(value, repo_root)
    return value


def normalize_string(value: str, repo_root: Path) -> str:
    normalized = value.replace(str(repo_root), "<repo>")
    normalized = re.sub(r"/tmp/[A-Za-z0-9._/-]+", "<tmp>", normalized)
    for pattern, replacement in AUTO_ID_PATTERNS:
        normalized = pattern.sub(replacement, normalized)
    return normalized


if __name__ == "__main__":
    raise SystemExit(main())
