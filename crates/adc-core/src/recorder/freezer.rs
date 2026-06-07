use std::{collections::BTreeMap, fs, path::Path};

use crate::{AdcError, AdcResult};

use super::{
    coverage::{observation_coverage_for_freeze, CoverageBuildContext},
    io::{write_json, write_jsonl},
    logs::{blackout_report_for_log_snapshot, RecorderLogSnapshot},
    loss::{loss_report_for_buffer_with_quality, samples_within_freeze_budget},
    marker::freeze_reason_for_marker,
    model::{
        FrozenWindowPersistence, PreservationReason, RecorderBudget, RecorderFreeze,
        RecorderFrozenWindow, RecorderIncident, RecorderMarker, RecorderTriggerFreeze,
        RecorderTriggerFreezeRequest, TimeRange,
    },
    quality::medium_quality,
    ring::RecorderRing,
    trigger_artifacts::default_trigger_decision_for_freeze,
    validation::{validate_preservation_reason_name, validate_recorder_file_segment},
};

pub fn freeze_recorder_marker(
    artifact_root: impl AsRef<Path>,
    incident_id: &str,
    window_id: &str,
    marker: &RecorderMarker,
    ring: &RecorderRing,
    budget: &RecorderBudget,
) -> AdcResult<RecorderFreeze> {
    freeze_recorder_marker_with_log_snapshot(
        artifact_root,
        incident_id,
        window_id,
        marker,
        ring,
        budget,
        None,
    )
}

pub fn freeze_recorder_marker_with_log_snapshot(
    artifact_root: impl AsRef<Path>,
    incident_id: &str,
    window_id: &str,
    marker: &RecorderMarker,
    ring: &RecorderRing,
    budget: &RecorderBudget,
    log_snapshot: Option<&RecorderLogSnapshot>,
) -> AdcResult<RecorderFreeze> {
    validate_recorder_file_segment(incident_id, "incident_id")?;
    validate_recorder_file_segment(window_id, "window_id")?;
    validate_recorder_file_segment(&marker.marker_id, "marker_id")?;
    let incident_dir = artifact_root
        .as_ref()
        .join("recorder")
        .join("incidents")
        .join(incident_id);
    fs::create_dir_all(&incident_dir).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to create recorder incident directory {}: {err}",
            incident_dir.display()
        ))
    })?;

    let buffer_status = ring.status();
    let (freeze_samples, sample_quality) = samples_within_freeze_budget(ring.samples(), budget)?;
    let loss_report = loss_report_for_buffer_with_quality(
        window_id,
        &buffer_status,
        &freeze_samples,
        sample_quality,
    );
    let start = ring
        .samples()
        .first()
        .map(|sample| sample.time_mono_ns)
        .unwrap_or(marker.received_at_mono_ns);
    let end = ring
        .samples()
        .last()
        .map(|sample| sample.time_mono_ns)
        .unwrap_or(marker.received_at_mono_ns);

    let mut artifact_refs = BTreeMap::new();
    artifact_refs.insert(
        "marker".to_string(),
        format!("artifact://recorder/incidents/{incident_id}/marker.json"),
    );
    artifact_refs.insert(
        "samples".to_string(),
        format!("artifact://recorder/incidents/{incident_id}/samples.jsonl"),
    );
    artifact_refs.insert(
        "loss_report".to_string(),
        format!("artifact://recorder/incidents/{incident_id}/loss_report.json"),
    );
    artifact_refs.insert(
        "observation_coverage".to_string(),
        format!("artifact://recorder/incidents/{incident_id}/coverage.json"),
    );
    if log_snapshot.is_some() {
        artifact_refs.insert(
            "log_events".to_string(),
            format!("artifact://recorder/incidents/{incident_id}/log_events.jsonl"),
        );
        artifact_refs.insert(
            "log_source_status".to_string(),
            format!("artifact://recorder/incidents/{incident_id}/log_source_status.json"),
        );
        artifact_refs.insert(
            "blackout_report".to_string(),
            format!("artifact://recorder/incidents/{incident_id}/blackout_report.json"),
        );
    }
    let time_range = TimeRange { start, end };
    let expected_signals = ring.expected_signals();
    let coverage = observation_coverage_for_freeze(
        CoverageBuildContext {
            incident_id,
            window_id,
            target_id: &buffer_status.target_id,
            time_range: time_range.clone(),
        },
        &buffer_status,
        &expected_signals,
        &loss_report,
        budget,
    );

    let frozen_window = RecorderFrozenWindow {
        schema_version: "obs.recorder_frozen_window.v1".to_string(),
        window_id: window_id.to_string(),
        incident_id: incident_id.to_string(),
        target_id: buffer_status.target_id.clone(),
        marker_id: Some(marker.marker_id.clone()),
        freeze_reason: freeze_reason_for_marker(marker),
        preservation_reason: PreservationReason {
            kind: freeze_reason_for_marker(marker),
            name: "marker_received".to_string(),
        },
        time_range_mono_ns: time_range,
        pre_window_ms: budget.max_retention_ms,
        post_window_ms: 0,
        persistence: FrozenWindowPersistence {
            persistence_mode: "bounded_artifact_bundle".to_string(),
            survives_daemon_restart: true,
            survives_target_reboot: false,
            bounded_by: vec![
                "max_freeze_bytes".to_string(),
                "max_disk_bytes".to_string(),
                "max_frozen_incidents".to_string(),
                "retention_policy".to_string(),
                "target_reboot_survival_storage_dependent".to_string(),
                "write_durability_best_effort_no_fsync".to_string(),
            ],
        },
        artifact_refs,
        loss_report: loss_report.clone(),
        data_quality: loss_report.data_quality.clone(),
    };

    let incident = RecorderIncident {
        schema_version: "obs.recorder_incident.v1".to_string(),
        incident_id: incident_id.to_string(),
        target_id: buffer_status.target_id,
        incident_state: "frozen".to_string(),
        previous_state: "freezing".to_string(),
        marker_id: Some(marker.marker_id.clone()),
        freeze_reason: frozen_window.freeze_reason.clone(),
        frozen_window_ref: Some(format!(
            "artifact://recorder/incidents/{incident_id}/frozen_window.json"
        )),
        loss_report_ref: Some(format!(
            "artifact://recorder/incidents/{incident_id}/loss_report.json"
        )),
        created_at_mono_ns: marker.received_at_mono_ns,
        updated_at_mono_ns: marker.received_at_mono_ns,
        data_quality: frozen_window.data_quality.clone(),
    };

    write_json(&incident_dir.join("marker.json"), marker)?;
    write_jsonl(&incident_dir.join("samples.jsonl"), &freeze_samples)?;
    write_json(&incident_dir.join("loss_report.json"), &loss_report)?;
    write_json(&incident_dir.join("coverage.json"), &coverage)?;
    if let Some(snapshot) = log_snapshot {
        let mut source_status = snapshot.source_status.clone();
        source_status.artifact_refs.insert(
            "log_events".to_string(),
            format!("artifact://recorder/incidents/{incident_id}/log_events.jsonl"),
        );
        source_status.artifact_refs.insert(
            "blackout_report".to_string(),
            format!("artifact://recorder/incidents/{incident_id}/blackout_report.json"),
        );
        let snapshot_with_refs = RecorderLogSnapshot {
            source_status,
            events: snapshot.events.clone(),
        };
        let blackout_report = blackout_report_for_log_snapshot(
            &frozen_window.target_id,
            incident_id,
            window_id,
            frozen_window.time_range_mono_ns.clone(),
            &snapshot_with_refs,
        );
        write_jsonl(&incident_dir.join("log_events.jsonl"), &snapshot.events)?;
        write_json(
            &incident_dir.join("log_source_status.json"),
            &snapshot_with_refs.source_status,
        )?;
        write_json(&incident_dir.join("blackout_report.json"), &blackout_report)?;
    }
    write_json(&incident_dir.join("frozen_window.json"), &frozen_window)?;
    write_json(&incident_dir.join("incident.json"), &incident)?;

    Ok(RecorderFreeze {
        incident,
        marker: marker.clone(),
        frozen_window,
        run_dir: incident_dir,
    })
}

pub fn freeze_recorder_trigger(
    artifact_root: impl AsRef<Path>,
    incident_id: &str,
    window_id: &str,
    trigger_name: &str,
    trigger_time_mono_ns: u64,
    ring: &RecorderRing,
    budget: &RecorderBudget,
) -> AdcResult<RecorderTriggerFreeze> {
    freeze_recorder_trigger_with_decision(
        artifact_root,
        RecorderTriggerFreezeRequest {
            incident_id,
            window_id,
            trigger_name,
            trigger_time_mono_ns,
            trigger_decision: None,
        },
        ring,
        budget,
    )
}

pub fn freeze_recorder_trigger_with_decision(
    artifact_root: impl AsRef<Path>,
    request: RecorderTriggerFreezeRequest<'_>,
    ring: &RecorderRing,
    budget: &RecorderBudget,
) -> AdcResult<RecorderTriggerFreeze> {
    let incident_id = request.incident_id;
    let window_id = request.window_id;
    let trigger_name = request.trigger_name;
    let trigger_time_mono_ns = request.trigger_time_mono_ns;
    validate_recorder_file_segment(incident_id, "incident_id")?;
    validate_recorder_file_segment(window_id, "window_id")?;
    validate_preservation_reason_name(trigger_name)?;
    let incident_dir = artifact_root
        .as_ref()
        .join("recorder")
        .join("incidents")
        .join(incident_id);
    fs::create_dir_all(&incident_dir).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to create recorder incident directory {}: {err}",
            incident_dir.display()
        ))
    })?;

    let buffer_status = ring.status();
    let (freeze_samples, sample_quality) = samples_within_freeze_budget(ring.samples(), budget)?;
    let loss_report = loss_report_for_buffer_with_quality(
        window_id,
        &buffer_status,
        &freeze_samples,
        sample_quality,
    );
    let start = ring
        .samples()
        .first()
        .map(|sample| sample.time_mono_ns)
        .unwrap_or(trigger_time_mono_ns);
    let end = ring
        .samples()
        .last()
        .map(|sample| sample.time_mono_ns)
        .unwrap_or(trigger_time_mono_ns);

    let mut artifact_refs = BTreeMap::new();
    artifact_refs.insert(
        "trigger_event".to_string(),
        format!("artifact://recorder/incidents/{incident_id}/trigger_event.json"),
    );
    artifact_refs.insert(
        "trigger_decision".to_string(),
        format!("artifact://recorder/incidents/{incident_id}/trigger_decision.json"),
    );
    artifact_refs.insert(
        "samples".to_string(),
        format!("artifact://recorder/incidents/{incident_id}/samples.jsonl"),
    );
    artifact_refs.insert(
        "loss_report".to_string(),
        format!("artifact://recorder/incidents/{incident_id}/loss_report.json"),
    );
    artifact_refs.insert(
        "observation_coverage".to_string(),
        format!("artifact://recorder/incidents/{incident_id}/coverage.json"),
    );
    let time_range = TimeRange { start, end };
    let expected_signals = ring.expected_signals();
    let coverage = observation_coverage_for_freeze(
        CoverageBuildContext {
            incident_id,
            window_id,
            target_id: &buffer_status.target_id,
            time_range: time_range.clone(),
        },
        &buffer_status,
        &expected_signals,
        &loss_report,
        budget,
    );

    let frozen_window = RecorderFrozenWindow {
        schema_version: "obs.recorder_frozen_window.v1".to_string(),
        window_id: window_id.to_string(),
        incident_id: incident_id.to_string(),
        target_id: buffer_status.target_id.clone(),
        marker_id: None,
        freeze_reason: "trigger_policy".to_string(),
        preservation_reason: PreservationReason {
            kind: "trigger_policy".to_string(),
            name: trigger_name.to_string(),
        },
        time_range_mono_ns: time_range,
        pre_window_ms: budget.max_retention_ms,
        post_window_ms: 0,
        persistence: FrozenWindowPersistence {
            persistence_mode: "bounded_artifact_bundle".to_string(),
            survives_daemon_restart: true,
            survives_target_reboot: false,
            bounded_by: vec![
                "max_freeze_bytes".to_string(),
                "max_disk_bytes".to_string(),
                "max_frozen_incidents".to_string(),
                "retention_policy".to_string(),
                "target_reboot_survival_storage_dependent".to_string(),
                "write_durability_best_effort_no_fsync".to_string(),
            ],
        },
        artifact_refs,
        loss_report: loss_report.clone(),
        data_quality: loss_report.data_quality.clone(),
    };

    let incident = RecorderIncident {
        schema_version: "obs.recorder_incident.v1".to_string(),
        incident_id: incident_id.to_string(),
        target_id: buffer_status.target_id,
        incident_state: "frozen".to_string(),
        previous_state: "freezing".to_string(),
        marker_id: None,
        freeze_reason: "trigger_policy".to_string(),
        frozen_window_ref: Some(format!(
            "artifact://recorder/incidents/{incident_id}/frozen_window.json"
        )),
        loss_report_ref: Some(format!(
            "artifact://recorder/incidents/{incident_id}/loss_report.json"
        )),
        created_at_mono_ns: trigger_time_mono_ns,
        updated_at_mono_ns: trigger_time_mono_ns,
        data_quality: frozen_window.data_quality.clone(),
    };

    let coverage_ref = format!("artifact://recorder/incidents/{incident_id}/coverage.json");
    let trigger_event_ref =
        format!("artifact://recorder/incidents/{incident_id}/trigger_event.json");
    let trigger_decision_ref =
        format!("artifact://recorder/incidents/{incident_id}/trigger_decision.json");
    let decision = request.trigger_decision.unwrap_or_else(|| {
        default_trigger_decision_for_freeze(
            incident_id,
            trigger_name,
            &coverage_ref,
            &trigger_event_ref,
        )
    });
    write_json(
        &incident_dir.join("trigger_event.json"),
        &serde_json::json!({
            "schema_version": "obs.recorder_trigger_event.v1",
            "trigger_name": trigger_name,
            "trigger_time_mono_ns": trigger_time_mono_ns,
            "agent_contract": "preservation_reason_only",
            "root_cause_claim": false,
            "trigger_decision_ref": trigger_decision_ref,
            "coverage_ref": coverage_ref,
            "data_quality": medium_quality()
        }),
    )?;
    write_json(&incident_dir.join("trigger_decision.json"), &decision)?;
    write_jsonl(&incident_dir.join("samples.jsonl"), &freeze_samples)?;
    write_json(&incident_dir.join("loss_report.json"), &loss_report)?;
    write_json(&incident_dir.join("coverage.json"), &coverage)?;
    write_json(&incident_dir.join("frozen_window.json"), &frozen_window)?;
    write_json(&incident_dir.join("incident.json"), &incident)?;

    Ok(RecorderTriggerFreeze {
        incident,
        frozen_window,
        run_dir: incident_dir,
    })
}
