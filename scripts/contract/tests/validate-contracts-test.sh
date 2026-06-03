#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"

"${REPO_ROOT}/scripts/contract/validate-contracts.py" \
  --schema-dir "${REPO_ROOT}/schemas" \
  --fixture-dir "${REPO_ROOT}/tests/golden"
