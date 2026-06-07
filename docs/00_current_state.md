# Current State

Agent Debug Compass is an evidence-governed debugging runtime for AI Agents on
local, Raspberry Pi-first, and same-LAN edge targets.

This file is the implementation truth table. It separates what is implemented,
what is partial, what is hardware-optional, and what remains future work.

## Contract Boundary

- ADC records observations, information debt, coverage, loss, trust, and safe
  next investigation context.
- ADC does not infer or assert root cause.
- Raw artifacts are not returned wholesale as initial Agent context.
- Agent-facing retrieval is layered: Agent context, evidence index, bounded
  windows or series, typed `artifact://...` refs, and explicit bounded raw
  slices.
- MCP tools and resources use the `obs.*` / `obs://` namespace.
- New public Agent-facing contracts are expected to have JSON Schema, golden
  fixtures, generated output validation where surfaced, enum vocabulary,
  `data_quality` / `artifact_trust` semantics, and coverage manifest entries.

## Implementation Truth Table

| Area | Status | CLI | MCP | Contracts | Tests / gates | Known limits |
|---|---|---|---|---|---|---|
| Local snapshot and bounded capture | Implemented | `adc snapshot`, `adc observe`, `adc target capture` | `obs.snapshot`, `obs.observe` where surfaced | `obs.evidence_index.v2`, event/timeline/window refs | workspace tests, demo, `make verify` | Linux `/proc`-style targets; no broad non-Linux claim |
| Evidence retrieval | Implemented | `adc evidence get/window/series/raw-slice`, `adc investigate ref` | `obs.get_ref`, resource refs | `obs.ref_resolution.v1`, `obs.artifact_trust.v1` | ref resolver tests, contract fixtures | Returned text is bounded/truncated; raw full dumps stay behind refs |
| Agent context | Implemented | `adc agent-context` | `obs.get_agent_context` | `obs.agent_context.v1` | CLI/MCP generated fixtures, agent context tests | Large implementation module remains a maintainability follow-up |
| Symptom-first investigation | Implemented | `adc investigate bug` | `obs.investigate_bug` | `obs.symptom_context.v1`, hypotheses, probe plan, safety policy | contract/integration tests, dogfood | Provides falsifiable investigation state, not a conclusion |
| Investigation start / continue / sessions | Implemented | `adc investigate start/continue/session/cleanup-sessions` | `obs.start_investigation`, `obs.continue_investigation`, `obs.get_investigation_session` | `obs.investigation_start.v1`, `obs.investigation_continue.v1` | integration tests, generated MCP validation | Cleanup is dry-run-first; raw content stays behind refs |
| Service investigation | Implemented | `adc investigate service <name>`, `observe --service-name` | `obs.investigate_service` | `obs.service_investigation.v1` surface through generated outputs | service investigation tests | Depends on available service/journal tooling and user permissions |
| Route pack registry | Implemented | `adc investigate route-packs` | `obs.list_route_packs` | route pack outputs in investigation contracts | route pack tests | Cause-neutral route packs only |
| Probe result recording | Implemented for non-executing outcomes | `adc investigate probe-result missing-capability`, `policy-denied` | `obs.record_probe_result` | `obs.probe_result.v1` | contract tests, MCP tests | Recording-only; no probe execution engine |
| Fleet discovery and explicit inventory | Implemented | `adc fleet discover`, inventory-based observe/capture/preflight | controller fleet tools | fleet evidence/context contracts | fleet tests, dogfood | Same-LAN/rootless paths verified; not broad network scanner |
| Managed MCP target transport | Implemented | `adc fleet init/invite/enroll/targets/preflight/observe/capture` | managed target listener and controller tools | managed fleet outputs | managed MCP tests, e2e scripts | Listener is explicit/default-off; token/mTLS setup required |
| Target MCP mode | Implemented | `adc-mcp --target-mode` | target-local `obs.*` subset | MCP tool/resource list | MCP tool list tests | Controller fleet/discovery tools hidden in target mode |
| Flight Recorder memory ring | Implemented MVP | `adc arm`, `adc-targetd --service*`, `adc recorder status` | status via existing ref/MCP resolver surfaces; dedicated recorder MCP tools deferred | `obs.recorder_status.v1`, buffer/budget/overhead contracts | recorder tests, service tests, `make contract` | Memory-backed and volatile; no disk-backed rolling ring |
| Flight Recorder marker freeze | Implemented MVP | `adc recorder mark`, `incidents`, `incident get` | dedicated recorder MCP tools deferred | marker, marker result, incident, frozen window, loss report | recorder/daemon tests | Current runtime freezes retained pre-window evidence only; `post_window_ms=0` |
| Flight Recorder artifact refs | Implemented | `adc investigate ref --ref artifact://recorder/...` | `obs.get_ref` | `obs.ref_resolution.v1`, `obs.artifact_trust.v1` | resolver tests, generated fixtures | Default Agent outputs avoid raw local filesystem paths |
| Observation coverage / expected signals | Implemented MVP | incident `coverage_ref` via recorder incident resolution and ref resolver | `obs.get_ref` for `coverage.json` | `obs.recorder_observation_coverage.v1` | recorder coverage tests, benchmark | Expected signal model is profile/static mapping; richer device capabilities are future work |
| Recorder persistent incident budget | Implemented | daemon admission and refused marker/trigger outputs | via generated/ref surfaces | `obs.recorder_budget_status.v1`, freeze decision | service tests | No retention sweeper/deletion policy yet |
| Coverage-aware trigger decisions | Implemented v1 | daemon trigger path, incident/scoped trigger decision refs | `obs.get_ref` for trigger decision refs | `obs.trigger_policy.v1`, `obs.trigger_decision.v1`, trigger event | trigger tests, service tests, contract validation | Cooldown/hysteresis state is service-run scoped; correlation/vertical trigger policy deferred |
| Recorder resource discipline | Implemented MVP | `adc recorder status` exposes resource status; target smoke script for target55 | via recorder status/ref surfaces; dedicated recorder MCP tools deferred | `obs.recorder_resource_status.v1`, `obs.recorder_power_policy.v1`, `obs.recorder_degradation_decision.v1` | recorder/service tests, contract fixtures, target55 smoke | Linux power supply detection is best-effort; wakeup rate may be unknown; battery-low mode can be simulated for deterministic validation; aggressive profile intervals are pressure-safe clamped for semantic counter sampling |
| Recorder log cursor / blackout semantics | Implemented MVP for append-only app logs | `adc-targetd --service*`, `adc recorder incident get`, `adc investigate ref` | `obs.get_ref` for recorder log refs | `obs.recorder_log_source_status.v1`, `obs.recorder_blackout_report.v1`, `obs.artifact_trust.v1` | cursor tests, service tests, contract fixtures | Rootless append-only file cursor only; live journald and `/dev/kmsg` always-on cursors remain future work; log text is data-only and bounded |
| Dataset manifest export | Implemented MVP | `adc recorder export-dataset` | dedicated recorder MCP tools deferred | `obs.dataset_manifest.v1` | contract fixtures, CLI generated outputs | Local benchmark/regression export only; external sharing/redaction policy is future work |
| Agent debugging benchmark | Implemented static benchmark | `scripts/benchmarks/run-agent-debug-benchmark.py` | not applicable | `obs.agent_debug_benchmark_report.v1` output | benchmark test | Static checked-in scenarios; live generated benchmark remains future work |
| Agent quality dogfood | Implemented local script | `scripts/e2e/run-agent-quality-dogfood.sh` | exercises CLI/MCP surfaces | `obs.agent_quality_dogfood.v2` output | dogfood script | Local synthetic dogfood, not a substitute for real target smoke |
| Security / supply-chain checks | Implemented scripts | `make security-check`, `scripts/security/run-rust-security-checks.sh` | not applicable | reports under `reports/security` | security script tests | Optional `cargo geiger` may fail to produce a usable report; existing allowed audit warnings are documented |
| Raspberry Pi target smoke | Scripted / hardware-optional | `scripts/e2e/target/*.sh` | target MCP/fleet flows | smoke outputs | optional target smoke scripts | Requires hardware and setup; not required for local default verification |
| Optional privileged perf/ftrace/KO | Experimental / hardware-optional | install/smoke scripts | not default MCP surface | capability reports where available | optional smoke scripts | Not required for rootless default operation |
| Recorder MCP dedicated tools | Deferred | not yet dedicated | future `obs.recorder_*` tools | existing contracts already present | future tests | Use CLI/ref resolver today; dedicated MCP recorder surface is future work |
| Bounded post-window capture | Deferred | `post_window_ms` field exists | not implemented | frozen window contract forward-compatible | recorder tests assert current semantics | Current MVP is pre-window only |
| Disk-backed rolling ring | Deferred | none | none | storage status notes | not implemented | Requires write-wear and durability design |
| Live camera/inference vertical | Deferred | benchmark scenario exists | none | benchmark scenario JSON | static benchmark | Needs live workload/profile and target smoke path |

## Verified Environment

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

## Canonical Verification

Local default gate:

```bash
make verify
```

Contract gate:

```bash
make contract
```

Benchmark and dogfood:

```bash
bash scripts/benchmarks/tests/run-agent-debug-benchmark-test.sh
bash scripts/e2e/run-agent-quality-dogfood.sh
```

Security/supply-chain:

```bash
PATH="$HOME/.cargo/bin:$PATH" bash scripts/security/run-rust-security-checks.sh
```

See [COMMANDS.md](../COMMANDS.md) for the complete command map and
[tests/README.md](../tests/README.md) for test taxonomy.
