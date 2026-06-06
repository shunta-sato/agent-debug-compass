# Agent Debug Compass

Agent Debug Compass is the evidence-governed debugging runtime for AI Agents
operating on real edge devices.

It turns volatile target behavior into bounded evidence, artifact refs,
`data_quality`, `artifact_trust`, observation coverage, Flight Recorder incident
windows, and safe investigation contracts. It does not let Agents scrape shells
blindly, ingest raw artifacts wholesale, or assert root cause prematurely.

## Why It Exists

Direct-shell Agent debugging is a poor default for edge incidents:

- the Agent often starts after the important Tx evidence window has disappeared;
- raw logs and config can contain instruction-like target text;
- missing signals are easy to misread as absence of a problem;
- unsafe probes and ad hoc shell commands are hard to audit;
- fleet and target identity blur quickly when evidence is copied around.

ADC is the layer between an Agent and the target. It records observations,
preserves bounded windows, labels trust, exposes missing evidence, and gives the
Agent falsifiable investigation state instead of conclusions.

## What ADC Gives an Agent

- `obs.agent_context.v1`: compact first-read context for a run or fleet.
- `obs.symptom_context.v1`: symptom-first investigation state with typed facts,
  missing fact IDs, route packs, falsifiable hypotheses, and safe probe plans.
- `obs.ref_resolution.v1`: bounded `artifact://...` ref resolution with
  `artifact_trust`, truncation, and `data_quality`.
- Flight Recorder incident bundles: delayed-incident pre-window evidence,
  `loss_report`, `coverage`, trigger decisions, dataset metadata, and recorder
  overhead.
- Safety contracts: capability reports, safety policy, probe plans, and
  auditable probe results.

ADC is not a root-cause engine. It records evidence and information debt so an
Agent can investigate safely.

## Verified Environment

Current public readiness is Raspberry Pi-first.

Verified so far:

- Raspberry Pi 5 as local/controller machine.
- Raspberry Pi 4 as same-LAN target through target MCP/fleet flows.
- Linux `aarch64` userspace with rootless local observation paths.
- Rootless target MCP install into a user's home directory.

Not yet broadly verified:

- Jetson, QCOM/Snapdragon, x86 edge boxes, or non-Linux targets.
- Jetson-specific GPU/power/thermal collectors.
- Broad distribution/kernel compatibility beyond the tested Raspberry Pi
  OS/Linux setup.

Unsupported capabilities should become explicit `data_quality` gaps where
possible. Treat this release as validated only on the Raspberry Pi lab path
above unless you run your own target smoke.

## 3-Minute Source Demo

Preconditions:

- Linux with Rust/Cargo, `git`, `bash`, and standard `/proc` files.
- Writable `ADC_HOME`; root is not required for these commands.

From the repository root:

```bash
cargo test -q --workspace
export ADC_HOME="$PWD/.agent-debug-compass"
cargo run -q -p adc -- doctor
cargo run -q -p adc -- observe --run-id R-DEMO --duration-sec 5 --interval-ms 500
cargo run -q -p adc -- investigate bug --run-id R-DEMO --symptom "latency timeout"
```

The final command returns `obs.symptom_context.v1`: normalized symptom, selected
route packs, typed facts, missing fact IDs, ranked refs, falsifiable hypotheses,
safe probe candidates, safety policy, and `data_quality`.

## Flight Recorder Demo

Flight Recorder keeps an ultra-light in-memory ring in `adc-targetd`. A marker
or symptom trigger materializes a bounded incident bundle with retained
pre-window samples, coverage, loss semantics, artifact trust, trigger decisions,
and dataset-ready refs.

```bash
export ADC_HOME="$PWD/.agent-debug-compass"
cargo run -q -p adc -- arm --profile pi5_basic
cargo run -q -p adc -- recorder mark --symptom "camera frame drop observed around now"
cargo run -q -p adc-targetd -- --service-for-ms 1000
cargo run -q -p adc -- recorder incidents
cargo run -q -p adc -- recorder incident get --incident-id INC-marker-...
cargo run -q -p adc -- investigate ref --ref artifact://recorder/incidents/INC-marker-.../coverage.json
```

Current MVP behavior:

- continuous retention is memory-backed and volatile;
- frozen incidents are bounded artifact bundles;
- current runtime freezes retained pre-window evidence only;
- `post_window_ms` exists for forward compatibility and is `0`;
- recorder refs are `artifact://recorder/...`, not raw filesystem paths;
- trigger decisions are symptom/event preservation decisions, not cause claims.

## Core Workflows

### Symptom-First Investigation

```bash
adc observe --run-id R-APP --duration-sec 5 \
  --log-file app.log \
  --domain-events-file events.jsonl \
  --config-file app.conf \
  --service-name my-service

adc investigate bug --run-id R-APP --symptom "latency timeout" --service-name my-service
```

Use the result by checking `data_quality`, opening only recommended refs through
`obs.get_ref` / `adc investigate ref`, and treating hypotheses as falsifiable
state, not conclusions.

### Before/After Comparison

```bash
adc snapshot --run-id R-BEFORE
# run your repro or workload outside ADC
adc snapshot --run-id R-AFTER
adc compare --before R-BEFORE --after R-AFTER
```

The comparison returns bounded metric deltas and refs to the before/after
artifacts. The Agent should trust the comparison only if `profile_match` and
`data_quality` are acceptable.

### Bounded Service Investigation

```bash
adc investigate service ssh
```

This returns service state, process/port summary, bounded journal leads, raw
refs, next probes, and `data_quality` without exposing arbitrary shell.

### Same-LAN Fleet Observation

```bash
adc fleet discover --cidr 198.51.100.0/24 --write-inventory /tmp/adc-targets.yaml
adc fleet observe --inventory /tmp/adc-targets.yaml --fleet-run-id F-OBSERVE --duration-sec 5
adc agent-context --fleet-run-id F-OBSERVE --format json
```

Fleet context preserves target identity and partial success. Permission denied
or unreachable targets become per-target `data_quality`, not hidden failures.

## Binaries

| Binary | Role |
|---|---|
| `adc` | CLI for capture, evidence, investigation, recorder, fleet, and release workflows. |
| `adc-targetd` | Target-local service mode for armed profiles, Flight Recorder ring, markers, and trigger capture. |
| `adc-mcp` | MCP server/listener exposing bounded `obs.*` tools. |
| `adc-priv-helper` | Optional allowlisted helper for explicit privileged smoke paths. |
| `adc-workload` | Synthetic workload generator used by tests and demos. |
| `adc-demo-sensor-gateway` | Sensor gateway demo workload. |

## Agent Surface

The Agent-facing surface is `obs.*` over MCP plus the `adc` CLI for local and
scripted use.

Important contracts include:

- `obs.agent_context.v1`
- `obs.symptom_context.v1`
- `obs.investigation_start.v1`
- `obs.investigation_continue.v1`
- `obs.ref_resolution.v1`
- `obs.artifact_trust.v1`
- `obs.data_quality.v1`
- `obs.recorder_*`
- `obs.trigger_policy.v1`
- `obs.trigger_decision.v1`

The full public schema set is tracked through
`contracts/adc.contract_coverage.v1.json` and validated by `make contract`.

## Managed MCP Fleet Mode

Managed MCP can replace SSH for enrolled targets:

```bash
# on the target
adc-mcp --target-mode --managed-listen 0.0.0.0:8765 --managed-token-file /path/token

# on the controller
adc fleet enroll --target-id pi-target --transport managed_mcp --host pi-target.local --port 8765 --auth-token-file /path/token
```

Target mode exposes target-local observation tools only; controller fleet and
discovery tools are hidden.

## Demo, Benchmark, and Verification

```bash
bash scripts/demo/run-sensor-gateway-demo.sh --quick
bash scripts/benchmarks/tests/run-agent-debug-benchmark-test.sh
make contract
make verify
```

The benchmark currently uses checked-in scenarios. It includes
`camera_inference_degradation_flight_recorder`, which compares direct shell
after Ty, ADC on-demand after Ty, and ADC Flight Recorder with retained Tx
pre-window evidence. It scores evidence availability, missing-evidence
distinction, bounded refs, unsafe command count, and cause-claim avoidance. It
does not score root-cause accuracy.

See [COMMANDS.md](COMMANDS.md) for the complete command map and
[tests/README.md](tests/README.md) for test taxonomy.

## Current State and Roadmap

- Current implementation truth: [docs/00_current_state.md](docs/00_current_state.md)
- Flight Recorder architecture: [docs/architecture/adc-flight-recorder.md](docs/architecture/adc-flight-recorder.md)
- Investigation operating layer architecture:
  [docs/architecture/agent-investigation-operating-layer.md](docs/architecture/agent-investigation-operating-layer.md)
- Target setup: [docs/04_target_setup.md](docs/04_target_setup.md)
- Security checks: [docs/05_security_checks.md](docs/05_security_checks.md)
- Security posture: [SECURITY.md](SECURITY.md), [docs/security.md](docs/security.md)

## Release Bundle

```bash
bash scripts/package/build-release-bundle.sh --force
ls dist/agent-debug-compass-0.1.0-*-linux.tar.gz
```

Release bundles contain binaries, scripts, docs, profiles, packaging, benchmark
scenarios, and optional kernel probe source.

For public export:

```bash
bash scripts/package/create-public-tree.sh --output /tmp/agent-debug-compass-public --force --init-git
```

Do not push private development history or local generated result directories.
