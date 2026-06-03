#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

FAKE_BIN="$TMP_DIR/fake-bin"
HOME_DIR="$TMP_DIR/home"
XDG_CONFIG_HOME="$TMP_DIR/config"
XDG_DATA_HOME="$TMP_DIR/data"
mkdir -p "$FAKE_BIN" "$HOME_DIR" "$XDG_CONFIG_HOME" "$XDG_DATA_HOME"

cat >"$FAKE_BIN/adc-mcp" <<'BIN'
#!/usr/bin/env bash
exit 0
BIN
chmod +x "$FAKE_BIN/adc-mcp"

cat >"$FAKE_BIN/systemctl" <<'SYSTEMCTL'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$FAKE_SYSTEMCTL_LOG"
SYSTEMCTL
chmod +x "$FAKE_BIN/systemctl"

touch "$TMP_DIR/server.pem" "$TMP_DIR/server.key" "$TMP_DIR/ca.pem"

FAKE_SYSTEMCTL_LOG="$TMP_DIR/systemctl.log" \
HOME="$HOME_DIR" \
XDG_CONFIG_HOME="$XDG_CONFIG_HOME" \
XDG_DATA_HOME="$XDG_DATA_HOME" \
PATH="$FAKE_BIN:$PATH" \
  "$ROOT_DIR/scripts/install/install-managed-mcp-user-service.sh" \
  --listen 127.0.0.1:39246 \
  --generate-token \
  --tls-server-cert "$TMP_DIR/server.pem" \
  --tls-server-key "$TMP_DIR/server.key" \
  --tls-client-ca "$TMP_DIR/ca.pem" \
  --state-dir "$XDG_DATA_HOME/agent-debug-compass/state" \
  --unit-name adc-mcp-managed-test.service

UNIT="$XDG_CONFIG_HOME/systemd/user/adc-mcp-managed-test.service"
TOKEN="$XDG_DATA_HOME/agent-debug-compass/managed-mcp.token"

test -s "$TOKEN"
test "$(stat -c '%a' "$TOKEN")" = "600"
grep -q 'daemon-reload' "$TMP_DIR/systemctl.log"
grep -q 'enable adc-mcp-managed-test.service' "$TMP_DIR/systemctl.log"
grep -q 'restart adc-mcp-managed-test.service' "$TMP_DIR/systemctl.log"
grep -q 'Description=Agent Debug Compass managed MCP target listener' "$UNIT"
grep -q 'ExecStart=.*/adc-mcp --target-mode --managed-listen 127.0.0.1:39246 --managed-token-file '"$TOKEN" "$UNIT"
grep -q -- '--managed-tls-server-cert '"$TMP_DIR"'/server.pem' "$UNIT"
grep -q -- '--managed-tls-server-key '"$TMP_DIR"'/server.key' "$UNIT"
grep -q -- '--managed-tls-client-ca '"$TMP_DIR"'/ca.pem' "$UNIT"
grep -q 'Restart=on-failure' "$UNIT"
grep -q 'NoNewPrivileges=true' "$UNIT"
grep -q 'Environment=ADC_HOME='"$XDG_DATA_HOME"'/agent-debug-compass/state' "$UNIT"

if HOME="$HOME_DIR" \
  XDG_CONFIG_HOME="$XDG_CONFIG_HOME" \
  XDG_DATA_HOME="$XDG_DATA_HOME" \
  PATH="$FAKE_BIN:$PATH" \
    "$ROOT_DIR/scripts/install/install-managed-mcp-user-service.sh" \
    --listen '127.0.0.1:39246;rm' \
    --no-enable \
    >"$TMP_DIR/bad.stdout" 2>"$TMP_DIR/bad.stderr"; then
  echo "expected unsafe listen value to fail" >&2
  exit 1
fi
grep -q 'listen must be a plain value' "$TMP_DIR/bad.stderr"

DRY_RUN_OUTPUT="$TMP_DIR/dry-run.unit"
HOME="$HOME_DIR" \
XDG_CONFIG_HOME="$XDG_CONFIG_HOME" \
XDG_DATA_HOME="$XDG_DATA_HOME" \
PATH="$FAKE_BIN:$PATH" \
  "$ROOT_DIR/scripts/install/install-managed-mcp-user-service.sh" \
  --listen 127.0.0.1:39247 \
  --binary adc-mcp \
  --dry-run >"$DRY_RUN_OUTPUT"
grep -q -- '--managed-listen 127.0.0.1:39247' "$DRY_RUN_OUTPUT"
