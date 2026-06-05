# ADC Flight Recorder

## Status

Draft architecture direction for the next platform phase after the executable
Agent-facing contract gate.

## North Star

ADC Flight Recorder is a budget-governed, autonomous, Agent-facing edge evidence
recorder.

It continuously retains ultra-light semantic signals, autonomously triggers and
freezes bounded incident windows, preserves Tx evidence for Ty investigations,
and exposes coverage, `data_quality`, `artifact_trust`, recorder overhead, and
dataset metadata through strict Agent-facing contracts.

It is not a root-cause engine. It records evidence, trigger reasons, anomaly
signals, coverage, missing facts, and safe next investigation context without
asserting the true cause.

## Problem Frame

- Problem owner: AI Agents and operators debugging delayed-reported edge
  incidents.
- Current pain: on-demand investigation can start after the critical evidence
  window has already disappeared.
- Desired outcome: an Agent can inspect bounded incident-adjacent evidence around
  Tx without direct shell scraping or unbounded raw artifact dumps.
- Solution-first risk: adding heavier collectors before budget, trigger, window,
  trust, coverage, and dataset contracts would make the recorder part of the
  workload.
- Non-goals: root-cause inference, arbitrary shell, destructive probe execution,
  unbounded logs, raw video by default, always-on high-frequency tracing, graph
  database, and ML model training as the first step.

## Product Definition

ADC Flight Recorder preserves incident windows that existed before an Agent or
human started investigating.

The current MVP freezes retained pre-window evidence only. `post_window_ms` is
present in the contract for forward compatibility and remains `0` until bounded
post-window collection is implemented.

The canonical experience is:

```text
Ty: Agent asks for the incident window around a delayed report.

ADC returns:
  incident_id
  estimated Tx window
  trigger events
  bounded incident-adjacent evidence
  observation coverage
  missing/truncated signals
  artifact refs
  artifact trust
  recorder overhead
  dataset labels
  safe next investigation context
```

The Agent should be able to separate hypotheses such as thermal mitigation, CPU
scheduler pressure, memory pressure, driver warnings, and application/service
issues using preserved evidence rather than only Ty-time shell state.

## Non-Negotiable Principles

### 1. The Recorder Must Never Become the Workload

Every always-on signal, trigger, freeze, and burst deepening path is governed by
explicit budgets:

- CPU budget,
- memory budget,
- artifact byte budget,
- disk write budget,
- flash wear budget,
- wake-up rate,
- max incidents per hour,
- trigger cooldown,
- max burst duration.

Recorder self-overhead is evidence. If the recorder exceeds budget, it must
degrade explicitly and report the degradation through `data_quality`,
`obs.recorder_overhead.v1`, `obs.loss_report.v1`, and
`obs.recorder_frozen_window.v1`.

### 2. Loss Semantics Are a Core Feature

Flight Recorder will drop data. A recorder that never drops data is unsafe for
edge targets.

Every Agent-facing window must describe what was expected, what was recorded,
what was dropped, which collectors degraded, which gaps are known, and why loss
occurred. Missing evidence is not a footnote; it is investigation state.

Loss semantics must appear in recorder status, rolling buffer status, frozen
incident windows, observation coverage, freeze results, and dataset manifests.

### 3. Memory Ring Is Volatile by Contract

The first runtime uses a memory-backed rolling buffer.

This preserves pre-window evidence only while `adc-targetd` remains alive. It
does not guarantee evidence survival across daemon restart, target reboot,
kernel panic, power loss, or severe OOM pressure.

The volatility is not a hidden weakness. It is part of the Agent-facing
contract:

```json
{
  "storage_mode": "memory_ring",
  "volatile": true,
  "survives_daemon_restart": false,
  "survives_target_reboot": false,
  "survives_power_loss": false
}
```

If history is unavailable because of this envelope, ADC must report that through
recorder status, observation coverage, and `data_quality`. Agents must not infer
"nothing happened" from an empty volatile buffer.

### 4. Marker Is Not Trigger

Markers and triggers are different inputs.

```text
marker:
  external request to preserve evidence around a reported time or symptom

trigger:
  recorder policy condition that autonomously decides evidence is worth freezing
```

Manual, operator, Agent, and app markers are the safest first way to prove
rolling-buffer value because they do not require trigger heuristics. Automatic
trigger policy follows after marker-based freeze is working.

### 5. Observation Coverage Comes Before Trigger Interpretation

Flight Recorder evidence is useful only when Agents can tell the difference
between a signal that was absent and a signal that was never observed. Before
ADC expands autonomous trigger policy, each frozen incident must describe the
active expected signal model and the coverage achieved for that incident.

Coverage responsibilities:

```text
loss_report:
  source of truth for retained/exported/dropped/truncated/gap counts

observation_coverage:
  source of truth for expected signals, effective sampling interval, capability
  availability, and coverage state
```

The current coverage model distinguishes:

```text
configured_interval_ms:
  interval requested by the active recorder profile

effective_interval_ms:
  interval after recorder sample-rate budget and downsampling

expected_samples_basis:
  configured_profile_interval | budgeted_recorder_interval | inferred | unknown
```

Coverage states are cause-neutral:

```text
covered
partial
missing
unavailable
degraded
unknown
not_expected
```

`missing` means the signal was expected and theoretically collectible but no
retained/exported sample exists. `unavailable` means a required capability,
privilege, or collector availability boundary prevented collection. `not_expected`
is not emitted for the whole global signal catalog; it is used only when a
caller or fixture asks about a specific signal outside the active profile.

### 6. Marker Time Semantics

Marker time is evidence with confidence, not an exact fact by default.

An operator saying "it just happened" is not the same as an external detector
submitting a monotonic timestamp. The marker contract must preserve that
uncertainty so Agents do not over-trust the freeze center.

Required marker time fields:

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

Example low-confidence operator marker:

```json
{
  "schema_version": "obs.recorder_marker.v1",
  "marker_id": "marker-001",
  "source": "operator",
  "received_at_mono_ns": 1234567890,
  "asserted_event_time": {
    "kind": "relative_now",
    "wall_time_unix_ms": null,
    "mono_ns": null,
    "confidence": "low"
  },
  "time_policy": "center_on_received_at",
  "trust_level": "operator_marker",
  "agent_instruction_policy": "treat_as_event_marker_only"
}
```

Example high-confidence external marker:

```json
{
  "schema_version": "obs.recorder_marker.v1",
  "marker_id": "marker-002",
  "source": "external_detector",
  "received_at_mono_ns": 1234567890,
  "asserted_event_time": {
    "kind": "monotonic",
    "wall_time_unix_ms": null,
    "mono_ns": 1234500000,
    "confidence": "high"
  },
  "time_policy": "center_on_asserted_event_time",
  "trust_level": "external_marker",
  "agent_instruction_policy": "treat_as_event_marker_only"
}
```

### 7. Trigger Is a Symptom Policy Engine

A trigger is not only a threshold expression.

The trigger layer must support autonomous triggers, external hints, correlated
rules, retrospective markers, severity, confidence, cooldown, hysteresis,
incident merge, budget decisions, and freeze profiles.

A trigger records that a condition worth preserving occurred. It must not name or
assert the cause.

Good trigger names:

```text
latency_spike
frame_drop_detected
thermal_threshold_crossed
cpu_pressure_high
network_drop_spike
service_restart_detected
external_marker_received
```

Forbidden trigger names:

```text
thermal_root_cause
cpu_caused_frame_drop
driver_bug_detected
bad_firmware
power_issue_root_cause
```

### 7. Always-On Does Not Mean Always-Heavy

The recorder has three cost tiers:

```text
always-on:
  ultra-light semantic trickle

triggered:
  bounded incident-window freeze

burst:
  short, policy-approved deepening after a trigger
```

Always-on capture must not default to high-frequency perf/ftrace, raw logs, raw
video, or destructive probes.

### 8. Incident Data Must Be Evaluation-Ready

Every incident window should be usable later for trigger tuning, benchmarks, and
Agent evaluation.

Incident artifacts must include policy versions, device profile, workload phase
when known, capability report, coverage, `data_quality`, `artifact_trust`,
recorder overhead, and label lifecycle metadata.

## Architecture Layers

### Signal Sampler

Collects ultra-light semantic signals.

Initial Linux/Raspberry Pi-class signals:

- `cpu.summary`,
- `memory.summary`,
- `network.counters`,
- `thermal.zone`,
- `cpufreq.summary`,
- `process.topN`,
- `dmesg.cursor` or bounded mock cursor,
- `app.marker`,
- `adc.self_overhead`.

Jetson, QCOM, Android, ROS 2, GPU/NPU, and bus-specific signals are future
profiles. The contracts must stay edge-general.

### Rolling Buffer

Keeps pre-window evidence before Tx.

The MVP should start with memory-backed ring buffers and optional bounded
artifact snapshots. Disk-backed rings can follow after budget behavior is
measured.

The buffer stores semantic samples, counters, cursors, bounded slices, and
compressed facts. It must not default to unbounded logs, video, full traces, or
always-on high-frequency tracing.

The buffer must report configured retention, current retained range, expected
samples, recorded samples, dropped samples, known gap ranges, degraded
collectors, and storage volatility.

Continuous retention and frozen incidents have different persistence models:

```text
continuous rolling buffer:
  memory-backed
  non-persistent
  no continuous flash write requirement

frozen incident:
  may be materialized as a bounded artifact bundle
  subject to write budget, max_freeze_bytes, max_frozen_incidents, and retention policy
```

This is not a disk-backed rolling ring. It is a bounded incident export. The
distinction prevents "disk-backed ring deferred" from conflicting with "frozen
incident can be read later by an Agent".

Live recorder status may be written as a low-frequency heartbeat or on explicit
state transitions such as profile change, freeze, degradation, and daemon exit.
It must not be written at every sample iteration; `max_status_write_interval_ms`
is part of the recorder budget so status reporting does not become a continuous
flash-write path.

### Trigger Policy Engine

Evaluates autonomous and external trigger sources:

- self-autonomous signals such as thermal slope, CPU pressure, memory pressure,
  network drops, kmsg bursts, and recorder overhead budget exceedance;
- external hints such as frame drop markers, inference latency markers, ROS 2
  deadline missed events, watchdog events, or Android HAL warnings;
- correlation rules such as frame drop plus thermal rise plus cpufreq drop;
- retrospective markers submitted at Ty.

Initial detectors should be deterministic: threshold, slope, delta, EWMA/MAD-like
robust statistics, burst count, and correlation rules. ML detectors are deferred
to shadow mode after dataset discipline exists.

### Window Freezer

Freezes bounded incident windows on trigger or retrospective mark.

The frozen window includes:

- retained pre-window time range in the current MVP,
- trigger event refs,
- source coverage,
- missing/truncated signal accounting,
- bounded artifact refs,
- artifact trust,
- recorder overhead,
- freeze result,
- no root-cause claims.

The first freeze runtime should support marker-based freeze before automatic
trigger policy. Marker freeze proves the rolling buffer value with less risk of
turning triggers into cause detectors.

### Burst Deepening

After a trigger, the recorder may collect a short deepening burst selected by
policy and SafetyPolicy.

Examples:

- thermal/cpufreq trigger: higher-rate cpufreq/devfreq sample, process/thread CPU
  snapshot, bounded thermal dmesg slice;
- camera frame drop marker: process/thread CPU, app marker slice, bounded
  dmesg camera/thermal slice, optional ROS 2 topic stats if available;
- network trigger: interface counters, route/DNS snapshot, optional DDS discovery
  snapshot.

Burst deepening is never arbitrary shell and never destructive automation.

### Dataset Writer

Writes incident and comparison windows in an evaluation-ready format.

Window types:

- positive incident,
- negative background,
- near-miss,
- post-fix verification,
- manual retrospective.

Label types:

- weak label,
- domain label,
- Agent label,
- human label,
- verification label,
- final label.

## Agent-Facing Contracts

New Flight Recorder contracts:

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

PR4 defines the marker-first schema set:

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

`obs.trigger_policy.v1` and `obs.anomaly_score.v1` are deferred until richer
trigger policy and benchmark work. `obs.label_event.v1` and public sharing
dataset policy remain deferred.

Each contract follows the executable contract gate:

- JSON Schema,
- golden fixture,
- `additionalProperties: false`,
- enum-constrained vocabulary,
- coverage manifest entry,
- `data_quality` reuse where applicable,
- `artifact_trust` for target-originated text,
- generated CLI/MCP validation once surfaced,
- adversarial fixtures where prompt injection, root-cause promotion, or unsafe
  action drift is plausible.

## State Machines

### Recorder State

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

The state is exposed through `obs.recorder_status.v1`. `degraded`,
`over_budget`, and `error` require a `data_quality` note and a loss or budget
reason.

Allowed high-level transitions:

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

Forbidden transitions:

```text
disabled -> freezing
disabled -> frozen
recording -> exported
frozen -> frozen for the same incident_id
error -> frozen without a freeze_result
```

### Incident Lifecycle

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

The lifecycle is exposed through `obs.recorder_incident.v1` and
`obs.recorder_frozen_window.v1`. A frozen incident may be partial; partial freeze
must be represented by loss semantics rather than hidden.

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

### Freeze Reason

```text
external_marker
operator_marker
agent_marker
trigger_policy
budget_guard
daemon_shutdown
```

`budget_guard` and `daemon_shutdown` do not imply cause. They only explain why a
window was frozen or why history is partial.

## Freeze Persistence Model

Continuous rolling retention is memory-backed and volatile. Frozen incidents may
be materialized to disk as bounded artifact bundles:

```text
continuous_ring:
  persistence: memory_only
  survives_daemon_restart: false
  survives_target_reboot: false
  write_budget: zero_continuous_flash_writes

frozen_incident_bundle:
  persistence: bounded_artifact_bundle
  survives_daemon_restart: true_if_bundle_written
  survives_target_reboot: storage_dependent_not_claimed_by_mvp
  write_durability: best_effort_no_fsync
  bounded_by: max_freeze_bytes, max_disk_bytes, max_frozen_incidents, retention_policy
```

This is explicitly not a disk-backed rolling ring. It is a bounded export of a
selected incident window after marker or trigger selection.

The current MVP reports frozen incident target-reboot survival conservatively as
`false`; later storage-specific integrations may raise that only with measured
durability and retention guarantees.

## Budget Contract

PR4 must fix the initial budget vocabulary:

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

The incident-count budget is artifact-root scoped. `max_frozen_incidents` is
enforced against valid materialized incident bundles under
`recorder/incidents/`, not only against the current daemon process memory. This
prevents daemon restart from resetting the admission budget.

`existing_frozen_incidents` includes current-run incidents once they have been
materialized. `frozen_incidents_this_run` is informational and must not be added
again for admission decisions.

When incident inventory is malformed, unreadable, or symlinked, ADC fails closed
for freeze admission and reports the uncertainty through
`obs.recorder_budget_status.v1` and `data_quality`. PR6 does not delete old
incidents; budget exhaustion refuses new freezes until a future explicit
retention policy exists.

Budget exceedance is Agent-facing state, not an internal log line.

## Loss Report

Every recorder output that contains a time window must carry loss semantics,
either inline or by reference to `obs.loss_report.v1`.

Required loss fields:

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

Example:

```json
{
  "schema_version": "obs.loss_report.v1",
  "window_id": "win-001",
  "collector_loss": [
    {
      "collector_id": "thermal",
      "expected_samples": 60,
      "recorded_samples": 55,
      "dropped_samples": 5,
      "loss_confidence": "medium",
      "gap_ranges": [
        {
          "start_mono_ns": 33000000000,
          "end_mono_ns": 35000000000
        }
      ],
      "loss_reasons": ["ring_capacity_drop_oldest"]
    }
  ],
  "data_quality": {
    "dropped": true,
    "drop_count": 5,
    "throttled": false,
    "missing": [],
    "truncated": false,
    "clock_confidence": "medium",
    "notes": ["thermal ring over budget; oldest samples dropped"]
  }
}
```

## Current Daemon Delta

The existing `adc-targetd` daemon already provides:

```text
live sampling
profile-driven trigger evaluation
triggered evidence bundle creation
```

Flight Recorder adds:

```text
continuous rolling pre-window retention
explicit ring budget
drop accounting
bounded freeze
incident lifecycle
manual/external marker freeze
retrospective marker search
loss report
future bounded post-window capture contract
Agent-facing recorder status
```

This distinction matters because the current trigger bundle records the sample at
trigger time, while Flight Recorder preserves and explains the window around the
incident.

## CLI Surface

Implemented commands:

```bash
adc recorder status
adc recorder incidents
adc recorder incident get --incident-id INC-20260604-103122
adc recorder mark --symptom "camera frame drop observed around 10:31"
adc recorder export-dataset --selector profile=camera_inference_degradation
```

The existing top-level `adc arm --profile ...` and `adc disarm` commands remain
the daemon control surface. `adc recorder mark` queues a marker for
`adc-targetd`; the daemon freezes the retained memory-ring window into a bounded
incident bundle. `adc recorder incident get` reads a materialized incident, and
`adc recorder export-dataset` emits a local benchmark/regression manifest.
Incident resolution returns `artifact://recorder/...` refs by default. Those refs
are opened through bounded ADC ref resolution with artifact trust and
data_quality; local filesystem paths are not the default Agent-facing locator.

Planned commands:

```bash
adc recorder arm --profile camera_inference_degradation
adc recorder disarm
adc recorder coverage --incident-id INC-20260604-103122
```

Recorder commands must not expose arbitrary shell or destructive target
mutation.

## MCP Surface

Planned tools:

```text
obs.recorder_status
obs.list_incidents
obs.get_incident_window
obs.get_observation_coverage
obs.get_trigger_policy
obs.mark_incident
obs.export_dataset_manifest
```

MCP tools are read-only or recording-only unless future SafetyPolicy explicitly
approves a bounded action. PR4+ must not introduce probe execution through these
tools.

## Canonical Vertical

The first vertical is camera plus inference degradation under thermal and CPU
pressure.

The simulated scenario should contain:

- normal operation,
- thermal/cpufreq-like degradation,
- frame drop marker,
- inference latency marker,
- kmsg warning,
- post-fix normal window.

The recorder must preserve enough evidence for an Agent to distinguish:

- thermal mitigation,
- CPU scheduler saturation,
- memory pressure,
- camera or driver warning,
- application/service issue,
- insufficient evidence.

## Acceptance Criteria

1. Autonomous trigger freezes an incident without an app, human, or Agent marker.
2. External app marker can trigger or enrich an incident.
3. Retrospective marker at Ty can search retained buffers and return explicit
   coverage/missing evidence.
4. Incident windows include bounded retained pre-window evidence in the current
   MVP and do not dump unbounded raw logs.
5. Budgets are enforced or explicit degradation is recorded.
6. Recorder self-overhead appears in status and incident windows.
7. Missing or unavailable signals are represented through observation coverage
   and `data_quality`.
8. Target-originated text remains `treat_as_data_only`.
9. Trigger storms merge, cool down, or degrade instead of creating unbounded
   incidents.
10. Dataset export emits manifest metadata for incident, negative, near-miss,
    and post-fix windows.
11. A benchmark proves ADC Flight Recorder preserves Tx evidence unavailable to
    Ty-only shell or on-demand ADC.
12. ADC Flight Recorder never asserts root cause.
13. Recorder docs and contracts define recorder states, incident lifecycle,
    marker versus trigger, freeze reason enum, volatile memory-ring envelope, and
    loss report semantics.
14. Every frozen window contains `data_quality` and explicit loss semantics.
15. External/manual marker freeze is implemented before automatic trigger policy.
16. Trigger policies are symptom detectors, not cause detectors.
17. Continuous memory rings are volatile, while frozen incidents use explicit
    bounded artifact bundle persistence policy.
18. Marker contracts include received time, optional asserted event time, time
    confidence, source, trust, and centering policy.
19. Recorder and incident state machines define allowed and forbidden
    transitions.
20. PR4 schemas include recorder status, buffer status, budget, marker,
    incident, frozen window, loss report, and recorder overhead contracts.

## Initial Architecture Decision

### Decision Question

Should the first runtime use a memory ring buffer with bounded frozen incident
bundles, a disk-backed rolling ring, or capture-on-trigger only?

### Quality Drivers

| Driver | Scenario | Metric / threshold | Verification |
|---|---|---|---|
| Low overhead | Recorder runs on Raspberry Pi-class edge target while workload runs | Within configured CPU/memory/write budgets or explicit degrade | E2E overhead report and target smoke |
| Tx evidence availability | Agent starts after Tx | Pre-window coverage exists for selected signals | synthetic delayed-report benchmark |
| Flash safety | Recorder is armed for long periods | Disk writes remain bounded by policy | budget governor unit/integration tests |
| Agent safety | Incident contains untrusted text | Text is trust-labeled and not promoted | contract adversarial tests |
| Maintainability | Runtime grows across signals and profiles | sampler/buffer/trigger/freezer boundaries stay separate | code review and unit boundary tests |

### Options

| Option | Summary | Benefits | Risks |
|---|---|---|---|
| A | Memory ring plus bounded frozen bundles | Low continuous write amplification, clear budget control, Agent can read frozen incidents later | Continuous pre-window is lost on daemon restart before freeze |
| B | Disk-backed rolling ring first | Survives restart before freeze, more forensic value | Higher flash/write budget risk and more failure modes |
| C | Capture-on-trigger only | Minimal implementation | Does not solve Tx-before-Ty evidence loss |

### Decision

Choose Option A for the first runtime.

The first implementation should use memory-backed rolling buffers with explicit
drop accounting. Frozen incidents may be written as bounded artifact bundles
under explicit write, size, count, and retention budgets. Disk-backed rolling
retention can be added after budget metrics prove the write path is safe.
Capture-on-trigger alone is rejected because it does not solve the main product
problem.

## Migration Strategy

The current `adc-targetd` service loop should evolve rather than be replaced:

1. keep profile-driven operation and existing trigger bundle behavior;
2. add typed recorder contracts before runtime expansion;
3. introduce ring-buffered semantic samples behind the daemon loop;
4. freeze incident windows using the existing evidence bundle machinery;
5. add recorder-specific CLI/MCP surfaces;
6. add benchmark and dataset export after incident windows exist.
