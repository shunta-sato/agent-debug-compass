#!/usr/bin/env bash
set -euo pipefail

KIT_DIR=""
TARGET_ID=""
HOST=""
PORT="39245"
TLS_SERVER_NAME=""
TAG_ARGS=()

usage() {
  cat <<'USAGE'
Usage:
  scripts/install/create-managed-mcp-enrollment-kit.sh --kit-dir DIR --target-id ID --host HOST [options]

Options:
  --port PORT             Managed MCP port. Default: 39245
  --tls-server-name NAME  Certificate/server name. Default: HOST
  --tag TAG               Repeatable registry tag.
  -h, --help              Show this help.

Creates a local enrollment kit:
  DIR/controller/enrollment-kit.json
  DIR/controller/managed.token
  DIR/controller/target-ca.pem
  DIR/controller/controller.pem
  DIR/controller/controller.key
  DIR/target/managed.token
  DIR/target/server.pem
  DIR/target/server.key
  DIR/target/controller-ca.pem

Use `adc fleet enroll-kit --kit DIR/controller/enrollment-kit.json` on the
controller. Copy DIR/target/* to the target and start the managed MCP listener
with those files. This script does not use SSH and does not require sudo.
USAGE
}

die() {
  printf '[create-managed-mcp-enrollment-kit.sh] ERROR: %s\n' "$*" >&2
  exit 1
}

json_escape() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//$'\n'/ }"
  printf '%s' "$value"
}

validate_plain() {
  local label="$1"
  local value="$2"
  if [[ -z "$value" || "$value" == -* || "$value" =~ [[:space:]\"\'\`\$\;\|] ]]; then
    die "$label must be a plain value without whitespace or shell metacharacters"
  fi
}

validate_target_id() {
  validate_plain "target-id" "$1"
  if [[ "$1" == */* || "$1" == "." || "$1" == ".." || "$1" == *..* ]]; then
    die "target-id must not contain path separators or traversal segments"
  fi
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --kit-dir)
        KIT_DIR="${2:?missing --kit-dir value}"
        shift 2
        ;;
      --target-id)
        TARGET_ID="${2:?missing --target-id value}"
        shift 2
        ;;
      --host)
        HOST="${2:?missing --host value}"
        shift 2
        ;;
      --port)
        PORT="${2:?missing --port value}"
        shift 2
        ;;
      --tls-server-name)
        TLS_SERVER_NAME="${2:?missing --tls-server-name value}"
        shift 2
        ;;
      --tag)
        TAG_ARGS+=("${2:?missing --tag value}")
        shift 2
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        die "unknown argument: $1"
        ;;
    esac
  done
}

tags_json() {
  local first=1
  printf '['
  for tag in "${TAG_ARGS[@]}"; do
    validate_plain "tag" "$tag"
    if [[ "$first" -eq 0 ]]; then
      printf ', '
    fi
    first=0
    printf '"%s"' "$(json_escape "$tag")"
  done
  printf ']'
}

main() {
  parse_args "$@"
  [[ -n "$KIT_DIR" ]] || die "missing --kit-dir"
  [[ -n "$TARGET_ID" ]] || die "missing --target-id"
  [[ -n "$HOST" ]] || die "missing --host"
  [[ "$PORT" =~ ^[0-9]+$ ]] || die "port must be numeric"
  validate_target_id "$TARGET_ID"
  validate_plain "host" "$HOST"
  TLS_SERVER_NAME="${TLS_SERVER_NAME:-$HOST}"
  validate_plain "tls-server-name" "$TLS_SERVER_NAME"
  command -v openssl >/dev/null 2>&1 || die "openssl is required"

  local controller_dir target_dir work_dir
  install -d -m 0700 "$KIT_DIR"
  KIT_DIR="$(cd "$KIT_DIR" && pwd)"
  controller_dir="$KIT_DIR/controller"
  target_dir="$KIT_DIR/target"
  work_dir="$KIT_DIR/work"
  install -d -m 0700 "$controller_dir" "$target_dir" "$work_dir"

  local ca_cert ca_key server_cert server_key server_csr server_ext client_cert client_key client_csr client_ext token server_san
  ca_cert="$work_dir/ca.pem"
  ca_key="$work_dir/ca.key"
  server_cert="$work_dir/server.pem"
  server_key="$work_dir/server.key"
  server_csr="$work_dir/server.csr"
  server_ext="$work_dir/server.ext"
  client_cert="$work_dir/controller.pem"
  client_key="$work_dir/controller.key"
  client_csr="$work_dir/controller.csr"
  client_ext="$work_dir/controller.ext"
  token="$work_dir/managed.token"

  umask 077
  openssl rand -hex 32 >"$token"
  openssl req -x509 -newkey rsa:2048 -nodes -days 365 \
    -subj "/CN=adc-managed-kit-ca-$TARGET_ID" \
    -addext basicConstraints=critical,CA:TRUE \
    -addext keyUsage=critical,keyCertSign,cRLSign \
    -keyout "$ca_key" -out "$ca_cert" >/dev/null 2>&1
  openssl req -newkey rsa:2048 -nodes \
    -subj "/CN=$TLS_SERVER_NAME" \
    -keyout "$server_key" -out "$server_csr" >/dev/null 2>&1
  server_san="DNS:$TLS_SERVER_NAME"
  if [[ "$TLS_SERVER_NAME" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    server_san="IP:$TLS_SERVER_NAME"
  fi
  printf '%s\n' "subjectAltName=$server_san" 'extendedKeyUsage=serverAuth' >"$server_ext"
  openssl x509 -req -in "$server_csr" -CA "$ca_cert" -CAkey "$ca_key" -CAcreateserial \
    -days 365 -extfile "$server_ext" -out "$server_cert" >/dev/null 2>&1
  openssl req -newkey rsa:2048 -nodes \
    -subj "/CN=adc-controller-$TARGET_ID" \
    -keyout "$client_key" -out "$client_csr" >/dev/null 2>&1
  printf '%s\n' 'extendedKeyUsage=clientAuth' >"$client_ext"
  openssl x509 -req -in "$client_csr" -CA "$ca_cert" -CAkey "$ca_key" -CAcreateserial \
    -days 365 -extfile "$client_ext" -out "$client_cert" >/dev/null 2>&1

  install -m 0600 "$token" "$controller_dir/managed.token"
  install -m 0600 "$ca_cert" "$controller_dir/target-ca.pem"
  install -m 0600 "$client_cert" "$controller_dir/controller.pem"
  install -m 0600 "$client_key" "$controller_dir/controller.key"
  install -m 0600 "$token" "$target_dir/managed.token"
  install -m 0600 "$server_cert" "$target_dir/server.pem"
  install -m 0600 "$server_key" "$target_dir/server.key"
  install -m 0600 "$ca_cert" "$target_dir/controller-ca.pem"

  cat >"$controller_dir/enrollment-kit.json" <<JSON
{
  "schema_version": "obs.managed_mcp_enrollment_kit.v1",
  "target": {
    "target_id": "$(json_escape "$TARGET_ID")",
    "transport": "managed_mcp",
    "host": "$(json_escape "$HOST")",
    "port": $PORT,
    "auth_token_file": "$(json_escape "$controller_dir/managed.token")",
    "tls_ca_file": "$(json_escape "$controller_dir/target-ca.pem")",
    "tls_client_cert_file": "$(json_escape "$controller_dir/controller.pem")",
    "tls_client_key_file": "$(json_escape "$controller_dir/controller.key")",
    "tls_server_name": "$(json_escape "$TLS_SERVER_NAME")",
    "tags": $(tags_json),
    "trust_state": "trusted",
    "enrollment_mode": "kit"
  }
}
JSON
  chmod 0600 "$controller_dir/enrollment-kit.json"

  cat >"$target_dir/README.txt" <<EOF
Copy this directory to the target and start:

adc-mcp --target-mode \\
  --managed-listen 0.0.0.0:$PORT \\
  --managed-token-file managed.token \\
  --managed-tls-server-cert server.pem \\
  --managed-tls-server-key server.key \\
  --managed-tls-client-ca controller-ca.pem
EOF
  printf '%s\n' "$controller_dir/enrollment-kit.json"
}

main "$@"
