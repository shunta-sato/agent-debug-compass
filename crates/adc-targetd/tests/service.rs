use std::{fs, path::Path, process::Command};

#[test]
fn service_once_writes_state_and_recovers_existing_runs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_dir = temp.path().join("runs/R-OLD");
    fs::create_dir_all(&run_dir).expect("run dir");
    fs::write(run_dir.join("manifest.json"), "{}").expect("manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_adc-targetd"))
        .arg("--service-once")
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run service once");

    assert!(
        output.status.success(),
        "service-once failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let state_path = temp.path().join("daemon/state.json");
    assert!(state_path.is_file());
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(state_path).expect("state")).expect("state json");
    assert_eq!(state["service"], "adc-targetd");
    assert_eq!(state["status"], "ready");
    assert_eq!(state["recovered_runs"][0], "R-OLD");
}

#[test]
fn service_for_ms_captures_kmsg_fixture_trigger_for_armed_profile() {
    let temp = tempfile::tempdir().expect("tempdir");
    let profile_dir = temp.path().join("profiles");
    fs::create_dir_all(&profile_dir).expect("profile dir");
    fs::write(
        profile_dir.join("e2e_kmsg.yaml"),
        r#"
profile: e2e_kmsg
sampling:
  interval_ms: 10
always_on:
  collectors: [kmsg]
budgets:
  max_daemon_cpu_percent: 3
  max_memory_mb: 128
  max_artifact_mb_per_run: 16
triggers:
  - name: kmsg_warning_pattern
    type: kmsg_pattern
    severity_at_least: warning
    patterns: [warning, timeout]
"#,
    )
    .expect("profile");
    let kmsg_fixture = temp.path().join("kmsg.log");
    fs::write(&kmsg_fixture, "warning: synthetic timeout observed\n").expect("kmsg fixture");

    adc_core::arm_profile(temp.path(), "e2e_kmsg").expect("arm profile");

    let output = Command::new(env!("CARGO_BIN_EXE_adc-targetd"))
        .args(["--service-for-ms", "80"])
        .env("ADC_HOME", temp.path())
        .env("ADC_PROFILE_DIR", &profile_dir)
        .env("ADC_KMSG_FIXTURE", &kmsg_fixture)
        .output()
        .expect("run bounded service");

    assert!(
        output.status.success(),
        "service-for-ms failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("service summary json");
    let run_id = summary["captured_runs"][0]
        .as_str()
        .expect("captured run id");
    let incident_id = summary["frozen_incidents"][0]
        .as_str()
        .expect("frozen incident id");
    let run_dir = temp.path().join("runs").join(run_id);
    let incident_dir = temp.path().join("recorder/incidents").join(incident_id);
    assert_v2_top_level_layout(&run_dir);
    let evidence = fs::read_to_string(run_dir.join("evidence_index.yaml")).expect("evidence");
    let timeline = fs::read_to_string(run_dir.join("timeline.jsonl")).expect("timeline");
    assert!(evidence.contains("kmsg_warning_pattern"));
    assert!(evidence.contains("raw_refs:"));
    assert!(timeline.contains(r#""source":"kmsg""#));
    assert!(timeline.contains("synthetic timeout"));
    let frozen_window: serde_json::Value = serde_json::from_slice(
        &fs::read(incident_dir.join("frozen_window.json")).expect("frozen window"),
    )
    .expect("frozen window json");
    let samples = fs::read_to_string(incident_dir.join("samples.jsonl")).expect("samples");
    assert_eq!(frozen_window["freeze_reason"], "trigger_policy");
    assert_eq!(
        frozen_window["preservation_reason"]["name"],
        "kmsg_warning_pattern"
    );
    assert!(samples.contains("kmsg.cursor"));
}

#[test]
fn service_for_ms_freezes_pending_marker_from_retained_recorder_ring() {
    let temp = tempfile::tempdir().expect("tempdir");
    let profile_dir = temp.path().join("profiles");
    fs::create_dir_all(&profile_dir).expect("profile dir");
    fs::write(
        profile_dir.join("recorder_memory.yaml"),
        r#"
profile: recorder_memory
sampling:
  interval_ms: 10
always_on:
  collectors: [memory]
budgets:
  max_daemon_cpu_percent: 3
  max_memory_mb: 128
  max_artifact_mb_per_run: 16
triggers: []
"#,
    )
    .expect("profile");
    adc_core::arm_profile(temp.path(), "recorder_memory").expect("arm profile");
    let marker = adc_core::marker_at_received_time(
        "marker-service-001",
        "operator",
        "camera frame drop observed around now",
        1_000,
    );
    adc_core::write_pending_recorder_marker(temp.path(), &marker).expect("pending marker");

    let output = Command::new(env!("CARGO_BIN_EXE_adc-targetd"))
        .args(["--service-for-ms", "80"])
        .env("ADC_HOME", temp.path())
        .env("ADC_PROFILE_DIR", &profile_dir)
        .output()
        .expect("run bounded service");

    assert!(
        output.status.success(),
        "service-for-ms failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("service summary json");
    let incident_id = summary["frozen_incidents"][0]
        .as_str()
        .expect("frozen incident id");
    let incident_dir = temp.path().join("recorder/incidents").join(incident_id);
    let frozen_window: serde_json::Value = serde_json::from_slice(
        &fs::read(incident_dir.join("frozen_window.json")).expect("frozen window"),
    )
    .expect("frozen window json");
    let samples = fs::read_to_string(incident_dir.join("samples.jsonl")).expect("samples");
    assert_eq!(frozen_window["marker_id"], "marker-service-001");
    assert!(samples.contains("memory.summary"));
    assert!(frozen_window["loss_report"]["collector_loss"]
        .as_array()
        .expect("collector loss")
        .iter()
        .any(|loss| loss["collector_id"] == "memory.summary"));
    let recorder_status: serde_json::Value = serde_json::from_slice(
        &fs::read(temp.path().join("recorder/status.json")).expect("recorder status"),
    )
    .expect("recorder status json");
    assert_eq!(recorder_status["schema_version"], "obs.recorder_status.v1");
    assert!(recorder_status["buffer_status"]["signals"]
        .as_array()
        .expect("signals")
        .iter()
        .any(|signal| signal["signal_id"] == "memory.summary"
            && signal["recorded_samples"].as_u64().unwrap_or(0) > 0));
    assert_eq!(
        recorder_status["buffer_status"]["data_quality"]["throttled"],
        true
    );
    assert!(recorder_status["buffer_status"]["data_quality"]["notes"]
        .as_array()
        .expect("notes")
        .iter()
        .any(|note| note
            .as_str()
            .is_some_and(|note| note.contains("max_samples_per_second"))));
}

#[test]
fn service_for_ms_throttles_pending_marker_storm_to_recorder_budget() {
    let temp = tempfile::tempdir().expect("tempdir");
    let profile_dir = temp.path().join("profiles");
    fs::create_dir_all(&profile_dir).expect("profile dir");
    fs::write(
        profile_dir.join("recorder_memory.yaml"),
        r#"
profile: recorder_memory
sampling:
  interval_ms: 10
always_on:
  collectors: [memory]
budgets:
  max_daemon_cpu_percent: 3
  max_memory_mb: 128
  max_artifact_mb_per_run: 16
triggers: []
"#,
    )
    .expect("profile");
    adc_core::arm_profile(temp.path(), "recorder_memory").expect("arm profile");
    for index in 0..6 {
        let marker = adc_core::marker_at_received_time(
            format!("marker-storm-{index}"),
            "operator",
            "frame drops repeated quickly",
            1_000 + index,
        );
        adc_core::write_pending_recorder_marker(temp.path(), &marker).expect("pending marker");
    }

    let output = Command::new(env!("CARGO_BIN_EXE_adc-targetd"))
        .args(["--service-for-ms", "80"])
        .env("ADC_HOME", temp.path())
        .env("ADC_PROFILE_DIR", &profile_dir)
        .output()
        .expect("run bounded service");

    assert!(
        output.status.success(),
        "service-for-ms failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("service summary json");
    assert_eq!(
        summary["frozen_incidents"]
            .as_array()
            .expect("frozen incidents")
            .len(),
        adc_core::default_recorder_budget().max_frozen_incidents as usize
    );
    assert_eq!(summary["data_quality"]["throttled"], true);
    assert!(summary["data_quality"]["notes"]
        .as_array()
        .expect("notes")
        .iter()
        .any(|note| note
            .as_str()
            .is_some_and(|note| note.contains("max_frozen_incidents"))));
    let refused_result: serde_json::Value = serde_json::from_slice(
        &fs::read(
            temp.path()
                .join("recorder/markers/results/marker-storm-5.json"),
        )
        .expect("refused marker result"),
    )
    .expect("marker result json");
    assert_eq!(
        refused_result["schema_version"],
        "obs.recorder_marker_result.v1"
    );
    assert_eq!(refused_result["status"], "refused");
    assert_eq!(refused_result["reason"], "max_frozen_incidents_exceeded");
    assert_eq!(refused_result["data_quality"]["throttled"], true);
}

fn assert_v2_top_level_layout(run_dir: &Path) {
    let mut entries = fs::read_dir(run_dir)
        .expect("run dir")
        .map(|entry| {
            entry
                .expect("run dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    entries.sort();
    assert_eq!(
        entries,
        [
            "evidence_index.yaml",
            "manifest.json",
            "overhead_report.json",
            "raw",
            "timeline.jsonl",
            "windows",
        ]
    );
}
