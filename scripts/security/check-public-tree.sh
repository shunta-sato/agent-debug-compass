#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(pwd)"

usage() {
  cat <<'USAGE'
Usage:
  scripts/security/check-public-tree.sh [--root DIR]

Checks that a prepared public Agent Debug Compass tree does not contain private
agent tooling, generated artifacts, old public product names, or obvious local
target/secrets markers.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)
      ROOT_DIR="${2:?missing --root value}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

ROOT_DIR="$(cd "$ROOT_DIR" && pwd)"
failures=()

add_failure() {
  failures+=("$1")
}

for forbidden in \
  ".agents" \
  ".codex" \
  "docs/01_development_requirements.md" \
  "docs/02_basic_design.md" \
  "docs/03_e2e_test_design.md" \
  "target" \
  "dist" \
  "demo-results" \
  "dogfood-results" \
  "e2e-results" \
  "reports" \
  "tmp"
do
  if [[ -e "$ROOT_DIR/$forbidden" ]]; then
    add_failure "forbidden path exists: $forbidden"
  fi
done

patterns=(
  'rca-probed'
  'rca-probe'
  'rca-mcp-server'
  'rca-priv-helper'
  'rca-workload'
  'rca-demo-sensor-gateway'
  'rca-core'
  'rca_core'
  'RCA_PROBED_HOME'
  'rca_sensor_probe'
  'target55'
  'lab-target'
  'lab-pi5-a'
  '/home/satoshun'
  'github\.com/satoshun'
  'semcj'
  'BEGIN [A-Z ]*PRIVATE KEY'
  '192\.168\.'
)

if command -v rg >/dev/null 2>&1; then
  for pattern in "${patterns[@]}"; do
    if match="$(cd "$ROOT_DIR" && rg -n --hidden \
      --glob '!.git/**' \
      --glob '!Cargo.lock' \
      --glob '!scripts/security/check-public-tree.sh' \
      --glob '!scripts/security/tests/check-public-tree-test.sh' \
      "$pattern" . || true)"; then
      if [[ -n "$match" ]]; then
        add_failure "forbidden pattern [$pattern]: $(printf '%s\n' "$match" | head -n 3)"
      fi
    fi
  done
else
  for pattern in "${patterns[@]}"; do
    if match="$(grep -RInE --exclude-dir=.git "$pattern" "$ROOT_DIR" 2>/dev/null || true)"; then
      if [[ -n "$match" ]]; then
        add_failure "forbidden pattern [$pattern]: $(printf '%s\n' "$match" | head -n 3)"
      fi
    fi
  done
fi

if command -v rg >/dev/null 2>&1; then
  if match="$(cd "$ROOT_DIR" && rg -n --hidden \
    --glob '!.git/**' \
    --glob '*.md' \
    -i 'codex' . || true)"; then
    if [[ -n "$match" ]]; then
      add_failure "forbidden markdown Codex reference: $(printf '%s\n' "$match" | head -n 3)"
    fi
  fi
  if match="$(cd "$ROOT_DIR" && rg -n --hidden \
    --glob '!.git/**' \
    --glob '*.md' \
    '[ぁ-んァ-ン一-龯]' . || true)"; then
    if [[ -n "$match" ]]; then
      add_failure "forbidden non-English markdown text: $(printf '%s\n' "$match" | head -n 3)"
    fi
  fi
fi

if [[ "${#failures[@]}" -gt 0 ]]; then
  printf 'public tree check failed for %s\n' "$ROOT_DIR" >&2
  for failure in "${failures[@]}"; do
    printf -- '- %s\n' "$failure" >&2
  done
  exit 1
fi

printf 'public tree check passed: %s\n' "$ROOT_DIR"
