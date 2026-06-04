#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIXTURE_DIR="${REPO_ROOT}/target/contract-fixtures"

rm -rf "${FIXTURE_DIR}"
mkdir -p "${FIXTURE_DIR}"

ADC_CONTRACT_FIXTURE_DIR="${FIXTURE_DIR}" cargo test -q -p adc --test contract_outputs
ADC_CONTRACT_FIXTURE_DIR="${FIXTURE_DIR}" cargo test -q -p adc-mcp --test contract_outputs

"${REPO_ROOT}/scripts/contract/normalize-generated-fixture.py" \
  --fixture-dir "${FIXTURE_DIR}" \
  --repo-root "${REPO_ROOT}"

"${REPO_ROOT}/scripts/contract/validate-contracts.py" \
  --schema-dir "${REPO_ROOT}/schemas" \
  --fixture-dir "${FIXTURE_DIR}"
