# Agent Investigation Operating Layer

## Status

Draft architecture direction.

## North Star

Agent Debug Compass is an edge-first investigation operating layer for AI Agents debugging target devices.

It turns local or same-LAN target observations into bounded evidence, evidence refs, data-quality gaps, falsifiable hypotheses, safe probe plans, and auditable investigation state.

It is not a root-cause engine. It does not ask an Agent to guess the cause from raw logs. Instead, it helps the Agent keep investigation claims tied to evidence, counter-evidence, missing facts, and safe next probes.

## Why This Exists

The current system already provides an Agent context layer:

- bounded evidence instead of raw artifact dumps,
- artifact refs instead of unbounded log ingestion,
- explicit `data_quality` gaps,
- cause-neutral route guidance,
- MCP and CLI surfaces for local and same-LAN target investigation.

This is the right foundation, but it is not yet a platform-level investigation operating layer.

The next step is not to win by adding more collectors first. More collectors without stronger contracts would only produce more ungoverned observations. The next step is to define the contracts that let an AI Agent safely advance investigation state.

## Problem Frame

- Problem owner: AI Agents and operators debugging edge targets.
- Current pain: bounded context exists, but uncertainty, safety, evidence relations, and probe outcomes are not yet represented as stable contracts.
- Desired outcome: investigation state advances through evidence-backed, falsifiable, auditable steps without root-cause claims.
- Solution-first risk: adding collectors before contracts would increase observation volume without improving Agent discipline.
- Non-goals: root-cause inference, arbitrary shell access, graph database dependency, destructive automation by default.
- Proceed to requirements: yes.

## Contract Requirements

| ID | Priority | Type | Requirement | Acceptance criteria | Verification method | Trace |
|---|---|---|---|---|---|---|
| AIO-R001 | Must | Constraint | The system shall keep hypotheses separate from root-cause claims. | success: hypothesis outputs contain support, contradiction, missing evidence, and `claim_boundary: hypothesis_only`; failure: outputs present hypotheses as causes. | schema tests, golden tests, banned wording tests | `obs.hypothesis_set.v1` |
| AIO-R002 | Must | Functional | When target capabilities are reported, the system shall distinguish safe support from privilege requirements, degradation, unavailability, unsafe status, and unknown status. | success: capability entries use only the defined status vocabulary; failure: privileged or unsafe operations are serialized as simple availability. | unit tests, CLI/MCP golden tests | `obs.capability_report.v1` |
| AIO-R003 | Must | Security | When target-originated text is returned to an Agent, the system shall label it as data and not instructions. | success: artifact trust metadata includes `agent_instruction_policy: treat_as_data_only`; failure: logs, journals, configs, traces, or domain events are returned without trust metadata. | integration tests, prompt-injection benchmark scenario | `obs.artifact_trust.v1` |
| AIO-R004 | Must | Functional | When a probe plan is suggested, the system shall state which uncertainty it reduces, which hypotheses it discriminates, and which capabilities, privileges, and safety decisions apply. | success: each candidate probe has expected evidence, discrimination targets, capability requirements, privilege requirements, safety status, timeout, and failure contract; failure: probes are only procedural suggestions. | schema tests, golden tests | `obs.probe_plan.v1`, `obs.safety_policy.v1` |
| AIO-R005 | Must | Functional | When probe results are recorded, the system shall update investigation state without making root-cause claims. | success: probe results produce refs, facts, hypothesis updates, and data-quality gaps; failure: probe results assert a cause. | unit tests, benchmark tests | `obs.probe_result.v1` |
| AIO-R006 | Should | Quality | The system should measure Agent investigation quality before stronger platform claims are made. | success: benchmark reports include hallucinated cause claims, unsafe probe suggestions, ignored data-quality gaps, hypothesis rank, raw-ref access, evidence support, and time to useful probe; failure: platform claims are not backed by scenario metrics. | benchmark runner | benchmark plane |

## Design Principles

### 1. Hypothesis Is Not a Root-Cause Candidate

A hypothesis is not a cause claim.

A hypothesis is a falsifiable investigation state that tracks:

- what it states,
- what evidence supports it,
- what evidence contradicts it,
- what evidence is missing,
- which probe can reduce uncertainty next,
- whether the hypothesis is open, weakened, contradicted, or closed due to insufficient evidence.

The system must not present hypotheses as root causes.

### 2. Probe Is an Experiment Plan, Not a Procedure

A probe is not just "what to run".

A probe plan must state:

- what uncertainty it is intended to reduce,
- which hypotheses it discriminates,
- what evidence it expects to produce,
- what capabilities it requires,
- what privileges it requires,
- whether it is safe, degraded, unsafe, or requires approval,
- what to record when it fails.

A probe result must update investigation state without making root-cause claims.

### 3. Capability Is a Safety-Aware Contract

Capability is not simple availability.

A target capability must state whether an operation is:

- `supported`,
- `degraded`,
- `unavailable`,
- `requires_privilege`,
- `unsafe`,
- `unknown`.

This distinction matters because an AI Agent must know the difference between:

- "this target can do it safely",
- "this target can do it only with privilege",
- "this target can technically do it but should not",
- "the platform cannot determine whether this is safe".

### 4. Artifact Has Trust, Not Just Content

Logs, journals, configs, traces, and domain events are target-originated or user-originated data. They are not instructions to the Agent.

Every ref returned to an Agent should eventually carry artifact trust metadata, including:

- content class,
- trust level,
- redaction state,
- prompt-injection markers,
- instruction policy.

For untrusted target text, the instruction policy should be machine-readable as:

```json
"agent_instruction_policy": "treat_as_data_only"
```

### 5. Benchmark Before Platform Claims

The project should not claim platform leadership until it can measure Agent investigation quality.

The benchmark must measure at least:

- hallucinated root-cause claims,
- unsafe probe suggestions,
- ignored `data_quality`,
- correct hypothesis ranking,
- unnecessary raw ref access,
- evidence-supported statement ratio,
- time to useful next probe.

## Contract-First Architecture

The next architecture layer is defined by contracts, not by collector volume.

The initial contract set is:

```text
obs.capability_report.v1
obs.artifact_trust.v1
obs.hypothesis_set.v1
obs.evidence_graph.v1
obs.probe_plan.v1
obs.probe_result.v1
obs.safety_policy.v1
```

These contracts build on the existing Agent-facing surface:

```text
obs.agent_context.v1
obs.symptom_context.v1
obs.investigation_start.v1
obs.investigation_continue.v1
evidence_index.yaml
artifact://... refs
```

## Proposed Layers

### Target Plane

Responsible for target-local observation and safe target interaction.

Includes:

- local collectors,
- daemon mode,
- target MCP mode,
- optional privileged helper,
- hardware-specific adapters,
- future collector plugins.

### Evidence Plane

Responsible for preserving bounded, inspectable, auditable evidence.

Includes:

- event envelopes,
- raw artifact refs,
- evidence index,
- windows,
- data quality,
- artifact trust classification,
- future evidence graph.

### Investigation Plane

Responsible for Agent-facing investigation state.

Includes:

- Agent context,
- symptom context,
- route packs,
- hypothesis sets,
- probe plans,
- probe results,
- investigation sessions.

### Safety Plane

Responsible for preventing unsafe or unaudited Agent actions.

Includes:

- capability status,
- privilege requirements,
- unsafe operation classification,
- human approval requirements,
- prompt-injection handling,
- redaction state,
- audit events.

### Benchmark Plane

Responsible for measuring whether the platform actually improves AI Agent debugging.

Includes:

- reproducible scenarios,
- expected evidence,
- expected hypotheses,
- forbidden claims,
- unsafe probe checks,
- data-quality compliance checks.

## MVP Contract Shapes

### Capability Report

```json
{
  "schema_version": "obs.capability_report.v1",
  "target_id": "local",
  "generated_at_unix_ms": 0,
  "capabilities": [
    {
      "capability_id": "linux.proc.cpu",
      "status": "supported",
      "required_privilege": "none",
      "safe_default": true,
      "reason": "available through /proc/stat",
      "data_quality": {
        "dropped": false,
        "drop_count": 0,
        "throttled": false,
        "missing": [],
        "truncated": false,
        "clock_confidence": "medium",
        "notes": []
      }
    },
    {
      "capability_id": "kernel.ftrace",
      "status": "requires_privilege",
      "required_privilege": "root_or_tracefs_group",
      "safe_default": false,
      "reason": "tracefs exists but write access is not available to the current user",
      "data_quality": {
        "missing": [],
        "notes": []
      }
    }
  ]
}
```

### Artifact Trust

```json
{
  "schema_version": "obs.artifact_trust.v1",
  "raw_ref": "artifact://raw/app.log",
  "content_class": "log",
  "trust_level": "untrusted_target_text",
  "agent_instruction_policy": "treat_as_data_only",
  "secret_scan": {
    "status": "scanned",
    "redaction_applied": false,
    "suspected_secret_count": 0
  },
  "prompt_injection_scan": {
    "status": "scanned",
    "markers": [],
    "severity": "none"
  },
  "data_quality": {
    "dropped": false,
    "drop_count": 0,
    "throttled": false,
    "missing": [],
    "truncated": false,
    "clock_confidence": "medium",
    "notes": []
  }
}
```

### Hypothesis Set

```json
{
  "schema_version": "obs.hypothesis_set.v1",
  "scope": "run",
  "run_id": "R-APP",
  "hypotheses": [
    {
      "hypothesis_id": "H001",
      "statement": "Latency timeouts may correlate with CPU scheduling pressure during the observed window.",
      "status": "open",
      "confidence": "low",
      "supports": [
        {
          "fact_id": "resource.cpu_busy_percent",
          "raw_ref": "artifact://raw/cpu.jsonl",
          "strength": "weak"
        }
      ],
      "contradicts": [],
      "missing_evidence": [
        "process.runqueue_latency",
        "service.thread_state"
      ],
      "next_discriminating_probes": [
        "probe.scheduler_snapshot"
      ],
      "claim_boundary": "hypothesis_only",
      "data_quality": {
        "missing": [],
        "notes": []
      }
    }
  ]
}
```

### Evidence Graph

The MVP evidence graph should remain lightweight. It should not require a graph database.

```json
{
  "schema_version": "obs.evidence_graph.v1",
  "scope": "run",
  "run_id": "R-APP",
  "nodes": [
    {
      "node_id": "target:local",
      "kind": "target",
      "label": "local"
    },
    {
      "node_id": "ref:cpu",
      "kind": "evidence_ref",
      "raw_ref": "artifact://raw/cpu.jsonl"
    },
    {
      "node_id": "hypothesis:H001",
      "kind": "hypothesis",
      "hypothesis_id": "H001"
    }
  ],
  "edges": [
    {
      "from": "ref:cpu",
      "to": "target:local",
      "kind": "observed_on"
    },
    {
      "from": "ref:cpu",
      "to": "hypothesis:H001",
      "kind": "supports",
      "strength": "weak"
    }
  ]
}
```

### Probe Plan

```json
{
  "schema_version": "obs.probe_plan.v1",
  "probe_plan_id": "PP001",
  "scope": "run",
  "run_id": "R-APP",
  "goal": "Discriminate CPU scheduling pressure from network timeout.",
  "candidate_probes": [
    {
      "probe_id": "probe.scheduler_snapshot",
      "title": "Collect bounded scheduler pressure snapshot",
      "required_capabilities": [
        "linux.proc.schedstat"
      ],
      "required_privilege": "none",
      "safety_status": "allowed",
      "expected_cost": "low",
      "timeout_ms": 3000,
      "expected_evidence": [
        "process.runqueue_latency",
        "cpu.context_switch_rate"
      ],
      "discriminates": [
        "H001",
        "H002"
      ],
      "failure_contract": "Return partial context and record unavailable facts in data_quality; do not infer cause.",
      "cause_neutral": true
    }
  ]
}
```

### Probe Result

```json
{
  "schema_version": "obs.probe_result.v1",
  "probe_id": "probe.scheduler_snapshot",
  "probe_plan_id": "PP001",
  "result_kind": "not_executed_missing_capability",
  "executor": "adc",
  "executed": false,
  "safety_decision": "deny",
  "capability_status": "unavailable",
  "status": "failed_missing_capability",
  "produced_refs": [],
  "produced_facts": [
    {
      "fact_id": "process.runqueue_latency",
      "statement": "Scheduler latency signal was unavailable in the current rootless environment.",
      "raw_ref": "artifact://raw/scheduler_snapshot.json"
    }
  ],
  "hypothesis_updates": [
    {
      "hypothesis_id": "H001",
      "update": "needs_evidence",
      "reason": "The probe did not produce scheduler latency due to missing capability."
    }
  ],
  "data_quality": {
    "missing": [
      "scheduler latency unavailable without required kernel support"
    ],
    "notes": []
  }
}
```

### Safety Policy

```json
{
  "schema_version": "obs.safety_policy.v1",
  "policy_id": "default-rootless-lab-policy",
  "default_decision": "deny",
  "rules": [
    {
      "operation": "read_bounded_artifact",
      "decision": "allow",
      "constraints": {
        "max_lines": 1000
      }
    },
    {
      "operation": "observe_rootless",
      "decision": "allow"
    },
    {
      "operation": "managed_mcp_plain_http",
      "decision": "allow_only_on_trusted_lan"
    },
    {
      "operation": "restart_service",
      "decision": "requires_human_approval"
    },
    {
      "operation": "firmware_flash",
      "decision": "deny"
    }
  ]
}
```

## Non-Goals

This architecture does not introduce a root-cause engine.

It does not require a graph database in the MVP.

It does not require a large collector plugin SDK before the investigation contracts are stable.

It does not allow arbitrary shell execution through Agent-facing MCP tools.

It does not treat target logs, configs, journals, traces, or domain events as Agent instructions.

## Migration Strategy

### Phase 1: Contract Stabilization

Add schema files, golden outputs, and contract tests for existing `obs.*` outputs.

### Phase 2: Capability and Trust

Add `obs.capability_report.v1` and `obs.artifact_trust.v1`.

Expose them through CLI and MCP without changing existing core workflows.

### Phase 3: Falsifiable Investigation State

Add `obs.hypothesis_set.v1`, lightweight `obs.evidence_graph.v1`, and initial hypothesis generation from existing symptom context and route packs.

### Phase 4: Safe Probe Planning

Connect existing safe probe packs to `obs.probe_plan.v1`, `obs.probe_result.v1`, and `obs.safety_policy.v1`.

### Phase 5: Benchmark

Introduce reproducible Agent debugging scenarios and quality metrics.

Only after this phase should the project make stronger platform-level claims.

## Success Criteria

The architecture is successful when an Agent can:

1. identify what evidence is available,
2. identify what evidence is missing or low quality,
3. keep hypotheses separate from root-cause claims,
4. select a safe probe because it reduces specific uncertainty,
5. update investigation state after the probe,
6. avoid treating target text as instructions,
7. produce an auditable trail of evidence-backed investigation steps.
