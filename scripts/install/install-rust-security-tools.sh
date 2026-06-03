#!/usr/bin/env bash
set -Eeuo pipefail

readonly SCRIPT_NAME="$(basename "$0")"

# Pinned for the repository's current Rust 1.85 toolchain.
readonly CARGO_DENY_VERSION="0.18.3"
readonly CARGO_AUDIT_VERSION="0.22.1"
readonly CARGO_GEIGER_VERSION="0.13.0"
readonly CARGO_MACHETE_VERSION="0.8.0"
readonly CARGO_VET_VERSION="0.10.2"

mode="dry-run"
install_root=""
install_geiger=0

usage() {
  cat <<'USAGE'
Usage:
  scripts/install/install-rust-security-tools.sh [--dry-run]
  scripts/install/install-rust-security-tools.sh --apply [--root DIR] [--with-geiger]

Options:
  --dry-run      Print pinned tool versions and install commands. Default.
  --apply        Install pinned Cargo security tools with cargo install --locked.
  --root DIR     Install into a custom Cargo install root. Optional.
  --with-geiger  Also install cargo-geiger for optional unsafe reports.
  -h, --help     Show this help.

Installs:
  cargo-deny, cargo-audit, cargo-machete, cargo-vet

Optional:
  cargo-geiger 0.13.0. It currently pulls native OpenSSL dependencies while
  building, so it is opt-in to keep minimal target setup small.

The script intentionally does not install npm/node, cargo-binstall, or unpinned
latest versions. cargo-deny is pinned to 0.18.3 because 0.19.x requires Rust 1.88.
USAGE
}

log() {
  printf '[%s] %s\n' "$SCRIPT_NAME" "$*"
}

die() {
  printf '[%s] ERROR: %s\n' "$SCRIPT_NAME" "$*" >&2
  exit 1
}

run() {
  log "+ $*"
  "$@"
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --dry-run)
        mode="dry-run"
        ;;
      --apply)
        mode="apply"
        ;;
      --root)
        install_root="${2:?missing --root value}"
        shift
        ;;
      --with-geiger)
        install_geiger=1
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        die "unknown argument: $1"
        ;;
    esac
    shift
  done
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

cargo_install_args() {
  local crate="$1"
  local version="$2"
  printf '%s\n' install --locked --version "$version" "$crate"
}

print_plan() {
  log "mode: $mode"
  if [[ -n "$install_root" ]]; then
    log "cargo install root: $install_root"
  else
    log "cargo install root: default Cargo install root"
  fi
  log "rust toolchain:"
  rustc --version
  cargo --version
  log "pinned tools:"
  printf '  %-14s %s\n' cargo-deny "$CARGO_DENY_VERSION"
  printf '  %-14s %s\n' cargo-audit "$CARGO_AUDIT_VERSION"
  printf '  %-14s %s\n' cargo-machete "$CARGO_MACHETE_VERSION"
  printf '  %-14s %s\n' cargo-vet "$CARGO_VET_VERSION"
  if [[ "$install_geiger" -eq 1 ]]; then
    log "optional tools:"
    printf '  %-14s %s\n' cargo-geiger "$CARGO_GEIGER_VERSION"
  else
    log "optional tools skipped: cargo-geiger $CARGO_GEIGER_VERSION (--with-geiger)"
  fi
}

install_tool() {
  local crate="$1"
  local version="$2"
  local args
  mapfile -t args < <(cargo_install_args "$crate" "$version")
  if [[ -n "$install_root" ]]; then
    run cargo "${args[@]}" --root "$install_root"
  else
    run cargo "${args[@]}"
  fi
}

apply_install() {
  install_tool cargo-deny "$CARGO_DENY_VERSION"
  install_tool cargo-audit "$CARGO_AUDIT_VERSION"
  install_tool cargo-machete "$CARGO_MACHETE_VERSION"
  install_tool cargo-vet "$CARGO_VET_VERSION"
  if [[ "$install_geiger" -eq 1 ]]; then
    install_tool cargo-geiger "$CARGO_GEIGER_VERSION"
  fi
}

verify_installed() {
  local failed=0
  local tools=(cargo-deny cargo-audit cargo-machete cargo-vet)
  if [[ "$install_geiger" -eq 1 ]]; then
    tools+=(cargo-geiger)
  fi
  for tool in "${tools[@]}"; do
    if command -v "$tool" >/dev/null 2>&1; then
      "$tool" --version 2>/dev/null | head -n 1 || true
    else
      printf '%s MISSING\n' "$tool" >&2
      failed=1
    fi
  done
  return "$failed"
}

main() {
  parse_args "$@"
  require_command rustc
  require_command cargo
  print_plan

  if [[ "$mode" == "dry-run" ]]; then
    log "dry-run complete. Re-run with --apply to install pinned tools."
    return 0
  fi

  apply_install
  if [[ -n "$install_root" ]]; then
    export PATH="$install_root/bin:$PATH"
  else
    export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
  fi
  verify_installed
}

main "$@"
