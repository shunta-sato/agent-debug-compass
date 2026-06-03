#!/usr/bin/env bash
set -Eeuo pipefail

readonly SCRIPT_NAME="$(basename "$0")"
readonly PERF_PACKAGES=(linux-perf)

mode="dry-run"
run_update="yes"
strict_sources="no"

usage() {
  cat <<'USAGE'
Usage:
  scripts/install/install-target-perf.sh [--dry-run]
  scripts/install/install-target-perf.sh --apply

Options:
  --dry-run          Simulate the apt install only. This is the default.
  --apply            Run apt-get update and install linux-perf.
  --no-update        Skip apt-get update when using --apply.
  --strict-sources   Fail if apt source hardening checks find warnings.
  -h, --help         Show this help.

Installs the target perf userspace tool required for privileged ftrace/perf smoke:
  linux-perf

It intentionally does not install trace-cmd, kernel headers, build tools, node,
npm, or any global language package-manager tools.
USAGE
}

log() {
  printf '[%s] %s\n' "$SCRIPT_NAME" "$*"
}

warn() {
  printf '[%s] WARNING: %s\n' "$SCRIPT_NAME" "$*" >&2
}

die() {
  printf '[%s] ERROR: %s\n' "$SCRIPT_NAME" "$*" >&2
  exit 1
}

run() {
  log "+ $*"
  "$@"
}

sudo_cmd() {
  if [[ "${EUID}" -eq 0 ]]; then
    "$@"
  else
    sudo "$@"
  fi
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
      --no-update)
        run_update="no"
        ;;
      --strict-sources)
        strict_sources="yes"
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

check_platform() {
  require_command apt-get
  require_command apt-cache
  require_command apt-config
  require_command dpkg
  if [[ "${mode}" == "apply" && "${EUID}" -ne 0 ]]; then
    require_command sudo
  fi

  local arch
  arch="$(dpkg --print-architecture)"
  if [[ "${arch}" != "arm64" ]]; then
    warn "expected arm64 for Raspberry Pi 5 target, got ${arch}"
  fi
}

check_apt_security_defaults() {
  local dump
  dump="$(apt-config dump)"

  case "${dump}" in
    *'Acquire::AllowInsecureRepositories "1"'*)
      die "apt allows insecure repositories"
      ;;
  esac

  case "${dump}" in
    *'Acquire::AllowWeakRepositories "1"'*)
      die "apt allows weak repositories"
      ;;
  esac

  case "${dump}" in
    *'Acquire::AllowDowngradeToInsecureRepositories "1"'*)
      die "apt allows downgrade to insecure repositories"
      ;;
  esac
}

check_source_file() {
  local file="$1"
  local warnings=0

  if [[ "${file}" == *.sources ]]; then
    if grep -Eq '^[[:space:]]*Types:[[:space:]]*.*\bdeb\b' "${file}" \
      && ! grep -Eq '^[[:space:]]*Signed-By:' "${file}"; then
      warn "${file} has deb entries without Signed-By"
      warnings=1
    fi
  else
    if grep -Eq '^[[:space:]]*deb[[:space:]]' "${file}" \
      && ! grep -Eq '^[[:space:]]*deb[[:space:]].*\[.*signed-by=' "${file}"; then
      warn "${file} has deb entries without per-source signed-by"
      warnings=1
    fi
  fi

  return "${warnings}"
}

check_apt_sources() {
  local warnings=0
  local file
  local files=()

  if [[ -f /etc/apt/sources.list ]]; then
    files+=(/etc/apt/sources.list)
  fi
  if [[ -d /etc/apt/sources.list.d ]]; then
    while IFS= read -r file; do
      files+=("${file}")
    done < <(find /etc/apt/sources.list.d -maxdepth 1 -type f \( -name '*.list' -o -name '*.sources' \) | sort)
  fi

  for file in "${files[@]}"; do
    check_source_file "${file}" || warnings=1
  done

  if [[ "${warnings}" -ne 0 && "${strict_sources}" == "yes" ]]; then
    die "apt source hardening check failed"
  fi
}

print_plan() {
  log "mode: ${mode}"
  log "apt update before install: ${run_update} (apply mode only)"
  log "packages:"
  printf '  %s\n' "${PERF_PACKAGES[@]}"
  log "apt candidate:"
  apt-cache policy "${PERF_PACKAGES[@]}" | sed 's/^/  /'
  log "post-install smoke check:"
  printf '  %s\n' "scripts/e2e/target/run-privileged-smoke.sh ftrace-perf-smoke --duration-sec 2"
}

simulate_install() {
  run apt-get -s install --no-install-recommends "${PERF_PACKAGES[@]}"
}

apply_install() {
  if [[ "${run_update}" == "yes" ]]; then
    run sudo_cmd apt-get update
  fi
  run sudo_cmd env DEBIAN_FRONTEND=noninteractive \
    apt-get install --no-install-recommends -y "${PERF_PACKAGES[@]}"
}

verify_install() {
  require_command perf
  perf --version
}

main() {
  parse_args "$@"
  check_platform
  check_apt_security_defaults
  check_apt_sources
  print_plan

  if [[ "${mode}" == "dry-run" ]]; then
    simulate_install
    log "dry-run complete. Re-run with --apply to install linux-perf."
  else
    apply_install
    verify_install
  fi
}

main "$@"
