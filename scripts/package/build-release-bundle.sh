#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERSION="${ADC_RELEASE_VERSION:-0.1.0}"
TARGET_TRIPLE="${ADC_RELEASE_TARGET:-$(uname -m)-linux}"
BUNDLE_NAME="agent-debug-compass-${VERSION}-${TARGET_TRIPLE}"
DIST_DIR="${ADC_RELEASE_DIST:-$ROOT_DIR/dist}"
BUNDLE_DIR="$DIST_DIR/$BUNDLE_NAME"
ARCHIVE="$DIST_DIR/$BUNDLE_NAME.tar.gz"
BUILD_RELEASE=1
FORCE=0

usage() {
  cat <<'USAGE'
Usage: build-release-bundle.sh [--no-build] [--force] [--version VERSION]

Builds a release bundle with:
  bin/adc-targetd
  bin/adc
  bin/adc-workload
  bin/adc-demo-sensor-gateway
  bin/adc-mcp
  bin/adc-priv-helper
  docs/
  demos/
  profiles/
  packaging/systemd/adc-targetd.service
  kernel/adc_sensor_probe/
  scripts/install/
  scripts/demo/
  scripts/security/
  scripts/e2e/run-e2e.sh
  scripts/e2e/run-agent-quality-dogfood.sh
  scripts/e2e/target/
    run-pi5-release-smoke.sh
    run-target-mcp-fleet-smoke.sh
  scripts/e2e/tests/
    run-managed-mcp-mtls-smoke.sh
    run-managed-mcp-enrollment-kit-test.sh
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-build)
      BUILD_RELEASE=0
      shift
      ;;
    --force)
      FORCE=1
      shift
      ;;
    --version)
      VERSION="${2:?missing version}"
      BUNDLE_NAME="agent-debug-compass-${VERSION}-${TARGET_TRIPLE}"
      BUNDLE_DIR="$DIST_DIR/$BUNDLE_NAME"
      ARCHIVE="$DIST_DIR/$BUNDLE_NAME.tar.gz"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ "$BUILD_RELEASE" -eq 1 ]]; then
  cargo build --workspace --release
fi

build_dir="${CARGO_TARGET_DIR:-$ROOT_DIR/target}/release"
for bin in adc-targetd adc adc-workload adc-demo-sensor-gateway adc-mcp adc-priv-helper; do
  if [[ ! -x "$build_dir/$bin" ]]; then
    echo "missing release binary: $build_dir/$bin" >&2
    echo "run without --no-build or build the workspace release profile first" >&2
    exit 1
  fi
done

if [[ -e "$BUNDLE_DIR" || -e "$ARCHIVE" ]]; then
  if [[ "$FORCE" -ne 1 ]]; then
    echo "bundle output already exists; pass --force to replace it" >&2
    exit 1
  fi
  rm -rf -- "$BUNDLE_DIR"
  rm -f -- "$ARCHIVE" "$ARCHIVE.sha256"
fi

mkdir -p "$BUNDLE_DIR/bin" "$BUNDLE_DIR/docs" "$BUNDLE_DIR/demos" "$BUNDLE_DIR/profiles" "$BUNDLE_DIR/packaging/systemd" "$BUNDLE_DIR/scripts/install" "$BUNDLE_DIR/scripts/demo" "$BUNDLE_DIR/scripts/security" "$BUNDLE_DIR/scripts/e2e/tests" "$BUNDLE_DIR/scripts/e2e/target"

for bin in adc-targetd adc adc-workload adc-demo-sensor-gateway adc-mcp adc-priv-helper; do
  install -m 0755 "$build_dir/$bin" "$BUNDLE_DIR/bin/$bin"
done

cp -R "$ROOT_DIR/docs/." "$BUNDLE_DIR/docs/"
cp -R "$ROOT_DIR/demos/." "$BUNDLE_DIR/demos/"
cp -R "$ROOT_DIR/profiles/." "$BUNDLE_DIR/profiles/"
install -m 0644 "$ROOT_DIR/packaging/systemd/adc-targetd.service" "$BUNDLE_DIR/packaging/systemd/adc-targetd.service"
install -m 0755 "$ROOT_DIR/scripts/install/install-dev-env.sh" "$BUNDLE_DIR/scripts/install/install-dev-env.sh"
install -m 0755 "$ROOT_DIR/scripts/install/install-managed-mcp-user-service.sh" "$BUNDLE_DIR/scripts/install/install-managed-mcp-user-service.sh"
install -m 0755 "$ROOT_DIR/scripts/install/create-managed-mcp-enrollment-kit.sh" "$BUNDLE_DIR/scripts/install/create-managed-mcp-enrollment-kit.sh"
install -m 0755 "$ROOT_DIR/scripts/install/provision-managed-mcp-target.sh" "$BUNDLE_DIR/scripts/install/provision-managed-mcp-target.sh"
install -m 0755 "$ROOT_DIR/scripts/install/install-target-mcp-binaries.sh" "$BUNDLE_DIR/scripts/install/install-target-mcp-binaries.sh"
install -m 0755 "$ROOT_DIR/scripts/install/install-rust-security-tools.sh" "$BUNDLE_DIR/scripts/install/install-rust-security-tools.sh"
install -m 0755 "$ROOT_DIR/scripts/install/install-target-perf.sh" "$BUNDLE_DIR/scripts/install/install-target-perf.sh"
install -m 0755 "$ROOT_DIR/scripts/install/install-target-smoke-sudoers.sh" "$BUNDLE_DIR/scripts/install/install-target-smoke-sudoers.sh"
install -m 0755 "$ROOT_DIR/scripts/install/install-target-smoke-ko.sh" "$BUNDLE_DIR/scripts/install/install-target-smoke-ko.sh"
install -m 0755 "$ROOT_DIR/scripts/demo/run-sensor-gateway-demo.sh" "$BUNDLE_DIR/scripts/demo/run-sensor-gateway-demo.sh"
install -m 0755 "$ROOT_DIR/scripts/security/run-rust-security-checks.sh" "$BUNDLE_DIR/scripts/security/run-rust-security-checks.sh"
install -m 0755 "$ROOT_DIR/scripts/e2e/run-e2e.sh" "$BUNDLE_DIR/scripts/e2e/run-e2e.sh"
install -m 0755 "$ROOT_DIR/scripts/e2e/run-agent-quality-dogfood.sh" "$BUNDLE_DIR/scripts/e2e/run-agent-quality-dogfood.sh"
install -m 0755 "$ROOT_DIR/scripts/e2e/merge-target-smoke.sh" "$BUNDLE_DIR/scripts/e2e/merge-target-smoke.sh"
install -m 0755 "$ROOT_DIR/scripts/e2e/tests/run-managed-mcp-mtls-smoke.sh" "$BUNDLE_DIR/scripts/e2e/tests/run-managed-mcp-mtls-smoke.sh"
install -m 0755 "$ROOT_DIR/scripts/e2e/tests/run-managed-mcp-enrollment-kit-test.sh" "$BUNDLE_DIR/scripts/e2e/tests/run-managed-mcp-enrollment-kit-test.sh"
install -m 0755 "$ROOT_DIR/scripts/e2e/target/run-pi5-release-smoke.sh" "$BUNDLE_DIR/scripts/e2e/target/run-pi5-release-smoke.sh"
install -m 0755 "$ROOT_DIR/scripts/e2e/target/run-target-mcp-fleet-smoke.sh" "$BUNDLE_DIR/scripts/e2e/target/run-target-mcp-fleet-smoke.sh"
install -m 0755 "$ROOT_DIR/scripts/e2e/target/adc-target-smoke-root.sh" "$BUNDLE_DIR/scripts/e2e/target/adc-target-smoke-root.sh"
install -m 0755 "$ROOT_DIR/scripts/e2e/target/run-privileged-smoke.sh" "$BUNDLE_DIR/scripts/e2e/target/run-privileged-smoke.sh"
install -m 0755 "$ROOT_DIR/scripts/e2e/target/run-perf.sh" "$BUNDLE_DIR/scripts/e2e/target/run-perf.sh"

mkdir -p "$BUNDLE_DIR/kernel/adc_sensor_probe/tests"
install -m 0644 "$ROOT_DIR/kernel/adc_sensor_probe/Kbuild" "$BUNDLE_DIR/kernel/adc_sensor_probe/Kbuild"
install -m 0644 "$ROOT_DIR/kernel/adc_sensor_probe/Makefile" "$BUNDLE_DIR/kernel/adc_sensor_probe/Makefile"
install -m 0644 "$ROOT_DIR/kernel/adc_sensor_probe/adc_sensor_probe.c" "$BUNDLE_DIR/kernel/adc_sensor_probe/adc_sensor_probe.c"
install -m 0644 "$ROOT_DIR/kernel/adc_sensor_probe/adc_sensor_probe.h" "$BUNDLE_DIR/kernel/adc_sensor_probe/adc_sensor_probe.h"
install -m 0755 "$ROOT_DIR/kernel/adc_sensor_probe/tests/selftest.sh" "$BUNDLE_DIR/kernel/adc_sensor_probe/tests/selftest.sh"

cat >"$BUNDLE_DIR/manifest.json" <<JSON
{
  "name": "$BUNDLE_NAME",
  "version": "$VERSION",
  "target": "$TARGET_TRIPLE",
  "contains": [
    "release binaries",
    "target setup documentation",
    "sample profiles",
    "systemd service",
    "demo content and runner",
    "install, demo, security, and E2E scripts",
    "privileged target smoke sudoers installer",
    "optional KO source scaffold"
  ]
}
JSON

tar -C "$DIST_DIR" -czf "$ARCHIVE" "$BUNDLE_NAME"
sha256sum "$ARCHIVE" >"$ARCHIVE.sha256"

printf 'bundle=%s\narchive=%s\nchecksum=%s\n' "$BUNDLE_DIR" "$ARCHIVE" "$ARCHIVE.sha256"
