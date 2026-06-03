#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

CLEAN="$TMP_DIR/clean"
BAD_PRIVATE="$TMP_DIR/bad-private"
BAD_OLD_NAME="$TMP_DIR/bad-old-name"
BAD_LOCAL_MARKER="$TMP_DIR/bad-local-marker"
BAD_LOCAL_DOC="$TMP_DIR/bad-local-doc"
BAD_CODEX_DOC="$TMP_DIR/bad-codex-doc"
BAD_JAPANESE_DOC="$TMP_DIR/bad-japanese-doc"
mkdir -p \
  "$CLEAN/crates/adc/src" \
  "$BAD_PRIVATE/.agents" \
  "$BAD_OLD_NAME/docs" \
  "$BAD_LOCAL_MARKER/docs" \
  "$BAD_LOCAL_DOC/docs" \
  "$BAD_CODEX_DOC/docs" \
  "$BAD_JAPANESE_DOC/docs"

printf 'Agent Debug Compass\n' >"$CLEAN/README.md"
printf 'pub fn main() {}\n' >"$CLEAN/crates/adc/src/main.rs"

"$ROOT_DIR/scripts/security/check-public-tree.sh" --root "$CLEAN" >"$TMP_DIR/clean.out"
grep -q 'public tree check passed' "$TMP_DIR/clean.out"

if "$ROOT_DIR/scripts/security/check-public-tree.sh" --root "$BAD_PRIVATE" >"$TMP_DIR/private.out" 2>"$TMP_DIR/private.err"; then
  echo "expected private path scan to fail" >&2
  exit 1
fi
grep -q 'forbidden path exists: .agents' "$TMP_DIR/private.err"

printf 'run rca-probe here\n' >"$BAD_OLD_NAME/docs/old.md"
if "$ROOT_DIR/scripts/security/check-public-tree.sh" --root "$BAD_OLD_NAME" >"$TMP_DIR/old.out" 2>"$TMP_DIR/old.err"; then
  echo "expected old-name scan to fail" >&2
  exit 1
fi
grep -Fq 'forbidden pattern [rca-probe]' "$TMP_DIR/old.err"

printf 'connect to lab-target here\n' >"$BAD_LOCAL_MARKER/docs/local.md"
if "$ROOT_DIR/scripts/security/check-public-tree.sh" --root "$BAD_LOCAL_MARKER" >"$TMP_DIR/local.out" 2>"$TMP_DIR/local.err"; then
  echo "expected local marker scan to fail" >&2
  exit 1
fi
grep -Fq 'forbidden pattern [lab-target]' "$TMP_DIR/local.err"

printf 'private\n' >"$BAD_LOCAL_DOC/docs/01_development_requirements.md"
if "$ROOT_DIR/scripts/security/check-public-tree.sh" --root "$BAD_LOCAL_DOC" >"$TMP_DIR/local-doc.out" 2>"$TMP_DIR/local-doc.err"; then
  echo "expected local-only doc scan to fail" >&2
  exit 1
fi
grep -q 'forbidden path exists: docs/01_development_requirements.md' "$TMP_DIR/local-doc.err"

printf 'How to use Codex with this project\n' >"$BAD_CODEX_DOC/docs/readme.md"
if "$ROOT_DIR/scripts/security/check-public-tree.sh" --root "$BAD_CODEX_DOC" >"$TMP_DIR/codex.out" 2>"$TMP_DIR/codex.err"; then
  echo "expected Codex markdown scan to fail" >&2
  exit 1
fi
grep -q 'forbidden markdown Codex reference' "$TMP_DIR/codex.err"

printf '日本語は禁止\n' >"$BAD_JAPANESE_DOC/docs/readme.md"
if "$ROOT_DIR/scripts/security/check-public-tree.sh" --root "$BAD_JAPANESE_DOC" >"$TMP_DIR/japanese.out" 2>"$TMP_DIR/japanese.err"; then
  echo "expected non-English markdown scan to fail" >&2
  exit 1
fi
grep -q 'forbidden non-English markdown text' "$TMP_DIR/japanese.err"
