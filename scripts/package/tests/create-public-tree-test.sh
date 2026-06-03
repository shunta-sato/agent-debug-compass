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

git -C "$OUTPUT_DIR" log --oneline -1 | grep -q "chore: initial Agent Debug Compass public repository"
git -C "$OUTPUT_DIR" config user.name | grep -q "Shunta Sato"
git -C "$OUTPUT_DIR" config user.email | grep -q "shunta.sato@gmail.com"
