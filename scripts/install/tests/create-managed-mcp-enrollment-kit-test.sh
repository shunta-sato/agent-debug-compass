#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

KIT_PATH="$("$ROOT_DIR/scripts/install/create-managed-mcp-enrollment-kit.sh" \
  --kit-dir "$TMP_DIR/kit" \
  --target-id kit-target \
  --host kit-target.local \
  --port 39249 \
  --tag lab \
  --tag kit)"

test "$KIT_PATH" = "$TMP_DIR/kit/controller/enrollment-kit.json"
test -s "$KIT_PATH"
test "$(stat -c '%a' "$KIT_PATH")" = "600"
test -s "$TMP_DIR/kit/controller/managed.token"
test -s "$TMP_DIR/kit/controller/target-ca.pem"
test -s "$TMP_DIR/kit/controller/controller.pem"
test -s "$TMP_DIR/kit/controller/controller.key"
test -s "$TMP_DIR/kit/target/managed.token"
test -s "$TMP_DIR/kit/target/server.pem"
test -s "$TMP_DIR/kit/target/server.key"
test -s "$TMP_DIR/kit/target/controller-ca.pem"
grep -q '"schema_version": "obs.managed_mcp_enrollment_kit.v1"' "$KIT_PATH"
grep -q '"target_id": "kit-target"' "$KIT_PATH"
grep -q '"enrollment_mode": "kit"' "$KIT_PATH"
grep -q '"tls_server_name": "kit-target.local"' "$KIT_PATH"
grep -q '"lab"' "$KIT_PATH"
grep -q -- '--managed-tls-server-cert server.pem' "$TMP_DIR/kit/target/README.txt"
openssl x509 -in "$TMP_DIR/kit/target/server.pem" -noout -ext subjectAltName \
  | grep -q 'DNS:kit-target.local'

IP_KIT_PATH="$("$ROOT_DIR/scripts/install/create-managed-mcp-enrollment-kit.sh" \
  --kit-dir "$TMP_DIR/ip-kit" \
  --target-id ip-kit-target \
  --host 192.0.2.55 \
  --tls-server-name 192.0.2.55)"
test -s "$IP_KIT_PATH"
openssl x509 -in "$TMP_DIR/ip-kit/target/server.pem" -noout -ext subjectAltName \
  | grep -q 'IP Address:192.0.2.55'

if "$ROOT_DIR/scripts/install/create-managed-mcp-enrollment-kit.sh" \
  --kit-dir "$TMP_DIR/bad" \
  --target-id 'bad;id' \
  --host kit-target.local \
  >"$TMP_DIR/bad.stdout" 2>"$TMP_DIR/bad.stderr"; then
  echo "expected unsafe target-id to fail" >&2
  exit 1
fi
grep -q 'target-id must be a plain value' "$TMP_DIR/bad.stderr"

if "$ROOT_DIR/scripts/install/create-managed-mcp-enrollment-kit.sh" \
  --kit-dir "$TMP_DIR/bad-path" \
  --target-id '../bad' \
  --host kit-target.local \
  >"$TMP_DIR/bad-path.stdout" 2>"$TMP_DIR/bad-path.stderr"; then
  echo "expected path-like target-id to fail" >&2
  exit 1
fi
grep -q 'target-id must not contain path separators' "$TMP_DIR/bad-path.stderr"
