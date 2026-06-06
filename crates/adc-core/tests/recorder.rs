use std::time::Duration;

use adc_core::{
    arm_profile, default_recorder_budget, freeze_recorder_marker, freeze_recorder_trigger,
    marker_at_received_time, read_recorder_status_artifact, recorder_budget_for_power_mode,
    recorder_incident_budget_status, recorder_power_snapshot_from_sysfs,
    recorder_resource_status_for_overhead, recorder_ring_capacity_for_budget, recorder_status_for,
    run_service_for, write_pending_recorder_marker, RecorderAdmissionDecision,
    RecorderAdmissionRefusalReason, RecorderBatteryState, RecorderPowerMode, RecorderPowerSource,
    RecorderRing, RecorderSample, RecorderSampleRateGovernor, RecorderSignalSample,
    RecorderStatusWriteGovernor, RetainedArtifactBytesEstimateScope,
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
fn recorder_power_snapshot_reports_unknown_when_power_supply_is_absent() {
    let temp = tempfile::tempdir().expect("tempdir");
    let snapshot = recorder_power_snapshot_from_sysfs(temp.path().join("missing-power-supply"));

    assert_eq!(snapshot.power_source, RecorderPowerSource::Unknown);
    assert_eq!(snapshot.battery_state, RecorderBatteryState::Unknown);
    assert!(snapshot.battery_percent.is_none());
    assert!(snapshot
        .data_quality
        .missing
        .iter()
        .any(|missing| missing.contains("battery state is unavailable")));
}

#[test]
fn battery_low_policy_reduces_recorder_budget_without_mutating_default_budget() {
    let default_budget = default_recorder_budget();
    let battery_low_budget =
        recorder_budget_for_power_mode(&default_budget, RecorderPowerMode::BatteryLow);

    assert_eq!(default_budget.max_samples_per_second, 16);
    assert_eq!(battery_low_budget.max_samples_per_second, 1);
    assert_eq!(battery_low_budget.max_retention_ms, 10_000);
    assert!(battery_low_budget.data_quality.throttled);
}

#[test]
fn resource_status_separates_continuous_ring_writes_from_status_and_freeze_writes() {
    let mut ring = RecorderRing::new("local", 4, 60_000);
    ring.push(sample(1_000, "memory.summary", 42.0));
    let budget = default_recorder_budget();
    let status = recorder_status_for(
        "local",
        Some("recorder_memory"),
        Some("armed"),
        "recording",
        ring.status(),
        budget.clone(),
    );
    let resource_status =
        recorder_resource_status_for_overhead("local", &budget, &status.overhead, None);

    assert_eq!(resource_status.continuous_ring_disk_write_bytes, 0);
    assert_eq!(
        resource_status.status_write_bytes,
        status.overhead.status_write_bytes
    );
    assert_eq!(
        resource_status.frozen_artifact_write_bytes,
        status.overhead.frozen_artifact_bytes
    );
    assert_eq!(resource_status.network_upload_bytes, 0);
    assert!(resource_status
        .data_quality
        .notes
        .iter()
        .any(|note| note.contains("memory-backed")));
}

#[test]
fn recorder_budget_status_counts_current_run_incidents_without_double_counting() {
    let temp = tempfile::tempdir().expect("tempdir");
    materialize_recorder_incident(temp.path(), "INC-current-run-001");

    let status = recorder_incident_budget_status(temp.path(), &default_recorder_budget(), 1);

    assert_eq!(status.existing_frozen_incidents, 1);
    assert_eq!(status.frozen_incidents_this_run, 1);
    assert_eq!(
        status.remaining_frozen_incidents,
        default_recorder_budget().max_frozen_incidents - 1
    );
    assert!(status.current_run_included_in_existing);
    assert_eq!(status.admission_decision, RecorderAdmissionDecision::Accept);
}

#[test]
fn marker_freeze_writes_observation_coverage_for_expected_absent_signals() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut ring = RecorderRing::with_expected_signals(
        "local",
        8,
        60_000,
        ["memory.summary".to_string(), "thermal.zone".to_string()],
    );
    ring.push(sample(1_000, "memory.summary", 42.0));
    let marker = marker_at_received_time(
        "marker-coverage",
        "operator",
        "camera frame drop observed around now",
        1_000,
    );

    freeze_recorder_marker(
        temp.path(),
        "INC-coverage",
        "win-coverage",
        &marker,
        &ring,
        &default_recorder_budget(),
    )
    .expect("freeze recorder incident");

    let coverage_path = temp
        .path()
        .join("recorder/incidents/INC-coverage/coverage.json");
    let coverage: serde_json::Value =
        serde_json::from_slice(&std::fs::read(coverage_path).expect("coverage artifact"))
            .expect("coverage json");
    assert_eq!(
        coverage["schema_version"],
        "obs.recorder_observation_coverage.v1"
    );
    assert_eq!(
        coverage["loss_report_ref"],
        "artifact://recorder/incidents/INC-coverage/loss_report.json"
    );
    let signals = coverage["signals"].as_array().expect("coverage signals");
    assert!(signals.iter().any(|signal| {
        signal["signal_id"] == "memory.summary"
            && signal["coverage_state"] == "covered"
            && signal["exported_samples"].as_u64() == Some(1)
    }));
    assert!(signals.iter().any(|signal| {
        signal["signal_id"] == "thermal.zone"
            && signal["coverage_state"] == "missing"
            && signal["data_quality"]["missing"]
                .as_array()
                .expect("thermal missing")
                .iter()
                .any(|item| item.as_str().unwrap_or("").contains("thermal.zone"))
    }));
    assert!(
        signals
            .iter()
            .all(|signal| signal["coverage_state"] != "not_expected"),
        "coverage should not enumerate unrelated global signals as not_expected"
    );
    assert!(
        signals
            .iter()
            .all(|signal| signal["signal_id"] != "cpu.summary"),
        "coverage should include expected profile signals only by default"
    );
}

#[test]
fn marker_freeze_coverage_reports_effective_interval_and_unavailable_signals() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut budget = default_recorder_budget();
    budget.max_samples_per_second = 16;
    let memory = adc_core::recorder_expected_signal_for_id("memory.summary", 10);
    let mut thermal = adc_core::recorder_expected_signal_for_id("thermal.zone", 10);
    thermal.capability_status = adc_core::CapabilityStatus::Unavailable;
    thermal
        .data_quality
        .missing
        .push("thermal.zone expected but linux.sysfs.thermal_zone unavailable".to_string());
    let mut ring = RecorderRing::with_expected_signal_model("local", 8, 60_000, [memory, thermal]);
    ring.push(sample(1_000, "memory.summary", 42.0));
    let marker = marker_at_received_time(
        "marker-effective-interval",
        "operator",
        "camera frame drop observed around now",
        1_000,
    );

    freeze_recorder_marker(
        temp.path(),
        "INC-effective-interval",
        "win-effective-interval",
        &marker,
        &ring,
        &budget,
    )
    .expect("freeze recorder incident");

    let coverage: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            temp.path()
                .join("recorder/incidents/INC-effective-interval/coverage.json"),
        )
        .expect("coverage artifact"),
    )
    .expect("coverage json");
    let signals = coverage["signals"].as_array().expect("coverage signals");
    let memory_coverage = signals
        .iter()
        .find(|signal| signal["signal_id"] == "memory.summary")
        .expect("memory coverage");
    assert_eq!(memory_coverage["configured_interval_ms"], 10);
    assert_eq!(memory_coverage["effective_interval_ms"], 63);
    assert_eq!(
        memory_coverage["expected_samples_basis"],
        "budgeted_recorder_interval"
    );
    let thermal_coverage = signals
        .iter()
        .find(|signal| signal["signal_id"] == "thermal.zone")
        .expect("thermal coverage");
    assert_eq!(thermal_coverage["coverage_state"], "unavailable");
    assert_eq!(thermal_coverage["capability_status"], "unavailable");
    assert!(thermal_coverage["loss_reasons"]
        .as_array()
        .expect("loss reasons")
        .iter()
        .any(|reason| reason == "required_capability_unavailable"));
}

#[test]
fn recorder_budget_status_truncates_inventory_after_decision_limit() {
    let temp = tempfile::tempdir().expect("tempdir");
    let budget = default_recorder_budget();
    for index in 0..=budget.max_frozen_incidents {
        materialize_recorder_incident(temp.path(), &format!("INC-budget-{index}"));
    }

    let status = recorder_incident_budget_status(temp.path(), &budget, 0);

    assert_eq!(
        status.existing_frozen_incidents,
        budget.max_frozen_incidents + 1
    );
    assert_eq!(status.remaining_frozen_incidents, 0);
    assert!(status.inventory_truncated);
    assert_eq!(
        status.retained_artifact_bytes_estimate_scope,
        RetainedArtifactBytesEstimateScope::CountedIncidentsOnly
    );
    assert_eq!(status.admission_decision, RecorderAdmissionDecision::Refuse);
    assert_eq!(
        status.admission_refusal_reason,
        Some(RecorderAdmissionRefusalReason::MaxFrozenIncidentsExceeded)
    );
}

#[test]
fn recorder_budget_status_fails_closed_for_malformed_incident_inventory() {
    let temp = tempfile::tempdir().expect("tempdir");
    let incident_dir = temp.path().join("recorder/incidents/INC-malformed");
    std::fs::create_dir_all(&incident_dir).expect("incident dir");
    std::fs::write(incident_dir.join("incident.json"), "{not-json").expect("malformed incident");

    let status = recorder_incident_budget_status(temp.path(), &default_recorder_budget(), 0);

    assert_eq!(
        status.admission_decision,
        RecorderAdmissionDecision::UnknownFailClosed
    );
    assert_eq!(
        status.admission_refusal_reason,
        Some(RecorderAdmissionRefusalReason::IncidentInventoryUnreliable)
    );
    assert_eq!(status.malformed_entry_count, 1);
    assert!(status.data_quality.throttled);
}

#[cfg(unix)]
#[test]
fn recorder_budget_status_does_not_follow_symlinked_incident_entries() {
    let temp = tempfile::tempdir().expect("tempdir");
    let incidents_dir = temp.path().join("recorder/incidents");
    std::fs::create_dir_all(&incidents_dir).expect("incidents dir");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&outside).expect("outside dir");
    std::os::unix::fs::symlink(&outside, incidents_dir.join("INC-symlink"))
        .expect("symlink incident dir");

    let status = recorder_incident_budget_status(temp.path(), &default_recorder_budget(), 0);

    assert_eq!(status.existing_frozen_incidents, 0);
    assert_eq!(
        status.admission_decision,
        RecorderAdmissionDecision::UnknownFailClosed
    );
    assert_eq!(status.malformed_entry_count, 1);
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

fn materialize_recorder_incident(artifact_root: &std::path::Path, incident_id: &str) {
    let mut ring = RecorderRing::new("local", 2, 60_000);
    ring.push(sample(1_000, "memory.summary", 42.0));
    let marker = marker_at_received_time(
        format!("marker-{incident_id}"),
        "operator",
        "existing retained incident",
        1_000,
    );
    freeze_recorder_marker(
        artifact_root,
        incident_id,
        &format!("win-{incident_id}"),
        &marker,
        &ring,
        &default_recorder_budget(),
    )
    .expect("materialize recorder incident");
}
