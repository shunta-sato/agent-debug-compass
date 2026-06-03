#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BINARY_DIR="${ADC_RELEASE_BINARY_DIR:-$ROOT_DIR/target/release}"
SSH_HOST=""
SSH_USER=""
SSH_PORT=""
SSH_STRICT_HOST_KEY_CHECKING="accept-new"
SSH_KNOWN_HOSTS_FILE=""
MANAGED_HOST=""
TARGET_ID=""
KIT_DIR=""
REMOTE_BIN_DIR=".local/bin"
REMOTE_CONFIG_DIR=".local/share/agent-debug-compass/managed-mcp"
REMOTE_STATE_DIR=".local/share/agent-debug-compass/state"
LISTEN_ADDR="0.0.0.0:39245"
MANAGED_PORT="39245"
TLS_SERVER_NAME=""
UNIT_NAME="adc-mcp-managed.service"
RESULT_ROOT=""
DRY_RUN=0
NO_PREFLIGHT=0
PREFLIGHT_ATTEMPTS=10
PREFLIGHT_SLEEP_SEC="0.5"
TAG_ARGS=()

usage() {
  cat <<'USAGE'
Usage:
  scripts/install/provision-managed-mcp-target.sh --ssh-host HOST --managed-host HOST --target-id ID [options]

Options:
  --ssh-host HOST        SSH host or alias used only for provisioning.
  --ssh-user USER        SSH user. Default: current ssh config user.
  --ssh-port PORT        SSH port.
  --ssh-strict-host-key-checking MODE
                         OpenSSH StrictHostKeyChecking mode: yes, accept-new, or no. Default: accept-new.
  --ssh-known-hosts-file PATH
                         Optional OpenSSH UserKnownHostsFile path.
  --managed-host HOST    TCP-reachable managed_mcp host/IP for steady-state observation.
  --target-id ID         Managed fleet target ID.
  --binary-dir DIR       Local release binary dir. Default: target/release.
  --kit-dir DIR          Local enrollment kit dir. Default: XDG_DATA_HOME/agent-debug-compass/enrollment-kits/ID.
  --remote-bin-dir DIR   Remote user-local binary dir. Default: .local/bin.
  --remote-config-dir DIR Remote managed MCP config dir. Default: .local/share/agent-debug-compass/managed-mcp.
  --remote-state-dir DIR Remote ADC_HOME. Default: .local/share/agent-debug-compass/state.
  --listen ADDR:PORT     Target listener address. Default: 0.0.0.0:39245.
  --managed-port PORT    Controller TCP port for managed_mcp. Default: 39245.
  --tls-server-name NAME Certificate/server name. Default: managed host.
  --tag TAG              Repeatable registry tag.
  --unit-name NAME       systemd user unit name. Default: adc-mcp-managed.service.
  --result-root DIR      Write enroll.json, preflight.json, and provision_report.json.
  --no-preflight         Skip final managed_mcp preflight.
  --preflight-attempts N Retry managed_mcp preflight N times. Default: 10.
  --preflight-sleep-sec S Sleep seconds between preflight attempts. Default: 0.5.
  --dry-run              Print a non-secret action plan without SSH/SCP/registry writes.
  -h, --help             Show this help.

This script uses SSH only as a guarded provisioning carrier. Steady-state
observation uses authenticated managed_mcp with bearer token plus mTLS.
USAGE
}

log() {
  printf '[provision-managed-mcp-target.sh] %s\n' "$*" >&2
}

die() {
  printf '[provision-managed-mcp-target.sh] ERROR: %s\n' "$*" >&2
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
    die "target-id must be a single safe path segment"
  fi
}

validate_unit_name() {
  validate_plain "unit-name" "$1"
  if [[ "$1" == */* || "$1" != *.service ]]; then
    die "unit-name must be a simple .service file name"
  fi
}

quote_remote() {
  local value="$1"
  printf "'%s'" "${value//\'/\'\\\'\'}"
}

data_home() {
  if [[ -n "${XDG_DATA_HOME:-}" ]]; then
    printf '%s' "$XDG_DATA_HOME"
  else
    printf '%s/.local/share' "$HOME"
  fi
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --ssh-host|--host)
        SSH_HOST="${2:?missing --ssh-host value}"
        shift 2
        ;;
      --ssh-user|--user)
        SSH_USER="${2:?missing --ssh-user value}"
        shift 2
        ;;
      --ssh-port)
        SSH_PORT="${2:?missing --ssh-port value}"
        shift 2
        ;;
      --ssh-strict-host-key-checking)
        SSH_STRICT_HOST_KEY_CHECKING="${2:?missing --ssh-strict-host-key-checking value}"
        shift 2
        ;;
      --ssh-known-hosts-file)
        SSH_KNOWN_HOSTS_FILE="${2:?missing --ssh-known-hosts-file value}"
        shift 2
        ;;
      --managed-host)
        MANAGED_HOST="${2:?missing --managed-host value}"
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
      --kit-dir)
        KIT_DIR="${2:?missing --kit-dir value}"
        shift 2
        ;;
      --remote-bin-dir)
        REMOTE_BIN_DIR="${2:?missing --remote-bin-dir value}"
        shift 2
        ;;
      --remote-config-dir)
        REMOTE_CONFIG_DIR="${2:?missing --remote-config-dir value}"
        shift 2
        ;;
      --remote-state-dir)
        REMOTE_STATE_DIR="${2:?missing --remote-state-dir value}"
        shift 2
        ;;
      --listen)
        LISTEN_ADDR="${2:?missing --listen value}"
        shift 2
        ;;
      --managed-port)
        MANAGED_PORT="${2:?missing --managed-port value}"
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
      --unit-name)
        UNIT_NAME="${2:?missing --unit-name value}"
        shift 2
        ;;
      --result-root)
        RESULT_ROOT="${2:?missing --result-root value}"
        shift 2
        ;;
      --no-preflight)
        NO_PREFLIGHT=1
        shift
        ;;
      --preflight-attempts)
        PREFLIGHT_ATTEMPTS="${2:?missing --preflight-attempts value}"
        shift 2
        ;;
      --preflight-sleep-sec)
        PREFLIGHT_SLEEP_SEC="${2:?missing --preflight-sleep-sec value}"
        shift 2
        ;;
      --dry-run)
        DRY_RUN=1
        shift
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
  [[ -n "$SSH_HOST" ]] || die "missing required --ssh-host"
  [[ -n "$MANAGED_HOST" ]] || die "missing required --managed-host"
  [[ -n "$TARGET_ID" ]] || die "missing required --target-id"
  [[ "$LISTEN_ADDR" == *:* ]] || die "listen must include host:port"
  [[ "$MANAGED_PORT" =~ ^[0-9]+$ ]] || die "managed-port must be numeric"
  [[ "$PREFLIGHT_ATTEMPTS" =~ ^[0-9]+$ && "$PREFLIGHT_ATTEMPTS" -ge 1 ]] || die "preflight-attempts must be a positive integer"
  [[ "$PREFLIGHT_SLEEP_SEC" =~ ^[0-9]+([.][0-9]+)?$ ]] || die "preflight-sleep-sec must be a non-negative number"
  if [[ -n "$SSH_PORT" && ! "$SSH_PORT" =~ ^[0-9]+$ ]]; then
    die "ssh-port must be numeric"
  fi
  case "$SSH_STRICT_HOST_KEY_CHECKING" in
    yes|accept-new|no) ;;
    *) die "ssh-strict-host-key-checking must be yes, accept-new, or no" ;;
  esac
  validate_plain "ssh-host" "$SSH_HOST"
  if [[ -n "$SSH_KNOWN_HOSTS_FILE" ]]; then
    validate_plain "ssh-known-hosts-file" "$SSH_KNOWN_HOSTS_FILE"
  fi
  validate_plain "managed-host" "$MANAGED_HOST"
  validate_target_id "$TARGET_ID"
  validate_plain "binary-dir" "$BINARY_DIR"
  validate_plain "remote-bin-dir" "$REMOTE_BIN_DIR"
  validate_plain "remote-config-dir" "$REMOTE_CONFIG_DIR"
  validate_plain "remote-state-dir" "$REMOTE_STATE_DIR"
  validate_plain "listen" "$LISTEN_ADDR"
  TLS_SERVER_NAME="${TLS_SERVER_NAME:-$MANAGED_HOST}"
  validate_plain "tls-server-name" "$TLS_SERVER_NAME"
  validate_unit_name "$UNIT_NAME"
  if [[ -n "$SSH_USER" ]]; then
    validate_plain "ssh-user" "$SSH_USER"
  fi
  for tag in "${TAG_ARGS[@]}"; do
    validate_plain "tag" "$tag"
  done
  for bin in adc adc-mcp; do
    [[ -x "$BINARY_DIR/$bin" ]] || die "missing local binary: $BINARY_DIR/$bin"
  done
  [[ -x "$ROOT_DIR/scripts/install/create-managed-mcp-enrollment-kit.sh" ]] || die "missing enrollment kit generator"
  [[ -x "$ROOT_DIR/scripts/install/install-managed-mcp-user-service.sh" ]] || die "missing managed MCP service installer"
}

ssh_destination() {
  if [[ -n "$SSH_USER" ]]; then
    printf '%s@%s' "$SSH_USER" "$SSH_HOST"
  else
    printf '%s' "$SSH_HOST"
  fi
}

ssh_base_args() {
  printf '%s\n' "-o" "BatchMode=yes" "-o" "ConnectTimeout=5" "-o" "StrictHostKeyChecking=$SSH_STRICT_HOST_KEY_CHECKING"
  if [[ -n "$SSH_KNOWN_HOSTS_FILE" ]]; then
    printf '%s\n' "-o" "UserKnownHostsFile=$SSH_KNOWN_HOSTS_FILE"
  fi
  if [[ -n "$SSH_PORT" ]]; then
    printf '%s\n' "-p" "$SSH_PORT"
  fi
}

scp_base_args() {
  printf '%s\n' "-q" "-o" "BatchMode=yes" "-o" "ConnectTimeout=5" "-o" "StrictHostKeyChecking=$SSH_STRICT_HOST_KEY_CHECKING"
  if [[ -n "$SSH_KNOWN_HOSTS_FILE" ]]; then
    printf '%s\n' "-o" "UserKnownHostsFile=$SSH_KNOWN_HOSTS_FILE"
  fi
  if [[ -n "$SSH_PORT" ]]; then
    printf '%s\n' "-P" "$SSH_PORT"
  fi
}

remote_abs() {
  local home="$1"
  local value="$2"
  if [[ "$value" == /* ]]; then
    printf '%s' "$value"
  else
    printf '%s/%s' "$home" "$value"
  fi
}

run_adc() {
  if [[ -n "${ADC_HOME:-}" ]]; then
    ADC_HOME="$ADC_HOME" "$BINARY_DIR/adc" "$@"
  else
    "$BINARY_DIR/adc" "$@"
  fi
}

scan_host_fingerprint() {
  local host="$1"
  command -v ssh-keyscan >/dev/null 2>&1 || return 0
  command -v ssh-keygen >/dev/null 2>&1 || return 0
  local scan_args=("-T" "5")
  if [[ -n "$SSH_PORT" ]]; then
    scan_args+=("-p" "$SSH_PORT")
  fi
  ssh-keyscan "${scan_args[@]}" "$host" 2>/dev/null \
    | ssh-keygen -lf - 2>/dev/null \
    | head -n 1 || true
}

ssh_host_fingerprint() {
  local fingerprint
  fingerprint="$(scan_host_fingerprint "$SSH_HOST")"
  if [[ -z "$fingerprint" && "$MANAGED_HOST" != "$SSH_HOST" ]]; then
    fingerprint="$(scan_host_fingerprint "$MANAGED_HOST")"
  fi
  if [[ -z "$fingerprint" ]]; then
    printf 'unavailable: ssh-keyscan returned no host key for ssh_host=%s managed_host=%s' "$SSH_HOST" "$MANAGED_HOST"
  else
    printf '%s' "$fingerprint"
  fi
}

write_report() {
  local output_path="$1"
  local status="$2"
  local remote_home="$3"
  local remote_bin_abs="$4"
  local remote_config_abs="$5"
  local remote_state_abs="$6"
  local fingerprint="$7"
  cat >"$output_path" <<JSON
{
  "schema_version": "obs.managed_mcp_provision.v1",
  "target_id": "$(json_escape "$TARGET_ID")",
  "ssh_destination": "$(json_escape "$(ssh_destination)")",
  "managed_host": "$(json_escape "$MANAGED_HOST")",
  "managed_port": $MANAGED_PORT,
  "listen": "$(json_escape "$LISTEN_ADDR")",
  "transport": "managed_mcp",
  "enrollment_mode": "kit",
  "status": "$(json_escape "$status")",
  "root_required": false,
  "ssh_strict_host_key_checking": "$(json_escape "$SSH_STRICT_HOST_KEY_CHECKING")",
  "ssh_known_hosts_file": "$(json_escape "$SSH_KNOWN_HOSTS_FILE")",
  "ssh_host_fingerprint": "$(json_escape "$fingerprint")",
  "remote_home": "$(json_escape "$remote_home")",
  "remote_binary": "$(json_escape "$remote_bin_abs/adc-mcp")",
  "remote_config_dir": "$(json_escape "$remote_config_abs")",
  "remote_state_dir": "$(json_escape "$remote_state_abs")",
  "unit_name": "$(json_escape "$UNIT_NAME")"
}
JSON
}

main() {
  parse_args "$@"
  require_inputs

  if [[ -z "$KIT_DIR" ]]; then
    KIT_DIR="$(data_home)/adc-targetd/enrollment-kits/$TARGET_ID"
  fi
  validate_plain "kit-dir" "$KIT_DIR"

  local destination
  destination="$(ssh_destination)"
  mapfile -t ssh_args < <(ssh_base_args)
  mapfile -t scp_args < <(scp_base_args)

  if [[ "$DRY_RUN" -eq 1 ]]; then
    cat <<JSON
{
  "schema_version": "obs.managed_mcp_provision_plan.v1",
  "dry_run": true,
  "target_id": "$(json_escape "$TARGET_ID")",
  "ssh_destination": "$(json_escape "$destination")",
  "managed_host": "$(json_escape "$MANAGED_HOST")",
  "managed_port": $MANAGED_PORT,
  "listen": "$(json_escape "$LISTEN_ADDR")",
  "transport": "managed_mcp",
  "root_required": false,
  "ssh_strict_host_key_checking": "$(json_escape "$SSH_STRICT_HOST_KEY_CHECKING")",
  "ssh_known_hosts_file": "$(json_escape "$SSH_KNOWN_HOSTS_FILE")",
  "kit_dir": "$(json_escape "$KIT_DIR")",
  "remote_bin_dir": "$(json_escape "$REMOTE_BIN_DIR")",
  "remote_config_dir": "$(json_escape "$REMOTE_CONFIG_DIR")",
  "remote_state_dir": "$(json_escape "$REMOTE_STATE_DIR")",
  "unit_name": "$(json_escape "$UNIT_NAME")"
}
JSON
    return
  fi

  if [[ -n "$RESULT_ROOT" ]]; then
    install -d -m 0700 "$RESULT_ROOT"
  fi
  if [[ -n "$SSH_KNOWN_HOSTS_FILE" ]]; then
    install -d -m 0700 "$(dirname "$SSH_KNOWN_HOSTS_FILE")"
  fi

  local tag_flags=()
  for tag in "${TAG_ARGS[@]}"; do
    tag_flags+=("--tag" "$tag")
  done

  log "creating enrollment kit for $TARGET_ID"
  local kit_path
  kit_path="$("$ROOT_DIR/scripts/install/create-managed-mcp-enrollment-kit.sh" \
    --kit-dir "$KIT_DIR" \
    --target-id "$TARGET_ID" \
    --host "$MANAGED_HOST" \
    --port "$MANAGED_PORT" \
    --tls-server-name "$TLS_SERVER_NAME" \
    "${tag_flags[@]}")"

  log "enrolling controller registry from kit"
  local enroll_json
  enroll_json="$(run_adc fleet enroll-kit --kit "$kit_path")"
  if [[ -n "$RESULT_ROOT" ]]; then
    printf '%s\n' "$enroll_json" >"$RESULT_ROOT/enroll.json"
  fi

  log "probing remote home for $destination"
  local fingerprint
  fingerprint="$(ssh_host_fingerprint)"
  if [[ -n "$RESULT_ROOT" ]]; then
    printf '%s\n' "$fingerprint" >"$RESULT_ROOT/ssh_host_fingerprint.txt"
  fi
  local remote_home
  remote_home="$(ssh "${ssh_args[@]}" "$destination" 'printf %s "$HOME"')"
  [[ -n "$remote_home" ]] || die "remote HOME probe returned empty output"
  validate_plain "remote HOME" "$remote_home"

  local remote_bin_abs remote_config_abs remote_state_abs remote_mcp_tmp
  remote_bin_abs="$(remote_abs "$remote_home" "$REMOTE_BIN_DIR")"
  remote_config_abs="$(remote_abs "$remote_home" "$REMOTE_CONFIG_DIR")"
  remote_state_abs="$(remote_abs "$remote_home" "$REMOTE_STATE_DIR")"
  remote_mcp_tmp="$remote_bin_abs/adc-mcp.$TARGET_ID.tmp"

  log "copying managed MCP files to $destination"
  ssh "${ssh_args[@]}" "$destination" \
    "mkdir -p -- $(quote_remote "$remote_bin_abs") $(quote_remote "$remote_config_abs") $(quote_remote "$remote_state_abs")"
  scp "${scp_args[@]}" \
    "$BINARY_DIR/adc-mcp" \
    "$destination:$remote_mcp_tmp"
  scp "${scp_args[@]}" \
    "$ROOT_DIR/scripts/install/install-managed-mcp-user-service.sh" \
    "$KIT_DIR/target/managed.token" \
    "$KIT_DIR/target/server.pem" \
    "$KIT_DIR/target/server.key" \
    "$KIT_DIR/target/controller-ca.pem" \
    "$destination:$remote_config_abs/"
  ssh "${ssh_args[@]}" "$destination" \
    "chmod 0755 -- $(quote_remote "$remote_mcp_tmp") && mv -f -- $(quote_remote "$remote_mcp_tmp") $(quote_remote "$remote_bin_abs/adc-mcp") && chmod 0700 -- $(quote_remote "$remote_config_abs") && chmod 0755 -- $(quote_remote "$remote_bin_abs/adc-mcp") $(quote_remote "$remote_config_abs/install-managed-mcp-user-service.sh") && chmod 0600 -- $(quote_remote "$remote_config_abs/managed.token") $(quote_remote "$remote_config_abs/server.pem") $(quote_remote "$remote_config_abs/server.key") $(quote_remote "$remote_config_abs/controller-ca.pem")"

  log "installing rootless managed MCP user service"
  ssh "${ssh_args[@]}" "$destination" \
    "$(quote_remote "$remote_config_abs/install-managed-mcp-user-service.sh") --listen $(quote_remote "$LISTEN_ADDR") --token-file $(quote_remote "$remote_config_abs/managed.token") --tls-server-cert $(quote_remote "$remote_config_abs/server.pem") --tls-server-key $(quote_remote "$remote_config_abs/server.key") --tls-client-ca $(quote_remote "$remote_config_abs/controller-ca.pem") --binary $(quote_remote "$remote_bin_abs/adc-mcp") --state-dir $(quote_remote "$remote_state_abs") --unit-name $(quote_remote "$UNIT_NAME")"

  local preflight_json=""
  if [[ "$NO_PREFLIGHT" -eq 0 ]]; then
    log "running managed MCP preflight"
    local attempt
    for ((attempt = 1; attempt <= PREFLIGHT_ATTEMPTS; attempt++)); do
      preflight_json="$(run_adc fleet preflight --selector "target=$TARGET_ID")"
      if grep -q '"status": "ready"' <<<"$preflight_json"; then
        break
      fi
      log "managed MCP preflight not ready attempt=$attempt/$PREFLIGHT_ATTEMPTS"
      if [[ "$attempt" -lt "$PREFLIGHT_ATTEMPTS" ]]; then
        sleep "$PREFLIGHT_SLEEP_SEC"
      fi
    done
    if ! grep -q '"status": "ready"' <<<"$preflight_json"; then
      if [[ -n "$RESULT_ROOT" ]]; then
        printf '%s\n' "$preflight_json" >"$RESULT_ROOT/preflight.json"
      fi
      die "managed MCP preflight did not report ready"
    fi
    if [[ -n "$RESULT_ROOT" ]]; then
      printf '%s\n' "$preflight_json" >"$RESULT_ROOT/preflight.json"
    fi
  fi

  local report_tmp
  report_tmp="$(mktemp)"
  write_report "$report_tmp" "ready" "$remote_home" "$remote_bin_abs" "$remote_config_abs" "$remote_state_abs" "$fingerprint"
  if [[ -n "$RESULT_ROOT" ]]; then
    install -m 0600 "$report_tmp" "$RESULT_ROOT/provision_report.json"
  fi
  cat "$report_tmp"
  rm -f "$report_tmp"
}

main "$@"
