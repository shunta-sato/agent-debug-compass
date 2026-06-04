use adc_core::{
    default_recorder_budget, freeze_recorder_marker, freeze_recorder_trigger,
    marker_at_received_time, recorder_status_for, RecorderRing, RecorderSample,
    RecorderSignalSample,
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
fn recorder_status_exposes_budget_overhead_and_volatility() {
    let mut ring = RecorderRing::new("local", 4, 60_000);
    ring.push(sample(1_000, "adc.self_overhead", 1.0));

    let status = recorder_status_for(
        "local",
        Some("camera_inference_degradation"),
        "armed",
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
    assert!(freeze.frozen_window.artifact_refs.contains_key("samples"));
    assert!(freeze.frozen_window.loss_report.data_quality.dropped);
    assert!(temp
        .path()
        .join("recorder/incidents/INC-001/frozen_window.json")
        .is_file());
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
