# COMMANDS.md

Canonical commands for Agent Debug Compass development, verification, packaging,
and artifact review.

Run commands from the repository root unless a section says otherwise.

## Environment

Required for full local verification:

- Rust toolchain and Cargo.
- Python 3.
- Bash.
- Linux userland with standard `/proc` files for runtime tests.

Recommended before contract validation:

```bash
python3 -m pip install -r scripts/contract/requirements.txt
```

Set `ADC_HOME` to isolate local runs:

```bash
export ADC_HOME="$PWD/.agent-debug-compass"
```

## Primary Gates

| Command | Purpose | Requires Cargo | Hardware |
|---|---|---:|---|
| `make verify` | Default local development gate: format, lint, check, Rust tests, script smoke, contract validation. | yes | no |
| `make contract` | Static and generated public contract validation plus coverage manifest check. | yes for generated fixtures | no |
| `make security-check` | Rust dependency/security/supply-chain checks. | yes | no |
| `make e2e-local` | Local E2E script with documented target skips. | yes | no |
| `make release-gate` | `verify`, security check, E2E local, and package build. | yes | no |
| `make package-release` | Build release bundle under `dist/`. | yes | no |

## Focused Rust Checks

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace --all-targets
cargo test --workspace
```

Useful focused tests:

```bash
cargo test -q -p adc-core --test recorder
cargo test -q -p adc-core --test trigger
cargo test -q -p adc --test daemon_ops
cargo test -q -p adc --test contract_outputs
cargo test -q -p adc-mcp --test contract_outputs
cargo test -q -p adc-targetd --test service
```

## Contract Checks

Static fixtures only:

```bash
python3 scripts/contract/validate-contracts.py \
  --schema-dir schemas \
  --fixture-dir tests/golden \
  --fixture-dir contracts
```

Adversarial/static validator tests:

```bash
bash scripts/contract/tests/validate-contracts-test.sh
```

Generated CLI/MCP fixtures:

```bash
bash scripts/contract/validate-generated-contracts.sh
```

Coverage manifest:

```bash
python3 scripts/contract/check-coverage.py \
  --schema-dir schemas \
  --coverage contracts/adc.contract_coverage.v1.json \
  --repo-root .
```

`make contract` runs all of the above.

## Demos and E2E

Sensor gateway demo:

```bash
bash scripts/demo/run-sensor-gateway-demo.sh --quick
bash scripts/demo/tests/run-sensor-gateway-demo-test.sh
```

Local E2E:

```bash
bash scripts/e2e/run-e2e.sh
```

Agent quality dogfood:

```bash
bash scripts/e2e/run-agent-quality-dogfood.sh
```

The dogfood emits a result directory under `e2e-results/agent-quality-dogfood-*`
with `obs.agent_quality_dogfood.v2` data and a strict report.

## Benchmarks

Static Agent debugging benchmark:

```bash
bash scripts/benchmarks/tests/run-agent-debug-benchmark-test.sh
```

Direct runner:

```bash
python3 scripts/benchmarks/run-agent-debug-benchmark.py \
  --scenario-dir benchmarks/scenarios \
  --output tmp/benchmark-report.json
```

Current benchmark scenarios are checked-in and static. They are useful for
contract and investigation-quality regression, not a claim of broad real-device
performance.

## Security and Supply Chain

```bash
PATH="$HOME/.cargo/bin:$PATH" bash scripts/security/run-rust-security-checks.sh
```

This runs `cargo deny`, `cargo audit`, `cargo machete`, and optional
`cargo geiger` if available. Existing allowed audit warnings are reported by the
tool output. Optional `cargo geiger` may fail to produce a usable metric report;
record that as a check note rather than silently ignoring it.

Dry-run help/smoke:

```bash
bash scripts/security/run-rust-security-checks.sh --dry-run
```

## Hardware-Optional Target Smoke

These commands require target setup and are not part of the default local gate:

```bash
bash scripts/e2e/target/run-pi5-release-smoke.sh
bash scripts/e2e/target/run-target-mcp-fleet-smoke.sh
bash scripts/e2e/target/run-perf-test.sh
```

PR10 recorder resource discipline on `target55`:

```bash
cargo build -p adc -p adc-targetd
bash scripts/e2e/target/run-target55-resource-discipline-smoke.sh \
  --host target55 \
  --binary-dir target/debug \
  --result-root tmp/target55-resource-discipline-smoke
```

This smoke copies the local `adc` and `adc-targetd` binaries to a temporary
directory on `target55`, runs a no-trigger continuous recorder check, simulates
`battery_low`, freezes a marker incident, resolves bounded recorder refs, and
copies a `summary.json` report back under the result root.

PR10 recorder load-impact smoke on `target55`:

```bash
cargo build -p adc -p adc-targetd
bash scripts/e2e/target/run-target55-recorder-load-impact-smoke.sh \
  --host target55 \
  --binary-dir target/debug \
  --result-root tmp/target55-recorder-load-impact-smoke
```

By default this is a production-safe deployability smoke. It uses a low-rate
recorder profile and fails if `adc-targetd` exceeds the production CPU threshold
or writes through the continuous memory ring. It reports `deployability_passed`,
`resource_violation`, workload slowdown, `adc-targetd` CPU seconds/ratio, peak
RSS, and recorder write categories in `load_impact_summary.json`. It does not
claim battery drain on AC-powered targets.

High-frequency always-on profiles are measured separately as stress findings,
not as deployability evidence:

```bash
bash scripts/e2e/target/run-target55-recorder-load-impact-smoke.sh \
  --host target55 \
  --binary-dir target/debug \
  --result-root tmp/target55-recorder-load-impact-stress \
  --profile-interval-ms 10 \
  --evaluation-mode high_frequency_stress
```

If this stress run reports `deployability_passed=false`, that is a known
resource discipline finding: 10ms global polling is not accepted as an
always-on production mode.

Optional install/provision helpers:

```bash
bash scripts/install/install-target-mcp-binaries.sh --help
bash scripts/install/provision-managed-mcp-target.sh --help
```

Do not treat hardware-optional scripts as skipped silently. Record the hardware
reason when they are not run.

## Release and Public Export

Release bundle:

```bash
bash scripts/package/build-release-bundle.sh --force
```

Public tree export:

```bash
bash scripts/package/create-public-tree.sh \
  --output /tmp/agent-debug-compass-public \
  --force \
  --init-git
```

Do not publish private development history, local `plans/`, generated result
directories, or temporary artifacts.

## Zip / Archive Caveat

Some zip extraction tools drop executable bits. If a script fails with
`Permission denied` after archive extraction, run it through `bash` or `python3`
explicitly:

```bash
bash scripts/e2e/run-e2e.sh
python3 scripts/contract/validate-contracts.py --schema-dir schemas --fixture-dir tests/golden --fixture-dir contracts
```

Release tarballs should preserve executable bits; zip artifacts should be
verified before treating script permission failures as repository regressions.
