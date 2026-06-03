#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
REPORT_DIR="${ADC_SECURITY_REPORT_DIR:-$ROOT_DIR/reports/security}"
DRY_RUN=0

# cargo install places binaries here by default, but non-login automation often
# does not inherit this path.
CARGO_BIN_DIR="${CARGO_HOME:-$HOME/.cargo}/bin"
if [[ -d "$CARGO_BIN_DIR" ]]; then
  export PATH="$CARGO_BIN_DIR:$PATH"
fi

usage() {
  cat <<'USAGE'
Usage:
  scripts/security/run-rust-security-checks.sh [--dry-run]

Runs Rust dependency/security/supply-chain static checks:
  cargo deny check bans licenses sources
  cargo audit
  cargo machete

Optional if cargo-geiger is installed:
  cargo geiger --all-targets for each workspace crate

Install tools with:
  scripts/install/install-rust-security-tools.sh --apply
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      DRY_RUN=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

require_tool() {
  local tool="$1"
  command -v "$tool" >/dev/null 2>&1 || {
    printf 'missing required tool: %s\n' "$tool" >&2
    printf 'install with: scripts/install/install-rust-security-tools.sh --apply\n' >&2
    return 1
  }
}

run_step() {
  printf '+ %s\n' "$*"
  "$@"
}

package_name_for_manifest() {
  sed -n 's/^name = "\(.*\)"/\1/p' "$1" | head -n 1
}

write_cargo_geiger_report() {
  local report_path="$REPORT_DIR/cargo-geiger.txt"
  local tmp_dir
  local manifest
  local package
  local stdout_path
  local stderr_path
  local diagnostics_path
  local status
  local generated=0
  local warning_count=0

  tmp_dir="$(mktemp -d)"
  shopt -s nullglob
  local manifests=("$ROOT_DIR"/crates/*/Cargo.toml)
  shopt -u nullglob

  {
    printf '# cargo-geiger unsafe report\n\n'
    printf 'Scope: workspace crates under crates/*/Cargo.toml\n'
    printf 'Mode: report-only; transitive dependency unsafe usage is visible but non-gating.\n'
    printf 'Note: cargo-geiger may return non-zero for generated-file scan warnings even when it produced a usable report.\n'
  } >"$report_path"

  if [[ "${#manifests[@]}" -eq 0 ]]; then
    printf '\nNo crate manifests found.\n' >>"$report_path"
    printf 'cargo-geiger report: %s\n' "$report_path"
    rm -rf "$tmp_dir"
    return 0
  fi

  for manifest in "${manifests[@]}"; do
    package="$(package_name_for_manifest "$manifest")"
    if [[ -z "$package" ]]; then
      package="$(basename "$(dirname "$manifest")")"
    fi
    stdout_path="$tmp_dir/$package.stdout"
    stderr_path="$tmp_dir/$package.stderr"
    diagnostics_path="$tmp_dir/$package.diagnostics"

    printf '+ cargo geiger --all-targets --manifest-path %s\n' "$manifest"
    if cargo geiger --all-targets --manifest-path "$manifest" >"$stdout_path" 2>"$stderr_path"; then
      status=0
    else
      status=$?
    fi

    {
      printf '\n## %s\n\n' "$package"
      printf 'manifest: %s\n' "${manifest#$ROOT_DIR/}"
      printf 'exit_status: %s\n\n' "$status"
      printf '### stdout\n\n```text\n'
      cat "$stdout_path"
      printf '\n```\n'
      grep -E '^(WARNING|error:|Failed to match)' "$stderr_path" >"$diagnostics_path" || true
      if [[ -s "$diagnostics_path" ]]; then
        printf '\n### stderr diagnostics\n\n```text\n'
        tail -n 80 "$diagnostics_path"
        printf '\n```\n'
      fi
    } >>"$report_path"

    if grep -q 'Metric output format' "$stdout_path"; then
      generated=1
      if [[ "$status" -ne 0 ]]; then
        warning_count=$((warning_count + 1))
      fi
    else
      printf 'cargo-geiger did not produce a metric report for %s; see %s\n' "$package" "$report_path" >&2
    fi
  done

  rm -rf "$tmp_dir"

  if [[ "$generated" -eq 1 ]]; then
    if [[ "$warning_count" -gt 0 ]]; then
      printf 'cargo-geiger report generated with non-gating warnings for %s package(s): %s\n' "$warning_count" "$report_path"
    else
      printf 'cargo-geiger report: %s\n' "$report_path"
    fi
  else
    printf 'cargo-geiger did not generate a usable report; see %s\n' "$report_path" >&2
  fi
}

if [[ "$DRY_RUN" -eq 1 ]]; then
  usage
  printf '\nConfigured report directory: %s\n' "$REPORT_DIR"
  exit 0
fi

require_tool cargo-deny
require_tool cargo-audit
require_tool cargo-machete

mkdir -p "$REPORT_DIR"

run_step cargo deny check --config "$ROOT_DIR/deny.toml" bans licenses sources
run_step cargo audit
run_step cargo machete

if command -v cargo-geiger >/dev/null 2>&1; then
  write_cargo_geiger_report
else
  printf 'cargo-geiger not installed; optional unsafe dependency report skipped.\n'
  printf 'cargo-geiger not installed; optional unsafe dependency report skipped.\n' >"$REPORT_DIR/cargo-geiger.txt"
fi
