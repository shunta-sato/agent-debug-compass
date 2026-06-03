#!/usr/bin/env bash
set -euo pipefail

LISTEN_ADDR="127.0.0.1:39245"
TOKEN_FILE=""
TLS_SERVER_CERT=""
TLS_SERVER_KEY=""
TLS_CLIENT_CA=""
STATE_DIR=""
BINARY="adc-mcp"
UNIT_NAME="adc-mcp-managed.service"
ENABLE=1
GENERATE_TOKEN=0
DRY_RUN=0

usage() {
  cat <<'USAGE'
Usage:
  scripts/install/install-managed-mcp-user-service.sh [options]

Options:
  --listen ADDR:PORT     Managed MCP listen address. Default: 127.0.0.1:39245
  --token-file PATH      Bearer token file. Default: XDG_DATA_HOME/agent-debug-compass/managed-mcp.token
  --tls-server-cert PATH Server certificate PEM for managed mTLS.
  --tls-server-key PATH  Server private key PEM for managed mTLS.
  --tls-client-ca PATH   CA certificate PEM used to verify controller client certs.
  --generate-token       Create the token file with a random token when missing.
  --state-dir PATH       ADC_HOME for the target listener. Default: XDG_DATA_HOME/agent-debug-compass/state
  --binary PATH          adc-mcp binary path or PATH command. Default: adc-mcp
  --unit-name NAME       systemd user unit name. Default: adc-mcp-managed.service
  --no-enable            Write the unit and daemon-reload, but do not enable/start it.
  --dry-run              Print the unit file and planned paths without writing or running systemctl.
  -h, --help             Show this help.

Installs a rootless systemd user service for:
  adc-mcp --target-mode --managed-listen ... --managed-token-file ...

The service exposes only target-local obs.* MCP tools and does not add any shell tool.
USAGE
}

log() {
  printf '[install-managed-mcp-user-service.sh] %s\n' "$*" >&2
}

die() {
  printf '[install-managed-mcp-user-service.sh] ERROR: %s\n' "$*" >&2
  exit 1
}

data_home() {
  if [[ -n "${XDG_DATA_HOME:-}" ]]; then
    printf '%s' "$XDG_DATA_HOME"
  else
    printf '%s/.local/share' "$HOME"
  fi
}

config_home() {
  if [[ -n "${XDG_CONFIG_HOME:-}" ]]; then
    printf '%s' "$XDG_CONFIG_HOME"
  else
    printf '%s/.config' "$HOME"
  fi
}

validate_plain() {
  local label="$1"
  local value="$2"
  if [[ -z "$value" || "$value" == -* || "$value" =~ [[:space:]\"\'\`\$\;\|] ]]; then
    die "$label must be a plain value without whitespace or shell metacharacters"
  fi
}

validate_unit_name() {
  local value="$1"
  validate_plain "unit-name" "$value"
  if [[ "$value" == */* || "$value" != *.service ]]; then
    die "unit-name must be a simple .service file name"
  fi
}

absolute_path() {
  local value="$1"
  if [[ "$value" == /* ]]; then
    printf '%s' "$value"
  else
    printf '%s/%s' "$PWD" "$value"
  fi
}

resolve_binary() {
  local value="$1"
  validate_plain "binary" "$value"
  if [[ "$value" == */* ]]; then
    absolute_path "$value"
    return
  fi
  command -v "$value" || die "binary not found on PATH: $value"
}

generate_token() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 32
  else
    od -An -tx1 -N32 /dev/urandom | tr -d ' \n'
    printf '\n'
  fi
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --listen)
        LISTEN_ADDR="${2:?missing --listen value}"
        shift 2
        ;;
      --token-file)
        TOKEN_FILE="${2:?missing --token-file value}"
        shift 2
        ;;
      --tls-server-cert)
        TLS_SERVER_CERT="${2:?missing --tls-server-cert value}"
        shift 2
        ;;
      --tls-server-key)
        TLS_SERVER_KEY="${2:?missing --tls-server-key value}"
        shift 2
        ;;
      --tls-client-ca)
        TLS_CLIENT_CA="${2:?missing --tls-client-ca value}"
        shift 2
        ;;
      --generate-token)
        GENERATE_TOKEN=1
        shift
        ;;
      --state-dir)
        STATE_DIR="${2:?missing --state-dir value}"
        shift 2
        ;;
      --binary)
        BINARY="${2:?missing --binary value}"
        shift 2
        ;;
      --unit-name)
        UNIT_NAME="${2:?missing --unit-name value}"
        shift 2
        ;;
      --no-enable)
        ENABLE=0
        shift
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

write_unit() {
  local unit_path="$1"
  local binary_path="$2"
  local token_path="$3"
  local state_path="$4"
  local tls_args="$5"
  cat >"$unit_path" <<UNIT
[Unit]
Description=Agent Debug Compass managed MCP target listener
Documentation=file:${PWD}/docs/04_target_setup.md
After=network-online.target

[Service]
Type=simple
ExecStart=${binary_path} --target-mode --managed-listen ${LISTEN_ADDR} --managed-token-file ${token_path}${tls_args}
Restart=on-failure
RestartSec=5s
Environment=ADC_HOME=${state_path}
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict

[Install]
WantedBy=default.target
UNIT
}

main() {
  parse_args "$@"
  [[ "$LISTEN_ADDR" == *:* ]] || die "listen must include host:port"
  validate_plain "listen" "$LISTEN_ADDR"
  validate_unit_name "$UNIT_NAME"
  local tls_enabled=0
  if [[ -n "$TLS_SERVER_CERT" || -n "$TLS_SERVER_KEY" || -n "$TLS_CLIENT_CA" ]]; then
    tls_enabled=1
    [[ -n "$TLS_SERVER_CERT" && -n "$TLS_SERVER_KEY" && -n "$TLS_CLIENT_CA" ]] || die "managed mTLS requires --tls-server-cert, --tls-server-key, and --tls-client-ca together"
    validate_plain "tls-server-cert" "$TLS_SERVER_CERT"
    validate_plain "tls-server-key" "$TLS_SERVER_KEY"
    validate_plain "tls-client-ca" "$TLS_CLIENT_CA"
  fi

  local default_data_home
  default_data_home="$(data_home)"
  if [[ -z "$TOKEN_FILE" ]]; then
    TOKEN_FILE="$default_data_home/agent-debug-compass/managed-mcp.token"
  fi
  if [[ -z "$STATE_DIR" ]]; then
    STATE_DIR="$default_data_home/agent-debug-compass/state"
  fi
  validate_plain "token-file" "$TOKEN_FILE"
  validate_plain "state-dir" "$STATE_DIR"

  local binary_path token_path state_path unit_dir unit_path tls_args
  binary_path="$(resolve_binary "$BINARY")"
  token_path="$(absolute_path "$TOKEN_FILE")"
  state_path="$(absolute_path "$STATE_DIR")"
  unit_dir="$(config_home)/systemd/user"
  unit_path="$unit_dir/$UNIT_NAME"
  tls_args=""
  if [[ "$tls_enabled" -eq 1 ]]; then
    tls_args=" --managed-tls-server-cert $(absolute_path "$TLS_SERVER_CERT") --managed-tls-server-key $(absolute_path "$TLS_SERVER_KEY") --managed-tls-client-ca $(absolute_path "$TLS_CLIENT_CA")"
  fi

  if [[ "$DRY_RUN" -eq 1 ]]; then
    log "dry-run unit_path=$unit_path"
    log "dry-run token_file=$token_path"
    log "dry-run state_dir=$state_path"
    write_unit /dev/stdout "$binary_path" "$token_path" "$state_path" "$tls_args"
    return
  fi

  install -d -m 0700 "$(dirname "$token_path")" "$state_path" "$unit_dir"
  if [[ "$tls_enabled" -eq 1 ]]; then
    [[ -r "$(absolute_path "$TLS_SERVER_CERT")" ]] || die "tls server cert is not readable"
    [[ -r "$(absolute_path "$TLS_SERVER_KEY")" ]] || die "tls server key is not readable"
    [[ -r "$(absolute_path "$TLS_CLIENT_CA")" ]] || die "tls client CA is not readable"
  fi
  if [[ ! -f "$token_path" ]]; then
    [[ "$GENERATE_TOKEN" -eq 1 ]] || die "token file does not exist; pass --generate-token to create it"
    umask 077
    generate_token >"$token_path"
  fi
  [[ -s "$token_path" ]] || die "token file is empty"
  chmod 0600 "$token_path"

  local tmp_unit
  tmp_unit="$(mktemp "$unit_dir/.${UNIT_NAME}.XXXXXX")"
  write_unit "$tmp_unit" "$binary_path" "$token_path" "$state_path" "$tls_args"
  chmod 0644 "$tmp_unit"
  mv "$tmp_unit" "$unit_path"

  systemctl --user daemon-reload
  if [[ "$ENABLE" -eq 1 ]]; then
    systemctl --user enable "$UNIT_NAME"
    systemctl --user restart "$UNIT_NAME"
  fi

  log "installed $unit_path"
  log "token file: $token_path"
  log "state dir: $state_path"
}

main "$@"
