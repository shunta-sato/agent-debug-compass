#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
OUTPUT="$("$ROOT_DIR/scripts/install/install-rust-security-tools.sh" --dry-run 2>&1)"

grep -q 'mode: dry-run' <<<"$OUTPUT"
grep -q 'cargo-deny' <<<"$OUTPUT"
grep -q '0.18.3' <<<"$OUTPUT"
grep -q 'cargo-audit' <<<"$OUTPUT"
grep -q 'cargo-machete' <<<"$OUTPUT"
grep -q '0.8.0' <<<"$OUTPUT"
grep -q 'cargo-vet' <<<"$OUTPUT"
grep -q 'optional tools skipped: cargo-geiger' <<<"$OUTPUT"
grep -q 'dry-run complete' <<<"$OUTPUT"

WITH_GEIGER_OUTPUT="$("$ROOT_DIR/scripts/install/install-rust-security-tools.sh" --dry-run --with-geiger 2>&1)"
grep -q 'optional tools:' <<<"$WITH_GEIGER_OUTPUT"
grep -q 'cargo-geiger' <<<"$WITH_GEIGER_OUTPUT"
