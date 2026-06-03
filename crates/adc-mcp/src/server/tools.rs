use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use serde_json::json;

use super::ServerMode;

pub(super) fn tool_definitions(mode: ServerMode) -> Vec<Tool> {
    all_tool_definitions()
        .into_iter()
        .filter(|tool| mode.allows_tool(tool.name.as_ref()))
        .collect()
}

fn all_tool_definitions() -> Vec<Tool> {
    let read_only = ToolAnnotations::new()
        .read_only(true)
        .destructive(false)
        .idempotent(true)
        .open_world(false);
    let observation_write = ToolAnnotations::new()
        .read_only(false)
        .destructive(false)
        .idempotent(false)
        .open_world(false);

    vec![
        Tool::new(
            "obs.status",
            "Return target observability status.",
            empty_schema(),
        )
        .annotate(read_only.clone()),
        Tool::new(
            "obs.doctor",
            "Return install and capability readiness without requiring root residency.",
            empty_schema(),
        )
        .annotate(read_only.clone()),
        Tool::new(
            "obs.preflight",
            "Return target-local readiness for observation without requiring root.",
            target_preflight_schema(),
        )
        .annotate(read_only.clone()),
        Tool::new(
            "obs.snapshot",
            "Create a target-local snapshot and evidence index.",
            snapshot_schema(),
        )
        .annotate(observation_write.clone()),
        Tool::new(
            "obs.observe",
            "Run a bounded local observation and return an Agent context pack.",
            observe_schema(),
        )
        .annotate(observation_write.clone()),
        Tool::new(
            "obs.get_capabilities",
            "Return a safety-aware target capability report.",
            empty_schema(),
        )
        .annotate(read_only.clone()),
        Tool::new(
            "obs.get_agent_context",
            "Return compact Agent-ready context for an existing run.",
            agent_context_schema(),
        )
        .annotate(read_only.clone()),
        Tool::new(
            "obs.investigate_bug",
            "Compile a symptom-first cause-neutral Agent context pack for an existing run or fleet.",
            investigate_bug_schema(),
        )
        .annotate(observation_write.clone()),
        Tool::new(
            "obs.start_investigation",
            "Return a one-shot Agent context pack with an explicit investigation route.",
            start_investigation_schema(),
        )
        .annotate(observation_write.clone()),
        Tool::new(
            "obs.continue_investigation",
            "Open selected bounded route refs and return the next compact investigation context.",
            continue_investigation_schema(),
        )
        .annotate(observation_write.clone()),
        Tool::new(
            "obs.get_investigation_session",
            "Return persisted investigation session state for resumable Agent work.",
            investigation_session_schema(),
        )
        .annotate(read_only.clone()),
        Tool::new(
            "obs.record_probe_result",
            "Record a bounded probe result contract without executing a probe.",
            probe_result_schema(),
        )
        .annotate(observation_write.clone()),
        Tool::new(
            "obs.list_route_packs",
            "Return typed investigation route packs and their required facts.",
            empty_schema(),
        )
        .annotate(read_only.clone()),
        Tool::new(
            "obs.get_evidence_index",
            "Return the bounded v2 evidence index for a run.",
            run_id_schema(),
        )
        .annotate(read_only.clone()),
        Tool::new(
            "obs.get_window",
            "Return one bounded evidence window for a run.",
            window_schema(),
        )
        .annotate(read_only.clone()),
        Tool::new(
            "obs.get_signal_series",
            "Return bounded timeline events for one source.",
            signal_series_schema(),
        )
        .annotate(read_only.clone()),
        Tool::new(
            "obs.get_raw_slice",
            "Return a bounded slice from an explicit raw artifact ref.",
            raw_slice_schema(),
        )
        .annotate(read_only.clone()),
        Tool::new(
            "obs.get_ref",
            "Resolve a typed artifact ref such as raw, window, manifest, evidence, or context.",
            agent_ref_schema(),
        )
        .annotate(read_only.clone()),
        Tool::new(
            "obs.suggest_next_probe",
            "Return cause-neutral next observation options from evidence debt.",
            run_id_schema(),
        )
        .annotate(read_only.clone()),
        Tool::new(
            "obs.search_evidence",
            "Search observed evidence events with a bounded limit.",
            search_schema(),
        )
        .annotate(read_only.clone()),
        Tool::new(
            "obs.compare_runs",
            "Compare two runs with bounded metric deltas and evidence refs.",
            compare_schema(),
        )
        .annotate(read_only.clone()),
        Tool::new(
            "obs.investigate_service",
            "Return a bounded cause-neutral Linux service investigation pack.",
            service_investigation_schema(),
        )
        .annotate(read_only.clone()),
        Tool::new(
            "obs.discover_targets",
            "Discover same-network target candidates from the local neighbor table.",
            discover_targets_schema(),
        )
        .annotate(read_only.clone()),
        Tool::new(
            "obs.fleet_preflight",
            "Validate an explicit fleet inventory before observation.",
            fleet_preflight_schema(),
        )
        .annotate(read_only.clone()),
        Tool::new(
            "obs.fleet_observe",
            "Run bounded observation for an explicit fleet inventory.",
            fleet_capture_schema(),
        )
        .annotate(observation_write.clone()),
        Tool::new(
            "obs.fleet_snapshot",
            "Run snapshot for an explicit fleet inventory.",
            fleet_snapshot_schema(),
        )
        .annotate(observation_write.clone()),
        Tool::new(
            "obs.fleet_capture",
            "Run bounded capture for an explicit fleet inventory.",
            fleet_capture_schema(),
        )
        .annotate(observation_write.clone()),
        Tool::new(
            "obs.fleet_investigate_service",
            "Return bounded cause-neutral Linux service investigation packs for a fleet.",
            fleet_service_investigation_schema(),
        )
        .annotate(read_only.clone()),
        Tool::new(
            "obs.get_fleet_evidence",
            "Return bounded fleet evidence for a fleet run.",
            fleet_run_schema(),
        )
        .annotate(read_only),
    ]
}

fn empty_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {},
    }))
    .expect("static empty schema is an object")
}

fn run_id_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["run_id"],
        "properties": {
            "run_id": {
                "type": "string",
                "description": "Single relative run id segment."
            }
        },
    }))
    .expect("static run_id schema is an object")
}

fn target_preflight_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "target_id": {
                "type": "string",
                "description": "Target identity to stamp into evidence."
            }
        },
    }))
    .expect("static target preflight schema is an object")
}

fn snapshot_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["run_id"],
        "properties": {
            "run_id": {"type": "string"},
            "target_id": {"type": "string"}
        },
    }))
    .expect("static snapshot schema is an object")
}

fn agent_context_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "run_id": {
                "type": "string",
                "description": "Single relative run id segment, or latest."
            },
            "fleet_run_id": {
                "type": "string",
                "description": "Single relative fleet run id segment."
            },
            "service_name": {
                "type": "string",
                "description": "Optional service pack name to fuse into run investigation context."
            }
        },
    }))
    .expect("static agent context schema is an object")
}

fn start_investigation_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "run_id": {
                "type": "string",
                "description": "Single relative run id segment, or latest."
            },
            "fleet_run_id": {
                "type": "string",
                "description": "Single relative fleet run id segment, or latest."
            },
            "service_name": {
                "type": "string",
                "description": "Optional Linux service/unit name to collect and fuse before routing."
            },
            "inventory_path": {
                "type": "string",
                "description": "Required to collect a fresh fleet service pack; existing fleet packs can be reused without it."
            },
            "max_journal_lines": {
                "type": "integer",
                "minimum": 1,
                "maximum": 200
            }
        },
    }))
    .expect("static start investigation schema is an object")
}

fn investigate_bug_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["symptom"],
        "properties": {
            "run_id": {
                "type": "string",
                "description": "Single relative run id segment, or latest."
            },
            "fleet_run_id": {
                "type": "string",
                "description": "Single relative fleet run id segment, or latest."
            },
            "service_name": {
                "type": "string",
                "description": "Optional Linux service/unit name to fuse into the symptom context."
            },
            "inventory_path": {
                "type": "string",
                "description": "Required when collecting fresh fleet service context."
            },
            "symptom": {
                "type": "string",
                "description": "Agent/user symptom text or a supported symptom enum."
            },
            "max_journal_lines": {
                "type": "integer",
                "minimum": 1,
                "maximum": 200
            }
        },
    }))
    .expect("static investigate bug schema is an object")
}

fn continue_investigation_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["current_step_id"],
        "properties": {
            "run_id": {
                "type": "string",
                "description": "Single relative run id segment, or latest."
            },
            "fleet_run_id": {
                "type": "string",
                "description": "Single relative fleet run id segment, or latest."
            },
            "service_name": {
                "type": "string",
                "description": "Optional service/unit name to retain in the continued route."
            },
            "route_id": {
                "type": "string",
                "description": "Optional route id guard."
            },
            "session_id": {
                "type": "string",
                "description": "Optional deterministic session id to persist."
            },
            "current_step_id": {
                "type": "string",
                "description": "Investigation route step id to open, such as IR001."
            },
            "open_ref_labels": {
                "type": "array",
                "items": {"type": "string"},
                "description": "Optional subset of labels from the selected route step."
            },
            "open_raw_refs": {
                "type": "array",
                "items": {"type": "string"},
                "description": "Optional explicit bounded refs to open."
            },
            "max_ref_lines": {
                "type": "integer",
                "minimum": 1,
                "maximum": 1000
            }
        },
    }))
    .expect("static continue investigation schema is an object")
}

fn investigation_session_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["session_id"],
        "properties": {
            "run_id": {
                "type": "string",
                "description": "Single relative run id segment, or latest."
            },
            "fleet_run_id": {
                "type": "string",
                "description": "Single relative fleet run id segment, or latest."
            },
            "session_id": {
                "type": "string",
                "description": "Investigation session id returned by obs.continue_investigation."
            }
        },
    }))
    .expect("static investigation session schema is an object")
}

fn observe_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["run_id", "duration_ms"],
        "properties": {
            "run_id": {"type": "string"},
            "duration_ms": {"type": "integer", "minimum": 1},
            "interval_ms": {"type": "integer", "minimum": 1},
            "target_id": {"type": "string"},
            "profile_id": {"type": "string"},
            "log_file": {"type": "string"},
            "domain_events_file": {"type": "string"},
            "config_file": {"type": "string"},
            "service_name": {"type": "string"},
            "otlp_file": {"type": "string"},
            "journald_jsonl_file": {"type": "string"},
            "perfetto_file": {"type": "string"}
        },
    }))
    .expect("static observe schema is an object")
}

fn window_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["run_id", "window_id"],
        "properties": {
            "run_id": {
                "type": "string",
                "description": "Single relative run id segment."
            },
            "window_id": {
                "type": "string",
                "description": "Single relative window id segment."
            }
        },
    }))
    .expect("static window schema is an object")
}

fn search_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["run_id"],
        "properties": {
            "run_id": {"type": "string"},
            "source": {"type": "string"},
            "event_type": {"type": "string"},
            "contains": {"type": "string"},
            "limit": {"type": "integer", "minimum": 1, "maximum": 100}
        },
    }))
    .expect("static search schema is an object")
}

fn signal_series_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["run_id", "source"],
        "properties": {
            "run_id": {"type": "string"},
            "source": {"type": "string"},
            "limit": {"type": "integer", "minimum": 1, "maximum": 100}
        },
    }))
    .expect("static signal series schema is an object")
}

fn raw_slice_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["run_id", "raw_ref"],
        "properties": {
            "run_id": {"type": "string"},
            "raw_ref": {"type": "string", "description": "artifact://raw/... ref from evidence_index"},
            "limit": {"type": "integer", "minimum": 1, "maximum": 1000}
        },
    }))
    .expect("static raw slice schema is an object")
}

fn agent_ref_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["ref"],
        "properties": {
            "run_id": {
                "type": "string",
                "description": "Single relative run id segment. Required for run-scoped refs; omit for service investigation refs."
            },
            "ref": {
                "type": "string",
                "description": "artifact://... ref from Agent context, evidence index, or target dossier."
            },
            "limit": {"type": "integer", "minimum": 1, "maximum": 1000}
        },
    }))
    .expect("static agent ref schema is an object")
}

fn probe_result_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["probe_plan_id", "probe_id", "missing_fact"],
        "properties": {
            "probe_plan_id": {"type": "string"},
            "probe_id": {"type": "string"},
            "missing_fact": {"type": "string"},
            "hypothesis_ids": {
                "type": "array",
                "items": {"type": "string"}
            }
        },
    }))
    .expect("static probe result schema is an object")
}

fn compare_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["before_run_id", "after_run_id"],
        "properties": {
            "before_run_id": {"type": "string"},
            "after_run_id": {"type": "string"}
        },
    }))
    .expect("static compare schema is an object")
}

fn service_investigation_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["service_name"],
        "properties": {
            "service_name": {
                "type": "string",
                "description": "Linux service/unit name without path separators."
            },
            "max_journal_lines": {
                "type": "integer",
                "minimum": 1,
                "maximum": 200
            }
        },
    }))
    .expect("static service investigation schema is an object")
}

fn discover_targets_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["network_cidr"],
        "properties": {
            "network_cidr": {
                "type": "string",
                "description": "IPv4 /24-or-narrower CIDR to filter neighbor-table candidates."
            }
        },
    }))
    .expect("static discover targets schema is an object")
}

fn fleet_snapshot_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["inventory_path", "fleet_run_id"],
        "properties": {
            "inventory_path": {"type": "string"},
            "fleet_run_id": {"type": "string"}
        },
    }))
    .expect("static fleet snapshot schema is an object")
}

fn fleet_preflight_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["inventory_path"],
        "properties": {
            "inventory_path": {"type": "string"}
        },
    }))
    .expect("static fleet preflight schema is an object")
}

fn fleet_capture_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["inventory_path", "fleet_run_id", "duration_ms"],
        "properties": {
            "inventory_path": {"type": "string"},
            "fleet_run_id": {"type": "string"},
            "duration_ms": {"type": "integer", "minimum": 1},
            "interval_ms": {"type": "integer", "minimum": 1}
        },
    }))
    .expect("static fleet capture schema is an object")
}

fn fleet_service_investigation_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["inventory_path", "fleet_run_id", "service_name"],
        "properties": {
            "inventory_path": {"type": "string"},
            "fleet_run_id": {"type": "string"},
            "service_name": {"type": "string"},
            "max_journal_lines": {"type": "integer", "minimum": 1, "maximum": 200}
        },
    }))
    .expect("static fleet service investigation schema is an object")
}

fn fleet_run_schema() -> JsonObject {
    serde_json::from_value(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["fleet_run_id"],
        "properties": {
            "fleet_run_id": {"type": "string"}
        },
    }))
    .expect("static fleet run schema is an object")
}
