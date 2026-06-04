# Current State

Agent Debug Compass is a v2 evidence-first Agent observation context layer for Raspberry Pi 5 and small fleet targets.

## Current Contract

- The core output is `evidence_index.yaml`.
- Agent-facing retrieval is layered: Agent context pack, evidence index, bounded windows or series, typed artifact refs, and explicit raw slices.
- MCP tools and resources use the `obs.*` / `obs://` namespace.
- CLI workflows use `adc doctor`, `observe`, `agent-context`, `evidence ...`, `investigate bug`, `investigate route-packs`, `investigate start`, `investigate continue`, `investigate session`, `investigate cleanup-sessions`, `investigate service`, `target capture`, `fleet discover/init/enroll/targets/preflight/observe/capture/investigate service`, `compare`, and `next-probe`.
- Raw artifacts are never returned wholesale as initial Agent context.
- Agent Debug Compass records observations and information debt; it does not infer causes.

## Completed Baseline

- Snapshot and bounded capture generate evidence indexes, timelines, windows, raw artifacts, overhead reports, and manifests.
- Local target capture, same-network discovery, explicit-inventory fleet snapshot, and bounded fleet capture are implemented.
- Fleet transport supports local targets, target MCP endpoints over SSH stdio, and explicit authenticated `managed_mcp` target listeners through bounded `obs.*` MCP tool calls. Managed MCP reloads its token file per request for restart-free rotation, supports optional mutual TLS, handles each connection independently so slow clients do not block other Agent requests, has a rootless systemd user-service installer for supervised target listeners, provides an enrollment kit generator plus `fleet enroll-kit`, and includes a guarded remote provisioner that uses SSH only for first-time rootless target bootstrap.
- Per-target evidence keeps `target_id`, `fleet_run_id`, profile, capability refs where available, and artifact refs separated.
- The sensor gateway demo produces bounded Agent context from evidence, windows, series, comparisons, and raw refs.
- Agent context packs can now be generated directly from run or fleet evidence, including target dossier, representative raw-series-derived stats, salience-ranked refs, cause-neutral Agent playbook, machine-readable investigation route, runtime snapshots, FD/thread and kernel optional-probe snapshots, optional log/domain/config/service/interop facts, overhead, next probes, budget reduction, and OpenMetrics/OTLP/journald/Perfetto summary export.
- Symptom-first investigation is available through CLI `adc investigate bug` and MCP `obs.investigate_bug`; it returns `obs.symptom_context.v1` with normalized symptom, compiled selected/rejected route packs, typed facts, explicit missing fact IDs, ranked refs, declarative safe probe packs, context budget, and persisted `symptom_context.json` / `compiled_route.json` / `fact_gap_report.json` without cause inference.
- A fixed cause-neutral service investigation pack is available through CLI `adc investigate service <name>` and MCP `obs.investigate_service`; it returns service state, process/port summary, bounded journal leads with recency/window summary, raw refs, data_quality, and next probes without exposing arbitrary shell. `observe --service-name` reuses the same bounded service state path, and unavailable/permission-denied data is explicit instead of encoded as zero. Service investigation refs are resolvable through CLI `adc investigate ref --ref ...` and MCP `obs.get_ref` without a `run_id`.
- Fleet service investigation is available through CLI `adc fleet investigate service <name>` and controller MCP `obs.fleet_investigate_service`; it collects per-target bounded service packs over local, MCP-over-SSH, or managed MCP transports while preserving partial success and per-target `data_quality`.
- One-shot investigation start is available through CLI `adc investigate start` and MCP `obs.start_investigation`; it returns `obs.investigation_start.v1` with compact Agent context plus `obs.investigation_route.v1` steps, expected answer shape, typed branch predicates, stop conditions, bounded refs, target IDs, and route-level `data_quality`. Full context remains available through `agent-context` / `obs.get_agent_context`.
- Adaptive investigation continuation is available through CLI `adc investigate continue` and MCP `obs.continue_investigation`; it opens selected route refs through bounded resolvers, extracts typed evidence facts, evaluates typed route conditions into `matched` / `not_matched` / `unknown` with missing fact IDs, returns `obs.investigation_continue.v1` with branch evaluations and ranked next actions, persists `investigation_sessions/<session_id>.json` plus `<session_id>.state.json`, and keeps raw content out of the response. Session state is readable through CLI `investigate session` and MCP `obs.get_investigation_session`; session cleanup is dry-run-first and supports age-based execution through `investigate cleanup-sessions`. Fleet service starts also persist `fleet_semantic_diff.json` with typed service/process/port/journal/data_quality diff fields and target-level quality classes.
- The route pack registry is available through CLI `adc investigate route-packs` and MCP `obs.list_route_packs`; it currently covers service health, latency/timeouts, memory growth, CPU saturation, network degradation, disk/IO pressure, config/deploy drift, and thermal/power edge degradation without adding cause inference.
- `evidence ref` and MCP `obs.get_ref` resolve typed refs for raw, window, manifest, evidence, timeline, context, and service investigation artifacts without forcing Agents to know which lower-level getter accepts each ref kind.
- Fleet context now supports discovery inventory writing, `latest` aliases, deduped remediation hints, and Markdown target matrices for partial-success investigation.
- Fleet preflight now performs per-target readiness checks. Remote targets use MCP-over-SSH stdio calls to `obs.status`, `obs.doctor`, and `obs.preflight`; inventories may set `mcp_server_path` for rootless user-local target installs.
- Managed fleet registry supports rootless `fleet init`, `fleet invite`, `fleet enroll`, `fleet targets`, and selector-based `fleet preflight/snapshot/observe --selector all|enrolled|target=<id>|tag=<tag>|transport=<transport>` so Agents do not need to pass a hand-written inventory on every run.
- Target MCP `--target-mode` exposes only target-local observation tools/resources; controller fleet/discovery tools and fleet resource templates are hidden from enrolled targets.
- Fleet Agent context now includes per-target summaries with event counts, source counts, evidence refs, target dossiers, salience-ranked top leads, grouped failure classes, action-grade next steps, and target data_quality before an Agent opens lower-level refs.
- Raspberry Pi 5 release-binary smoke and overhead measurement are scripted under `scripts/e2e/target/run-pi5-release-smoke.sh`.
- Rootless target MCP bootstrap is scripted under `scripts/install/install-target-mcp-binaries.sh`, and reusable target MCP fleet smoke is scripted under `scripts/e2e/target/run-target-mcp-fleet-smoke.sh` with inventory-derived expected target counts.
- Reusable Agent investigation quality dogfood is scripted under `scripts/e2e/run-agent-quality-dogfood.sh`; it emits `obs.agent_quality_scorecard.v2` plus `STRICT_DOGFOOD_REPORT.md` and scores symptom-first context compilation, direct-shell comparison, typed routes, session resume, managed MCP, degraded fleet, budget, and safety/privacy assertions.
- Fleet Agent context includes cross-target captured/failed/event/source totals without making cause claims.
- Target MCP-over-SSH fleet smoke is exercised from the Pi 5 controller to a separate Raspberry Pi target using rootless user-local `mcp_server_path` install.
- Dependency/security/supply-chain checks are scripted under `scripts/security/`.

## Remaining Platform Work

- No blocking v2 platform work remains before starting Flight Recorder planning for the default Agent-first local/same-LAN managed MCP path. Future non-blocking UX improvements can add richer batch provisioning, but the current working transport is explicit bearer-token HTTP JSON-RPC MCP with optional mutual TLS, enrollment kit automation, guarded SSH-carried rootless provisioning, SSH trust-policy controls, best-effort host-key fingerprint reporting, and no listener by default.
- Optional privileged capability smoke for perf/ftrace/KO remains available through explicit target smoke scripts; it is not required for default non-root operation.

## Verification Baseline

Run these before handoff or release:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -q --workspace
scripts/demo/tests/run-sensor-gateway-demo-test.sh
scripts/e2e/run-e2e.sh
PATH="$HOME/.cargo/bin:$PATH" scripts/security/run-rust-security-checks.sh
scripts/e2e/run-agent-quality-dogfood.sh
```
