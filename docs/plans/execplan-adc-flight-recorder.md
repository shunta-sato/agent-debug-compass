# ExecPlan: ADC Flight Recorder

## Goal

Implement ADC Flight Recorder: a budget-governed, autonomous, Agent-facing
evidence recorder for edge targets.

The recorder continuously retains ultra-light semantic signals, autonomously
triggers and freezes bounded incident windows, preserves Tx evidence for Ty
investigations, exposes coverage, `data_quality`, `artifact_trust`, and overhead
through strict contracts, supports retrospective markers, produces
evaluation-ready datasets, and proves through benchmarks that delayed-reported
edge incidents are easier and safer to investigate than with direct shell or
on-demand collection alone.

This is not a root-cause engine.

## Current Baseline

The repository already provides:

- executable Agent-facing contract validation through schema, golden fixtures,
  generated CLI/MCP outputs, semantic invariants, trace fixtures, and coverage
  manifest checks;
- `obs.agent_context.v1`, `obs.ref_resolution.v1`, `obs.artifact_trust.v1`,
  `obs.data_quality.v1`, hypothesis, probe, safety, and investigation contracts;
- `adc-targetd` with service modes, active profile loading, live samples, trigger
  evaluation, and triggered evidence bundle generation;
- profile-driven sampling, always-on collectors, budgets, triggers, and capture
  profiles;
- local/same-LAN managed MCP and target MCP transports;
- benchmark and dogfood scripts for Agent investigation quality.

The missing Flight Recorder capability is not more collectors first. The missing
capability is autonomous, budgeted preservation of incident-adjacent windows
before Ty. The current runtime MVP freezes retained pre-window evidence; bounded
post-window collection remains future work.

## Project-Level Role

This program establishes ADC Flight Recorder as the runtime counterpart to the
executable contract gate.

After this workstream, any new recorder-facing Agent contract must:

- have JSON Schema,
- have at least one golden fixture,
- be validated through `make verify`,
- use enum-constrained external vocabulary,
- reject unknown fields,
- preserve `data_quality` and `artifact_trust` semantics,
- appear in `adc.contract_coverage.v1`,
- avoid root-cause claim promotion.

## Requirements

| ID | Priority | Type | Requirement | Acceptance criteria | Verification |
|---|---|---|---|---|---|
| FR-R001 | Must | Functional | While armed, the recorder shall retain bounded pre-window semantic samples for configured signals. | Incident windows include pre-window coverage, or explicit missing/drop reasons. | rolling buffer unit tests, E2E |
| FR-R002 | Must | Functional | When external/manual markers are received, the recorder shall freeze or enrich a bounded incident window without treating marker text as instructions. | Marker refs include `artifact_trust`, data-only policy, and a bounded freeze result. | integration and adversarial tests |
| FR-R003 | Must | Functional | When a retrospective marker is submitted at Ty, the recorder shall search retained buffers and report coverage/missing evidence. | `adc recorder mark` returns an incident window with coverage, loss semantics, and `data_quality`. | CLI/MCP tests |
| FR-R004 | Must | Functional | When autonomous trigger policy matches, the recorder shall freeze a bounded incident window without app/human/Agent input. | `obs.recorder_trigger_event.v1`, `obs.recorder_incident.v1`, and `obs.recorder_frozen_window.v1` are emitted. | synthetic CPU/thermal/cpufreq test |
| FR-R005 | Must | NFR | The recorder shall stay within configured CPU, memory, artifact, and disk write budgets or degrade explicitly. | Recorder status and incident windows show overhead and degrade reason. | overhead tests and target smoke |
| FR-R006 | Must | Security | The recorder shall never expose arbitrary shell or destructive probe execution. | CLI/MCP surfaces are read-only or recording-only. | security/static checks |
| FR-R007 | Must | Contract | Recorder outputs shall follow the PR2/PR3 executable contract discipline. | schemas, fixtures, coverage, invariants, generated outputs pass `make contract`. | contract gate |
| FR-R008 | Should | Evaluation | The recorder should produce dataset-ready incident, negative, near-miss, and post-fix windows. | `obs.dataset_manifest.v1` exports policy/profile/label/coverage metadata. | dataset export test |
| FR-R009 | Should | Benchmark | ADC FR should outperform Ty-only direct shell and on-demand ADC for delayed incident evidence availability. | benchmark compares evidence availability, hypothesis support, false claim rate, unsafe command count, and overhead. | benchmark harness |
| FR-R010 | Must | Functional | Every Agent-facing window shall include loss semantics. | Expected, recorded, dropped, gap, degraded collector, and loss reason fields are present or referenced. | schema and invariant tests |
| FR-R011 | Must | Functional | The memory-backed ring volatility envelope shall be Agent-facing. | Recorder status reports volatility and restart/reboot/power-loss survival booleans. | schema and CLI/MCP tests |
| FR-R012 | Must | Functional | Trigger policy shall be symptom-oriented rather than cause-oriented. | Trigger names and generated fields reject root-cause-like wording. | adversarial contract tests |
| FR-R013 | Must | Functional | Continuous retention and frozen incident persistence shall be separate contracts. | Continuous ring is memory-only; frozen incidents may persist only as bounded artifact bundles under write and retention budgets. | schema and freezer tests |
| FR-R014 | Must | Functional | Marker time shall include confidence and centering policy. | Marker contracts include received time, optional asserted event time, confidence, source, and time policy. | schema and marker-freeze tests |

## Architecture Direction

The first runtime uses memory-backed rolling buffers with explicit drop
accounting plus bounded frozen incident artifact bundles.

Disk-backed rings are deferred until the budget governor can prove write safety.
Capture-on-trigger-only is rejected because it does not preserve Tx pre-window
evidence.

Memory-backed retention is volatile by contract. It preserves history only while
`adc-targetd` remains alive and does not guarantee survival across daemon
restart, target reboot, power loss, kernel panic, or severe OOM pressure. These
limits are reported through recorder status, observation coverage, loss reports,
and `data_quality`.

Continuous rolling retention and frozen incident persistence are separate:

```text
continuous rolling buffer:
  memory-backed
  non-persistent
  no continuous flash write requirement

frozen incident:
  may be materialized as a bounded artifact bundle
  subject to write budget, max_freeze_bytes, max_frozen_incidents, and retention policy
```

This is not a disk-backed rolling ring. It is a bounded incident export that
lets an Agent read a frozen incident later without continuously writing the ring
to flash.

Core boundaries:

- Signal Sampler: produces low-cost semantic samples.
- Rolling Buffer: retains bounded samples by signal.
- Marker Handler: accepts external/manual/Agent/app markers and freezes retained
  evidence before trigger heuristics are introduced.
- Trigger Policy Engine: evaluates autonomous and correlation trigger policies.
- Window Freezer: creates bounded incident windows and evidence bundles.
- Budget Governor: enforces CPU, memory, artifact, disk write, incident rate,
  burst duration, and cooldown limits.
- Dataset Writer: exports incident and comparison windows with label lifecycle
  metadata.
- Agent Surface: exposes status, incidents, coverage, policy, markers, and
  dataset manifests through CLI/MCP without shell execution.

## Contracts

Initial recorder contracts:

```text
obs.recorder_status.v1
obs.recorder_buffer_status.v1
obs.recorder_budget.v1
obs.recorder_marker.v1
obs.recorder_marker_result.v1
obs.recorder_incident_list.v1
obs.recorder_incident.v1
obs.recorder_incident_resolution.v1
obs.recorder_frozen_window.v1
obs.loss_report.v1
obs.recorder_overhead.v1
obs.trigger_policy.v1
obs.recorder_trigger_event.v1
obs.anomaly_score.v1
obs.label_event.v1
obs.dataset_manifest.v1
```

Each contract must include:

- schema with `additionalProperties: false`,
- golden fixture,
- coverage manifest entry,
- `data_quality` where reliability can degrade,
- `artifact_trust` for target-originated text refs,
- generated CLI/MCP fixtures once surfaced,
- invariant/adversarial tests where needed.

PR4 schema scope is fixed to:

```text
obs.recorder_status.v1
obs.recorder_buffer_status.v1
obs.recorder_budget.v1
obs.recorder_marker.v1
obs.recorder_marker_result.v1
obs.recorder_incident_list.v1
obs.recorder_incident.v1
obs.recorder_incident_resolution.v1
obs.recorder_frozen_window.v1
obs.loss_report.v1
obs.recorder_overhead.v1
obs.recorder_trigger_event.v1
obs.dataset_manifest.v1
```

`obs.trigger_policy.v1` and `obs.anomaly_score.v1` remain in the overall goal
but are deferred until richer trigger policy and benchmark work.
`obs.label_event.v1` and public sharing dataset policy remain deferred.

## State and Loss Semantics

Recorder states:

```text
disabled
armed
recording
degraded
freezing
frozen
over_budget
error
```

Allowed recorder transitions:

```text
disabled -> armed
armed -> recording
recording -> degraded
recording -> over_budget
recording -> freezing
degraded -> recording
degraded -> freezing
over_budget -> degraded
freezing -> frozen
freezing -> error
frozen -> recording
error -> disabled
```

Forbidden recorder transitions:

```text
disabled -> freezing
disabled -> frozen
recording -> exported
frozen -> frozen for the same incident_id
error -> frozen without a freeze result
```

Incident states:

```text
marker_received
pre_window_selected
post_window_collecting
freezing
frozen
expired
exported
discarded
```

Allowed incident transitions:

```text
marker_received -> pre_window_selected
pre_window_selected -> post_window_collecting
post_window_collecting -> freezing
pre_window_selected -> freezing
freezing -> frozen
frozen -> exported
frozen -> expired
frozen -> discarded
expired -> discarded
```

Forbidden incident transitions:

```text
marker_received -> exported
pre_window_selected -> exported
post_window_collecting -> exported
recording -> exported without a frozen incident
expired -> exported unless a bounded artifact bundle already exists
discarded -> exported
```

Freeze reasons:

```text
external_marker
operator_marker
agent_marker
trigger_policy
budget_guard
daemon_shutdown
```

Every frozen window must carry or reference loss semantics:

```text
expected_samples
recorded_samples
dropped_samples
gap_ranges
collectors_degraded
loss_reasons
loss_confidence
data_quality
```

Loss invariants:

```text
expected_samples >= recorded_samples when expected_samples is known
dropped_samples is not necessarily expected_samples - recorded_samples
gap_ranges require dropped_samples > 0 or an explicit missing reason
recorded_samples = 0 must distinguish absent collector from degraded collector
loss_confidence must be explicit when expected_samples is estimated
```

Marker time semantics:

```text
marker_id
source
received_at_mono_ns
asserted_event_time.kind
asserted_event_time.wall_time_unix_ms
asserted_event_time.mono_ns
asserted_event_time.confidence
time_policy
trust_level
agent_instruction_policy
```

Allowed `asserted_event_time.kind` values:

```text
relative_now
wall_time
monotonic
unknown
```

Allowed `time_policy` values:

```text
center_on_received_at
center_on_asserted_event_time
search_near_received_at
search_near_asserted_event_time
```

Initial budget vocabulary:

```text
max_memory_bytes
max_samples_per_second
max_collectors
max_frozen_incidents
max_freeze_bytes
max_post_window_ms
max_retention_ms
max_ref_lines
max_cpu_percent
max_disk_bytes
collector_priority
```

Initial degradation policies:

```text
drop_oldest
drop_low_priority_collector
downsample
stop_collector
refuse_freeze
partial_freeze
```

## PR Sequence

## Implementation Status

As of 2026-06-04 on the Flight Recorder implementation branch:

- PR4 contract scope is implemented through schema files, golden fixtures, and
  contract coverage for recorder status, buffer status, budget, marker,
  incident, frozen window, loss report, recorder overhead, and dataset manifest.
- PR5 runtime scope is implemented for a memory-backed recorder ring, recorder
  status, bounded sample retention, and explicit volatility semantics.
- PR6 marker scope is implemented through pending recorder markers consumed by
  `adc-targetd`, bounded incident materialization, incident listing, and incident
  retrieval.
- PR7 autonomous trigger scope is implemented for existing daemon trigger
  matches, preserving trigger windows as `trigger_policy` incidents with
  symptom-oriented trigger-name guards and `max_frozen_incidents` throttling.
- PR8 benchmark scope is implemented as a deterministic
  `camera_inference_degradation_flight_recorder` comparison of direct shell,
  on-demand ADC, and ADC Flight Recorder.
- PR9 dataset readiness is implemented for local benchmark/regression dataset
  manifests. Public sharing datasets, label lifecycle events, and MCP recorder
  tools remain future work.

### PR4: Flight Recorder Architecture and Contracts

Deliver:

- `docs/architecture/adc-flight-recorder.md`,
- `docs/plans/execplan-adc-flight-recorder.md`,
- recorder contract schemas,
- golden fixtures,
- coverage manifest updates,
- contract invariants for no root-cause claims, data-only target text, budget
  status, volatile memory ring envelope, loss semantics, and bounded windows.

Acceptance:

- `make contract` validates all new recorder schemas and fixtures.
- All new public schemas appear in `adc.contract_coverage.v1`.
- Docs clearly state non-goals and runtime boundaries.
- Docs and contracts define recorder states, incident lifecycle, marker versus
  trigger, freeze reason enum, volatile memory-ring envelope, and loss report
  semantics.
- Docs and contracts define allowed and forbidden recorder/incident transitions.
- Continuous memory rings are volatile, while frozen incidents have explicit
  bounded artifact bundle persistence policy.
- Marker contracts include received time, optional asserted event time, time
  confidence, source, trust, and centering policy.
- Budget contracts define memory, sample-rate, freeze-size, incident-count,
  retention, collector priority, and degradation policies.
- Trigger names are planned as symptom/event names only; root-cause-like trigger
  names are invalid fixtures before trigger runtime lands.
- Every frozen window references `obs.loss_report.v1` and `obs.data_quality.v1`.
- No heavy runtime is introduced.

### PR5: Rolling Buffer and Budget Governor Runtime

Deliver:

- `adc-targetd` memory ring buffer for configured semantic signals,
- signal sampler abstraction,
- recorder overhead sampler,
- budget governor,
- `obs.recorder_status.v1`,
- `obs.recorder_buffer_status.v1`,
- `obs.recorder_budget.v1`,
- `obs.recorder_overhead.v1`,
- CLI `adc recorder status`,
- generated CLI/MCP contract fixtures where surfaced.

Initial signals:

- CPU summary,
- memory summary,
- network counters,
- thermal zones when available,
- cpufreq when available,
- process top-N when cheap,
- kmsg cursor/mock,
- ADC self-overhead.

Acceptance:

- ring buffer retains pre-window samples with bounded capacity;
- missing signals are recorded through `data_quality`;
- budget exceedance degrades explicitly;
- no disk-backed always-on ring yet.

### PR6: External and Manual Marker Freeze

Deliver:

- `obs.recorder_marker.v1`,
- `obs.recorder_incident.v1`,
- `obs.recorder_frozen_window.v1`,
- `obs.loss_report.v1` runtime population,
- `adc recorder mark`,
- `adc recorder incidents`,
- `adc recorder incident get`,
- MCP marker and incident read tools.

Acceptance:

- external/operator/Agent marker freezes a bounded retained pre-window from
  buffers;
- marker text remains `treat_as_data_only`;
- incident windows expose coverage, trust, data quality, and overhead;
- marker time confidence and centering policy are preserved;
- frozen incidents are materialized only as bounded artifact bundles under
  write and retention budgets;
- unavailable Tx evidence is explicit through loss semantics;
- no unbounded raw logs.

### PR7: Autonomous Trigger Policy

Deliver:

- trigger policy parser,
- threshold, slope, delta, burst, and correlation triggers,
- trigger cooldown and hysteresis,
- incident merge/storm control,
- symptom-oriented trigger naming guard,
- `obs.trigger_policy.v1`,
- `obs.recorder_trigger_event.v1`,
- autonomous freeze integration.

Acceptance:

- autonomous synthetic incident freezes a bounded retained pre-window in the
  current MVP; bounded post-window capture is a follow-up;
- repeated trigger storms merge/cool down/degrade;
- trigger names and outputs do not promote causes;
- automatic trigger policy reuses the same freeze/loss contracts as marker flow.

### PR8: Camera and Inference Degradation Vertical

Deliver:

- `profiles/camera_inference_degradation.v1.yaml`,
- synthetic workload markers,
- thermal/cpufreq-like mock signals,
- benchmark harness comparing:
  - direct shell after Ty,
  - ADC on-demand only,
  - ADC Flight Recorder,
- report with evidence availability, overhead, false claim rate, unsafe command
  count, drop accounting accuracy, incident freeze latency, write bytes per
  frozen incident, and hypothesis separation support.

Acceptance:

- ADC FR preserves Tx evidence unavailable to Ty-only shell;
- benchmark can distinguish thermal/cpufreq-like degradation from generic CPU
  saturation or missing evidence;
- benchmark measures pre-window evidence availability, recorder memory overhead,
  write bytes per frozen incident, unsafe command count, time to useful
  hypothesis, and hypothesis rank improvement;
- recorder overhead stays within policy or degrades explicitly.

### PR9: Dataset Readiness

Dataset scope is initially limited to:

```text
benchmark dataset:
  incident traces for Agent evaluation

regression dataset:
  ADC contract and runtime regression fixtures
```

Public sharing datasets are deferred because redaction, licensing, proprietary
logs, PII, and secret-handling boundaries need a separate policy review.

Deliver:

- `obs.dataset_manifest.v1`,
- negative/background windows,
- near-miss windows,
- post-fix verification windows,
- `obs.label_event.v1`,
- deterministic dataset export,
- `adc recorder export-dataset`,
- `obs.export_dataset_manifest`.

Acceptance:

- dataset manifest includes policy versions, device profile, workload phase,
  capability report refs, coverage, labels, overhead, loss reports, artifact
  trust, redaction status, and data quality;
- export is deterministic and bounded;
- no root-cause labels are created by ADC itself.

## Test Strategy

Test first by layer:

1. contract schema and adversarial fixtures,
2. ring buffer unit tests,
3. budget governor unit tests,
4. trigger policy unit tests,
5. freezer integration tests,
6. CLI/MCP generated contract tests,
7. E2E delayed incident scenario,
8. benchmark comparison,
9. Raspberry Pi target smoke and overhead measurement.

Required gates before each PR:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -q --workspace
make contract
make verify
scripts/e2e/run-e2e.sh
PATH="$HOME/.cargo/bin:$PATH" scripts/security/run-rust-security-checks.sh
scripts/e2e/run-agent-quality-dogfood.sh
```

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Recorder becomes workload | Invalidates edge debugging value | memory ring first, explicit budgets, self-overhead evidence |
| Contract explosion | Slows implementation and weakens schemas | add only used contracts, enforce coverage manifest |
| Trigger storms | Unbounded incidents and writes | cooldown, hysteresis, merge, max incidents per hour |
| Evidence loss on daemon restart | Pre-window loss | record as `data_quality`; disk ring deferred until budget is safe |
| Target text prompt injection | Agent may treat logs/markers as instructions | artifact trust, data-only policy, adversarial fixtures |
| Root-cause drift | ADC becomes inference engine | banned promotion invariants and benchmark checks |
| Hardware-specific lock-in | Pi-only design | edge-general contracts, Pi path as first profile |

## Done Definition

This goal is complete when:

1. recorder contracts are schema-backed and covered by the contract gate;
2. `adc-targetd` retains bounded pre-window semantic samples under budget;
3. external/manual markers freeze incident windows before automatic trigger policy
   is introduced;
4. autonomous triggers freeze incident windows after marker freeze is proven;
5. retrospective markers can recover retained Tx evidence or report missing data;
6. incident windows include coverage, data quality, artifact trust, overhead, and
   bounded refs;
7. every incident window carries or references loss semantics;
8. marker time confidence and centering policy are preserved;
9. frozen incidents persist only as bounded artifact bundles under write and
   retention budgets;
10. trigger storms are controlled;
11. dataset export supports incident, negative, near-miss, and post-fix windows;
12. benchmark proves ADC FR improves delayed-reported incident investigation over
   direct shell and on-demand collection alone;
13. no recorder output asserts root cause.

## Handoff

Current status:

- PR3 is merged into `main`.
- PR4 is implemented as a collapsed Flight Recorder MVP covering contracts,
  memory-backed ring runtime, marker freeze, trigger freeze, local dataset
  manifest export, and benchmark scaffolding.
- Current runtime intentionally freezes pre-window retained evidence only
  (`post_window_ms = 0`). Bounded post-window collection, MCP recorder tools,
  richer signals, public sharing datasets, and durable disk-backed rings remain
  future work.

Next steps:

1. Keep runtime semantics aligned with Agent-facing contracts: retention,
   budget, persistence, path safety, marker outcome, loss, and wrapper schemas.
2. Run `make contract`, Rust workspace tests, E2E, benchmark, demo, and security
   gates before merge.
3. Plan follow-up PRs for post-window capture and MCP recorder tools only after
   the pre-window Flight Recorder MVP is stable.
