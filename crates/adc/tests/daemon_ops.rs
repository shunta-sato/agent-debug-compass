use std::{fs, process::Command};

#[test]
fn arm_disarm_capture_and_evidence_get_use_persistent_state() {
    let temp = tempfile::tempdir().expect("tempdir");

    let arm = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["arm", "--profile", "pi5_basic"])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run arm");
    assert!(
        arm.status.success(),
        "arm failed: {}",
        String::from_utf8_lossy(&arm.stderr)
    );
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(temp.path().join("daemon/state.json")).expect("state"))
            .expect("state json");
    assert_eq!(state["active_profile"], "pi5_basic");
    assert_eq!(state["status"], "armed");

    let capture = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "capture",
            "--run-id",
            "R-CAPTURE-001",
            "--duration-ms",
            "10",
            "--interval-ms",
            "10",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run capture");
    assert!(
        capture.status.success(),
        "capture failed: {}",
        String::from_utf8_lossy(&capture.stderr)
    );
    let capture_response: serde_json::Value =
        serde_json::from_slice(&capture.stdout).expect("capture json");
    assert_eq!(capture_response["run_id"], "R-CAPTURE-001");
    assert_eq!(capture_response["target_id"], "local");

    let evidence = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["evidence", "get", "--run-id", "R-CAPTURE-001"])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run evidence get");
    assert!(
        evidence.status.success(),
        "evidence get failed: {}",
        String::from_utf8_lossy(&evidence.stderr)
    );
    let evidence_stdout = String::from_utf8(evidence.stdout).expect("evidence utf8");
    assert!(evidence_stdout.contains("run_id: R-CAPTURE-001"));
    assert!(evidence_stdout.contains("raw_refs:"));

    let disarm = Command::new(env!("CARGO_BIN_EXE_adc"))
        .arg("disarm")
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run disarm");
    assert!(
        disarm.status.success(),
        "disarm failed: {}",
        String::from_utf8_lossy(&disarm.stderr)
    );
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(temp.path().join("daemon/state.json")).expect("state"))
            .expect("state json");
    assert!(state["active_profile"].is_null());
    assert_eq!(state["status"], "ready");
    assert_eq!(state["last_run_id"], "R-CAPTURE-001");
}

#[test]
fn recorder_mark_returns_artifact_ref_not_local_path() {
    let temp = tempfile::tempdir().expect("tempdir");

    let output = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "recorder",
            "mark",
            "--symptom",
            "camera frame drop",
            "--marker-id",
            "marker-ref-test",
            "--format",
            "json",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run recorder mark");
    assert!(
        output.status.success(),
        "recorder mark failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("marker result json");
    let pending_ref = response["pending_marker_ref"]
        .as_str()
        .expect("pending marker ref");
    assert_eq!(
        pending_ref,
        "artifact://recorder/markers/pending/marker-ref-test.json"
    );
    assert!(
        !pending_ref.contains(temp.path().to_string_lossy().as_ref()),
        "pending marker ref must not expose local artifact root"
    );
}

#[test]
fn recorder_incident_get_rejects_traversal_and_reads_trigger_incidents() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut ring = adc_core::RecorderRing::new("local", 4, 60_000);
    ring.push(adc_core::RecorderSample {
        time_mono_ns: 1_000,
        signals: vec![adc_core::RecorderSignalSample {
            signal_id: "kmsg.cursor".to_string(),
            value: 1.0,
        }],
    });
    adc_core::freeze_recorder_trigger(
        temp.path(),
        "INC-TRIGGER-safe",
        "win-trigger-safe",
        "kmsg_warning_pattern",
        1_000,
        &ring,
        &adc_core::default_recorder_budget(),
    )
    .expect("trigger freeze");

    let traversal = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["recorder", "incident", "get", "--incident-id", "../outside"])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run traversal incident get");
    assert!(
        !traversal.status.success(),
        "traversal incident get unexpectedly succeeded"
    );
    assert!(
        String::from_utf8_lossy(&traversal.stderr).contains("single safe recorder file segment")
    );

    let get = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "recorder",
            "incident",
            "get",
            "--incident-id",
            "INC-TRIGGER-safe",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run trigger incident get");
    assert!(
        get.status.success(),
        "trigger incident get failed: {}",
        String::from_utf8_lossy(&get.stderr)
    );
    let response: serde_json::Value =
        serde_json::from_slice(&get.stdout).expect("incident resolution json");
    assert_eq!(
        response["schema_version"],
        "obs.recorder_incident_resolution.v1"
    );
    assert!(response["marker"].is_null());
    assert_eq!(
        response["trigger_event"]["schema_version"],
        "obs.recorder_trigger_event.v1"
    );
    assert!(
        response.get("incident_dir").is_none(),
        "default recorder incident resolution must not expose local paths"
    );
    assert_eq!(
        response["incident_ref"],
        "artifact://recorder/incidents/INC-TRIGGER-safe/incident.json"
    );
    assert_eq!(
        response["frozen_window_ref"],
        "artifact://recorder/incidents/INC-TRIGGER-safe/frozen_window.json"
    );
    assert_eq!(
        response["loss_report_ref"],
        "artifact://recorder/incidents/INC-TRIGGER-safe/loss_report.json"
    );
    assert_eq!(
        response["coverage_ref"],
        "artifact://recorder/incidents/INC-TRIGGER-safe/coverage.json"
    );
    assert_eq!(
        response["samples_ref"],
        "artifact://recorder/incidents/INC-TRIGGER-safe/samples.jsonl"
    );

    let coverage_ref = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "investigate",
            "ref",
            "--ref",
            "artifact://recorder/incidents/INC-TRIGGER-safe/coverage.json",
            "--limit",
            "20",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run coverage ref");
    assert!(
        coverage_ref.status.success(),
        "coverage ref failed: {}",
        String::from_utf8_lossy(&coverage_ref.stderr)
    );
    let coverage_resolution: serde_json::Value =
        serde_json::from_slice(&coverage_ref.stdout).expect("coverage ref json");
    assert_eq!(
        coverage_resolution["ref_kind"],
        "recorder_observation_coverage"
    );
    assert_eq!(
        coverage_resolution["artifact_trust"]["content_class"],
        "recorder_observation_coverage"
    );
}

#[test]
fn recorder_status_reads_live_status_artifact_when_available() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut ring = adc_core::RecorderRing::new("local", 4, 60_000);
    ring.push(adc_core::RecorderSample {
        time_mono_ns: 1_000,
        signals: vec![adc_core::RecorderSignalSample {
            signal_id: "memory.summary".to_string(),
            value: 42.0,
        }],
    });
    let status = adc_core::recorder_status_for(
        "local",
        Some("recorder_memory"),
        Some("armed"),
        "recording",
        ring.status(),
        adc_core::default_recorder_budget(),
    );
    adc_core::write_recorder_status_artifact(temp.path(), &status).expect("write status");

    let output = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["recorder", "status"])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run recorder status");
    assert!(
        output.status.success(),
        "recorder status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("recorder status json");
    assert_eq!(response["recorder_state"], "recording");
    assert!(response["buffer_status"]["signals"]
        .as_array()
        .expect("signals")
        .iter()
        .any(|signal| signal["signal_id"] == "memory.summary"
            && signal["recorded_samples"].as_u64().unwrap_or(0) > 0));
}
