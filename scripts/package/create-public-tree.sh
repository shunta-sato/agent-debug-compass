#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUTPUT_DIR=""
FORCE=0
INIT_GIT=0

usage() {
  cat <<'USAGE'
Usage:
  scripts/package/create-public-tree.sh --output DIR [--force] [--init-git]

Creates a clean public Agent Debug Compass source tree without private history,
.agents/.codex, generated artifacts, plans, or local dogfood outputs.

The resulting tree is intended to become the initial public GitHub repository:
  agent-debug-compass
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output)
      OUTPUT_DIR="${2:?missing --output value}"
      shift 2
      ;;
    --force)
      FORCE=1
      shift
      ;;
    --init-git)
      INIT_GIT=1
      shift
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

[[ -n "$OUTPUT_DIR" ]] || {
  usage >&2
  exit 2
}

case "$OUTPUT_DIR" in
  /|"$ROOT_DIR"|"$ROOT_DIR/"|"$HOME"|"$HOME/")
    printf 'refusing unsafe output dir: %s\n' "$OUTPUT_DIR" >&2
    exit 1
    ;;
esac

OUTPUT_DIR="$(mkdir -p "$(dirname "$OUTPUT_DIR")" && cd "$(dirname "$OUTPUT_DIR")" && pwd)/$(basename "$OUTPUT_DIR")"
TMP_DIR="$(mktemp -d "$(dirname "$OUTPUT_DIR")/.agent-debug-compass-public.XXXXXX")"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

if [[ -e "$OUTPUT_DIR" ]]; then
  if [[ "$FORCE" -ne 1 ]]; then
    printf 'output dir exists; pass --force to replace it: %s\n' "$OUTPUT_DIR" >&2
    exit 1
  fi
  rm -rf -- "$OUTPUT_DIR"
fi

copy_path() {
  local path="$1"
  if [[ -e "$ROOT_DIR/$path" ]]; then
    mkdir -p "$TMP_DIR/$(dirname "$path")"
    cp -a "$ROOT_DIR/$path" "$TMP_DIR/$path"
  fi
}

for file in \
  .gitignore \
  Cargo.toml \
  Cargo.lock \
  README.md \
  LICENSE \
  SECURITY.md \
  CONTRIBUTING.md \
  CHANGELOG.md \
  Makefile \
  deny.toml
do
  copy_path "$file"
done

for dir in \
  benchmarks \
  crates \
  docs \
  demos \
  scripts \
  packaging \
  profiles \
  kernel \
  schemas \
  supply-chain \
  tests/golden
do
  copy_path "$dir"
done

rm -f \
  "$TMP_DIR/docs/01_development_requirements.md" \
  "$TMP_DIR/docs/02_basic_design.md" \
  "$TMP_DIR/docs/03_e2e_test_design.md" \
  "$TMP_DIR/docs/07_agent_first_dogfood_findings.md" \
  "$TMP_DIR/docs/08_world_class_agent_context_dogfood.md" \
  "$TMP_DIR"/docs/09_* \
  "$TMP_DIR/docs/10_ssh_fleet_timeout_bug_report.md" \
  "$TMP_DIR"/docs/11_*

find "$TMP_DIR" -type d \( \
  -name target -o \
  -name dist -o \
  -name demo-results -o \
  -name dogfood-results -o \
  -name e2e-results -o \
  -name reports -o \
  -name tmp -o \
  -name test-results \
  \) -prune -exec rm -rf {} +

find "$TMP_DIR/kernel" -type f \( \
  -name '*.ko' -o \
  -name '*.mod' -o \
  -name '*.mod.c' -o \
  -name '*.o' -o \
  -name '*.cmd' -o \
  -name 'Module.symvers' -o \
  -name 'modules.order' \
  \) -delete 2>/dev/null || true

"$ROOT_DIR/scripts/security/check-public-tree.sh" --root "$TMP_DIR"

if [[ "$INIT_GIT" -eq 1 ]]; then
  git -C "$TMP_DIR" init -q
  git -C "$TMP_DIR" config user.name "Shunta Sato"
  git -C "$TMP_DIR" config user.email "shunta.sato@gmail.com"
  git -C "$TMP_DIR" add .
  git -C "$TMP_DIR" commit -q -m "chore: initial Agent Debug Compass public repository"
fi

mv "$TMP_DIR" "$OUTPUT_DIR"
trap - EXIT
printf 'public_tree=%s\n' "$OUTPUT_DIR"
