use std::{os::unix::fs::PermissionsExt, process::Command};

#[test]
fn observe_writes_agent_context_and_agent_context_command_reads_it() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-CLI-AGENT-CONTEXT";

    let observe = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "observe",
            "--run-id",
            run_id,
            "--duration-ms",
            "120",
            "--interval-ms",
            "40",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("observe");
    assert!(
        observe.status.success(),
        "observe failed: {}",
        String::from_utf8_lossy(&observe.stderr)
    );
    let observe_json: serde_json::Value =
        serde_json::from_slice(&observe.stdout).expect("observe json");
    assert_eq!(observe_json["run_id"], run_id);
    assert!(observe_json["agent_context"]
        .as_str()
        .expect("context path")
        .contains("agent_context.md"));
    assert!(temp
        .path()
        .join("runs")
        .join(run_id)
        .join("agent_context.md")
        .is_file());
    assert!(temp
        .path()
        .join("runs")
        .join(run_id)
        .join("agent_context.json")
        .is_file());
    assert!(temp
        .path()
        .join("runs")
        .join(run_id)
        .join("raw/fd_thread_snapshot.json")
        .is_file());
    assert!(temp
        .path()
        .join("runs")
        .join(run_id)
        .join("raw/kernel_probe_snapshot.json")
        .is_file());

    let context = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["agent-context", "--run-id", run_id, "--format", "json"])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("agent-context");
    assert!(
        context.status.success(),
        "agent-context failed: {}",
        String::from_utf8_lossy(&context.stderr)
    );
    let context_json: serde_json::Value =
        serde_json::from_slice(&context.stdout).expect("context json");
    assert_eq!(context_json["schema_version"], "obs.agent_context.v1");
    assert_eq!(context_json["run_id"], run_id);
    assert!(context_json["derived_facts"]
        .as_array()
        .expect("facts")
        .iter()
        .any(|fact| fact["kind"] == "cpu_busy_percent"));
    assert!(context_json["derived_facts"]
        .as_array()
        .expect("facts")
        .iter()
        .any(|fact| fact["kind"] == "process_snapshot"));
    assert!(context_json["derived_facts"]
        .as_array()
        .expect("facts")
        .iter()
        .any(|fact| fact["kind"] == "io_snapshot"));
    assert!(context_json["derived_facts"]
        .as_array()
        .expect("facts")
        .iter()
        .any(|fact| fact["kind"] == "thermal_snapshot"));
    assert!(context_json["derived_facts"]
        .as_array()
        .expect("facts")
        .iter()
        .any(|fact| fact["kind"] == "fd_thread_snapshot"));
    assert!(context_json["derived_facts"]
        .as_array()
        .expect("facts")
        .iter()
        .any(|fact| fact["kind"] == "kernel_optional_probe_snapshot"));
    assert!(context_json["recommended_refs"]
        .as_array()
        .expect("refs")
        .iter()
        .any(|reference| reference["raw_ref"] == "artifact://raw/cpu.jsonl"));

    let markdown = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["agent-context", "--run-id", run_id])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("agent-context markdown");
    assert!(
        markdown.status.success(),
        "agent-context markdown failed: {}",
        String::from_utf8_lossy(&markdown.stderr)
    );
    let markdown_stdout = String::from_utf8(markdown.stdout).expect("markdown utf8");
    assert!(markdown_stdout.contains("# Agent Context"));
    assert!(markdown_stdout.contains("## Investigation Route"));
    assert!(markdown_stdout.contains("## Recommended Refs"));
    assert!(!markdown_stdout.to_ascii_lowercase().contains("root cause"));

    let metrics = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "agent-context",
            "--run-id",
            run_id,
            "--format",
            "openmetrics",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("agent-context openmetrics");
    assert!(
        metrics.status.success(),
        "agent-context openmetrics failed: {}",
        String::from_utf8_lossy(&metrics.stderr)
    );
    let metrics_stdout = String::from_utf8(metrics.stdout).expect("metrics utf8");
    assert!(metrics_stdout.contains("adc_agent_context_info"));
    assert!(!metrics_stdout.contains("idle_jiffies"));

    let otlp = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["agent-context", "--run-id", run_id, "--format", "otlp-json"])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("agent-context otlp");
    assert!(
        otlp.status.success(),
        "agent-context otlp failed: {}",
        String::from_utf8_lossy(&otlp.stderr)
    );
    let otlp_json: serde_json::Value = serde_json::from_slice(&otlp.stdout).expect("otlp json");
    assert!(otlp_json["resourceMetrics"].is_array());

    let journald = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "agent-context",
            "--run-id",
            run_id,
            "--format",
            "journald-jsonl",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("agent-context journald");
    assert!(
        journald.status.success(),
        "agent-context journald failed: {}",
        String::from_utf8_lossy(&journald.stderr)
    );
    let journald_stdout = String::from_utf8(journald.stdout).expect("journald utf8");
    assert!(journald_stdout.contains("\"ADC_RUN_ID\":\"R-CLI-AGENT-CONTEXT\""));
    assert!(!journald_stdout.contains("idle_jiffies"));

    let perfetto = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "agent-context",
            "--run-id",
            run_id,
            "--format",
            "perfetto-json",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("agent-context perfetto");
    assert!(
        perfetto.status.success(),
        "agent-context perfetto failed: {}",
        String::from_utf8_lossy(&perfetto.stderr)
    );
    let perfetto_json: serde_json::Value =
        serde_json::from_slice(&perfetto.stdout).expect("perfetto json");
    assert!(perfetto_json["traceEvents"].is_array());
}

#[test]
fn investigate_bug_creates_symptom_context_without_preexisting_run_id() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app_log = temp.path().join("app.log");
    std::fs::write(
        &app_log,
        "info start\nwarning queue depth high\nerror timeout request_id=abc\n",
    )
    .expect("log");

    let output = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "investigate",
            "bug",
            "--symptom",
            "latency timeout",
            "--service-name",
            "ssh",
            "--duration-ms",
            "120",
            "--interval-ms",
            "40",
            "--log-file",
            app_log.to_str().expect("log path"),
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("investigate bug");
    assert!(
        output.status.success(),
        "investigate bug failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let context: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("symptom context json");
    assert_eq!(context["schema_version"], "obs.symptom_context.v1");
    assert_eq!(context["symptom"]["normalized"], "latency_timeout");
    assert_eq!(context["scope"], "run");
    assert!(context["run_id"]
        .as_str()
        .expect("auto run id")
        .starts_with("R-SYMPTOM-"));
    assert!(context["compiled_route"]["selected_packs"]
        .as_array()
        .expect("selected packs")
        .iter()
        .any(|pack| pack["domain"] == "latency_timeouts"));
    assert_eq!(
        context["hypothesis_set"]["schema_version"],
        "obs.hypothesis_set.v1"
    );
    assert!(context["hypothesis_set"]["hypotheses"]
        .as_array()
        .expect("hypotheses")
        .iter()
        .all(|hypothesis| hypothesis["claim_boundary"] == "hypothesis_only"));
    assert_eq!(context["probe_plan"]["schema_version"], "obs.probe_plan.v1");
    assert!(context["probe_plan"]["candidate_probes"]
        .as_array()
        .expect("candidate probes")
        .iter()
        .all(|probe| probe["cause_neutral"] == true));
    assert_eq!(context["safety_policy"]["default_decision"], "deny");
    assert!(context["facts"]
        .as_array()
        .expect("facts")
        .iter()
        .any(|fact| fact["fact_id"] == "signal.signal_line_count"));
    assert!(context["facts"]
        .as_array()
        .expect("facts")
        .iter()
        .any(|fact| fact["fact_id"] == "resource.cpu_busy_percent"));
    assert!(!context["missing_fact_ids"]
        .as_array()
        .expect("missing facts")
        .iter()
        .any(|fact_id| fact_id == "resource.cpu_busy_percent"));
    assert!(!context["next_safe_probes"]
        .as_array()
        .expect("safe probes")
        .is_empty());
    let rendered = String::from_utf8(output.stdout).expect("utf8");
    assert!(!rendered.to_ascii_lowercase().contains("root cause"));
    assert!(!rendered.contains("secret"));
}

#[test]
fn investigate_probe_result_records_missing_capability_without_running_probe() {
    let output = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "investigate",
            "probe-result",
            "--probe-plan-id",
            "PP001",
            "--probe-id",
            "probe.scheduler_snapshot",
            "--missing-fact",
            "process.runqueue_latency",
            "--hypothesis-id",
            "H001",
        ])
        .output()
        .expect("probe result");
    assert!(
        output.status.success(),
        "probe result failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("probe result json");
    assert_eq!(result["schema_version"], "obs.probe_result.v1");
    assert_eq!(result["status"], "failed_missing_capability");
    assert_eq!(result["hypothesis_updates"][0]["update"], "needs_evidence");
    assert!(result["data_quality"]["missing"]
        .as_array()
        .expect("missing")
        .iter()
        .any(|value| value
            .as_str()
            .is_some_and(|text| text.contains("process.runqueue_latency"))));
}

#[test]
fn observe_can_attach_logs_domain_events_redacted_config_and_service_state() {
    let temp = tempfile::tempdir().expect("tempdir");
    let app_log = temp.path().join("app.log");
    let domain_events = temp.path().join("domain_events.jsonl");
    let config = temp.path().join("config.env");
    let otlp = temp.path().join("otlp_metrics.json");
    let journald = temp.path().join("journald.jsonl");
    let perfetto = temp.path().join("perfetto_trace.json");
    std::fs::write(&app_log, "info start\nerror timeout request_id=abc\n").expect("log");
    std::fs::write(
        &domain_events,
        r#"{"event_type":"queue_backlog","queue_depth":99}"#,
    )
    .expect("domain events");
    std::fs::write(&config, "retry_backoff_ms=0\ntoken=secret-value\n").expect("config");
    std::fs::write(
        &otlp,
        r#"{"resourceMetrics":[{"scopeMetrics":[{"metrics":[{"name":"queue.depth"}]}]}]}"#,
    )
    .expect("otlp");
    std::fs::write(&journald, "{\"MESSAGE\":\"timeout\",\"PRIORITY\":\"4\"}\n").expect("journald");
    std::fs::write(
        &perfetto,
        r#"{"traceEvents":[{"name":"frame_gap","ph":"i"}]}"#,
    )
    .expect("perfetto");

    let observe = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "observe",
            "--run-id",
            "R-CLI-FUNCTIONAL-EVIDENCE",
            "--duration-ms",
            "120",
            "--interval-ms",
            "40",
            "--log-file",
            app_log.to_str().expect("log path"),
            "--domain-events-file",
            domain_events.to_str().expect("domain path"),
            "--config-file",
            config.to_str().expect("config path"),
            "--service-name",
            "sensor-gateway",
            "--otlp-file",
            otlp.to_str().expect("otlp path"),
            "--journald-jsonl-file",
            journald.to_str().expect("journald path"),
            "--perfetto-file",
            perfetto.to_str().expect("perfetto path"),
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("observe");
    assert!(
        observe.status.success(),
        "observe failed: {}",
        String::from_utf8_lossy(&observe.stderr)
    );

    let run_dir = temp.path().join("runs/R-CLI-FUNCTIONAL-EVIDENCE");
    let redacted =
        std::fs::read_to_string(run_dir.join("raw/config_redacted.txt")).expect("redacted config");
    assert!(redacted.contains("token=<redacted>"));
    assert!(!redacted.contains("secret-value"));
    assert!(run_dir.join("raw/app.log").is_file());
    assert!(run_dir.join("raw/domain_events.jsonl").is_file());
    assert!(run_dir.join("raw/service_state.json").is_file());
    assert!(run_dir.join("raw/otlp_metrics.json").is_file());
    assert!(run_dir.join("raw/journald.jsonl").is_file());
    assert!(run_dir.join("raw/perfetto_trace.json").is_file());

    let context = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "agent-context",
            "--run-id",
            "R-CLI-FUNCTIONAL-EVIDENCE",
            "--format",
            "json",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("context");
    assert!(
        context.status.success(),
        "context failed: {}",
        String::from_utf8_lossy(&context.stderr)
    );
    let context_json: serde_json::Value =
        serde_json::from_slice(&context.stdout).expect("context json");
    let facts = context_json["derived_facts"].as_array().expect("facts");
    assert!(facts.iter().any(|fact| fact["kind"] == "log_error_slice"));
    assert!(facts
        .iter()
        .any(|fact| fact["kind"] == "domain_event_count"));
    assert!(facts.iter().any(|fact| fact["kind"] == "config_snapshot"));
    assert!(facts.iter().any(|fact| fact["kind"] == "service_state"));
    assert!(facts.iter().any(|fact| fact["kind"] == "otlp_metric_count"));
    assert!(facts
        .iter()
        .any(|fact| fact["kind"] == "journald_entry_count"));
    assert!(facts
        .iter()
        .any(|fact| fact["kind"] == "perfetto_event_count"));
    let playbook = &context_json["playbook"];
    assert_eq!(playbook["schema_version"], "obs.agent_playbook.v1");
    let steps = playbook["steps"].as_array().expect("playbook steps");
    assert!((2..=5).contains(&steps.len()));
    assert_eq!(steps[0]["cause_neutral"], true);
    assert!(steps.iter().any(|step| {
        step["refs"]
            .as_array()
            .expect("refs")
            .iter()
            .any(|reference| reference["raw_ref"] == "artifact://raw/app.log")
    }));
    let serialized_playbook = serde_json::to_string(playbook).expect("playbook json");
    assert!(!serialized_playbook
        .to_ascii_lowercase()
        .contains("root cause"));
    assert!(!serialized_playbook
        .to_ascii_lowercase()
        .contains("likely cause"));
    let route = &context_json["investigation_route"];
    assert_eq!(route["schema_version"], "obs.investigation_route.v1");
    assert_eq!(route["scope"], "run");
    assert_eq!(route["run_id"], "R-CLI-FUNCTIONAL-EVIDENCE");
    assert!(route["route_summary"]
        .as_array()
        .expect("route summary")
        .iter()
        .any(|summary| summary
            .as_str()
            .expect("summary")
            .contains("service/log/domain")));
    let route_steps = route["steps"].as_array().expect("route steps");
    assert!((2..=7).contains(&route_steps.len()));
    assert!(route_steps.iter().all(|step| step["cause_neutral"] == true));
    assert!(route_steps.iter().any(|step| {
        step["refs"]
            .as_array()
            .expect("refs")
            .iter()
            .any(|reference| reference["raw_ref"] == "artifact://raw/service_state.json")
    }));
    assert!(route_steps.iter().any(|step| {
        step["refs"]
            .as_array()
            .expect("refs")
            .iter()
            .any(|reference| reference["raw_ref"] == "artifact://raw/app.log")
    }));
    assert!(route_steps.iter().any(|step| {
        step["refs"]
            .as_array()
            .expect("refs")
            .iter()
            .any(|reference| reference["raw_ref"] == "artifact://raw/cpu.jsonl")
    }));
    assert!(route_steps.iter().all(|step| step["expected_answer"]
        .as_str()
        .is_some_and(|value| !value.is_empty())));
    assert!(route_steps.iter().all(|step| step["branch_conditions"]
        .as_array()
        .is_some_and(|branches| !branches.is_empty())));
    assert!(route["budget"]["raw_ref_count"].as_u64().expect("raw refs") > 0);
    let serialized_route = serde_json::to_string(route).expect("route json");
    assert!(!serialized_route.to_ascii_lowercase().contains("root cause"));
    assert!(!serialized_route
        .to_ascii_lowercase()
        .contains("likely cause"));
    let rendered = String::from_utf8(context.stdout).expect("context utf8");
    assert!(!rendered.contains("secret-value"));
}

#[test]
fn investigate_continue_opens_selected_step_and_persists_session() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fake_bin = temp.path().join("fake-bin");
    std::fs::create_dir_all(&fake_bin).expect("fake bin");
    let systemctl = fake_bin.join("systemctl");
    std::fs::write(
        &systemctl,
        "#!/usr/bin/env bash\ncat <<'OUT'\nId=ssh.service\nLoadState=loaded\nActiveState=active\nSubState=running\nMainPID=4242\nFragmentPath=/usr/lib/systemd/system/ssh.service\nOUT\n",
    )
    .expect("fake systemctl");
    let mut perms = std::fs::metadata(&systemctl)
        .expect("metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&systemctl, perms).expect("chmod");
    let app_log = temp.path().join("app.log");
    let domain_events = temp.path().join("domain_events.jsonl");
    let config = temp.path().join("config.env");
    std::fs::write(&app_log, "info start\nerror timeout request_id=abc\n").expect("log");
    std::fs::write(
        &domain_events,
        r#"{"event_type":"queue_backlog","queue_depth":99}"#,
    )
    .expect("domain events");
    std::fs::write(&config, "retry_backoff_ms=0\ntoken=secret-value\n").expect("config");
    let old_path = std::env::var("PATH").unwrap_or_default();

    let observe = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "observe",
            "--run-id",
            "R-CLI-CONTINUE",
            "--duration-ms",
            "120",
            "--interval-ms",
            "40",
            "--log-file",
            app_log.to_str().expect("log path"),
            "--domain-events-file",
            domain_events.to_str().expect("domain path"),
            "--config-file",
            config.to_str().expect("config path"),
            "--service-name",
            "ssh",
        ])
        .env("ADC_HOME", temp.path())
        .env("PATH", format!("{}:{old_path}", fake_bin.display()))
        .output()
        .expect("observe");
    assert!(
        observe.status.success(),
        "observe failed: {}",
        String::from_utf8_lossy(&observe.stderr)
    );

    let start = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "investigate",
            "start",
            "--run-id",
            "R-CLI-CONTINUE",
            "--service-name",
            "ssh",
            "--journal-lines",
            "2",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("investigation start");
    assert!(
        start.status.success(),
        "investigation start failed: {}",
        String::from_utf8_lossy(&start.stderr)
    );
    let start_json: serde_json::Value = serde_json::from_slice(&start.stdout).expect("start json");
    assert!(start_json["investigation_route"]["steps"]
        .as_array()
        .expect("route steps")
        .iter()
        .flat_map(|step| step["branch_conditions"]
            .as_array()
            .expect("branch conditions"))
        .any(|condition| condition["predicate"].is_object()));

    let continue_output = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "investigate",
            "continue",
            "--run-id",
            "R-CLI-CONTINUE",
            "--step-id",
            "IR001",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("investigation continue");
    assert!(
        continue_output.status.success(),
        "investigation continue failed: {}",
        String::from_utf8_lossy(&continue_output.stderr)
    );
    let continue_json: serde_json::Value =
        serde_json::from_slice(&continue_output.stdout).expect("continue json");
    assert_eq!(
        continue_json["schema_version"],
        "obs.investigation_continue.v1"
    );
    assert_eq!(continue_json["scope"], "run");
    assert_eq!(continue_json["run_id"], "R-CLI-CONTINUE");
    assert_eq!(continue_json["current_step_id"], "IR001");
    assert!(continue_json["opened_refs"]
        .as_array()
        .expect("opened refs")
        .iter()
        .any(|entry| entry["label"] == "service_state"));
    assert!(continue_json["opened_refs"]
        .as_array()
        .expect("opened refs")
        .iter()
        .any(|entry| entry["label"] == "service_state"
            && entry["facts"]
                .as_array()
                .expect("service facts")
                .iter()
                .any(|fact| fact["fact_id"] == "service.availability"
                    && fact["value"] == "available")));
    assert!(continue_json["opened_refs"]
        .as_array()
        .expect("opened refs")
        .iter()
        .any(|entry| entry["label"] == "service.port_summary"
            && entry["facts"]
                .as_array()
                .expect("port facts")
                .iter()
                .any(|fact| fact["fact_id"] == "port.availability")));
    assert!(continue_json["opened_refs"]
        .as_array()
        .expect("opened refs")
        .iter()
        .filter(|entry| entry["label"] == "service.port_summary")
        .all(|entry| entry["facts"]
            .as_array()
            .expect("port facts")
            .iter()
            .all(|fact| fact["fact_id"] != "service.availability")));
    assert!(continue_json["opened_refs"]
        .as_array()
        .expect("opened refs")
        .iter()
        .all(|entry| entry["text"].is_null()));
    assert!(continue_json["new_facts"]
        .as_array()
        .expect("new facts")
        .iter()
        .any(|fact| fact["kind"] == "opened_service_state"));
    let branch_evaluations = continue_json["branch_evaluations"]
        .as_array()
        .expect("branch evaluations");
    assert!(branch_evaluations
        .iter()
        .any(|branch| branch["step_id"] == "IR001"
            && branch["status"] == "matched"
            && branch["next_step_id"] == "IR002"));
    assert!(branch_evaluations
        .iter()
        .any(|branch| branch["step_id"] == "IR001"
            && branch["status"] == "not_matched"
            && branch["next_step_id"] == "IR-DQ"));
    for branch in branch_evaluations {
        if branch["next_step_id"] == "IR-DQ" || branch["next_step_id"] == "IR002" {
            let matched_facts = branch["matched_facts"].as_array().expect("matched facts");
            assert!(matched_facts.iter().all(|fact| !fact
                .as_str()
                .expect("fact")
                .contains("Port summary availability=unavailable")));
            assert!(branch["missing_fact_ids"].is_array());
        }
    }
    assert!(branch_evaluations.iter().all(|branch| {
        matches!(
            branch["status"].as_str().expect("branch status"),
            "matched" | "not_matched" | "unknown"
        )
    }));
    let next_actions = continue_json["next_actions"]
        .as_array()
        .expect("next actions");
    assert!(!next_actions.is_empty());
    assert!(next_actions
        .iter()
        .any(|action| action["next_step_id"] == "IR002"
            && action["refs"]
                .as_array()
                .is_some_and(|refs| !refs.is_empty())));
    assert!(continue_json["opened_refs"]
        .as_array()
        .expect("opened refs")
        .iter()
        .any(|entry| entry["label"] == "service.port_summary"
            && entry["summary"]
                .as_str()
                .expect("summary")
                .starts_with("Port summary")));
    assert!(continue_json["investigation_route"]["steps"]
        .as_array()
        .expect("route steps")
        .iter()
        .all(|step| step["step_id"] != "IR001"));
    assert!(
        continue_json["budget"]["returned_bytes"]
            .as_u64()
            .expect("returned bytes")
            < 12_000
    );
    let rendered = String::from_utf8(continue_output.stdout).expect("continue utf8");
    assert!(!rendered.contains("secret-value"));
    assert!(!rendered.to_ascii_lowercase().contains("root cause"));
    assert!(temp
        .path()
        .join("runs/R-CLI-CONTINUE/investigation_route.json")
        .is_file());
    let session_id = continue_json["session_id"].as_str().expect("session id");
    assert!(temp
        .path()
        .join("runs/R-CLI-CONTINUE/investigation_sessions")
        .join(format!("{session_id}.json"))
        .is_file());
    let session_state_path = temp
        .path()
        .join("runs/R-CLI-CONTINUE/investigation_sessions")
        .join(format!("{session_id}.state.json"));
    assert!(session_state_path.is_file());
    let session_state: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&session_state_path).expect("session state"))
            .expect("session state json");
    assert_eq!(
        session_state["schema_version"],
        "obs.investigation_session_state.v1"
    );
    assert!(session_state["completed_steps"]
        .as_array()
        .expect("completed steps")
        .iter()
        .any(|step| step == "IR001"));
    assert!(session_state["completed_refs"]
        .as_array()
        .expect("completed refs")
        .iter()
        .any(|reference| reference["label"] == "service_state"));
    assert!(session_state["branch_evaluations"]
        .as_array()
        .expect("state branch evaluations")
        .iter()
        .any(|branch| branch["status"] == "matched"));
    assert!(session_state["next_actions"]
        .as_array()
        .expect("state next actions")
        .iter()
        .any(|action| action["next_step_id"] == "IR002"));
    assert_eq!(
        continue_json["raw_refs"]["investigation_session_state"],
        format!("artifact://investigation_sessions/{session_id}.state.json")
    );
    let continue_second = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "investigate",
            "continue",
            "--run-id",
            "R-CLI-CONTINUE",
            "--step-id",
            "IR002",
            "--session-id",
            session_id,
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("second investigation continue");
    assert!(
        continue_second.status.success(),
        "second continue failed: {}",
        String::from_utf8_lossy(&continue_second.stderr)
    );
    let session_output = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "investigate",
            "session",
            "--run-id",
            "R-CLI-CONTINUE",
            "--session-id",
            session_id,
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("investigation session");
    assert!(
        session_output.status.success(),
        "session read failed: {}",
        String::from_utf8_lossy(&session_output.stderr)
    );
    let resumed_state: serde_json::Value =
        serde_json::from_slice(&session_output.stdout).expect("resumed session json");
    assert!(resumed_state["completed_steps"]
        .as_array()
        .expect("completed steps")
        .iter()
        .any(|step| step == "IR001"));
    assert!(resumed_state["completed_steps"]
        .as_array()
        .expect("completed steps")
        .iter()
        .any(|step| step == "IR002"));
    assert!(resumed_state["completed_refs"]
        .as_array()
        .expect("completed refs")
        .iter()
        .any(|reference| reference["label"] == "log"));
    assert!(resumed_state["compact_summary"]
        .as_array()
        .expect("compact summary")
        .iter()
        .any(|entry| entry
            .as_str()
            .expect("summary")
            .contains("completed_steps=2")));
    assert!(resumed_state["branch_evaluations"]
        .as_array()
        .expect("branch evaluations")
        .iter()
        .any(|branch| branch["step_id"] == "IR002"));
    assert_eq!(
        resumed_state["retention_policy"]["cleanup_mode"],
        "manual_dry_run_first"
    );
    let cleanup_output = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "investigate",
            "cleanup-sessions",
            "--run-id",
            "R-CLI-CONTINUE",
            "--max-sessions",
            "0",
            "--dry-run",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("session cleanup dry-run");
    assert!(
        cleanup_output.status.success(),
        "cleanup dry-run failed: {}",
        String::from_utf8_lossy(&cleanup_output.stderr)
    );
    let cleanup: serde_json::Value =
        serde_json::from_slice(&cleanup_output.stdout).expect("cleanup json");
    assert_eq!(
        cleanup["schema_version"],
        "obs.investigation_session_cleanup.v1"
    );
    assert_eq!(cleanup["dry_run"], true);
    assert!(
        cleanup["candidate_count"]
            .as_u64()
            .expect("candidate count")
            > 0
    );
    assert!(cleanup["candidates"]
        .as_array()
        .expect("cleanup candidates")
        .iter()
        .all(|candidate| candidate["deleted"] == false
            && candidate["age_seconds"].as_u64().is_some()));
    assert!(session_state_path.is_file());
    let cleanup_execute = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "investigate",
            "cleanup-sessions",
            "--run-id",
            "R-CLI-CONTINUE",
            "--max-age-days",
            "0",
            "--execute",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("session cleanup execute");
    assert!(
        cleanup_execute.status.success(),
        "cleanup execute failed: {}",
        String::from_utf8_lossy(&cleanup_execute.stderr)
    );
    let executed: serde_json::Value =
        serde_json::from_slice(&cleanup_execute.stdout).expect("cleanup execute json");
    assert_eq!(executed["dry_run"], false);
    assert!(executed["deleted_count"].as_u64().expect("deleted count") > 0);
    assert!(!session_state_path.is_file());
    let manifest = std::fs::read_to_string(temp.path().join("runs/R-CLI-CONTINUE/manifest.json"))
        .expect("manifest");
    assert!(manifest.contains("investigation_route.json"));
    assert!(manifest.contains("investigation_sessions/"));
    assert!(manifest.contains(".state.json"));
}

#[test]
fn investigate_route_packs_lists_typed_cause_neutral_packs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let output = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["investigate", "route-packs"])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("route packs");
    assert!(
        output.status.success(),
        "route packs failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let registry: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("route pack registry json");
    assert_eq!(registry["schema_version"], "obs.route_pack_registry.v1");
    assert!(registry["packs"]
        .as_array()
        .expect("packs")
        .iter()
        .any(|pack| pack["domain"] == "latency_timeouts"));
    assert!(registry["packs"]
        .as_array()
        .expect("packs")
        .iter()
        .all(|pack| pack["cause_neutral"] == true));
}

#[test]
fn observe_service_name_records_real_service_state_not_placeholder() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fake_bin = temp.path().join("fake-bin");
    std::fs::create_dir_all(&fake_bin).expect("fake bin");
    let systemctl = fake_bin.join("systemctl");
    std::fs::write(
        &systemctl,
        "#!/usr/bin/env bash\ncat <<'OUT'\nId=ssh.service\nLoadState=loaded\nActiveState=active\nSubState=running\nMainPID=4242\nFragmentPath=/usr/lib/systemd/system/ssh.service\nOUT\n",
    )
    .expect("fake systemctl");
    let mut perms = std::fs::metadata(&systemctl)
        .expect("metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&systemctl, perms).expect("chmod");

    let old_path = std::env::var("PATH").unwrap_or_default();
    let observe = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "observe",
            "--run-id",
            "R-CLI-SERVICE-REAL",
            "--duration-ms",
            "120",
            "--interval-ms",
            "40",
            "--service-name",
            "ssh",
        ])
        .env("ADC_HOME", temp.path())
        .env("PATH", format!("{}:{old_path}", fake_bin.display()))
        .output()
        .expect("observe");
    assert!(
        observe.status.success(),
        "observe failed: {}",
        String::from_utf8_lossy(&observe.stderr)
    );

    let state: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            temp.path()
                .join("runs/R-CLI-SERVICE-REAL/raw/service_state.json"),
        )
        .expect("service state"),
    )
    .expect("state json");
    assert_eq!(state["active_state"], "active");
    assert_eq!(state["sub_state"], "running");
    assert_eq!(state["availability"], "available");

    let context = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "agent-context",
            "--run-id",
            "R-CLI-SERVICE-REAL",
            "--format",
            "json",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("context");
    assert!(
        context.status.success(),
        "context failed: {}",
        String::from_utf8_lossy(&context.stderr)
    );
    let context_json: serde_json::Value =
        serde_json::from_slice(&context.stdout).expect("context json");
    let service_fact = context_json["derived_facts"]
        .as_array()
        .expect("facts")
        .iter()
        .find(|fact| fact["kind"] == "service_state")
        .expect("service fact");
    assert_eq!(service_fact["attributes"]["active_state"], "active");
    assert_eq!(service_fact["attributes"]["availability"], "available");
    assert!(service_fact["statement"]
        .as_str()
        .expect("statement")
        .contains("active/running"));
}

#[test]
fn unavailable_service_state_does_not_outrank_direct_log_evidence() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fake_bin = temp.path().join("fake-bin");
    std::fs::create_dir_all(&fake_bin).expect("fake bin");
    let systemctl = fake_bin.join("systemctl");
    std::fs::write(
        &systemctl,
        "#!/usr/bin/env sh\nprintf '%s\n' 'Unit not found' >&2\nexit 1\n",
    )
    .expect("fake systemctl");
    let mut perms = std::fs::metadata(&systemctl)
        .expect("metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&systemctl, perms).expect("chmod");
    let log_path = temp.path().join("app.log");
    std::fs::write(
        &log_path,
        "info boot\nerror request timeout id=abc\nwarning retrying request\n",
    )
    .expect("log");
    let old_path = std::env::var("PATH").unwrap_or_default();

    let observe = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "observe",
            "--run-id",
            "R-CLI-UNAVAILABLE-SERVICE",
            "--duration-ms",
            "120",
            "--interval-ms",
            "40",
            "--log-file",
            log_path.to_str().expect("log path"),
            "--service-name",
            "missing-service",
        ])
        .env("ADC_HOME", temp.path())
        .env("PATH", format!("{}:{old_path}", fake_bin.display()))
        .output()
        .expect("observe");
    assert!(
        observe.status.success(),
        "observe failed: {}",
        String::from_utf8_lossy(&observe.stderr)
    );
    let context = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "agent-context",
            "--run-id",
            "R-CLI-UNAVAILABLE-SERVICE",
            "--format",
            "json",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("context");
    assert!(
        context.status.success(),
        "context failed: {}",
        String::from_utf8_lossy(&context.stderr)
    );
    let context_json: serde_json::Value =
        serde_json::from_slice(&context.stdout).expect("context json");
    let facts = context_json["derived_facts"].as_array().expect("facts");
    assert_eq!(facts[0]["kind"], "log_error_slice");
    let service_fact = facts
        .iter()
        .find(|fact| fact["kind"] == "service_state")
        .expect("service fact");
    assert_eq!(service_fact["attributes"]["availability"], "unavailable");
}

#[test]
fn investigate_service_returns_cause_neutral_pack_without_shell_surface() {
    let temp = tempfile::tempdir().expect("tempdir");
    let output = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "investigate",
            "service",
            "definitely-not-a-real-adc-service",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("investigate service");

    assert!(
        output.status.success(),
        "investigate service failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let pack: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json pack");
    assert_eq!(pack["schema_version"], "obs.service_investigation.v1");
    assert_eq!(pack["service_name"], "definitely-not-a-real-adc-service");
    assert_eq!(pack["root_required"], false);
    assert!(pack["service_state"]["active_state"].is_string());
    assert!(pack["data_quality"]["missing"].is_array());
    assert!(pack["next_probe_options"]
        .as_array()
        .expect("next probes")
        .iter()
        .any(|probe| probe["probe_id"] == "observe_service_window"));
    let serialized = String::from_utf8(output.stdout).expect("utf8");
    assert!(!serialized.to_ascii_lowercase().contains("root cause"));
    assert!(!serialized.contains("shell"));
}

#[test]
fn investigate_service_marks_unavailable_ports_and_summarizes_journal_recency() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fake_bin = temp.path().join("fake-bin");
    std::fs::create_dir_all(&fake_bin).expect("fake bin");
    let systemctl = fake_bin.join("systemctl");
    std::fs::write(
        &systemctl,
        "#!/usr/bin/env bash\ncat <<'OUT'\nId=ssh.service\nLoadState=loaded\nActiveState=active\nSubState=running\nMainPID=999999\nFragmentPath=/usr/lib/systemd/system/ssh.service\nOUT\n",
    )
    .expect("fake systemctl");
    let journalctl = fake_bin.join("journalctl");
    std::fs::write(
        &journalctl,
        "#!/usr/bin/env bash\ncat <<'OUT'\n2026-05-27T00:01:00+09:00 host sshd[1]: Started OpenSSH server\n2026-05-27T00:02:00+09:00 host sshd[2]: Failed password for example-user from 192.0.2.10 port 12345 ssh2\n2026-05-27T00:03:00+09:00 host sshd[3]: timeout while flushing session\nOUT\n",
    )
    .expect("fake journalctl");
    for path in [&systemctl, &journalctl] {
        let mut perms = std::fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).expect("chmod");
    }

    let old_path = std::env::var("PATH").unwrap_or_default();
    let output = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["investigate", "service", "ssh", "--journal-lines", "3"])
        .env("ADC_HOME", temp.path())
        .env("PATH", format!("{}:{old_path}", fake_bin.display()))
        .output()
        .expect("investigate service");
    assert!(
        output.status.success(),
        "investigate service failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let pack: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json pack");
    assert_eq!(pack["service_state"]["active_state"], "active");
    assert_eq!(pack["port_summary"]["availability"], "unavailable");
    assert!(pack["port_summary"]["socket_inode_count"].is_null());
    assert!(pack["port_summary"]["unavailable_reason"]
        .as_str()
        .expect("reason")
        .contains("fd unavailable"));
    assert_eq!(pack["journal_summary"]["requested_line_count"], 3);
    assert_eq!(pack["journal_summary"]["returned_lead_count"], 2);
    assert_eq!(
        pack["journal_summary"]["oldest_timestamp"],
        "2026-05-27T00:02:00+09:00"
    );
    assert_eq!(
        pack["journal_summary"]["newest_timestamp"],
        "2026-05-27T00:03:00+09:00"
    );
    assert_eq!(pack["journal_summary"]["window_basis"], "last_n_lines");
}

#[test]
fn investigate_start_builds_one_shot_agent_route_with_service_pack() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fake_bin = temp.path().join("fake-bin");
    std::fs::create_dir_all(&fake_bin).expect("fake bin");
    let systemctl = fake_bin.join("systemctl");
    std::fs::write(
        &systemctl,
        "#!/usr/bin/env bash\ncat <<'OUT'\nId=ssh.service\nLoadState=loaded\nActiveState=active\nSubState=running\nMainPID=999999\nFragmentPath=/usr/lib/systemd/system/ssh.service\nOUT\n",
    )
    .expect("fake systemctl");
    let journalctl = fake_bin.join("journalctl");
    std::fs::write(
        &journalctl,
        "#!/usr/bin/env bash\ncat <<'OUT'\n2026-05-27T00:02:00+09:00 host sshd[2]: Failed password for example-user from 192.0.2.10 port 12345 ssh2\n2026-05-27T00:03:00+09:00 host sshd[3]: timeout while flushing session\nOUT\n",
    )
    .expect("fake journalctl");
    for path in [&systemctl, &journalctl] {
        let mut perms = std::fs::metadata(path).expect("metadata").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).expect("chmod");
    }

    let observe = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "observe",
            "--run-id",
            "R-INVESTIGATION-START",
            "--duration-ms",
            "120",
            "--interval-ms",
            "40",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("observe");
    assert!(
        observe.status.success(),
        "observe failed: {}",
        String::from_utf8_lossy(&observe.stderr)
    );

    let old_path = std::env::var("PATH").unwrap_or_default();
    let start = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "investigate",
            "start",
            "--run-id",
            "R-INVESTIGATION-START",
            "--service-name",
            "ssh",
            "--journal-lines",
            "2",
        ])
        .env("ADC_HOME", temp.path())
        .env("PATH", format!("{}:{old_path}", fake_bin.display()))
        .output()
        .expect("investigate start");
    assert!(
        start.status.success(),
        "investigate start failed: {}",
        String::from_utf8_lossy(&start.stderr)
    );
    let response: serde_json::Value =
        serde_json::from_slice(&start.stdout).expect("investigation start json");
    assert_eq!(response["schema_version"], "obs.investigation_start.v1");
    assert_eq!(response["scope"], "run");
    assert_eq!(response["run_id"], "R-INVESTIGATION-START");
    assert_eq!(
        response["agent_context"]["schema_version"],
        "obs.agent_context.v1"
    );
    assert_eq!(
        response["investigation_route"]["schema_version"],
        "obs.investigation_route.v1"
    );
    assert_eq!(response["investigation_route"]["service_name"], "ssh");
    assert!(
        response["investigation_route"]["raw_refs"]["service.journal_leads"]
            .as_str()
            .expect("journal ref")
            .contains("artifact://service_investigations/ssh/journal_leads.json")
    );
    assert!(response["investigation_route"]["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .all(|step| step["cause_neutral"] == true));
    assert!(temp
        .path()
        .join("service_investigations/ssh/service_investigation.json")
        .is_file());
    let serialized = String::from_utf8(start.stdout).expect("utf8");
    assert!(
        serialized.len() < 12_000,
        "start pack should stay compact, was {} bytes",
        serialized.len()
    );
    assert!(!serialized.to_ascii_lowercase().contains("root cause"));
    assert!(!serialized.to_ascii_lowercase().contains("likely cause"));
    assert!(!serialized.contains("shell"));
}

#[test]
fn investigate_ref_opens_service_investigation_raw_ref() {
    let temp = tempfile::tempdir().expect("tempdir");
    let service_name = "definitely-not-a-real-adc-service";
    let output = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["investigate", "service", service_name])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("investigate service");
    assert!(
        output.status.success(),
        "investigate service failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let pack: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json pack");
    let service_ref = pack["raw_refs"]["service_state"]
        .as_str()
        .expect("service ref");

    let resolved = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["investigate", "ref", "--ref", service_ref, "--limit", "20"])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("investigate ref");
    assert!(
        resolved.status.success(),
        "investigate ref failed: {}",
        String::from_utf8_lossy(&resolved.stderr)
    );
    let resolved_json: serde_json::Value =
        serde_json::from_slice(&resolved.stdout).expect("resolved json");
    assert_eq!(resolved_json["ref_kind"], "service_investigation");
    assert_eq!(resolved_json["content_type"], "application/json");
    assert!(resolved_json["text"]
        .as_str()
        .expect("resolved text")
        .contains("active_state"));
}

#[test]
fn subcommand_help_does_not_require_runtime_flags() {
    for args in [
        vec!["observe", "--help"],
        vec!["agent-context", "--help"],
        vec!["fleet", "enroll", "--help"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_adc"))
            .args(args)
            .output()
            .expect("help");
        assert!(
            output.status.success(),
            "help failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("help utf8");
        assert!(stdout.contains("Usage:"), "help output was {stdout}");
        assert!(!stdout.contains("missing required flag"));
    }
}

#[test]
fn doctor_reports_ready_without_requesting_root_residency() {
    let temp = tempfile::tempdir().expect("tempdir");
    let doctor = Command::new(env!("CARGO_BIN_EXE_adc"))
        .arg("doctor")
        .env("ADC_HOME", temp.path())
        .output()
        .expect("doctor");
    assert!(
        doctor.status.success(),
        "doctor failed: {}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    let doctor_json: serde_json::Value =
        serde_json::from_slice(&doctor.stdout).expect("doctor json");
    assert_eq!(doctor_json["service"], "adc");
    assert_eq!(doctor_json["status"], "ready");
    assert_eq!(doctor_json["root_required"], false);
    assert!(doctor_json["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .any(|check| check["name"] == "artifact_root"));
}

#[test]
fn target_preflight_reports_local_tool_and_artifact_root_readiness() {
    let temp = tempfile::tempdir().expect("tempdir");
    let preflight = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["target", "preflight", "--target", "local-a"])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("target preflight");
    assert!(
        preflight.status.success(),
        "target preflight failed: {}",
        String::from_utf8_lossy(&preflight.stderr)
    );
    let preflight_json: serde_json::Value =
        serde_json::from_slice(&preflight.stdout).expect("target preflight json");
    assert_eq!(preflight_json["schema_version"], "obs.target_preflight.v1");
    assert_eq!(preflight_json["target_id"], "local-a");
    assert_eq!(preflight_json["status"], "ready");
    assert_eq!(preflight_json["root_required"], false);
    assert!(preflight_json["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .any(|check| check["name"] == "artifact_root_writable" && check["status"] == "ok"));
}

#[test]
fn agent_context_supports_latest_run_and_fleet_context() {
    let temp = tempfile::tempdir().expect("tempdir");
    for run_id in ["R-LATEST-A", "R-LATEST-B"] {
        let observe = Command::new(env!("CARGO_BIN_EXE_adc"))
            .args([
                "observe",
                "--run-id",
                run_id,
                "--duration-ms",
                "120",
                "--interval-ms",
                "40",
            ])
            .env("ADC_HOME", temp.path())
            .output()
            .expect("observe");
        assert!(
            observe.status.success(),
            "observe failed: {}",
            String::from_utf8_lossy(&observe.stderr)
        );
    }

    let latest = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["agent-context", "--run-id", "latest", "--format", "json"])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("latest context");
    assert!(
        latest.status.success(),
        "latest context failed: {}",
        String::from_utf8_lossy(&latest.stderr)
    );
    let latest_json: serde_json::Value =
        serde_json::from_slice(&latest.stdout).expect("latest json");
    assert_eq!(latest_json["run_id"], "R-LATEST-B");

    let inventory_path = temp.path().join("targets.yaml");
    std::fs::write(
        &inventory_path,
        r#"
targets:
  - id: local-a
    transport: local
  - id: unsupported-b
    transport: serial
"#,
    )
    .expect("inventory");

    let preflight = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "fleet",
            "preflight",
            "--inventory",
            inventory_path.to_str().expect("inventory path"),
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("fleet preflight");
    assert!(
        preflight.status.success(),
        "fleet preflight failed: {}",
        String::from_utf8_lossy(&preflight.stderr)
    );
    let preflight_json: serde_json::Value =
        serde_json::from_slice(&preflight.stdout).expect("preflight json");
    assert_eq!(preflight_json["schema_version"], "obs.fleet_preflight.v1");
    assert_eq!(preflight_json["root_required"], false);
    assert_eq!(preflight_json["status"], "degraded");
    assert_eq!(preflight_json["target_count"], 2);
    assert_eq!(preflight_json["ready_count"], 1);
    assert_eq!(preflight_json["failed_count"], 1);
    assert_eq!(preflight_json["targets"][0]["target_id"], "local-a");
    assert_eq!(preflight_json["targets"][0]["status"], "ready");
    assert_eq!(preflight_json["targets"][1]["target_id"], "unsupported-b");
    assert_eq!(preflight_json["targets"][1]["status"], "unsupported");

    let observe = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "fleet",
            "observe",
            "--inventory",
            inventory_path.to_str().expect("inventory path"),
            "--fleet-run-id",
            "F-CLI-AGENT",
            "--duration-ms",
            "120",
            "--interval-ms",
            "40",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("fleet observe");
    assert!(
        observe.status.success(),
        "fleet observe failed: {}",
        String::from_utf8_lossy(&observe.stderr)
    );

    let fleet_context = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "agent-context",
            "--fleet-run-id",
            "F-CLI-AGENT",
            "--format",
            "json",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("fleet context");
    assert!(
        fleet_context.status.success(),
        "fleet context failed: {}",
        String::from_utf8_lossy(&fleet_context.stderr)
    );
    let fleet_json: serde_json::Value =
        serde_json::from_slice(&fleet_context.stdout).expect("fleet json");
    assert_eq!(fleet_json["schema_version"], "obs.agent_context.fleet.v1");
    assert_eq!(fleet_json["captured_count"], 1);
    assert_eq!(fleet_json["failed_count"], 1);
    assert_eq!(fleet_json["target_summaries"][0]["target_id"], "local-a");
    assert!(
        fleet_json["target_summaries"][0]["event_count"]
            .as_u64()
            .expect("event count")
            > 0
    );
    assert_eq!(fleet_json["cross_target_summary"]["captured_count"], 1);
    assert_eq!(fleet_json["cross_target_summary"]["failed_count"], 1);
    assert!(
        fleet_json["cross_target_summary"]["total_event_count"]
            .as_u64()
            .expect("total event count")
            > 0
    );
    let hints = fleet_json["remediation_hints"]
        .as_array()
        .expect("hints")
        .iter()
        .map(|hint| hint.as_str().expect("hint").to_string())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        hints.len(),
        fleet_json["remediation_hints"]
            .as_array()
            .expect("hints")
            .len()
    );
    assert!(fleet_json["remediation_hints"]
        .as_array()
        .expect("hints")
        .iter()
        .any(|hint| hint
            .as_str()
            .expect("hint")
            .contains("unsupported transport")));

    let latest_fleet_context = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "agent-context",
            "--fleet-run-id",
            "latest",
            "--format",
            "json",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("latest fleet context");
    assert!(
        latest_fleet_context.status.success(),
        "latest fleet context failed: {}",
        String::from_utf8_lossy(&latest_fleet_context.stderr)
    );
    let latest_fleet_json: serde_json::Value =
        serde_json::from_slice(&latest_fleet_context.stdout).expect("latest fleet json");
    assert_eq!(latest_fleet_json["fleet_run_id"], "F-CLI-AGENT");

    let fleet_markdown = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["agent-context", "--fleet-run-id", "latest"])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("fleet markdown");
    assert!(
        fleet_markdown.status.success(),
        "fleet markdown failed: {}",
        String::from_utf8_lossy(&fleet_markdown.stderr)
    );
    let fleet_markdown = String::from_utf8(fleet_markdown.stdout).expect("markdown utf8");
    assert!(fleet_markdown.contains("## Target Matrix"));
    assert!(fleet_markdown.contains("## Investigation Route"));
    assert!(fleet_markdown.contains("local-a"));
    assert!(fleet_markdown.contains("unsupported-b"));
}
