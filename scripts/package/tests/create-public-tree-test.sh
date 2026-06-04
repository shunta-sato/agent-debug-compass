#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

OUTPUT_DIR="$TMP_DIR/agent-debug-compass-public"

HOME="$TMP_DIR/no-git-identity-home" \
  "$ROOT_DIR/scripts/package/create-public-tree.sh" \
  --output "$OUTPUT_DIR" \
  --init-git \
  >"$TMP_DIR/create.out"

grep -q "public_tree=$OUTPUT_DIR" "$TMP_DIR/create.out"
test -d "$OUTPUT_DIR/.git"
test ! -e "$OUTPUT_DIR/.agents"
test ! -e "$OUTPUT_DIR/plans"
test -e "$OUTPUT_DIR/schemas/obs.capability_report.v1.schema.json"
test -e "$OUTPUT_DIR/contracts/adc.contract_coverage.v1.json"
test -e "$OUTPUT_DIR/benchmarks/scenarios/prompt_injection_log.json"
test -e "$OUTPUT_DIR/tests/golden/obs.hypothesis_set.v1.min.json"
test -x "$OUTPUT_DIR/scripts/contract/validate-contracts.py"
test -x "$OUTPUT_DIR/scripts/contract/validate-generated-contracts.sh"
test -x "$OUTPUT_DIR/scripts/contract/normalize-generated-fixture.py"
test -x "$OUTPUT_DIR/scripts/contract/check-coverage.py"
test -e "$OUTPUT_DIR/scripts/contract/requirements.txt"
test -x "$OUTPUT_DIR/scripts/benchmarks/run-agent-debug-benchmark.py"

git -C "$OUTPUT_DIR" log --oneline -1 | grep -q "chore: initial Agent Debug Compass public repository"
git -C "$OUTPUT_DIR" config user.name | grep -q "Shunta Sato"
git -C "$OUTPUT_DIR" config user.email | grep -q "shunta.sato@gmail.com"
