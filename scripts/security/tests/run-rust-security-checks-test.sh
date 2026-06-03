#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

MOCK_BIN="$TMP_DIR/cargo-home/bin"
REPORT_DIR="$TMP_DIR/reports"
mkdir -p "$MOCK_BIN" "$REPORT_DIR"

cat >"$MOCK_BIN/cargo-deny" <<'MOCK'
#!/usr/bin/env bash
printf 'mock cargo-deny %s\n' "$*"
MOCK

cat >"$MOCK_BIN/cargo-audit" <<'MOCK'
#!/usr/bin/env bash
printf 'mock cargo-audit %s\n' "$*"
MOCK

cat >"$MOCK_BIN/cargo-machete" <<'MOCK'
#!/usr/bin/env bash
printf 'mock cargo-machete %s\n' "$*"
MOCK

cat >"$MOCK_BIN/cargo-geiger" <<'MOCK'
#!/usr/bin/env bash
printf 'cargo-geiger 0.13.0\n'
MOCK

cat >"$MOCK_BIN/cargo" <<'MOCK'
#!/usr/bin/env bash
case "${1:-}" in
  deny)
    printf 'mock cargo deny %s\n' "${*:2}"
    exit 0
    ;;
  audit)
    printf 'mock cargo audit %s\n' "${*:2}"
    exit 0
    ;;
  machete)
    printf 'mock cargo machete %s\n' "${*:2}"
    exit 0
    ;;
  geiger)
    shift
    ;;
  *)
    printf 'unexpected cargo args: %s\n' "$*" >&2
    exit 2
    ;;
esac

manifest=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --manifest-path)
      manifest="${2:?missing manifest path}"
      shift
      ;;
  esac
  shift
done

package="$(basename "$(dirname "$manifest")")"
printf 'Metric output format: x/y\n'
printf '    x = unsafe code used by the build\n'
printf '    y = total unsafe code found in the crate\n\n'
printf 'Functions  Expressions  Impls  Traits  Methods  Dependency\n'
printf '0/0        0/0          0/0    0/0     0/0      ?  %s 0.1.0\n' "$package"
printf 'WARNING: mocked generated file warning\n' >&2
printf 'error: Found 1 warnings\n' >&2
exit 1
MOCK

chmod +x "$MOCK_BIN"/cargo "$MOCK_BIN"/cargo-deny "$MOCK_BIN"/cargo-audit \
  "$MOCK_BIN"/cargo-machete "$MOCK_BIN"/cargo-geiger

OUTPUT="$(
  CARGO_HOME="$TMP_DIR/cargo-home" \
  ADC_SECURITY_REPORT_DIR="$REPORT_DIR" \
  "$ROOT_DIR/scripts/security/run-rust-security-checks.sh" 2>&1
)"

grep -q 'mock cargo deny' <<<"$OUTPUT"
grep -q 'mock cargo audit' <<<"$OUTPUT"
grep -q 'mock cargo machete' <<<"$OUTPUT"
grep -q 'cargo-geiger report generated with non-gating warnings' <<<"$OUTPUT"
grep -q '+ cargo geiger --all-targets --manifest-path ' <<<"$OUTPUT"

REPORT="$REPORT_DIR/cargo-geiger.txt"
test -s "$REPORT"
grep -q '# cargo-geiger unsafe report' "$REPORT"
grep -q '## adc-core' "$REPORT"
grep -q 'exit_status: 1' "$REPORT"
grep -q 'Metric output format: x/y' "$REPORT"
grep -q 'WARNING: mocked generated file warning' "$REPORT"
