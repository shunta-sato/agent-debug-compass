# Agent Debug Compass

Agent Debug Compass is an edge-first investigation operating layer for AI agents investigating target-device bugs.

It turns local or same-LAN target observations into compact, bounded investigation context with evidence refs, data-quality gaps, falsifiable hypotheses, safe probe plans, artifact trust labels, and auditable probe results.

## What It Is

Agent Debug Compass is not a root-cause engine and not a generic metrics dashboard. It is the investigation layer between an AI Agent and an edge target.

It helps an Agent answer:

- What target am I looking at?
- What bounded evidence is already available?
- Which refs should I open first?
- What is missing or low quality?
- Which hypotheses are still falsifiable, and what safe probe should reduce uncertainty next?

The Agent-facing surface is `obs.*` over MCP, plus the `adc` CLI for local and scripted use.

## Verified Environment

Current public readiness is Raspberry Pi-first, not a broad hardware compatibility claim.

Verified so far:

- Raspberry Pi 5 as the local/controller machine.
- Raspberry Pi 4 as a same-LAN target through target MCP/fleet flows.
- Linux `aarch64` userspace with rootless local observation paths.
- Rootless target MCP install into a user's home directory.

Not yet verified:

- Jetson, QCOM/Snapdragon, x86 edge boxes, or non-Linux targets.
- Jetson-specific GPU/power/thermal collectors.
- Broad distribution/kernel compatibility beyond the tested Raspberry Pi OS/Linux setup.

The code is designed so unsupported capabilities become `data_quality` gaps where possible, but this release should be treated as validated only on the Raspberry Pi 5 + Raspberry Pi 4 lab setup above.

## Concrete Agent Workflows

Agent Debug Compass does not ask an Agent to scrape `/proc`, parse logs, and decide which file to open first. It packages observations into a small context object that tells the Agent:

- the target/run/fleet identity,
- the highest-value refs to open next,
- the known facts and missing facts,
- the data-quality gaps,
- the next safe probe options.

The output is intentionally not a conclusion. It is an investigation starting point.

### 1. Start From a Symptom

Run a bounded observation, then compile it around the symptom:

```bash
adc observe --run-id R-APP --duration-sec 5 \
  --log-file app.log \
  --domain-events-file events.jsonl \
  --config-file app.conf \
  --service-name my-service

adc investigate bug --run-id R-APP --symptom "latency timeout" --service-name my-service
```

The Agent receives an `obs.symptom_context.v1` object like this, abridged:

```json
{
  "schema_version": "obs.symptom_context.v1",
  "symptom": {"normalized": "latency_timeout"},
  "target_summary": {"target_ids": ["local"], "captured_count": 1, "failed_count": 0},
  "agent_context": {
    "schema_version": "obs.agent_context.v1",
    "derived_fact_count": 15,
    "recommended_refs": [
      {"label": "log", "raw_ref": "artifact://raw/app.log"},
      {"label": "journald", "raw_ref": "artifact://raw/journald.jsonl"},
      {"label": "domain_event", "raw_ref": "artifact://raw/domain_events.jsonl"}
    ]
  },
  "investigation_route": {
    "steps": [
      {
        "step_id": "IR001",
        "title": "Correlate service state with observed signals",
        "refs": [
          {"label": "service_state", "raw_ref": "artifact://raw/service_state.json"}
        ],
        "branch_conditions": [
          {"if_observed": "service availability is unavailable or state is unknown"},
          {"if_observed": "service is available and log/domain refs exist"}
        ],
        "cause_neutral": true
      }
    ]
  },
  "hypothesis_set": {
    "schema_version": "obs.hypothesis_set.v1",
    "hypotheses": [
      {
        "hypothesis_id": "H001",
        "status": "needs_evidence",
        "claim_boundary": "hypothesis_only",
        "missing_evidence": ["process.runqueue_latency"],
        "next_discriminating_probes": ["probe.baseline_observe"]
      }
    ]
  },
  "probe_plan": {
    "schema_version": "obs.probe_plan.v1",
    "candidate_probes": [
      {
        "probe_id": "probe.baseline_observe",
        "safety_status": "allowed",
        "cause_neutral": true
      }
    ]
  }
}
```

How an Agent uses it:

1. Check `data_quality` first. If evidence is missing or permission-limited, repair that before guessing.
2. Open only the recommended refs, for example `artifact://raw/app.log` or `artifact://raw/service_state.json`, through `obs.get_ref` / `obs.get_raw_slice`.
3. Use `hypothesis_set` as falsifiable investigation state, not as a conclusion.
4. Follow the route step and branch conditions, then call `obs.continue_investigation` with the opened refs.
5. Choose a probe from `probe_plan` only when it reduces explicit uncertainty and stays within `safety_policy`.

### 2. Compare Before and After a Reproduction

Capture the target before and after a workload that you run outside ADC:

```bash
adc snapshot --run-id R-BEFORE
# run your repro or workload outside ADC
adc snapshot --run-id R-AFTER
adc compare --before R-BEFORE --after R-AFTER
```

The output contains bounded deltas and refs:

```json
{
  "before_run_id": "R-BEFORE",
  "after_run_id": "R-AFTER",
  "profile_match": true,
  "metric_deltas": {
    "cpu.total_jiffies": {"delta": 89.0, "unit": "jiffies"},
    "memory.mem_available_kb": {"delta": -1040.0, "unit": "KiB"}
  },
  "raw_refs": {
    "before_timeline": "artifact://runs/R-BEFORE/timeline.jsonl",
    "after_timeline": "artifact://runs/R-AFTER/timeline.jsonl"
  },
  "data_quality": {"notes": ["profile ids match"]}
}
```

How an Agent uses it:

1. Trust the comparison only if `profile_match` and `data_quality` are acceptable.
2. Rank the investigation by `metric_deltas` instead of reading both raw timelines in full.
3. Open the before/after refs only for the metrics that moved.

### 3. Investigate a Service Without Dumping Journals

Ask for a fixed, bounded service pack:

```bash
adc investigate service ssh
```

The output is a cause-neutral service investigation:

```json
{
  "schema_version": "obs.service_investigation.v1",
  "service_name": "ssh",
  "service_state": {"availability": "available", "active_state": "active"},
  "journal_leads": [
    {"severity_hint": "warning", "message": "..."},
    {"severity_hint": "error", "message": "..."}
  ],
  "raw_refs": {
    "service_state": "artifact://service_investigations/ssh/service_state.json",
    "journal_leads": "artifact://service_investigations/ssh/journal_leads.json"
  },
  "next_probe_options": [
    {"probe_id": "observe_service_window", "required_privilege": "none"}
  ],
  "data_quality": {"missing": []}
}
```

How an Agent uses it:

1. Read service availability and journal lead counts without ingesting the whole journal.
2. Open `journal_leads` or `service_state` refs when the short pack is insufficient.
3. Select `observe_service_window` if resource series must be correlated with service logs.

### 4. Observe a Same-LAN Fleet, Including Partial Failure

Discover or enroll targets, then run a fleet observation:

```bash
adc fleet discover --cidr 198.51.100.0/24 --write-inventory /tmp/adc-targets.yaml
adc fleet observe --inventory /tmp/adc-targets.yaml --fleet-run-id F-OBSERVE --duration-sec 5
adc agent-context --fleet-run-id F-OBSERVE --format json
```

The fleet context separates target identities and preserves partial success:

```json
{
  "schema_version": "obs.agent_context.fleet.v1",
  "fleet_run_id": "F-OBSERVE",
  "target_count": 3,
  "captured_count": 2,
  "failed_count": 1,
  "target_matrix": [
    {"target_id": "pi-local", "status": "captured", "evidence_ref": "artifact://runs/.../evidence_index.yaml"},
    {"target_id": "pi-remote", "status": "captured", "evidence_ref": "artifact://fleet_runs/.../evidence_index.yaml"},
    {
      "target_id": "pi-denied",
      "status": "permission_denied",
      "data_quality": {"missing": ["permission_denied: obs.observe command failed"]}
    }
  ]
}
```

How an Agent uses it:

1. Continue with the two captured targets instead of discarding the whole fleet run.
2. Treat `permission_denied` as evidence debt, not as a hidden failure.
3. Open per-target `evidence_ref` values independently so target identity, profile, and artifacts never blur together.

Managed MCP can replace SSH for fleet targets:

```bash
# on the target
adc-mcp --target-mode --managed-listen 0.0.0.0:8765 --managed-token-file /path/token

# on the controller
adc fleet enroll --target-id pi-target --transport managed_mcp --host pi-target.local --port 8765 --auth-token-file /path/token
```

The Agent still receives the same fleet evidence shape, but the target transport is `managed_mcp` and target mode hides controller-only tools.

## Quick Start From Source

This path is for trying Agent Debug Compass from a source checkout. For a target install, use the release bundle or target setup guide.

Preconditions:

- Linux on `aarch64`; Raspberry Pi 5 is the primary verified host.
- Rust toolchain and Cargo installed.
- `git`, `bash`, and standard Linux `/proc` files available.
- A writable `ADC_HOME`; root is not required for the default commands below.
- Optional for richer service evidence: `systemctl` / `journalctl` available to the current user.
- Optional for same-LAN target tests: another Raspberry Pi reachable through SSH or managed MCP enrollment.

From the repository root:

```bash
cargo test -q --workspace
export ADC_HOME="$PWD/.agent-debug-compass"
cargo run -q -p adc -- doctor
cargo run -q -p adc -- observe --run-id R-DEMO --duration-sec 5 --interval-ms 500
cargo run -q -p adc -- investigate bug --run-id R-DEMO --symptom "latency timeout"
```

The last command returns `obs.symptom_context.v1`: normalized symptom, selected route packs, typed facts, missing fact IDs, recommended refs, falsifiable hypotheses, safe probe plans, safety policy, and `data_quality`.

For target installation on another Raspberry Pi, start with [docs/04_target_setup.md](docs/04_target_setup.md). For public release packaging, see the release bundle section below.

## Main Commands

```bash
# One bounded local observation.
ADC_HOME="$PWD/.agent-debug-compass" \
  cargo run -q -p adc -- observe --run-id R-LOCAL --duration-sec 10 --interval-ms 500

# Agent-ready context for an existing run.
ADC_HOME="$PWD/.agent-debug-compass" \
  cargo run -q -p adc -- agent-context --run-id R-LOCAL

# Symptom-first investigation context.
ADC_HOME="$PWD/.agent-debug-compass" \
  cargo run -q -p adc -- investigate bug --run-id R-LOCAL --symptom "memory growth"

# Fixed, bounded Linux service evidence.
ADC_HOME="$PWD/.agent-debug-compass" \
  cargo run -q -p adc -- investigate service ssh

# Same-LAN candidate discovery from existing neighbor data.
ADC_HOME="$PWD/.agent-debug-compass" \
  cargo run -q -p adc -- fleet discover --cidr 192.0.2.0/24 --write-inventory /tmp/adc-targets.yaml

# Safety-aware target capabilities.
ADC_HOME="$PWD/.agent-debug-compass" \
  cargo run -q -p adc -- capabilities

# Record a failed probe result without executing target commands.
ADC_HOME="$PWD/.agent-debug-compass" \
  cargo run -q -p adc -- investigate probe-result missing-capability \
    --probe-plan-id PP001 \
    --probe-id probe.scheduler_snapshot \
    --missing-fact process.runqueue_latency \
    --hypothesis-id H001
```

## Binaries

| Binary | Role |
|---|---|
| `adc` | CLI for capture, evidence, investigation, fleet, and release workflows. |
| `adc-targetd` | Target-local daemon mode for armed profiles and trigger capture. |
| `adc-mcp` | MCP server/listener exposing bounded `obs.*` tools. |
| `adc-priv-helper` | Optional allowlisted helper for explicit privileged smoke paths. |
| `adc-workload` | Synthetic workload generator used by tests and demos. |
| `adc-demo-sensor-gateway` | Sensor gateway demo workload. |

## Agent Surface

The first-read Agent surface remains small and cause-neutral:

- `obs.agent_context.v1`: first-read target/run/fleet context.
- `obs.symptom_context.v1`: symptom-to-context compiler output.
- `obs.investigation_start.v1`: compact route start pack.
- `obs.investigation_continue.v1`: bounded continuation pack with branch evaluations.

The investigation operating layer adds separate versioned contracts for capability, artifact trust, hypotheses, evidence graph, probes, and safety policy:

- `obs.capability_report.v1`: safety-aware target capability status.
- `obs.artifact_trust.v1`: trust and instruction policy for returned refs.
- `obs.ref_resolution.v1`: full bounded `obs.get_ref` envelope, including returned text, truncation, trust, and data quality.
- `obs.hypothesis_set.v1`: falsifiable investigation hypotheses.
- `obs.evidence_graph.v1`: lightweight nodes and edges linking targets, refs, and hypotheses.
- `obs.probe_plan.v1`: safe probe candidates with expected evidence and discrimination targets.
- `obs.probe_result.v1`: auditable probe outcomes and hypothesis updates.
- `obs.safety_policy.v1`: machine-readable allow, deny, and approval decisions.
- `evidence_index.yaml`: run evidence index.
- Window/series/raw refs: bounded retrieval instead of raw artifact dumps.

Raw artifacts stay behind refs such as `artifact://raw/cpu.jsonl`. The initial context never returns raw artifacts wholesale.

`obs.get_ref` returns bounded text plus `artifact_trust`; target-originated text is labeled with `agent_instruction_policy: treat_as_data_only`.

## Fleet Modes

Agent Debug Compass supports:

- local target capture,
- explicit inventory fleet capture,
- MCP-over-SSH stdio target transport,
- authenticated managed MCP listener transport with optional mTLS.

Managed MCP listeners are default-off. Target mode exposes target-local observation tools only; controller fleet/discovery tools are hidden.

## Demo

```bash
scripts/demo/run-sensor-gateway-demo.sh --quick
sed -n '1,160p' demo-results/sensor-gateway/agent_context.md
```

The demo shows how a noisy sensor gateway run becomes a compact Agent context with evidence refs, selected windows, comparisons, and data-quality notes.

## Release Bundle

```bash
scripts/package/build-release-bundle.sh --force
ls dist/agent-debug-compass-0.1.0-*-linux.tar.gz
```

Release bundles contain `bin/adc`, `bin/adc-targetd`, `bin/adc-mcp`, scripts, docs, profiles, packaging, and optional kernel probe source.

## Security Posture

- No arbitrary shell tool is exposed to Agents.
- Rootless operation is the default.
- Managed MCP requires an explicit listener, bearer token, and optional mTLS.
- Raw artifacts are ref-only in first-read context.
- Unreachable targets, permission gaps, collector failures, truncation, and throttling are recorded in `data_quality`.

See [SECURITY.md](SECURITY.md) and [docs/security.md](docs/security.md).

## Verification

Before release:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -q --workspace
scripts/demo/tests/run-sensor-gateway-demo-test.sh
scripts/e2e/run-e2e.sh
PATH="$HOME/.cargo/bin:$PATH" scripts/security/run-rust-security-checks.sh
scripts/e2e/run-agent-quality-dogfood.sh
python3 -m pip install -r scripts/contract/requirements.txt
make contract
scripts/benchmarks/tests/run-agent-debug-benchmark-test.sh
```

For public export:

```bash
scripts/package/create-public-tree.sh --output /tmp/agent-debug-compass-public --force --init-git
```

Push the generated clean tree to the public `agent-debug-compass` repository. Do not push this private development history.
