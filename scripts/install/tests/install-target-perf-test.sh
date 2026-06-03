#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
OUTPUT="$("$ROOT_DIR/scripts/install/install-target-perf.sh" --dry-run 2>&1)"

grep -q 'mode: dry-run' <<<"$OUTPUT"
grep -q 'linux-perf' <<<"$OUTPUT"
grep -q 'dry-run complete' <<<"$OUTPUT"
grep -q 'ftrace-perf-smoke --duration-sec 2' <<<"$OUTPUT"
