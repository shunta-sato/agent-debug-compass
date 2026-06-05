use std::time::Duration;

use adc_core::{
    arm_profile, default_recorder_budget, freeze_recorder_marker, freeze_recorder_trigger,
    marker_at_received_time, read_recorder_status_artifact, recorder_ring_capacity_for_budget,
    recorder_status_for, run_service_for, write_pending_recorder_marker, RecorderRing,
    RecorderSample, RecorderSampleRateGovernor, RecorderSignalSample, RecorderStatusWriteGovernor,
};

#[test]
fn recorder_ring_drops_oldest_and_reports_loss_semantics() {
    let mut ring = RecorderRing::new("local", 2, 1_000);

    ring.push(sample(1_000, "cpu.summary", 10.0));
    ring.push(sample(2_000, "cpu.summary", 20.0));
    ring.push(sample(3_000, "cpu.summary", 30.0));

    let status = ring.status();
    assert_eq!(status.schema_version, "obs.recorder_buffer_status.v1");
    assert_eq!(status.storage_mode, "memory_ring");
    assert!(status.volatile);
    assert!(!status.survives_daemon_restart);
    assert_eq!(status.signals[0].recorded_samples, 2);
    assert_eq!(status.signals[0].dropped_samples, 1);
    assert!(status.data_quality.dropped);
}

#[test]
fn recorder_ring_capacity_respects_retention_rate_and_memory_budget() {
    let mut budget = default_recorder_budget();
    budget.max_samples_per_second = 16;
    budget.max_retention_ms = 60_000;
    budget.max_memory_bytes = 8 * 1024 * 1024;
    assert_eq!(recorder_ring_capacity_for_budget(&budget), 960);

    budget.max_memory_bytes = 256;
    assert_eq!(recorder_ring_capacity_for_budget(&budget), 1);
}

#[test]
fn status_write_governor_throttles_heartbeat_writes() {
    let mut governor = RecorderStatusWriteGovernor::new(5_000);

    assert!(governor.should_write(1_000_000_000, false));
    assert!(!governor.should_write(1_010_000_000, false));
    assert!(!governor.should_write(5_999_999_999, false));
    assert!(governor.should_write(6_000_000_000, false));
    assert!(governor.should_write(6_010_000_000, true));
}

#[test]
fn sample_rate_governor_throttles_fast_profile_samples() {
    let mut governor = RecorderSampleRateGovernor::new(16);

    assert!(governor.should_record(1_000_000_000));
    assert!(!governor.should_record(1_010_000_000));
    assert!(!governor.should_record(1_061_000_000));
    assert!(governor.should_record(1_063_000_000));
}

#[test]
fn recorder_ring_evicts_by_retention_window() {
    let mut ring = RecorderRing::new("local", 100, 1);

    ring.push(sample(1_000_000, "cpu.summary", 10.0));
    ring.push(sample(3_000_000, "cpu.summary", 30.0));

    let status = ring.status();
    assert_eq!(status.signals[0].recorded_samples, 1);
    assert_eq!(status.signals[0].dropped_samples, 1);
    assert_eq!(status.current_retained_range_mono_ns.start, Some(3_000_000));
}

#[test]
fn recorder_ring_reports_dropped_only_signal_without_counting_it_recorded() {
    let mut ring = RecorderRing::new("local", 1, 60_000);

    ring.push(sample(1_000, "cpu.summary", 10.0));
    ring.push(sample(2_000, "memory.summary", 20.0));

    let status = ring.status();
    let cpu = status
        .signals
        .iter()
        .find(|signal| signal.signal_id == "cpu.summary")
        .expect("cpu signal status");
    assert_eq!(cpu.recorded_samples, 0);
    assert_eq!(cpu.dropped_samples, 1);
}

#[test]
fn recorder_ring_reports_expected_signal_absence_as_missing_evidence() {
    let mut ring = RecorderRing::with_expected_signals(
        "local",
        4,
        60_000,
        ["thermal.zone".to_string(), "memory.summary".to_string()],
    );
    ring.push(sample(1_000, "memory.summary", 42.0));

    let status = ring.status();
    let thermal = status
        .signals
        .iter()
        .find(|signal| signal.signal_id == "thermal.zone")
        .expect("thermal expected signal status");
    assert_eq!(thermal.recorded_samples, 0);
    assert!(thermal.degraded);
    assert!(thermal
        .data_quality
        .missing
        .iter()
        .any(|missing| missing.contains("thermal.zone")));
}

#[test]
fn recorder_status_exposes_budget_overhead_and_volatility() {
    let mut ring = RecorderRing::new("local", 4, 60_000);
    ring.push(sample(1_000, "adc.self_overhead", 1.0));

    let status = recorder_status_for(
        "local",
        Some("camera_inference_degradation"),
        Some("armed"),
        "recording",
        ring.status(),
        default_recorder_budget(),
    );

    assert_eq!(status.schema_version, "obs.recorder_status.v1");
    assert_eq!(status.recorder_state, adc_core::RecorderState::Recording);
    assert!(status.armed);
    assert!(status.storage.volatile);
    assert_eq!(status.budget.schema_version, "obs.recorder_budget.v1");
    assert_eq!(status.overhead.schema_version, "obs.recorder_overhead.v1");
}

#[test]
fn service_run_status_reports_scoped_recorder_overhead_bytes() {
    let temp = tempfile::tempdir().expect("tempdir");
    arm_profile(temp.path(), "pi5_basic").expect("arm profile");
    let marker = marker_at_received_time(
        "marker-overhead",
        "operator",
        "camera frame drop observed around now",
        1_000,
    );
    write_pending_recorder_marker(temp.path(), &marker).expect("write marker");

    run_service_for(
        temp.path(),
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("profiles"),
        Duration::from_millis(120),
    )
    .expect("service run");

    let status = read_recorder_status_artifact(temp.path()).expect("live recorder status");
    let overhead = serde_json::to_value(&status.overhead).expect("overhead json");
    assert_eq!(overhead["overhead_scope"], "service_run");
    assert!(overhead["status_write_bytes"].as_u64().unwrap_or(0) > 0);
    assert!(overhead["frozen_artifact_bytes"].as_u64().unwrap_or(0) > 0);
    assert!(overhead["samples_jsonl_bytes"].as_u64().unwrap_or(0) > 0);
    assert!(overhead["incident_count"].as_u64().unwrap_or(0) >= 1);
    assert!(
        overhead["estimated_memory_ring_bytes"]
            .as_u64()
            .unwrap_or(0)
            > 0
    );
    assert!(overhead["cpu_percent"].is_null());
    assert!(overhead["memory_bytes"].is_null());
    assert!(overhead["data_quality"]["missing"]
        .as_array()
        .expect("missing overhead evidence")
        .iter()
        .any(|missing| missing
            .as_str()
            .is_some_and(|value| value.contains("recorder CPU and memory overhead"))));
    let notes = overhead["data_quality"]["notes"]
        .as_array()
        .expect("overhead notes");
    assert!(notes.iter().any(|note| note.as_str().is_some_and(|value| {
        value.contains("artifact_bytes is a write-path retained-size estimate")
    })));
    assert!(notes.iter().any(|note| note.as_str().is_some_and(|value| {
        value.contains("status_write_bytes excludes the current status artifact write")
    })));
}

#[test]
fn marker_freeze_materializes_bounded_incident_bundle_with_loss_report() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut ring = RecorderRing::new("local", 2, 60_000);
    ring.push(sample(1_000, "cpu.summary", 10.0));
    ring.push(sample(2_000, "cpu.summary", 20.0));
    ring.push(sample(3_000, "cpu.summary", 30.0));

    let marker = marker_at_received_time(
        "marker-001",
        "operator",
        "camera frame drop observed around now",
        3_000,
    );
    let freeze = freeze_recorder_marker(
        temp.path(),
        "INC-001",
        "win-001",
        &marker,
        &ring,
        &default_recorder_budget(),
    )
    .expect("freeze marker");

    assert_eq!(freeze.incident.schema_version, "obs.recorder_incident.v1");
    assert_eq!(
        freeze.frozen_window.schema_version,
        "obs.recorder_frozen_window.v1"
    );
    assert_eq!(
        freeze.frozen_window.persistence.persistence_mode,
        "bounded_artifact_bundle"
    );
    assert!(freeze.frozen_window.persistence.survives_daemon_restart);
    assert!(!freeze.frozen_window.persistence.survives_target_reboot);
    assert!(freeze
        .frozen_window
        .persistence
        .bounded_by
        .iter()
        .any(|policy| policy.contains("best_effort_no_fsync")));
    assert!(freeze.frozen_window.artifact_refs.contains_key("samples"));
    assert!(freeze.frozen_window.loss_report.data_quality.dropped);
    assert!(temp
        .path()
        .join("recorder/incidents/INC-001/frozen_window.json")
        .is_file());
}

#[test]
fn marker_freeze_rejects_unsafe_incident_path_segments() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut ring = RecorderRing::new("local", 2, 60_000);
    ring.push(sample(1_000, "cpu.summary", 10.0));
    let marker = marker_at_received_time(
        "marker-001",
        "operator",
        "camera frame drop observed around now",
        1_000,
    );

    let err = freeze_recorder_marker(
        temp.path(),
        "../INC-001",
        "win-001",
        &marker,
        &ring,
        &default_recorder_budget(),
    )
    .expect_err("unsafe incident id must fail");
    assert!(err
        .to_string()
        .contains("single safe recorder file segment"));
}

#[test]
fn marker_freeze_truncates_samples_when_freeze_byte_budget_is_exceeded() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut ring = RecorderRing::new("local", 16, 60_000);
    for index in 0..16 {
        ring.push(sample(index, "cpu.summary", index as f64));
    }
    let marker = marker_at_received_time("marker-byte-budget", "operator", "frame drop", 16);
    let mut budget = default_recorder_budget();
    budget.max_freeze_bytes = 120;

    let freeze = freeze_recorder_marker(
        temp.path(),
        "INC-byte-budget",
        "win-byte-budget",
        &marker,
        &ring,
        &budget,
    )
    .expect("freeze marker");

    assert!(freeze.frozen_window.data_quality.truncated);
    let cpu_loss = freeze
        .frozen_window
        .loss_report
        .collector_loss
        .iter()
        .find(|loss| loss.collector_id == "cpu.summary")
        .expect("cpu loss");
    let exported_lines = std::fs::read_to_string(
        temp.path()
            .join("recorder/incidents/INC-byte-budget/samples.jsonl"),
    )
    .expect("samples")
    .lines()
    .count() as u64;
    assert_eq!(cpu_loss.recorded_samples, exported_lines);
    assert_eq!(cpu_loss.exported_samples, exported_lines);
    assert!(cpu_loss.retained_samples_before_freeze > cpu_loss.exported_samples);
    assert!(cpu_loss.truncated_samples_due_to_freeze_budget > 0);
    assert!(freeze
        .frozen_window
        .loss_report
        .data_quality
        .notes
        .iter()
        .any(|note| note.contains("max_freeze_bytes")));
    let samples = std::fs::read_to_string(
        temp.path()
            .join("recorder/incidents/INC-byte-budget/samples.jsonl"),
    )
    .expect("samples");
    assert!(samples.len() as u64 <= budget.max_freeze_bytes);
}

#[test]
fn trigger_freeze_rejects_root_cause_like_trigger_names() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut ring = RecorderRing::new("local", 2, 60_000);
    ring.push(sample(1_000, "cpu.summary", 10.0));

    let err = freeze_recorder_trigger(
        temp.path(),
        "INC-TRIGGER",
        "win-trigger",
        "cpu_root_cause_detected",
        1_000,
        &ring,
        &default_recorder_budget(),
    )
    .expect_err("root-cause-like trigger name must fail");

    assert!(err.to_string().contains("symptom/event oriented"));
}

fn sample(time_mono_ns: u64, signal_id: &str, value: f64) -> RecorderSample {
    RecorderSample {
        time_mono_ns,
        signals: vec![RecorderSignalSample {
            signal_id: signal_id.to_string(),
            value,
        }],
    }
}
