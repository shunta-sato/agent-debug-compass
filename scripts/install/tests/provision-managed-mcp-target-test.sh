#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

FAKE_BIN="$TMP_DIR/fake-bin"
LOCAL_BIN="$TMP_DIR/local-bin"
REMOTE_ROOT="$TMP_DIR/remote"
mkdir -p "$FAKE_BIN" "$LOCAL_BIN" "$REMOTE_ROOT"

cat >"$FAKE_BIN/ssh" <<'SSH'
#!/usr/bin/env bash
echo "ssh should not have been called" >&2
exit 99
SSH
chmod +x "$FAKE_BIN/ssh"

if PATH="$FAKE_BIN:$PATH" "$ROOT_DIR/scripts/install/provision-managed-mcp-target.sh" \
  --ssh-host example-target \
  --managed-host 192.0.2.55 \
  --target-id kit-rp4 \
  --binary-dir "$TMP_DIR/missing" \
  >"$TMP_DIR/missing.stdout" 2>"$TMP_DIR/missing.stderr"; then
  echo "expected missing binary path to fail" >&2
  exit 1
fi
grep -q 'missing local binary' "$TMP_DIR/missing.stderr"

cat >"$LOCAL_BIN/adc-mcp" <<'MCP'
#!/usr/bin/env bash
echo fake managed mcp server
MCP
chmod +x "$LOCAL_BIN/adc-mcp"

cat >"$LOCAL_BIN/adc" <<'PROBE'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "fleet" && "${2:-}" == "enroll-kit" ]]; then
  kit=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --kit)
        kit="$2"
        shift 2
        ;;
      *)
        shift
        ;;
    esac
  done
  grep -q '"schema_version": "obs.managed_mcp_enrollment_kit.v1"' "$kit"
  grep -q '"transport": "managed_mcp"' "$kit"
  cat <<'JSON'
{
  "schema_version": "obs.managed_fleet_registry.v1",
  "target_count": 1,
  "targets": [
    {
      "target_id": "kit-rp4",
      "transport": "managed_mcp",
      "enrollment_mode": "kit"
    }
  ]
}
JSON
  exit 0
fi
if [[ "${1:-}" == "fleet" && "${2:-}" == "preflight" ]]; then
  selector=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --selector)
        selector="$2"
        shift 2
        ;;
      *)
        shift
        ;;
    esac
  done
  test "$selector" = "target=kit-rp4"
  if [[ -n "${FAKE_PREFLIGHT_COUNT:-}" ]]; then
    count=0
    if [[ -f "$FAKE_PREFLIGHT_COUNT" ]]; then
      count="$(cat "$FAKE_PREFLIGHT_COUNT")"
    fi
    count=$((count + 1))
    printf '%s\n' "$count" >"$FAKE_PREFLIGHT_COUNT"
    if [[ "$count" -eq 1 ]]; then
      cat <<'JSON'
{
  "schema_version": "obs.fleet_preflight.v1",
  "status": "failed",
  "target_count": 1,
  "ready_count": 0,
  "failed_count": 1,
  "targets": [
    {
      "target_id": "kit-rp4",
      "transport": "managed_mcp",
      "status": "unreachable",
      "checks": [],
      "data_quality": {"missing": ["unreachable: booting"]}
    }
  ],
  "data_quality": {"missing": ["target kit-rp4 preflight: unreachable"]}
}
JSON
      exit 0
    fi
  fi
  cat <<'JSON'
{
  "schema_version": "obs.fleet_preflight.v1",
  "status": "ready",
  "target_count": 1,
  "ready_count": 1,
  "failed_count": 0,
  "targets": [
    {
      "target_id": "kit-rp4",
      "transport": "managed_mcp",
      "status": "ready",
      "checks": [],
      "data_quality": {"missing": []}
    }
  ],
  "data_quality": {"missing": []}
}
JSON
  exit 0
fi
echo "unexpected fake adc args: $*" >&2
exit 9
PROBE
chmod +x "$LOCAL_BIN/adc"

cat >"$FAKE_BIN/ssh" <<'SSH'
#!/usr/bin/env bash
set -euo pipefail
while [[ "${1:-}" == "-o" ]]; do
  shift 2
done
if [[ "${1:-}" == "-p" ]]; then
  shift 2
fi
dest="${1:?missing destination}"
shift
command_text="$*"
printf 'SSH %s %s\n' "$dest" "$command_text" >>"$FAKE_SSH_LOG"
case "$command_text" in
  *'printf %s "$HOME"'*)
    printf '/home/pi'
    ;;
  mkdir*)
    mkdir -p \
      "$FAKE_REMOTE_ROOT/home/pi/.local/bin" \
      "$FAKE_REMOTE_ROOT/home/pi/.local/share/agent-debug-compass/managed-mcp" \
      "$FAKE_REMOTE_ROOT/home/pi/.local/share/agent-debug-compass/state"
    ;;
  chmod*)
    if [[ "$command_text" == *adc-mcp.kit-rp4.tmp* ]]; then
      mv -f "$FAKE_REMOTE_ROOT/home/pi/.local/bin/adc-mcp.kit-rp4.tmp" \
        "$FAKE_REMOTE_ROOT/home/pi/.local/bin/adc-mcp"
    fi
    chmod 0700 "$FAKE_REMOTE_ROOT/home/pi/.local/share/agent-debug-compass/managed-mcp"
    chmod 0600 "$FAKE_REMOTE_ROOT/home/pi/.local/share/agent-debug-compass/managed-mcp/"* 2>/dev/null || true
    chmod 0755 "$FAKE_REMOTE_ROOT/home/pi/.local/bin/adc-mcp" 2>/dev/null || true
    chmod 0755 "$FAKE_REMOTE_ROOT/home/pi/.local/share/agent-debug-compass/managed-mcp/install-managed-mcp-user-service.sh" 2>/dev/null || true
    ;;
  *install-managed-mcp-user-service.sh*)
    printf '%s\n' "$command_text" >"$FAKE_REMOTE_ROOT/installer-command.txt"
    ;;
  *)
    echo "unexpected ssh command: $command_text" >&2
    exit 9
    ;;
esac
SSH
chmod +x "$FAKE_BIN/ssh"

cat >"$FAKE_BIN/scp" <<'SCP'
#!/usr/bin/env bash
set -euo pipefail
while [[ "${1:-}" == "-q" || "${1:-}" == "-o" ]]; do
  if [[ "${1:-}" == "-q" ]]; then
    shift
  else
    shift 2
  fi
done
if [[ "${1:-}" == "-P" ]]; then
  shift 2
fi
sources=()
while [[ $# -gt 1 ]]; do
  sources+=("$1")
  shift
done
dest="${1:?missing scp destination}"
remote_path="${dest#*:}"
if [[ "$remote_path" == */ ]]; then
  mkdir -p "$FAKE_REMOTE_ROOT$remote_path"
  for source in "${sources[@]}"; do
    cp "$source" "$FAKE_REMOTE_ROOT$remote_path/"
  done
else
  test "${#sources[@]}" = "1"
  mkdir -p "$(dirname "$FAKE_REMOTE_ROOT$remote_path")"
  cp "${sources[0]}" "$FAKE_REMOTE_ROOT$remote_path"
fi
SCP
chmod +x "$FAKE_BIN/scp"

cat >"$FAKE_BIN/ssh-keyscan" <<'KEYSCAN'
#!/usr/bin/env bash
cat <<'TXT'
example-target ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFakeHostKeyForProvisionTestOnly
TXT
KEYSCAN
chmod +x "$FAKE_BIN/ssh-keyscan"

cat >"$FAKE_BIN/ssh-keygen" <<'KEYGEN'
#!/usr/bin/env bash
cat >/dev/null
printf '256 SHA256:fake-provision-fingerprint example-target (ED25519)\n'
KEYGEN
chmod +x "$FAKE_BIN/ssh-keygen"

FAKE_REMOTE_ROOT="$REMOTE_ROOT" \
FAKE_SSH_LOG="$TMP_DIR/dry-run-ssh.log" \
PATH="$FAKE_BIN:$PATH" \
  "$ROOT_DIR/scripts/install/provision-managed-mcp-target.sh" \
  --ssh-host example-target \
  --managed-host 192.0.2.55 \
  --target-id kit-rp4 \
  --binary-dir "$LOCAL_BIN" \
  --kit-dir "$TMP_DIR/dry-kit" \
  --listen 0.0.0.0:39255 \
  --managed-port 39255 \
  --ssh-strict-host-key-checking yes \
  --ssh-known-hosts-file "$TMP_DIR/known_hosts" \
  --unit-name adc-mcp-managed-kit-rp4.service \
  --dry-run >"$TMP_DIR/dry-run.stdout" 2>"$TMP_DIR/dry-run.stderr"
grep -q '"dry_run": true' "$TMP_DIR/dry-run.stdout"
grep -q '"ssh_strict_host_key_checking": "yes"' "$TMP_DIR/dry-run.stdout"
grep -q '"ssh_known_hosts_file": "'"$TMP_DIR"'/known_hosts"' "$TMP_DIR/dry-run.stdout"
test ! -e "$TMP_DIR/dry-kit"
test ! -s "$TMP_DIR/dry-run-ssh.log"

if FAKE_REMOTE_ROOT="$REMOTE_ROOT" \
  FAKE_SSH_LOG="$TMP_DIR/bad-ssh.log" \
  PATH="$FAKE_BIN:$PATH" \
    "$ROOT_DIR/scripts/install/provision-managed-mcp-target.sh" \
    --ssh-host example-target \
    --managed-host 192.0.2.55 \
    --target-id '../bad' \
    --binary-dir "$LOCAL_BIN" \
    --dry-run >"$TMP_DIR/bad.stdout" 2>"$TMP_DIR/bad.stderr"; then
  echo "expected unsafe target-id to fail" >&2
  exit 1
fi
grep -q 'target-id must be a single safe path segment' "$TMP_DIR/bad.stderr"

FAKE_REMOTE_ROOT="$REMOTE_ROOT" \
FAKE_SSH_LOG="$TMP_DIR/ssh.log" \
PATH="$FAKE_BIN:$PATH" \
ADC_HOME="$TMP_DIR/controller-state" \
FAKE_PREFLIGHT_COUNT="$TMP_DIR/preflight-count" \
  "$ROOT_DIR/scripts/install/provision-managed-mcp-target.sh" \
  --ssh-host example-target \
  --managed-host 192.0.2.55 \
  --target-id kit-rp4 \
  --binary-dir "$LOCAL_BIN" \
  --kit-dir "$TMP_DIR/kit" \
  --remote-bin-dir .local/bin \
  --remote-config-dir .local/share/agent-debug-compass/managed-mcp \
  --remote-state-dir .local/share/agent-debug-compass/state \
  --listen 0.0.0.0:39255 \
  --managed-port 39255 \
  --ssh-strict-host-key-checking yes \
  --ssh-known-hosts-file "$TMP_DIR/known_hosts" \
  --preflight-attempts 2 \
  --preflight-sleep-sec 0 \
  --unit-name adc-mcp-managed-kit-rp4.service \
  --result-root "$TMP_DIR/results" >"$TMP_DIR/provision.stdout"

test -x "$REMOTE_ROOT/home/pi/.local/bin/adc-mcp"
test -s "$REMOTE_ROOT/home/pi/.local/share/agent-debug-compass/managed-mcp/managed.token"
test -s "$REMOTE_ROOT/home/pi/.local/share/agent-debug-compass/managed-mcp/server.pem"
test -s "$REMOTE_ROOT/home/pi/.local/share/agent-debug-compass/managed-mcp/server.key"
test -s "$REMOTE_ROOT/home/pi/.local/share/agent-debug-compass/managed-mcp/controller-ca.pem"
test -x "$REMOTE_ROOT/home/pi/.local/share/agent-debug-compass/managed-mcp/install-managed-mcp-user-service.sh"
test "$(stat -c '%a' "$REMOTE_ROOT/home/pi/.local/share/agent-debug-compass/managed-mcp/managed.token")" = "600"
grep -q -- "--listen '0.0.0.0:39255'" "$REMOTE_ROOT/installer-command.txt"
grep -q -- "--binary '/home/pi/.local/bin/adc-mcp'" "$REMOTE_ROOT/installer-command.txt"
grep -q -- "--tls-server-cert '/home/pi/.local/share/agent-debug-compass/managed-mcp/server.pem'" "$REMOTE_ROOT/installer-command.txt"
grep -q '"transport": "managed_mcp"' "$TMP_DIR/results/provision_report.json"
grep -q '"root_required": false' "$TMP_DIR/results/provision_report.json"
grep -q '"ssh_strict_host_key_checking": "yes"' "$TMP_DIR/results/provision_report.json"
grep -q 'SHA256:fake-provision-fingerprint' "$TMP_DIR/results/provision_report.json"
grep -q 'SHA256:fake-provision-fingerprint' "$TMP_DIR/results/ssh_host_fingerprint.txt"
grep -q '"status": "ready"' "$TMP_DIR/results/preflight.json"
test "$(cat "$TMP_DIR/preflight-count")" = "2"
grep -q '"target_id": "kit-rp4"' "$TMP_DIR/provision.stdout"
