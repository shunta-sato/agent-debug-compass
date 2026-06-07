# Test Taxonomy

ADC tests are organized by the surface they protect. This file maps directories
to verification intent and the commands maintainers should run before changes
ship.

## Contract Tests

| Surface | Location | Purpose | Command |
|---|---|---|---|
| Static golden fixtures | `tests/golden/*.json` | Minimal public Agent-facing examples for every schema. | `python3 scripts/contract/validate-contracts.py --schema-dir schemas --fixture-dir tests/golden --fixture-dir contracts` |
| Adversarial contract fixtures | `scripts/contract/tests/validate-contracts-test.sh` | Reject root-cause wording, unsafe refs, missing invariants, and malformed public contracts. | `bash scripts/contract/tests/validate-contracts-test.sh` |
| Generated CLI/MCP fixtures | `target/contract-fixtures/` | Prove real generated outputs match public schemas. | `bash scripts/contract/validate-generated-contracts.sh` |
| Coverage manifest | `contracts/adc.contract_coverage.v1.json` | Ensure public `obs.*` / `adc.*` schemas are not orphaned. | `python3 scripts/contract/check-coverage.py --schema-dir schemas --coverage contracts/adc.contract_coverage.v1.json --repo-root .` |

Run all contract checks:

```bash
make contract
```

## Rust Unit and Integration Tests

| Crate / directory | Protected area |
|---|---|
| `crates/adc-core/tests/` | Core model, collectors, evidence, route packs, investigation facts, recorder runtime, coverage, trigger decisions, fleet semantics. |
| `crates/adc/tests/` | CLI workflows, generated CLI contract outputs, daemon operations, evidence commands, fleet commands. |
| `crates/adc-mcp/tests/` | MCP tool list, resources, managed listener behavior, generated MCP contract outputs. |
| `crates/adc-targetd/tests/` | Target daemon service loop, status output, Flight Recorder marker/trigger/runtime behavior. |
| `crates/adc-workload/tests/` | Synthetic workload helpers used by demos/tests. |
| `crates/adc-demo-sensor-gateway/tests/` | Demo workload modes and bounded output behavior. |
| `crates/adc-priv-helper/tests/` | Allowlisted privileged helper behavior. |

Primary commands:

```bash
cargo test --workspace
cargo test -q -p adc-core --test recorder
cargo test -q -p adc-core --test trigger
cargo test -q -p adc-targetd --test service
cargo test -q -p adc --test contract_outputs
cargo test -q -p adc-mcp --test contract_outputs
```

## Script Smoke Tests

| Location | Purpose | Command |
|---|---|---|
| `scripts/demo/tests/` | Demo script smoke and artifact checks. | `bash scripts/demo/tests/run-sensor-gateway-demo-test.sh` |
| `scripts/e2e/tests/` | Local E2E wrappers and documented hardware skips. | `make test-scripts` or individual `bash scripts/e2e/tests/*.sh` |
| `scripts/install/tests/` | Install helper dry-runs and safety checks. | `make test-scripts` |
| `scripts/security/tests/` | Security script dry-run and public-tree checks. | `make test-scripts` |

`make verify` runs script syntax checks and the local script smoke tests.

## Benchmark and Dogfood

| Surface | Location | Purpose | Command |
|---|---|---|---|
| Static Agent debugging benchmark | `benchmarks/scenarios/`, `scripts/benchmarks/` | Measures investigation quality, bounded evidence use, data-quality handling, and Flight Recorder pre-window advantage. | `bash scripts/benchmarks/tests/run-agent-debug-benchmark-test.sh` |
| Agent quality dogfood | `scripts/e2e/run-agent-quality-dogfood.sh` | Strict end-to-end quality check for symptom-first context, direct-shell comparison, typed routes, fleet degradation, budget, and safety/privacy. | `bash scripts/e2e/run-agent-quality-dogfood.sh` |

The benchmark is static and checked-in. It is not a substitute for real target
smoke or live workload evaluation.

## Hardware-Optional Tests

Hardware target smoke scripts live under `scripts/e2e/target/`. They require
Raspberry Pi or configured target machines and are not part of the default local
gate.

Examples:

```bash
bash scripts/e2e/target/run-pi5-release-smoke.sh
bash scripts/e2e/target/run-target-mcp-fleet-smoke.sh
bash scripts/e2e/target/run-perf-test.sh
bash scripts/e2e/target/run-target55-resource-discipline-smoke.sh \
  --host target55 \
  --binary-dir target/debug \
  --result-root tmp/target55-resource-discipline-smoke
bash scripts/e2e/target/run-target55-recorder-load-impact-smoke.sh \
  --host target55 \
  --binary-dir target/debug \
  --result-root tmp/target55-recorder-load-impact-smoke
```

When these are skipped, record the hardware/setup reason in the PR or release
notes. The target55 resource-discipline smoke is the PR10 hardware gate: it
checks no-trigger continuous ring write behavior, simulated battery-low
degradation, marker freeze resource accounting, and bounded recorder refs on a
configured same-LAN target. The target55 load-impact smoke adds explicit
CPU+memory workload comparison and reports workload slowdown, `adc-targetd` CPU
seconds/ratio, peak RSS, recorder write categories, `deployability_passed`, and
`resource_violation`.

The default target55 load-impact smoke is production-safe deployability
evidence. Use `--evaluation-mode high_frequency_stress --profile-interval-ms 10`
to verify that aggressive configured intervals are pressure-safe clamped for
semantic counter sampling and do not become global high-frequency polling.

## Where to Add New Tests

- New public contract: add schema under `schemas/`, golden fixture under
  `tests/golden/`, contract coverage entry, generated fixture if surfaced, and
  adversarial fixture if safety/trust-sensitive.
- New core runtime behavior: add focused tests under `crates/adc-core/tests/`.
- New CLI output or workflow: add tests under `crates/adc/tests/` and generated
  contract fixtures if Agent-facing JSON is returned.
- New MCP surface: add tests under `crates/adc-mcp/tests/` and generated MCP
  fixtures.
- New daemon/Flight Recorder runtime path: add service tests under
  `crates/adc-targetd/tests/` plus recorder core tests where possible.
- New benchmark scenario: add checked-in scenario JSON under
  `benchmarks/scenarios/` and update benchmark assertions if a new metric is
  introduced.

## Default Gate

Before submitting a behavior or contract change:

```bash
make verify
make contract
bash scripts/benchmarks/tests/run-agent-debug-benchmark-test.sh
bash scripts/e2e/run-agent-quality-dogfood.sh
PATH="$HOME/.cargo/bin:$PATH" bash scripts/security/run-rust-security-checks.sh
```

Docs-only changes may run a smaller gate, but any change that affects
Agent-facing outputs, schemas, refs, recorder semantics, MCP tools, CLI JSON, or
runtime behavior should use the full gate.
