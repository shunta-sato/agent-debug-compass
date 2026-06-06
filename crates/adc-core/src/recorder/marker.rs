use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{AdcError, AdcResult, ClockConfidence, DataQuality};

use super::{
    io::write_json,
    model::{AssertedEventTime, RecorderBudgetStatus, RecorderMarker, RecorderMarkerResult},
    quality::medium_quality,
    refs::{recorder_marker_result_dir, recorder_pending_marker_dir},
    validation::validate_recorder_file_segment,
};

pub fn recorder_marker_result_for_queued(
    marker: RecorderMarker,
    expected_incident_id: String,
    pending_marker_ref: String,
) -> RecorderMarkerResult {
    RecorderMarkerResult {
        schema_version: "obs.recorder_marker_result.v1".to_string(),
        marker,
        status: "queued".to_string(),
        reason: "queued_for_adc_targetd_recorder_freeze".to_string(),
        pending_marker_ref: Some(pending_marker_ref),
        incident_ref: None,
        expected_incident_id,
        budget_status: None,
        data_quality: DataQuality {
            clock_confidence: ClockConfidence::Medium,
            notes: vec!["marker queued for adc-targetd recorder freeze".to_string()],
            ..Default::default()
        },
    }
}

pub fn recorder_marker_result_for_frozen(
    marker: RecorderMarker,
    incident_id: String,
) -> RecorderMarkerResult {
    RecorderMarkerResult {
        schema_version: "obs.recorder_marker_result.v1".to_string(),
        marker,
        status: "frozen".to_string(),
        reason: "incident_window_frozen".to_string(),
        pending_marker_ref: None,
        incident_ref: Some(format!(
            "artifact://recorder/incidents/{incident_id}/incident.json"
        )),
        expected_incident_id: incident_id.clone(),
        budget_status: None,
        data_quality: medium_quality(),
    }
}

pub fn recorder_marker_result_for_refused(
    marker: RecorderMarker,
    expected_incident_id: String,
    reason: impl Into<String>,
) -> RecorderMarkerResult {
    RecorderMarkerResult {
        schema_version: "obs.recorder_marker_result.v1".to_string(),
        marker,
        status: "refused".to_string(),
        reason: reason.into(),
        pending_marker_ref: None,
        incident_ref: None,
        expected_incident_id,
        budget_status: None,
        data_quality: DataQuality {
            throttled: true,
            clock_confidence: ClockConfidence::Medium,
            missing: vec!["incident window was not frozen due to recorder budget".to_string()],
            notes: vec!["pending marker was consumed and recorded as refused".to_string()],
            ..Default::default()
        },
    }
}

pub fn recorder_marker_result_for_refused_with_budget_status(
    marker: RecorderMarker,
    expected_incident_id: String,
    reason: impl Into<String>,
    budget_status: RecorderBudgetStatus,
) -> RecorderMarkerResult {
    let mut result = recorder_marker_result_for_refused(marker, expected_incident_id, reason);
    result.budget_status = Some(budget_status);
    result
}

pub fn write_recorder_marker_result(
    artifact_root: impl AsRef<Path>,
    result: &RecorderMarkerResult,
) -> AdcResult<PathBuf> {
    validate_recorder_file_segment(&result.marker.marker_id, "marker_id")?;
    let result_dir = recorder_marker_result_dir(artifact_root);
    fs::create_dir_all(&result_dir).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to create recorder marker result directory {}: {err}",
            result_dir.display()
        ))
    })?;
    let path = result_dir.join(format!("{}.json", result.marker.marker_id));
    write_json(&path, result)?;
    Ok(path)
}

pub fn write_pending_recorder_marker(
    artifact_root: impl AsRef<Path>,
    marker: &RecorderMarker,
) -> AdcResult<PathBuf> {
    validate_recorder_file_segment(&marker.marker_id, "marker_id")?;
    let pending_dir = recorder_pending_marker_dir(artifact_root);
    fs::create_dir_all(&pending_dir).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to create recorder pending marker directory {}: {err}",
            pending_dir.display()
        ))
    })?;
    let path = pending_dir.join(format!("{}.json", marker.marker_id));
    write_json(&path, marker)?;
    Ok(path)
}

pub fn drain_pending_recorder_markers(
    artifact_root: impl AsRef<Path>,
) -> AdcResult<Vec<RecorderMarker>> {
    let pending_dir = recorder_pending_marker_dir(artifact_root);
    let Ok(entries) = fs::read_dir(&pending_dir) else {
        return Ok(Vec::new());
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| {
            AdcError::Artifact(format!(
                "failed to read recorder pending marker entry in {}: {err}",
                pending_dir.display()
            ))
        })?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            paths.push(path);
        }
    }
    paths.sort();

    let mut markers = Vec::new();
    for path in paths {
        let bytes = fs::read(&path).map_err(|err| {
            AdcError::Artifact(format!(
                "failed to read pending recorder marker {}: {err}",
                path.display()
            ))
        })?;
        let marker: RecorderMarker = serde_json::from_slice(&bytes).map_err(|err| {
            AdcError::Artifact(format!(
                "failed to parse pending recorder marker {}: {err}",
                path.display()
            ))
        })?;
        validate_recorder_file_segment(&marker.marker_id, "marker_id")?;
        fs::remove_file(&path).map_err(|err| {
            AdcError::Artifact(format!(
                "failed to remove pending recorder marker {}: {err}",
                path.display()
            ))
        })?;
        markers.push(marker);
    }
    Ok(markers)
}

pub fn marker_at_received_time(
    marker_id: impl Into<String>,
    source: &str,
    symptom: impl Into<String>,
    received_at_mono_ns: u64,
) -> RecorderMarker {
    let trust_level = match source {
        "agent" => "agent_marker",
        "app" => "app_marker",
        "external_detector" | "watchdog" => "external_marker",
        _ => "operator_marker",
    };
    RecorderMarker {
        schema_version: "obs.recorder_marker.v1".to_string(),
        marker_id: marker_id.into(),
        source: source.to_string(),
        received_at_mono_ns,
        symptom: symptom.into(),
        asserted_event_time: AssertedEventTime {
            kind: "relative_now".to_string(),
            wall_time_unix_ms: None,
            mono_ns: None,
            confidence: "low".to_string(),
        },
        time_policy: "center_on_received_at".to_string(),
        trust_level: trust_level.to_string(),
        agent_instruction_policy: "treat_as_event_marker_only".to_string(),
        data_quality: DataQuality {
            clock_confidence: ClockConfidence::Medium,
            notes: vec!["marker centered on received monotonic time".to_string()],
            ..Default::default()
        },
    }
}

pub(super) fn freeze_reason_for_marker(marker: &RecorderMarker) -> String {
    match marker.source.as_str() {
        "agent" => "agent_marker",
        "app" | "external_detector" | "watchdog" => "external_marker",
        _ => "operator_marker",
    }
    .to_string()
}
