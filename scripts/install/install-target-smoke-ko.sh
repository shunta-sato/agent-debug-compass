#!/usr/bin/env bash
set -Eeuo pipefail

readonly SCRIPT_NAME="$(basename "$0")"
readonly ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly DEFAULT_MODULE="$ROOT_DIR/kernel/adc_sensor_probe/adc_sensor_probe.ko"
readonly DEFAULT_DEST_DIR="/usr/local/libexec/agent-debug-compass/kernel"
readonly MODULE_NAME="adc_sensor_probe"

mode="dry-run"
remove="no"
module_path="$DEFAULT_MODULE"
dest_dir="$DEFAULT_DEST_DIR"

usage() {
  cat <<'USAGE'
Usage:
  scripts/install/install-target-smoke-ko.sh [--dry-run]
  sudo scripts/install/install-target-smoke-ko.sh --apply
  sudo scripts/install/install-target-smoke-ko.sh --remove

Options:
  --dry-run           Print the install plan. Default.
  --apply             Install the built KO as root-owned smoke artifact.
  --remove            Remove the installed KO.
  --module PATH       Source KO. Default: kernel/adc_sensor_probe/adc_sensor_probe.ko.
  --dest-dir DIR      Destination directory. Default: /usr/local/libexec/agent-debug-compass/kernel.
  -h, --help          Show help.

This script intentionally does not build the KO as root. Run this first:
  make ko-selftest

Then install the resulting module for the privileged runtime smoke:
  sudo scripts/install/install-target-smoke-ko.sh --apply
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

sudo_cmd() {
  if [[ "$(id -u)" == "0" ]]; then
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
      --remove)
        mode="apply"
        remove="yes"
        ;;
      --module)
        module_path="${2:?missing module path}"
        shift
        ;;
      --dest-dir)
        dest_dir="${2:?missing destination directory}"
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
    shift
  done
}

dest_module() {
  printf '%s/%s.ko\n' "$dest_dir" "$MODULE_NAME"
}

validate_inputs() {
  [[ "$dest_dir" == /* ]] || die "--dest-dir must be absolute"
  [[ "$dest_dir" != *" "* ]] || die "--dest-dir must not contain spaces"
  command -v install >/dev/null 2>&1 || die "install command not found"
  if [[ "$remove" == "yes" ]]; then
    return 0
  fi
  [[ -f "$module_path" ]] || die "module not found: $module_path; run make ko-selftest first"
  [[ ! -L "$module_path" ]] || die "module path must not be a symlink"
  [[ "$(basename "$module_path")" == "${MODULE_NAME}.ko" ]] || die "unexpected module file name"
  command -v modinfo >/dev/null 2>&1 || die "modinfo command not found"
  local mod_name
  mod_name="$(modinfo -F name "$module_path")"
  [[ "$mod_name" == "$MODULE_NAME" ]] || die "unexpected module name from modinfo: $mod_name"
}

print_plan() {
  log "mode: $mode"
  log "remove: $remove"
  log "source module: $module_path"
  log "destination module: $(dest_module)"
  if [[ "$remove" != "yes" && -f "$module_path" ]]; then
    log "source sha256:"
    sha256sum "$module_path"
  fi
}

apply_install() {
  run sudo_cmd install -d -o root -g root -m 0755 "$dest_dir"
  run sudo_cmd install -o root -g root -m 0644 "$module_path" "$(dest_module)"
  log "installed. Verify with:"
  printf '  %s\n' "scripts/e2e/target/run-privileged-smoke.sh ko-runtime-smoke"
}

apply_remove() {
  run sudo_cmd rm -f "$(dest_module)"
  run sudo_cmd rmdir "$dest_dir" 2>/dev/null || true
  log "removed installed KO"
}

main() {
  parse_args "$@"
  validate_inputs
  print_plan
  if [[ "$mode" != "apply" ]]; then
    log "dry-run only; pass --apply or --remove to change the system"
    return 0
  fi
  if [[ "$remove" == "yes" ]]; then
    apply_remove
  else
    apply_install
  fi
}

main "$@"
