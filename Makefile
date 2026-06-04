.PHONY: help verify build-debug build-release format lint analysis test-unit test-integration test-scripts contract security-check release-gate demo-sensor-gateway e2e-local ko-selftest package-release

help:
	@printf '%s\n' \
	  'adc-targetd development commands:' \
	  '  make verify            Run format, lint, analysis, and tests' \
	  '  make build-debug       Build debug binaries' \
	  '  make build-release     Build release binaries' \
	  '  make format            Check Rust formatting' \
	  '  make lint              Run clippy' \
	  '  make analysis          Run static analysis checks' \
	  '  make test-unit         Run unit tests' \
	  '  make test-integration  Run integration tests' \
	  '  make test-scripts      Run shell syntax checks and script smoke tests' \
	  '  make contract          Validate schema/golden contract fixtures' \
	  '  make security-check    Run Rust dependency/security/supply-chain checks' \
	  '  make release-gate      Run verify, security-check, E2E, and package checks' \
	  '  make demo-sensor-gateway Run the release demo locally' \
	  '  make e2e-local         Run local E2E with documented target skips' \
	  '  make ko-selftest       Build optional KO self-test harness' \
	  '  make package-release   Build release bundle in dist/'

verify: format lint analysis test-unit test-integration test-scripts contract

build-debug:
	cargo build --workspace

build-release:
	cargo build --workspace --release

format:
	cargo fmt --all -- --check

lint:
	cargo clippy --workspace --all-targets -- -D warnings

analysis:
	cargo check --workspace --all-targets

test-unit:
	cargo test --workspace --lib --bins

test-integration:
	cargo test --workspace --tests

test-scripts:
	bash -n scripts/demo/*.sh scripts/e2e/run-e2e.sh scripts/e2e/merge-target-smoke.sh scripts/e2e/target/*.sh scripts/install/*.sh scripts/package/*.sh scripts/security/*.sh
	bash scripts/demo/tests/run-sensor-gateway-demo-test.sh
	bash scripts/e2e/tests/merge-target-smoke-test.sh
	bash scripts/e2e/tests/run-pi5-release-smoke-test.sh
	bash scripts/e2e/tests/run-perf-test.sh
	bash scripts/install/tests/install-target-perf-test.sh
	bash scripts/install/tests/install-rust-security-tools-test.sh
	bash scripts/security/tests/run-rust-security-checks-test.sh
	bash scripts/security/run-rust-security-checks.sh --dry-run

contract:
	python3 scripts/contract/validate-contracts.py --schema-dir schemas --fixture-dir tests/golden

security-check:
	scripts/security/run-rust-security-checks.sh

release-gate: verify security-check e2e-local package-release

demo-sensor-gateway:
	scripts/demo/run-sensor-gateway-demo.sh

e2e-local:
	scripts/e2e/run-e2e.sh

ko-selftest:
	kernel/adc_sensor_probe/tests/selftest.sh --build-only

package-release:
	scripts/package/build-release-bundle.sh --force
