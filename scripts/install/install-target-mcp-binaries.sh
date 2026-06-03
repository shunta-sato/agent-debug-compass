#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BINARY_DIR="${ADC_RELEASE_BINARY_DIR:-$ROOT_DIR/target/release}"
REMOTE_DIR="${ADC_TARGET_REMOTE_DIR:-.local/bin}"
RESULT_ROOT="${ADC_TARGET_BOOTSTRAP_RESULT_ROOT:-}"
HOST=""
TARGET_ID=""
USER_NAME=""
PORT=""

usage() {
  cat <<'USAGE'
Usage:
  scripts/install/install-target-mcp-binaries.sh --host HOST --target-id TARGET_ID [options]

Options:
  --binary-dir DIR     Local release binary directory. Default: target/release
  --remote-dir DIR     Remote user-local binary directory. Default: .local/bin
  --user USER          SSH user. Default: current ssh config user
  --port PORT          SSH port.
  --result-root DIR    Write bootstrap_report.json and fleet_preflight.json.
  -h, --help           Show this help.

Copies adc-mcp to the target without sudo, validates it through
MCP-over-SSH fleet preflight, and prints a ready-to-use fleet inventory stanza
with mcp_server_path. It does not handle passwords; SSH must work in BatchMode.
USAGE
}

log() {
  printf '[install-target-mcp-binaries.sh] %s\n' "$*" >&2
}

die() {
  printf '[install-target-mcp-binaries.sh] ERROR: %s\n' "$*" >&2
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
    die "$label must be a plain value without shell metacharacters"
  fi
}

validate_target_id() {
  local value="$1"
  if [[ -z "$value" || "$value" == *"/"* || "$value" == *".."* || "$value" =~ [[:space:]\"\'\`\$\;\|] ]]; then
    die "target-id must be a single safe path segment"
  fi
}

quote_remote() {
  local value="$1"
  printf "'%s'" "${value//\'/\'\\\'\'}"
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --host)
        HOST="${2:?missing --host value}"
        shift 2
        ;;
      --target-id)
        TARGET_ID="${2:?missing --target-id value}"
        shift 2
        ;;
      --binary-dir)
        BINARY_DIR="${2:?missing --binary-dir value}"
        shift 2
        ;;
      --remote-dir)
        REMOTE_DIR="${2:?missing --remote-dir value}"
        shift 2
        ;;
      --user)
        USER_NAME="${2:?missing --user value}"
        shift 2
        ;;
      --port)
        PORT="${2:?missing --port value}"
        shift 2
        ;;
      --result-root)
        RESULT_ROOT="${2:?missing --result-root value}"
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

require_inputs() {
  [[ -n "$HOST" ]] || die "missing required --host"
  [[ -n "$TARGET_ID" ]] || die "missing required --target-id"
  validate_plain "host" "$HOST"
  validate_target_id "$TARGET_ID"
  validate_plain "remote-dir" "$REMOTE_DIR"
  if [[ -n "$USER_NAME" ]]; then
    validate_plain "user" "$USER_NAME"
  fi
  if [[ -n "$PORT" && ! "$PORT" =~ ^[0-9]+$ ]]; then
    die "port must be numeric"
  fi
  for bin in adc adc-mcp; do
    [[ -x "$BINARY_DIR/$bin" ]] || die "missing local binary: $BINARY_DIR/$bin"
  done
}

ssh_destination() {
  if [[ -n "$USER_NAME" ]]; then
    printf '%s@%s' "$USER_NAME" "$HOST"
  else
    printf '%s' "$HOST"
  fi
}

ssh_base_args() {
  printf '%s\n' "-o" "BatchMode=yes" "-o" "ConnectTimeout=5" "-o" "StrictHostKeyChecking=accept-new"
  if [[ -n "$PORT" ]]; then
    printf '%s\n' "-p" "$PORT"
  fi
}

scp_base_args() {
  printf '%s\n' "-q" "-o" "BatchMode=yes" "-o" "ConnectTimeout=5" "-o" "StrictHostKeyChecking=accept-new"
  if [[ -n "$PORT" ]]; then
    printf '%s\n' "-P" "$PORT"
  fi
}

main() {
  parse_args "$@"
  require_inputs

  local destination
  destination="$(ssh_destination)"
  mapfile -t ssh_args < <(ssh_base_args)
  mapfile -t scp_args < <(scp_base_args)

  if [[ -n "$RESULT_ROOT" ]]; then
    mkdir -p "$RESULT_ROOT"
  fi

  log "probing remote home for $destination"
  local remote_home
  remote_home="$(ssh "${ssh_args[@]}" "$destination" 'printf %s "$HOME"')"
  [[ -n "$remote_home" ]] || die "remote HOME probe returned empty output"
  validate_plain "remote HOME" "$remote_home"

  local remote_abs_dir
  if [[ "$REMOTE_DIR" == /* ]]; then
    remote_abs_dir="$REMOTE_DIR"
  else
    remote_abs_dir="$remote_home/$REMOTE_DIR"
  fi
  local remote_mcp_server_path="$remote_abs_dir/adc-mcp"

  log "installing target MCP server to $destination:$remote_abs_dir"
  ssh "${ssh_args[@]}" "$destination" "mkdir -p -- $(quote_remote "$remote_abs_dir")"
  scp "${scp_args[@]}" \
    "$BINARY_DIR/adc-mcp" \
    "$destination:$remote_abs_dir/"
  ssh "${ssh_args[@]}" "$destination" "chmod 0755 -- $(quote_remote "$remote_mcp_server_path")"

  local preflight_inventory
  local preflight_state
  if [[ -n "$RESULT_ROOT" ]]; then
    preflight_inventory="$RESULT_ROOT/targets.yaml"
    preflight_state="$RESULT_ROOT/state"
    mkdir -p "$preflight_state"
  else
    preflight_inventory="$(mktemp)"
    preflight_state="$(mktemp -d)"
  fi
  write_inventory "$preflight_inventory" "$HOST" "$TARGET_ID" "$remote_mcp_server_path"

  local preflight_json
  preflight_json="$(ADC_HOME="$preflight_state" "$BINARY_DIR/adc" fleet preflight --inventory "$preflight_inventory")"
  grep -q '"status": "ready"' <<<"$preflight_json" || {
    if [[ -n "$RESULT_ROOT" ]]; then
      printf '%s\n' "$preflight_json" >"$RESULT_ROOT/fleet_preflight.json"
    fi
    die "target MCP fleet preflight did not report ready"
  }

  if [[ -n "$RESULT_ROOT" ]]; then
    printf '%s\n' "$preflight_json" >"$RESULT_ROOT/fleet_preflight.json"
    cat >"$RESULT_ROOT/bootstrap_report.json" <<JSON
{
  "schema_version": "obs.target_bootstrap.v1",
  "target_id": "$(json_escape "$TARGET_ID")",
  "host": "$(json_escape "$HOST")",
  "ssh_destination": "$(json_escape "$destination")",
  "remote_dir": "$(json_escape "$remote_abs_dir")",
  "mcp_server_path": "$(json_escape "$remote_mcp_server_path")",
  "transport": "mcp_stdio_over_ssh",
  "status": "ready",
  "root_required": false
}
JSON
  fi

  write_inventory /dev/stdout "$HOST" "$TARGET_ID" "$remote_mcp_server_path"
}

write_inventory() {
  local output_path="$1"
  local host="$2"
  local target_id="$3"
  local mcp_server_path="$4"
  {
  cat <<YAML
targets:
  - id: $target_id
    transport: mcp_stdio_over_ssh
    host: $host
YAML
  if [[ -n "$USER_NAME" ]]; then
    printf '    user: %s\n' "$USER_NAME"
  fi
  if [[ -n "$PORT" ]]; then
    printf '    port: %s\n' "$PORT"
  fi
  printf '    mcp_server_path: %s\n' "$mcp_server_path"
  } >"$output_path"
}

main "$@"
