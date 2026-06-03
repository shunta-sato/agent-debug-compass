use std::time::Duration;

use adc_core::{
    build_fleet_agent_context, build_run_agent_context, build_session_evidence_index,
    capture_fleet, capture_for, latest_run_id, render_agent_context_journald_jsonl,
    render_agent_context_markdown, render_agent_context_openmetrics,
    render_agent_context_otlp_json, render_agent_context_perfetto_json, resolve_agent_ref,
    validate_cause_neutral, AgentContextRequest, CaptureOptions, FleetAgentContextRequest,
    FleetCaptureOptions,
};

#[test]
fn run_agent_context_packages_existing_capture_for_agent_first_reading() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-AGENT-CONTEXT";

    capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
            collectors: vec![
                "cpu".to_string(),
                "memory".to_string(),
                "network".to_string(),
            ],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");

    let context = build_run_agent_context(
        temp.path(),
        AgentContextRequest {
            run_id: run_id.to_string(),
            service_name: None,
            max_markdown_bytes: 40 * 1024,
        },
    )
    .expect("agent context");

    assert_eq!(context.schema_version, "obs.agent_context.v1");
    assert_eq!(context.context_id, "ctx-R-AGENT-CONTEXT");
    assert_eq!(context.run_id.as_deref(), Some(run_id));
    assert_eq!(context.target_id.as_deref(), Some("local"));
    assert_eq!(context.primary_window.window_id, "W001");
    assert!(context
        .derived_facts
        .iter()
        .any(|fact| fact.kind == "cpu_busy_percent" && fact.statement.contains("CPU busy")));
    assert!(context
        .derived_facts
        .iter()
        .any(|fact| fact.kind == "memory_available"));
    assert!(context
        .derived_facts
        .iter()
        .any(|fact| fact.kind == "network_bytes"));
    assert!(context
        .recommended_refs
        .iter()
        .any(|reference| reference.raw_ref == "artifact://raw/cpu.jsonl"));
    assert!(context.overhead.as_ref().expect("overhead").artifact_bytes > 0);

    validate_cause_neutral(&context.evidence_index).expect("context embeds neutral evidence");
    let markdown = render_agent_context_markdown(&context).expect("markdown");
    assert!(markdown.contains("# Agent Context"));
    assert!(markdown.contains("## Data Quality"));
    assert!(markdown.len() <= 40 * 1024);
    assert!(!markdown.to_ascii_lowercase().contains("root cause"));
}

#[test]
fn run_agent_context_summarizes_full_raw_series_not_timeline_prefix() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-LONG-RUN-SUMMARY";

    capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
            collectors: vec![
                "cpu".to_string(),
                "memory".to_string(),
                "network".to_string(),
            ],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");

    let raw_dir = temp.path().join("runs").join(run_id).join("raw");
    let mut cpu = String::new();
    let mut memory = String::new();
    let mut network = String::new();
    for index in 0..120_u64 {
        let time_mono_ns = 1_000 + index * 1_000;
        cpu.push_str(&format!(
            r#"{{"sample_index":{index},"time_mono_ns":{time_mono_ns},"sample":{{"cpu_count":4,"total_jiffies":{},"idle_jiffies":{}}}}}"#,
            10_000 + index * 10,
            5_000 + index * 8
        ));
        cpu.push('\n');
        memory.push_str(&format!(
            r#"{{"sample_index":{index},"time_mono_ns":{time_mono_ns},"sample":{{"mem_total_kb":8192,"mem_available_kb":{},"mem_free_kb":{}}}}}"#,
            4_000_i64 - index as i64,
            2_000_i64 - index as i64
        ));
        memory.push('\n');
        network.push_str(&format!(
            r#"{{"sample_index":{index},"time_mono_ns":{time_mono_ns},"sample":{{"interfaces":[{{"interface":"eth0","rx_bytes":{},"tx_bytes":{},"rx_packets":{},"tx_packets":{},"rx_errors":0,"tx_errors":0,"rx_drops":0,"tx_drops":0}}]}}}}"#,
            1_000 + index * 100,
            2_000 + index * 200,
            10 + index,
            20 + index
        ));
        network.push('\n');
    }
    std::fs::write(raw_dir.join("cpu.jsonl"), cpu).expect("cpu raw");
    std::fs::write(raw_dir.join("memory.jsonl"), memory).expect("memory raw");
    std::fs::write(raw_dir.join("network.jsonl"), network).expect("network raw");

    let context = build_run_agent_context(
        temp.path(),
        AgentContextRequest {
            run_id: run_id.to_string(),
            service_name: None,
            max_markdown_bytes: 40 * 1024,
        },
    )
    .expect("context");

    let cpu_fact = context
        .derived_facts
        .iter()
        .find(|fact| fact.kind == "cpu_busy_percent")
        .expect("cpu fact");
    assert_eq!(cpu_fact.attributes["sample_count"], 120);
    assert_eq!(cpu_fact.attributes["coverage_start_mono_ns"], 1_000);
    assert_eq!(cpu_fact.attributes["coverage_end_mono_ns"], 120_000);
    assert_eq!(cpu_fact.attributes["coverage_mode"], "raw_series_full");
    assert_eq!(cpu_fact.attributes["busy_percent_avg"], 20.0);

    let memory_fact = context
        .derived_facts
        .iter()
        .find(|fact| fact.kind == "memory_available")
        .expect("memory fact");
    assert_eq!(memory_fact.attributes["sample_count"], 120);
    assert_eq!(memory_fact.attributes["mem_available_kb_delta"], -119);

    let network_fact = context
        .derived_facts
        .iter()
        .find(|fact| fact.kind == "network_bytes")
        .expect("network fact");
    assert_eq!(network_fact.attributes["sample_count"], 120);
    assert_eq!(network_fact.attributes["rx_bytes_delta"], 11_900);
    assert_eq!(network_fact.attributes["tx_bytes_delta"], 23_800);
}

#[test]
fn run_agent_context_includes_target_dossier_for_first_agent_read() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-TARGET-DOSSIER";

    capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
            collectors: vec!["cpu".to_string()],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");
    let raw_dir = temp.path().join("runs").join(run_id).join("raw");
    std::fs::write(raw_dir.join("config_redacted.txt"), "token=<redacted>\n").expect("config");
    std::fs::write(
        raw_dir.join("kernel_probe_snapshot.json"),
        r#"{
          "ftrace_available": true,
          "perf_available": true,
          "kprobe_available": true,
          "ko_loaded": false,
          "root_required": false
        }"#,
    )
    .expect("kernel probe snapshot");

    let context = build_run_agent_context(
        temp.path(),
        AgentContextRequest {
            run_id: run_id.to_string(),
            service_name: None,
            max_markdown_bytes: 40 * 1024,
        },
    )
    .expect("context");

    assert_eq!(context.target_dossier.target_id, "local");
    assert_eq!(
        context.target_dossier.profile_id.as_deref(),
        Some("capture_test")
    );
    assert_eq!(context.target_dossier.run_id.as_deref(), Some(run_id));
    assert_eq!(context.target_dossier.primary_window_id, "W001");
    assert!(context.target_dossier.raw_artifacts_are_ref_only);
    assert!(context
        .target_dossier
        .artifact_refs
        .contains_key("evidence_index"));
    assert!(context
        .target_dossier
        .artifact_refs
        .contains_key("manifest"));
    assert!(context
        .target_dossier
        .redacted_artifacts
        .contains(&"config".to_string()));
    assert_eq!(
        context.target_dossier.capability_summary["ftrace_available"],
        true
    );
    assert!(!context.target_dossier.root_required);

    let markdown = render_agent_context_markdown(&context).expect("markdown");
    assert!(markdown.contains("## Target Dossier"));
    assert!(markdown.contains("- target_id: `local`"));
    assert!(markdown.contains("- raw_artifacts_are_ref_only=true"));
}

#[test]
fn session_evidence_index_summarizes_runs_and_identifies_latest_run() {
    let temp = tempfile::tempdir().expect("tempdir");
    for run_id in ["R-SESSION-A", "R-SESSION-B"] {
        capture_for(
            temp.path(),
            CaptureOptions {
                run_id: run_id.to_string(),
                profile_id: "capture_test".to_string(),
                duration: Duration::from_millis(120),
                interval: Duration::from_millis(40),
                collectors: vec!["cpu".to_string(), "memory".to_string()],
                max_artifact_bytes: 512 * 1024 * 1024,
            },
        )
        .expect("capture");
    }

    let index = build_session_evidence_index(temp.path()).expect("session index");
    assert_eq!(index.schema_version, "obs.session_evidence.v1");
    assert_eq!(index.run_count, 2);
    assert_eq!(index.latest_run_id.as_deref(), Some("R-SESSION-B"));
    assert!(index.total_artifact_bytes > 0);
    assert!(index
        .runs
        .iter()
        .any(|run| run.run_id == "R-SESSION-A" && run.event_count > 0));
    assert_eq!(
        latest_run_id(temp.path()).expect("latest").as_deref(),
        Some("R-SESSION-B")
    );
}

#[test]
fn fleet_agent_context_preserves_partial_success_and_remediation_hints() {
    let temp = tempfile::tempdir().expect("tempdir");
    let inventory_path = temp.path().join("targets.yaml");
    std::fs::write(
        &inventory_path,
        r#"
targets:
  - id: local-a
    transport: local
  - id: serial-b
    transport: serial
"#,
    )
    .expect("inventory");

    capture_fleet(
        temp.path(),
        &inventory_path,
        FleetCaptureOptions {
            fleet_run_id: "F-AGENT-CONTEXT".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
        },
    )
    .expect("fleet capture");

    let context = build_fleet_agent_context(
        temp.path(),
        FleetAgentContextRequest {
            fleet_run_id: "F-AGENT-CONTEXT".to_string(),
            max_markdown_bytes: 40 * 1024,
        },
    )
    .expect("fleet context");

    assert_eq!(context.schema_version, "obs.agent_context.fleet.v1");
    assert_eq!(context.fleet_run_id, "F-AGENT-CONTEXT");
    assert_eq!(context.captured_count, 1);
    assert_eq!(context.failed_count, 1);
    assert!(context
        .target_matrix
        .iter()
        .any(|target| target.target_id == "local-a" && target.status == "captured"));
    let local_summary = context
        .target_summaries
        .iter()
        .find(|target| target.target_id == "local-a")
        .expect("local target summary");
    assert_eq!(local_summary.status, "captured");
    let dossier = local_summary
        .target_dossier
        .as_ref()
        .expect("captured target dossier");
    assert_eq!(dossier.target_id, "local-a");
    assert_eq!(dossier.fleet_run_id.as_deref(), Some("F-AGENT-CONTEXT"));
    assert!(dossier.raw_artifacts_are_ref_only);
    assert!(dossier.artifact_refs.contains_key("evidence_index"));
    assert!(dossier.capability_summary.is_empty() || dossier.capability_summary.len() <= 16);
    assert!(
        !local_summary.top_leads.is_empty(),
        "captured target should carry salience-ranked first-read refs"
    );
    assert!(local_summary.event_count.expect("event count") > 0);
    assert!(local_summary
        .sources
        .iter()
        .any(|source| source.source == "cpu" && source.event_count > 0));
    assert_eq!(context.cross_target_summary.captured_count, 1);
    assert_eq!(context.cross_target_summary.failed_count, 1);
    assert!(context.cross_target_summary.total_event_count > 0);
    assert!(context
        .cross_target_summary
        .source_totals
        .iter()
        .any(|source| source.source == "cpu" && source.event_count > 0));
    assert!(context
        .remediation_hints
        .iter()
        .any(|hint| hint.contains("unsupported transport")));
    assert!(context.failure_groups.iter().any(|group| {
        group.failure_class == "unsupported_transport"
            && group.targets == vec!["serial-b".to_string()]
            && group.next_action.contains("transport")
    }));
    assert!(context
        .recommended_refs
        .iter()
        .any(|reference| reference.raw_ref.contains("local-a")));
}

#[test]
fn run_agent_context_includes_logs_domain_events_config_and_service_state_when_present() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-FUNCTIONAL-EVIDENCE";
    capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
            collectors: vec!["cpu".to_string()],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");
    let raw_dir = temp.path().join("runs").join(run_id).join("raw");
    std::fs::write(
        raw_dir.join("app.log"),
        "info booted\nwarn queue high\nerror timeout request_id=abc\n",
    )
    .expect("app log");
    std::fs::write(
        raw_dir.join("domain_events.jsonl"),
        r#"{"event_type":"sensor_frame_gap","frame_id":"42","gap_ms":120}"#,
    )
    .expect("domain events");
    std::fs::write(
        raw_dir.join("config_redacted.txt"),
        "retry_backoff_ms=0\npassword=<redacted>\n",
    )
    .expect("config");
    std::fs::write(
        raw_dir.join("service_state.json"),
        r#"{"service":"sensor-gateway","active_state":"active","sub_state":"running"}"#,
    )
    .expect("service state");

    let context = build_run_agent_context(
        temp.path(),
        AgentContextRequest {
            run_id: run_id.to_string(),
            service_name: None,
            max_markdown_bytes: 40 * 1024,
        },
    )
    .expect("context");

    assert!(context
        .derived_facts
        .iter()
        .any(|fact| fact.source == "log" && fact.kind == "log_error_slice"));
    assert!(context
        .derived_facts
        .iter()
        .any(|fact| fact.source == "domain_event" && fact.kind == "domain_event_count"));
    assert!(context
        .derived_facts
        .iter()
        .any(|fact| fact.source == "config" && fact.kind == "config_snapshot"));
    assert!(context
        .derived_facts
        .iter()
        .any(|fact| fact.source == "service_state" && fact.kind == "service_state"));
    assert!(context.raw_refs.contains_key("app_log"));
    assert!(context.raw_refs.contains_key("domain_events"));
    assert!(context.raw_refs.contains_key("config"));
    assert!(context.raw_refs.contains_key("service_state"));
    let markdown = render_agent_context_markdown(&context).expect("markdown");
    assert!(markdown.contains("log_error_slice"));
    assert!(!markdown.contains("password=secret"));
}

#[test]
fn run_agent_context_recommends_refs_by_investigation_salience() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-SALIENCE";
    capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
            collectors: vec![
                "cpu".to_string(),
                "memory".to_string(),
                "network".to_string(),
            ],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");
    let raw_dir = temp.path().join("runs").join(run_id).join("raw");
    std::fs::write(
        raw_dir.join("app.log"),
        "info booted\nerror timeout request_id=abc\nwarn retry storm\n",
    )
    .expect("app log");
    std::fs::write(
        raw_dir.join("domain_events.jsonl"),
        r#"{"event_type":"sensor_frame_gap","frame_id":"42","gap_ms":120}"#,
    )
    .expect("domain events");
    std::fs::write(
        raw_dir.join("journald.jsonl"),
        "{\"MESSAGE\":\"timeout\",\"PRIORITY\":\"3\"}\n{\"MESSAGE\":\"ok\",\"PRIORITY\":\"6\"}\n",
    )
    .expect("journald");

    let context = build_run_agent_context(
        temp.path(),
        AgentContextRequest {
            run_id: run_id.to_string(),
            service_name: None,
            max_markdown_bytes: 40 * 1024,
        },
    )
    .expect("context");

    let labels = context
        .recommended_refs
        .iter()
        .map(|reference| reference.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(labels, vec!["log", "journald", "domain_event"]);
    assert!(context.recommended_refs.iter().all(|reference| {
        !matches!(reference.label.as_str(), "cpu" | "memory" | "network")
            && reference.reason.contains("salience")
    }));
}

#[test]
fn agent_ref_resolver_handles_raw_window_manifest_and_rejects_invalid_refs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-REF-RESOLVE";
    capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
            collectors: vec!["cpu".to_string()],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");

    let raw =
        resolve_agent_ref(temp.path(), run_id, "artifact://raw/cpu.jsonl", 1).expect("raw ref");
    assert_eq!(raw.ref_kind, "raw");
    assert_eq!(raw.returned_lines, 1);
    assert!(raw.truncated);
    assert!(raw.text.contains("total_jiffies"));

    let window = resolve_agent_ref(temp.path(), run_id, "artifact://windows/W001.yaml", 20)
        .expect("window ref");
    assert_eq!(window.ref_kind, "window");
    assert!(window.text.contains("window_id: W001"));

    let manifest =
        resolve_agent_ref(temp.path(), run_id, "artifact://manifest.json", 20).expect("manifest");
    assert_eq!(manifest.ref_kind, "manifest");
    assert!(manifest.text.contains("profile_id"));

    let err = resolve_agent_ref(temp.path(), run_id, "artifact://../manifest.json", 20)
        .expect_err("invalid ref rejected");
    assert!(err.to_string().contains("unsupported artifact ref"));
}

#[test]
fn agent_ref_resolver_groups_repeated_permission_denied_lines() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-REF-DATA-QUALITY-GROUPS";
    capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
            collectors: vec!["cpu".to_string()],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");
    let raw_dir = temp.path().join("runs").join(run_id).join("raw");
    std::fs::write(
        raw_dir.join("permission_noise.txt"),
        "fd_thread: pid 1 fd unavailable: Permission denied (os error 13)\n\
         fd_thread: pid 2 fd unavailable: Permission denied (os error 13)\n\
         fd_thread: pid 3 fd unavailable: Permission denied (os error 13)\n",
    )
    .expect("permission noise");

    let resolved = resolve_agent_ref(
        temp.path(),
        run_id,
        "artifact://raw/permission_noise.txt",
        20,
    )
    .expect("resolve noisy ref");

    assert!(resolved
        .data_quality
        .notes
        .iter()
        .any(|note| note.contains("permission_denied repeated 3 line(s)")));
    assert!(resolved
        .data_quality
        .missing
        .iter()
        .any(|missing| missing.contains("permission_denied: fd_thread")));
}

#[test]
fn run_agent_context_reduces_markdown_to_budget_without_losing_core_sections() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-BUDGET-REDUCE";
    capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
            collectors: vec![
                "cpu".to_string(),
                "memory".to_string(),
                "network".to_string(),
            ],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");
    let raw_dir = temp.path().join("runs").join(run_id).join("raw");
    std::fs::write(
        raw_dir.join("app.log"),
        "info booted\nerror timeout request_id=abc\nwarn retry storm\n",
    )
    .expect("app log");
    std::fs::write(
        raw_dir.join("domain_events.jsonl"),
        r#"{"event_type":"sensor_frame_gap","frame_id":"42","gap_ms":120}"#,
    )
    .expect("domain events");
    std::fs::write(
        raw_dir.join("journald.jsonl"),
        "{\"MESSAGE\":\"timeout\",\"PRIORITY\":\"3\"}\n",
    )
    .expect("journald");
    std::fs::write(
        raw_dir.join("perfetto_trace.json"),
        r#"{"traceEvents":[{"name":"frame_gap","ph":"i"},{"name":"request","ph":"X"}]}"#,
    )
    .expect("perfetto");
    std::fs::write(raw_dir.join("config_redacted.txt"), "token=<redacted>\n").expect("config");

    let context = build_run_agent_context(
        temp.path(),
        AgentContextRequest {
            run_id: run_id.to_string(),
            service_name: None,
            max_markdown_bytes: 1_600,
        },
    )
    .expect("context");
    let markdown = render_agent_context_markdown(&context).expect("markdown");

    assert!(
        markdown.len() <= 1_600,
        "markdown was {} bytes",
        markdown.len()
    );
    assert!(context.context_budget.truncated);
    assert!(context.data_quality.truncated);
    assert!(context
        .data_quality
        .notes
        .iter()
        .any(|note| note.contains("reduced")));
    assert!(markdown.contains("## Target Dossier"));
    assert!(markdown.contains("## Recommended Refs"));
    assert!(markdown.contains("## Data Quality"));
}

#[test]
fn run_agent_context_includes_advanced_optional_probe_snapshots_when_present() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-ADVANCED-PROBES";
    capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
            collectors: vec!["cpu".to_string()],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");
    let raw_dir = temp.path().join("runs").join(run_id).join("raw");
    std::fs::write(
        raw_dir.join("fd_thread_snapshot.json"),
        r#"{
          "process_count": 3,
          "accessible_process_count": 2,
          "inaccessible_process_count": 1,
          "total_fd_count": 12,
          "total_thread_count": 7,
          "root_required": false
        }"#,
    )
    .expect("fd/thread snapshot");
    std::fs::write(
        raw_dir.join("kernel_probe_snapshot.json"),
        r#"{
          "ftrace_available": true,
          "perf_available": false,
          "kprobe_available": true,
          "ko_loaded": false,
          "ko_source_present": true,
          "root_required": false,
          "data_quality": {
            "dropped": false,
            "drop_count": 0,
            "throttled": false,
            "truncated": false,
            "clock_confidence": "medium",
            "missing": ["perf: perf_event_paranoid blocks unprivileged counter use"],
            "notes": ["ko source present but module is not loaded"]
          }
        }"#,
    )
    .expect("kernel probe snapshot");

    let context = build_run_agent_context(
        temp.path(),
        AgentContextRequest {
            run_id: run_id.to_string(),
            service_name: None,
            max_markdown_bytes: 40 * 1024,
        },
    )
    .expect("context");

    let fd_fact = context
        .derived_facts
        .iter()
        .find(|fact| fact.kind == "fd_thread_snapshot")
        .expect("fd/thread fact");
    assert_eq!(fd_fact.source, "fd_thread");
    assert_eq!(fd_fact.attributes["total_fd_count"], 12);
    assert_eq!(fd_fact.attributes["total_thread_count"], 7);
    assert!(context.raw_refs.contains_key("fd_thread_snapshot"));

    let kernel_fact = context
        .derived_facts
        .iter()
        .find(|fact| fact.kind == "kernel_optional_probe_snapshot")
        .expect("kernel probe fact");
    assert_eq!(kernel_fact.source, "kernel_probe");
    assert_eq!(kernel_fact.attributes["ftrace_available"], true);
    assert_eq!(kernel_fact.attributes["perf_available"], false);
    assert!(kernel_fact
        .data_quality
        .missing
        .iter()
        .any(|missing| missing.contains("perf_event_paranoid")));
    assert!(context.raw_refs.contains_key("kernel_probe_snapshot"));
}

#[test]
fn run_agent_context_includes_deep_interop_import_facts_when_present() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-INTEROP-IMPORTS";
    capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
            collectors: vec!["cpu".to_string()],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");
    let raw_dir = temp.path().join("runs").join(run_id).join("raw");
    std::fs::write(
        raw_dir.join("otlp_metrics.json"),
        r#"{"resourceMetrics":[{"scopeMetrics":[{"metrics":[{"name":"queue.depth"},{"name":"request.errors"}]}]}]}"#,
    )
    .expect("otlp");
    std::fs::write(
        raw_dir.join("journald.jsonl"),
        "{\"MESSAGE\":\"timeout\",\"PRIORITY\":\"4\"}\n{\"MESSAGE\":\"ok\",\"PRIORITY\":\"6\"}\n",
    )
    .expect("journald");
    std::fs::write(
        raw_dir.join("perfetto_trace.json"),
        r#"{"traceEvents":[{"name":"frame_gap","ph":"i"},{"name":"request","ph":"X"}]}"#,
    )
    .expect("perfetto");

    let context = build_run_agent_context(
        temp.path(),
        AgentContextRequest {
            run_id: run_id.to_string(),
            service_name: None,
            max_markdown_bytes: 40 * 1024,
        },
    )
    .expect("context");

    assert!(context
        .derived_facts
        .iter()
        .any(|fact| fact.kind == "otlp_metric_count" && fact.attributes["metric_count"] == 2));
    assert!(context
        .derived_facts
        .iter()
        .any(|fact| fact.kind == "journald_entry_count"
            && fact.attributes["entry_count"] == 2
            && fact.attributes["warning_or_error_count"] == 1));
    assert!(context.derived_facts.iter().any(
        |fact| fact.kind == "perfetto_event_count" && fact.attributes["trace_event_count"] == 2
    ));
    assert!(context.raw_refs.contains_key("otlp_metrics"));
    assert!(context.raw_refs.contains_key("journald"));
    assert!(context.raw_refs.contains_key("perfetto_trace"));
}

#[test]
fn agent_context_exports_openmetrics_summary_without_raw_dump() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-OPENMETRICS";
    capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
            collectors: vec!["cpu".to_string(), "memory".to_string()],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");
    let context = build_run_agent_context(
        temp.path(),
        AgentContextRequest {
            run_id: run_id.to_string(),
            service_name: None,
            max_markdown_bytes: 40 * 1024,
        },
    )
    .expect("context");

    let metrics = render_agent_context_openmetrics(&context).expect("metrics");
    assert!(metrics.contains("adc_agent_context_info"));
    assert!(metrics.contains("adc_agent_context_derived_facts_total"));
    assert!(metrics.contains("adc_agent_context_artifact_bytes"));
    assert!(metrics.contains("run_id=\"R-OPENMETRICS\""));
    assert!(!metrics.contains("idle_jiffies"));
    assert!(!metrics.to_ascii_lowercase().contains("root cause"));
}

#[test]
fn agent_context_exports_otlp_journald_and_perfetto_summaries_without_raw_dump() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-DEEP-INTEROP";
    capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
            collectors: vec!["cpu".to_string(), "memory".to_string()],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");
    let context = build_run_agent_context(
        temp.path(),
        AgentContextRequest {
            run_id: run_id.to_string(),
            service_name: None,
            max_markdown_bytes: 40 * 1024,
        },
    )
    .expect("context");

    let otlp = render_agent_context_otlp_json(&context).expect("otlp");
    let otlp_json: serde_json::Value = serde_json::from_str(&otlp).expect("otlp json");
    assert_eq!(
        otlp_json["resourceMetrics"][0]["resource"]["attributes"][0]["key"],
        "service.name"
    );
    assert!(otlp.contains("obs.agent_context.derived_facts"));
    assert!(!otlp.contains("idle_jiffies"));

    let journald = render_agent_context_journald_jsonl(&context).expect("journald");
    let entries = journald
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("journald line"))
        .collect::<Vec<_>>();
    assert!(entries.iter().any(|entry| entry["MESSAGE"]
        .as_str()
        .expect("message")
        .contains("Agent context ready")));
    assert!(entries
        .iter()
        .all(|entry| entry["ADC_RUN_ID"] == "R-DEEP-INTEROP"));

    let perfetto = render_agent_context_perfetto_json(&context).expect("perfetto");
    let perfetto_json: serde_json::Value = serde_json::from_str(&perfetto).expect("perfetto json");
    assert!(perfetto_json["traceEvents"]
        .as_array()
        .expect("trace events")
        .iter()
        .any(|event| event["name"] == "obs.agent_context"));
    assert!(!perfetto.contains("idle_jiffies"));
}
