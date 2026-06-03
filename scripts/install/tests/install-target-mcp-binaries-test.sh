#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

if "$ROOT_DIR/scripts/install/install-target-mcp-binaries.sh" \
  --host example-target \
  --target-id harrikka-rp4 \
  --binary-dir "$TMP_DIR/missing" \
  >"$TMP_DIR/missing.stdout" 2>"$TMP_DIR/missing.stderr"; then
  echo "expected missing binary path to fail" >&2
  exit 1
fi
grep -q 'missing local binary' "$TMP_DIR/missing.stderr"

FAKE_BIN="$TMP_DIR/fake-bin"
LOCAL_BIN="$TMP_DIR/local-bin"
REMOTE_ROOT="$TMP_DIR/remote"
mkdir -p "$FAKE_BIN" "$LOCAL_BIN" "$REMOTE_ROOT"

cat >"$LOCAL_BIN/adc-mcp" <<'BIN'
#!/usr/bin/env bash
echo fake adc binary
BIN
chmod +x "$LOCAL_BIN/adc-mcp"

cat >"$LOCAL_BIN/adc" <<'PROBE'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "fleet" && "${2:-}" == "preflight" ]]; then
  inventory=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --inventory)
        inventory="$2"
        shift 2
        ;;
      *)
        shift
        ;;
    esac
  done
  grep -q 'transport: mcp_stdio_over_ssh' "$inventory"
  grep -q 'mcp_server_path: /home/pi/.local/bin/adc-mcp' "$inventory"
  cat <<'JSON'
{
  "schema_version": "obs.fleet_preflight.v1",
  "status": "ready",
  "target_count": 1,
  "ready_count": 1,
  "failed_count": 0,
  "targets": [
    {
      "target_id": "harrikka-rp4",
      "transport": "mcp_stdio_over_ssh",
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
printf '%s\n' "$dest" >>"$FAKE_SSH_LOG"
command_text="$*"
case "$command_text" in
  *'printf %s "$HOME"'*)
    printf '/home/pi'
    ;;
  mkdir*)
    mkdir -p "$FAKE_REMOTE_ROOT/home/pi/.local/bin"
    ;;
  chmod*)
    chmod 0755 "$FAKE_REMOTE_ROOT/home/pi/.local/bin/"* 2>/dev/null || true
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
mkdir -p "$FAKE_REMOTE_ROOT$remote_path"
for source in "${sources[@]}"; do
  cp "$source" "$FAKE_REMOTE_ROOT$remote_path/"
done
SCP
chmod +x "$FAKE_BIN/scp"

FAKE_REMOTE_ROOT="$REMOTE_ROOT" \
FAKE_SSH_LOG="$TMP_DIR/ssh.log" \
PATH="$FAKE_BIN:$PATH" \
  "$ROOT_DIR/scripts/install/install-target-mcp-binaries.sh" \
  --host example-target \
  --target-id harrikka-rp4 \
  --binary-dir "$LOCAL_BIN" \
  --result-root "$TMP_DIR/results" \
  >"$TMP_DIR/inventory.yaml"

grep -q '"schema_version": "obs.fleet_preflight.v1"' "$TMP_DIR/results/fleet_preflight.json"
grep -q 'host: example-target' "$TMP_DIR/inventory.yaml"
grep -q 'target_id' "$TMP_DIR/results/bootstrap_report.json"
grep -q 'transport: mcp_stdio_over_ssh' "$TMP_DIR/inventory.yaml"
grep -q 'mcp_server_path: /home/pi/.local/bin/adc-mcp' "$TMP_DIR/inventory.yaml"
test -x "$REMOTE_ROOT/home/pi/.local/bin/adc-mcp"
test ! -e "$REMOTE_ROOT/home/pi/.local/bin/adc"
