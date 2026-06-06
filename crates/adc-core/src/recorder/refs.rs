use std::{path::Path, path::PathBuf};

use crate::{AdcError, AdcResult};

use super::validation::validate_recorder_file_segment;

pub fn recorder_pending_marker_dir(artifact_root: impl AsRef<Path>) -> PathBuf {
    artifact_root.as_ref().join("recorder/markers/pending")
}

pub fn recorder_marker_result_dir(artifact_root: impl AsRef<Path>) -> PathBuf {
    artifact_root.as_ref().join("recorder/markers/results")
}

pub fn recorder_trigger_decision_dir(artifact_root: impl AsRef<Path>) -> PathBuf {
    artifact_root.as_ref().join("recorder/trigger-decisions")
}

pub fn recorder_pending_marker_ref(marker_id: &str) -> AdcResult<String> {
    validate_recorder_file_segment(marker_id, "marker_id")?;
    Ok(format!(
        "artifact://recorder/markers/pending/{marker_id}.json"
    ))
}

pub fn recorder_trigger_decision_ref(decision_id: &str) -> AdcResult<String> {
    validate_recorder_file_segment(decision_id, "decision_id")?;
    Ok(format!(
        "artifact://recorder/trigger-decisions/{decision_id}.json"
    ))
}

pub fn recorder_incident_artifact_ref(incident_id: &str, artifact_name: &str) -> AdcResult<String> {
    validate_recorder_file_segment(incident_id, "incident_id")?;
    match artifact_name {
        "incident.json"
        | "frozen_window.json"
        | "coverage.json"
        | "loss_report.json"
        | "samples.jsonl"
        | "marker.json"
        | "trigger_event.json"
        | "trigger_decision.json" => Ok(format!(
            "artifact://recorder/incidents/{incident_id}/{artifact_name}"
        )),
        _ => Err(AdcError::Artifact(format!(
            "unsupported recorder incident artifact {artifact_name}"
        ))),
    }
}
