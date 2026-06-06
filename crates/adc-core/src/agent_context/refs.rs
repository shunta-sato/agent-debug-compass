use std::{fs, path::PathBuf};

use super::{validate_segment, AgentRefResolution};
use crate::{AdcError, AdcResult, DataQuality};

pub fn resolve_agent_ref(
    artifact_root: impl AsRef<std::path::Path>,
    run_id: &str,
    ref_uri: &str,
    limit: usize,
) -> AdcResult<AgentRefResolution> {
    validate_segment(run_id, "run_id")?;
    if ref_uri.starts_with("artifact://raw/") {
        let slice = crate::read_raw_slice(artifact_root, run_id, ref_uri, limit)?;
        let text = slice.lines.join("\n");
        let artifact_trust = crate::classify_artifact_trust(
            ref_uri,
            crate::content_class_for_raw_ref(ref_uri),
            &text,
            &slice.data_quality,
        );
        return Ok(AgentRefResolution {
            schema_version: "obs.ref_resolution.v1".to_string(),
            run_id: run_id.to_string(),
            ref_uri: ref_uri.to_string(),
            ref_kind: "raw".to_string(),
            content_type: "text/plain".to_string(),
            returned_lines: slice.returned_lines,
            total_lines: slice.total_lines,
            truncated: slice.truncated,
            text,
            artifact_trust,
            data_quality: slice.data_quality,
        });
    }

    let (ref_kind, content_type, relative_path) = non_raw_ref_path(ref_uri)?;
    let path = artifact_root
        .as_ref()
        .join("runs")
        .join(run_id)
        .join(relative_path);
    let contents = fs::read_to_string(&path).map_err(|err| {
        AdcError::Artifact(format!("failed to resolve artifact ref {ref_uri}: {err}"))
    })?;
    let max_lines = limit.clamp(1, 1_000);
    let all_lines = contents.lines().map(str::to_string).collect::<Vec<_>>();
    let lines = all_lines
        .iter()
        .take(max_lines)
        .cloned()
        .collect::<Vec<_>>();
    let truncated = all_lines.len() > lines.len();
    let mut data_quality = DataQuality {
        truncated,
        clock_confidence: crate::ClockConfidence::Medium,
        ..Default::default()
    };
    if truncated {
        data_quality.notes.push(format!(
            "artifact ref returned {} of {} lines",
            lines.len(),
            all_lines.len()
        ));
    }
    let text = lines.join("\n");
    let artifact_trust = crate::classify_artifact_trust(
        ref_uri,
        crate::content_class_for_ref(ref_kind, content_type),
        &text,
        &data_quality,
    );
    Ok(AgentRefResolution {
        schema_version: "obs.ref_resolution.v1".to_string(),
        run_id: run_id.to_string(),
        ref_uri: ref_uri.to_string(),
        ref_kind: ref_kind.to_string(),
        content_type: content_type.to_string(),
        returned_lines: lines.len(),
        total_lines: all_lines.len(),
        truncated,
        text,
        artifact_trust,
        data_quality,
    })
}

pub fn resolve_global_agent_ref(
    artifact_root: impl AsRef<std::path::Path>,
    ref_uri: &str,
    limit: usize,
) -> AdcResult<AgentRefResolution> {
    if let Some((ref_kind, content_type, content_class, relative_path)) =
        recorder_ref_path(ref_uri)?
    {
        return resolve_global_path_ref(
            artifact_root.as_ref().join("recorder").join(relative_path),
            "global",
            ref_uri,
            ref_kind,
            content_type,
            content_class,
            limit,
        );
    }

    let Some(relative) = ref_uri.strip_prefix("artifact://service_investigations/") else {
        return Err(AdcError::Artifact(
            "global ref resolution supports artifact://service_investigations/... and artifact://recorder/... refs; pass run_id for run-scoped refs"
                .to_string(),
        ));
    };
    validate_relative_artifact_path(relative)?;
    let path = artifact_root
        .as_ref()
        .join("service_investigations")
        .join(relative);
    resolve_global_path_ref(
        path,
        "global",
        ref_uri,
        "service_investigation",
        "application/json",
        crate::content_class_for_ref("service_investigation", "application/json"),
        limit,
    )
}

fn resolve_global_path_ref(
    path: PathBuf,
    run_id: &str,
    ref_uri: &str,
    ref_kind: &str,
    content_type: &str,
    content_class: crate::ContentClass,
    limit: usize,
) -> AdcResult<AgentRefResolution> {
    let contents = fs::read_to_string(&path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to resolve artifact ref {} at {}: {err}",
            ref_uri,
            path.display()
        ))
    })?;
    let max_lines = limit.clamp(1, 1_000);
    let all_lines = contents.lines().map(str::to_string).collect::<Vec<_>>();
    let lines = all_lines
        .iter()
        .take(max_lines)
        .cloned()
        .collect::<Vec<_>>();
    let truncated = all_lines.len() > lines.len();
    let mut data_quality = DataQuality {
        truncated,
        clock_confidence: crate::ClockConfidence::Medium,
        ..Default::default()
    };
    if truncated {
        data_quality.notes.push(format!(
            "ref resolution returned {} of {} lines",
            lines.len(),
            all_lines.len()
        ));
    }
    let text = lines.join("\n");
    let artifact_trust =
        crate::classify_artifact_trust(ref_uri, content_class, &text, &data_quality);
    Ok(AgentRefResolution {
        schema_version: "obs.ref_resolution.v1".to_string(),
        run_id: run_id.to_string(),
        ref_uri: ref_uri.to_string(),
        ref_kind: ref_kind.to_string(),
        content_type: content_type.to_string(),
        returned_lines: lines.len(),
        total_lines: all_lines.len(),
        truncated,
        text,
        artifact_trust,
        data_quality,
    })
}

fn recorder_ref_path(
    ref_uri: &str,
) -> AdcResult<Option<(&'static str, &'static str, crate::ContentClass, PathBuf)>> {
    let Some(relative) = ref_uri.strip_prefix("artifact://recorder/") else {
        return Ok(None);
    };
    validate_relative_artifact_path(relative)?;
    let parts = relative.split('/').collect::<Vec<_>>();
    match parts.as_slice() {
        ["status.json"] => Ok(Some((
            "recorder_status",
            "application/json",
            crate::ContentClass::RecorderStatus,
            PathBuf::from("status.json"),
        ))),
        ["markers", kind @ ("pending" | "results"), file] => {
            let marker_id = file.strip_suffix(".json").ok_or_else(|| {
                AdcError::Artifact("recorder marker refs must end with .json".to_string())
            })?;
            crate::validate_recorder_file_segment(marker_id, "marker_id")?;
            let (ref_kind, content_class) = if *kind == "pending" {
                ("recorder_marker", crate::ContentClass::RecorderMarker)
            } else {
                (
                    "recorder_marker_result",
                    crate::ContentClass::RecorderMarkerResult,
                )
            };
            Ok(Some((
                ref_kind,
                "application/json",
                content_class,
                PathBuf::from("markers").join(kind).join(file),
            )))
        }
        ["trigger-decisions", file] => {
            let decision_id = file.strip_suffix(".json").ok_or_else(|| {
                AdcError::Artifact("recorder trigger decision refs must end with .json".to_string())
            })?;
            crate::validate_recorder_file_segment(decision_id, "decision_id")?;
            Ok(Some((
                "recorder_trigger_decision",
                "application/json",
                crate::ContentClass::TriggerDecision,
                PathBuf::from("trigger-decisions").join(file),
            )))
        }
        ["incidents", incident_id, artifact_name] => {
            crate::validate_recorder_file_segment(incident_id, "incident_id")?;
            let (ref_kind, content_type, content_class) = match *artifact_name {
                "incident.json" => (
                    "recorder_incident",
                    "application/json",
                    crate::ContentClass::RecorderIncident,
                ),
                "frozen_window.json" => (
                    "recorder_frozen_window",
                    "application/json",
                    crate::ContentClass::RecorderFrozenWindow,
                ),
                "coverage.json" => (
                    "recorder_observation_coverage",
                    "application/json",
                    crate::ContentClass::RecorderObservationCoverage,
                ),
                "loss_report.json" => (
                    "recorder_loss_report",
                    "application/json",
                    crate::ContentClass::LossReport,
                ),
                "samples.jsonl" => (
                    "recorder_signal_samples",
                    "application/jsonl",
                    crate::ContentClass::RecorderSignalSamples,
                ),
                "marker.json" => (
                    "recorder_marker",
                    "application/json",
                    crate::ContentClass::RecorderMarker,
                ),
                "trigger_event.json" => (
                    "recorder_trigger_event",
                    "application/json",
                    crate::ContentClass::TriggerEvent,
                ),
                "trigger_decision.json" => (
                    "recorder_trigger_decision",
                    "application/json",
                    crate::ContentClass::TriggerDecision,
                ),
                _ => {
                    return Err(AdcError::Artifact(format!(
                        "unsupported recorder incident artifact {artifact_name}"
                    )));
                }
            };
            Ok(Some((
                ref_kind,
                content_type,
                content_class,
                PathBuf::from("incidents")
                    .join(incident_id)
                    .join(artifact_name),
            )))
        }
        _ => Err(AdcError::Artifact(format!(
            "unsupported recorder artifact ref {ref_uri}"
        ))),
    }
}

pub(super) fn validate_relative_artifact_path(relative: &str) -> AdcResult<()> {
    if relative.is_empty()
        || relative.starts_with('/')
        || relative
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(AdcError::Artifact(format!(
            "invalid artifact ref path segment: {relative:?}"
        )));
    }
    Ok(())
}

fn non_raw_ref_path(ref_uri: &str) -> AdcResult<(&'static str, &'static str, PathBuf)> {
    let Some(rest) = ref_uri.strip_prefix("artifact://") else {
        return Err(AdcError::Artifact(format!(
            "unsupported artifact ref {ref_uri}; expected artifact://..."
        )));
    };
    match rest {
        "evidence_index.yaml" => Ok((
            "evidence_index",
            "application/x-yaml",
            PathBuf::from("evidence_index.yaml"),
        )),
        "manifest.json" => Ok(("manifest", "application/json", PathBuf::from("manifest.json"))),
        "overhead_report.json" => Ok((
            "overhead",
            "application/json",
            PathBuf::from("overhead_report.json"),
        )),
        "timeline.jsonl" => Ok(("timeline", "application/jsonl", PathBuf::from("timeline.jsonl"))),
        "agent_context.md" => Ok(("context", "text/markdown", PathBuf::from("agent_context.md"))),
        "agent_context.json" => Ok((
            "context",
            "application/json",
            PathBuf::from("agent_context.json"),
        )),
        _ if rest.starts_with("windows/") && rest.ends_with(".yaml") => {
            let window_file = rest.trim_start_matches("windows/");
            validate_artifact_file(window_file, "window ref")?;
            Ok((
                "window",
                "application/x-yaml",
                PathBuf::from("windows").join(window_file),
            ))
        }
        _ => Err(AdcError::Artifact(format!(
            "unsupported artifact ref {ref_uri}; supported refs include artifact://raw/..., artifact://windows/<id>.yaml, artifact://manifest.json, artifact://evidence_index.yaml"
        ))),
    }
}

fn validate_artifact_file(value: &str, label: &str) -> AdcResult<()> {
    if value.trim().is_empty()
        || value.contains('/')
        || value.contains('\\')
        || value.contains("..")
    {
        return Err(AdcError::Artifact(format!(
            "unsupported artifact ref segment for {label}"
        )));
    }
    Ok(())
}
