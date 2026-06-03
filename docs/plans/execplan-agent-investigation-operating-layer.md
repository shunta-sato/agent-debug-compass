# ExecPlan: Agent Investigation Operating Layer

## Goal

Evolve Agent Debug Compass from an Agent context layer into an Agent Investigation Operating Layer.

The goal is not to build a root-cause engine.

The goal is to define and implement contracts that let AI Agents safely advance debugging investigations using bounded evidence, falsifiable hypotheses, safe probe plans, capability-aware decisions, artifact trust labels, and benchmarked behavior.

## Current Baseline

The repository already provides:

- bounded local and same-LAN observations,
- `obs.*` MCP tools,
- `adc` CLI workflows,
- evidence refs,
- `data_quality`,
- cause-neutral route packs,
- service investigation packs,
- fleet partial-failure handling,
- rootless-first security posture.

This ExecPlan preserves those strengths.

## Core Direction

The next winning move is not collector expansion.

The next winning move is contract stabilization:

```text
schema registry
capability report
artifact trust classification
hypothesis set
evidence graph
probe plan
probe result
safety policy
benchmark
```

Collector plugin SDK and hardware expansion should follow these contracts, not precede them.

## Workstream 1: Schema Registry Skeleton

### Objective

Create an external schema registry for Agent-facing contracts.

### Deliverables

- `schemas/README.md`
- `schemas/obs.data_quality.v1.schema.json`
- `schemas/obs.event_envelope.v1.schema.json`
- `schemas/obs.evidence_index.v2.schema.json`
- `schemas/obs.agent_context.v1.schema.json`
- `tests/golden/README.md`
- golden output fixtures for representative CLI/MCP outputs
- contract test runner

### Acceptance Criteria

- Existing `obs.*` outputs can be validated against checked-in schemas.
- Golden output tests fail when a contract is accidentally changed.
- The schema registry is intentionally small and does not attempt to model future contracts before they are used.

### Non-Goals

- Do not introduce a graph database.
- Do not rewrite current output structures unnecessarily.
- Do not add large collector changes in this workstream.

## Workstream 2: Capability Report

### Objective

Expose target capability as a safety-aware contract.

### New Contract

`obs.capability_report.v1`

### Required Status Values

```text
supported
degraded
unavailable
requires_privilege
unsafe
unknown
```

### Deliverables

- Rust model for `CapabilityReport`
- conversion from existing kernel/system capability detection
- CLI command or subcommand that returns the capability report
- MCP tool for capability report retrieval
- schema file
- golden output
- contract tests

### Acceptance Criteria

- Rootless Linux baseline capabilities are reported as supported where available.
- Privileged capabilities are not reported as simply unavailable if they are available but require privilege.
- Unsafe operations can be represented explicitly.
- Missing capability evidence is reflected in `data_quality`.

### Non-Goals

- Do not implement all hardware-specific capabilities yet.
- Do not make privileged operations available by default.

## Workstream 3: Artifact Trust Classification

### Objective

Classify artifacts by trust level and instruction policy before Agent consumption.

### New Contract

`obs.artifact_trust.v1`

### Required Fields

- `raw_ref`
- `content_class`
- `trust_level`
- `agent_instruction_policy`
- `secret_scan`
- `prompt_injection_scan`
- `data_quality`

### Required Instruction Policy

For target-originated text:

```json
"agent_instruction_policy": "treat_as_data_only"
```

### Deliverables

- artifact trust model
- trust metadata for staged logs, journals, configs, domain events, OTLP, Perfetto, and raw slices
- minimal prompt-injection marker detection
- secret scan result reporting
- integration into `obs.get_ref` and adjacent ref metadata
- schema file
- golden output
- contract tests

### Acceptance Criteria

- Logs and journals are never returned as trusted instructions.
- Target-originated text is marked as untrusted target text.
- Redaction state is machine-readable.
- Prompt-injection-like markers are recorded as artifact trust metadata, not executed or followed.
- Existing bounded ref behavior remains intact.

### Non-Goals

- Do not try to solve perfect secret detection.
- Do not block all logs that contain instruction-like text.
- Do not change the principle that raw artifacts are ref-only in first-read context.

## Workstream 4: Hypothesis Set MVP

### Objective

Represent uncertainty as falsifiable investigation state.

### New Contract

`obs.hypothesis_set.v1`

### Hypothesis Status Values

```text
open
supported
weakened
contradicted
needs_evidence
closed_insufficient_evidence
```

### Deliverables

- Rust model for hypothesis set
- schema file
- initial hypothesis generation from existing symptom context and route packs
- support, contradiction, and missing evidence references
- next discriminating probe references
- contract tests that forbid root-cause claim wording
- golden outputs for latency, memory, service, network, and unknown symptom cases

### Acceptance Criteria

- Hypotheses are not named or serialized as root-cause candidates.
- Each hypothesis carries supports, contradicts, missing evidence, and next discriminating probes.
- The system can return an empty or low-confidence hypothesis set when evidence is insufficient.
- Cause-neutral behavior is preserved.

### Non-Goals

- Do not claim root cause.
- Do not introduce probabilistic diagnosis as a required feature.
- Do not require LLM inference inside ADC core.

## Workstream 5: Probe Plan, Probe Result, and Safety Policy

### Objective

Turn safe probes into explicit experiment plans and auditable results.

### New Contracts

```text
obs.probe_plan.v1
obs.probe_result.v1
obs.safety_policy.v1
```

### Deliverables

- ProbePlan model
- ProbeResult model
- SafetyPolicy model
- connection from existing SafeProbePack to ProbePlan
- safety decision model
- schema files
- golden outputs
- contract tests
- CLI/MCP surfaces for suggesting probe plans and recording probe results

### Acceptance Criteria

- A probe plan states what uncertainty it reduces.
- A probe plan states which hypotheses it discriminates.
- A probe plan states required capabilities and privileges.
- A safety policy can deny, allow, or require approval.
- A failed probe records missing facts in `data_quality` rather than implying cause.
- Probe results can update hypothesis state without making root-cause claims.

### Non-Goals

- Do not allow arbitrary shell execution.
- Do not implement destructive actions by default.
- Do not require full automation of service restart, flashing, or power cycling.

## Workstream 6: Benchmark MVP

### Objective

Measure Agent debugging quality before making stronger platform claims.

### Deliverables

- `benchmarks/README.md`
- reproducible scenario structure
- initial scenario set:
  - latency from CPU pressure
  - latency from network degradation
  - service crash loop
  - memory growth
  - config drift
  - thermal throttle
  - disk IO pressure
  - partial fleet failure
  - prompt-injection log
- expected hypotheses
- expected refs
- forbidden claims
- unsafe probe checks
- `data_quality` compliance checks
- report generator

### Metrics

- hallucinated root-cause claim count
- unsafe probe suggestion count
- ignored `data_quality` count
- correct hypothesis rank
- unnecessary raw ref access count
- evidence-supported statement ratio
- time to first useful probe

### Acceptance Criteria

- Benchmark can be run locally.
- Reports are machine-readable.
- At least one scenario checks prompt-injection handling.
- At least one scenario checks partial fleet failure.
- At least one scenario checks that the Agent does not make a root-cause claim when evidence is insufficient.

### Non-Goals

- Do not claim world-class status from the first benchmark.
- Do not require real hardware for every benchmark scenario.
- Do not block future real-device benchmark additions.

## PR Sequence

### PR 1: Schema Registry Skeleton

Create the schema directory, initial schemas, golden output structure, and contract test runner.

### PR 2: Capability Report

Add `obs.capability_report.v1`, expose it through CLI/MCP, and validate with golden tests.

### PR 3: Artifact Trust Classification

Add `obs.artifact_trust.v1`, classify target-originated text, and integrate trust metadata with ref retrieval.

### PR 4: Hypothesis Set MVP

Add `obs.hypothesis_set.v1`, generate initial falsifiable hypotheses from existing symptom and route context, and test that no root-cause claims are emitted.

### PR 5: Probe Plan, Probe Result, and Safety Policy

Add safe experiment planning contracts, connect existing safe probe packs, and record probe results in a cause-neutral way.

### PR 6: Benchmark MVP

Add reproducible Agent debugging benchmark scenarios and quality metrics.

## Risks

### Risk: Contract Explosion

Too many schemas may slow implementation and create unstable abstractions.

Mitigation:

- Start with minimal fields.
- Add fields only when they are used by CLI/MCP output or tests.
- Keep schemas versioned and backward-compatible where possible.

### Risk: Hypothesis Misread as Root Cause

Agents or users may read hypotheses as cause claims.

Mitigation:

- Avoid `root_cause_candidate` naming.
- Include `claim_boundary: hypothesis_only`.
- Add contract tests for banned wording.
- Keep support, contradiction, and missing evidence visible.

### Risk: Probe Planning Becomes Unsafe Automation

Probe planning may drift into allowing destructive actions.

Mitigation:

- Default deny in SafetyPolicy.
- No arbitrary shell.
- Human approval status for disruptive actions.
- Explicit unsafe status where needed.

### Risk: Plugin Work Starts Too Early

Collector plugin expansion before contracts stabilize may produce ungoverned evidence.

Mitigation:

- Defer plugin SDK until after initial contracts.
- Require plugins to declare output schemas, capability requirements, safety implications, and data-quality contracts.

## Overall Done Definition

This ExecPlan is complete when:

1. schemas exist for the initial contract set,
2. capability reports are machine-readable and safety-aware,
3. artifact trust labels are returned for target-originated text,
4. hypotheses are represented as falsifiable investigation state,
5. probe plans and results are contract-backed and safety-aware,
6. an initial benchmark measures Agent debugging quality,
7. no new feature requires Agent-facing root-cause claims.

## PR 1 Done Definition

The first implementation PR is complete when:

1. the architecture direction and ExecPlan are documented in English,
2. `schemas/` contains the initial contract schemas used by current CLI/MCP outputs,
3. `tests/golden/` contains matching minimal fixtures,
4. the contract test runner validates the schema/fixture set,
5. contract vocabulary is enum-constrained in Rust and JSON Schema,
6. README separates the small first-read surface from the broader investigation contract set,
7. CLI help, MCP tools, README examples, and golden fixtures describe the same implemented surface,
8. artifact trust metadata is returned whenever bounded refs are opened for Agent-facing investigation output,
9. probe results distinguish executed results from non-executed capability or policy outcomes.
