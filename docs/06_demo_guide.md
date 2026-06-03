# Release Demo Guide

The primary release demo is the sensor gateway bug demo. It shows how
`adc-targetd` turns buggy app symptoms into bounded evidence for a human or
Agent without dumping raw artifacts into the initial context.

## Run Locally

```bash
scripts/demo/run-sensor-gateway-demo.sh --quick
```

In a release bundle, run the same script from the bundle root. It will use
`bin/` binaries when they are present and fall back to `cargo run` in a source
checkout.

The default output root is:

```text
demo-results/sensor-gateway/
```

Important outputs:

- `agent_context.md`: bounded evidence/window/search/compare excerpts plus direct `obs.get_agent_context` output and investigation route.
- `reports/*.json`: command reports, comparisons, MCP tool list.
- `reports/daemon.agent_context.md`: direct Agent context pack for the daemon-triggered run.
- `reports/daemon.agent_context.prom`: OpenMetrics summary adapter output for interop smoke.
- `state/runs/*`: manifests, evidence indexes, windows, timelines, and raw artifact refs.

## Manual Capture Add-On

For a performance-style local check, run a bounded capture after the demo:

```bash
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- observe --run-id R-DEMO-CAPTURE-10S --duration-sec 10 --interval-ms 100
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- agent-context --run-id R-DEMO-CAPTURE-10S
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- agent-context --run-id R-DEMO-CAPTURE-10S --format openmetrics
```

In a release bundle, replace `cargo run -q -p adc --` with
`./bin/adc`.

For a service-first investigation path, attach a bounded service state to the
context and then open the dedicated service pack only when needed:

```bash
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- observe --run-id R-DEMO-SSH --duration-sec 5 --service-name ssh
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- agent-context --run-id R-DEMO-SSH
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- investigate bug --run-id R-DEMO-SSH --symptom "latency timeout" --service-name ssh
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- investigate route-packs
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- investigate start --run-id R-DEMO-SSH --service-name ssh
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- investigate continue --run-id R-DEMO-SSH --service-name ssh --step-id IR001
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- investigate continue --run-id R-DEMO-SSH --service-name ssh --step-id IR002 --session-id S-route-R-DEMO-SSH-IR001
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- investigate session --run-id R-DEMO-SSH --session-id S-route-R-DEMO-SSH-IR001
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- investigate service ssh
```

`investigate bug` returns `obs.symptom_context.v1`: normalized symptom, compiled
route packs, typed facts, explicit fact gaps, ranked refs, and declarative safe
probe packs. `investigate route-packs` lists the typed, cause-neutral route registry.
`continue` responses include extracted evidence facts and typed branch
evaluations; passing `--session-id` lets a later `investigate session` return a
compact resume state instead of relying on chat history.

## Same-network Fleet Add-On

For a small lab fleet, first discover conservative candidates from the local
neighbor table, then run an explicit inventory. Fleet output remains bounded:
start with `fleet_evidence.yaml`, then follow each target `evidence_ref`.

```bash
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- fleet discover --cidr 192.0.2.0/24

cat > /tmp/adc-demo-targets.yaml <<'YAML'
targets:
  - id: local-demo
    transport: local
  - id: pi5-demo-a
    transport: mcp_stdio_over_ssh
    host: pi5-demo-a.local
    mcp_server_path: /home/pi/.local/bin/adc-mcp
YAML

ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- fleet snapshot --inventory /tmp/adc-demo-targets.yaml --fleet-run-id F-DEMO-SNAPSHOT
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- fleet preflight --inventory /tmp/adc-demo-targets.yaml
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- fleet observe --inventory /tmp/adc-demo-targets.yaml --fleet-run-id F-DEMO-CAPTURE --duration-sec 5 --interval-ms 100
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- fleet investigate service ssh --inventory /tmp/adc-demo-targets.yaml --fleet-run-id F-DEMO-CAPTURE
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- investigate start --fleet-run-id F-DEMO-CAPTURE --service-name ssh --inventory /tmp/adc-demo-targets.yaml
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- investigate continue --fleet-run-id F-DEMO-CAPTURE --service-name ssh --step-id IR002
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- investigate session --fleet-run-id F-DEMO-CAPTURE --session-id S-route-F-DEMO-CAPTURE-IR002
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- fleet evidence --fleet-run-id F-DEMO-CAPTURE
ADC_HOME="$PWD/demo-results/sensor-gateway/state" \
  cargo run -q -p adc -- agent-context --fleet-run-id F-DEMO-CAPTURE
```

Remote targets need `adc-mcp` in `PATH` or a fixed `mcp_server_path`.
Authentication failures, unreachable hosts, and service collector gaps are
recorded in per-target `data_quality`; successful targets still produce useful
evidence and service packs.

## Agent Quality Dogfood Gate

After building or before release, run the strict Agent investigation quality
dogfood. By default it uses local observations plus a generated partial-success
fleet inventory; set `ADC_AGENT_QUALITY_INVENTORY` to point it at a real target
fleet.

```bash
scripts/e2e/run-agent-quality-dogfood.sh
sed -n '1,220p' e2e-results/agent-quality-dogfood-*/STRICT_DOGFOOD_REPORT.md
```

The gate emits `quality_scorecard.json` with schema
`obs.agent_quality_scorecard.v2`. It fails below the threshold if symptom-first
context compilation, direct-shell comparison, typed predicates, typed facts,
substring ambiguity regression, session compaction, age-based cleanup, packet
budgets, secret scan, or degraded-fleet evidence regress.

## Story

The demo runs three scenarios:

- `baseline`: normal sensor gateway behavior.
- `retry-storm`: immediate retry without backoff, producing warning evidence.
- `memory-leak`: bounded retained buffer after a synthetic error.

The useful contrast is not "adc-targetd found the root cause." The useful contrast
is that it creates a structured investigation path:

```text
investigation_start/route_packs -> investigation_continue/typed_branch_state/session -> agent_context/playbook -> evidence_index -> window/series -> raw_slice -> compare_runs
```

This is better than handing an Agent a pile of app logs, system logs, and manual
notes with unrelated timestamps.

## Pi5 Extension

The non-root path is the release acceptance path. On a Raspberry Pi 5 target,
the same demo can be combined with target setup and privileged smoke scripts to
add thermal, RP1/PCIe, perf, ftrace, and KO evidence where available.

If privileged target signals are not available, record the absence as
`data_quality` or documented skip rather than treating the demo as failed.
