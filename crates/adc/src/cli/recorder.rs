use std::{fs, path::Path};

use super::args::{optional_flag, required_flag};

pub(super) fn run(args: &[String]) -> Result<(), String> {
    match args {
        [cmd] if cmd == "status" => recorder_status(),
        [cmd, rest @ ..] if cmd == "mark" => recorder_mark(rest),
        [cmd] if cmd == "incidents" => recorder_incidents(),
        [cmd, subcmd, rest @ ..] if cmd == "incident" && subcmd == "get" => {
            recorder_incident_get(rest)
        }
        [cmd, rest @ ..] if cmd == "export-dataset" => recorder_export_dataset(rest),
        _ => Err("usage: adc recorder status".to_string()),
    }
}

fn recorder_status() -> Result<(), String> {
    let artifact_root = adc_core::snapshot::default_artifact_root();
    if let Ok(status) = adc_core::read_recorder_status_artifact(&artifact_root) {
        serde_json::to_writer_pretty(std::io::stdout(), &status)
            .map_err(|err| format!("failed to serialize recorder status: {err}"))?;
        println!();
        return Ok(());
    }
    let active_profile = adc_core::read_state(&artifact_root)
        .ok()
        .and_then(|state| state.active_profile);
    let ring = adc_core::RecorderRing::new("local", 1, 60_000);
    let current = if active_profile.is_some() {
        "armed"
    } else {
        "disabled"
    };
    let mut status = adc_core::recorder_status_for(
        "local",
        active_profile.as_deref(),
        None,
        current,
        ring.status(),
        adc_core::default_recorder_budget(),
    );
    status.data_quality.missing.push(
        "live recorder status artifact not found; adc-targetd may not be running".to_string(),
    );
    status
        .buffer_status
        .data_quality
        .missing
        .push("live recorder buffer status is unavailable".to_string());
    serde_json::to_writer_pretty(std::io::stdout(), &status)
        .map_err(|err| format!("failed to serialize recorder status: {err}"))?;
    println!();
    Ok(())
}

fn recorder_mark(args: &[String]) -> Result<(), String> {
    let symptom = required_flag(args, "--symptom")?;
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let received_at = monotonic_now_ns();
    let marker_id = optional_flag(args, "--marker-id")
        .map(str::to_string)
        .unwrap_or_else(|| format!("marker-{received_at}"));
    let incident_id = format!("INC-{marker_id}");
    let marker = adc_core::marker_at_received_time(&marker_id, "operator", symptom, received_at);
    adc_core::write_pending_recorder_marker(&artifact_root, &marker)
        .map_err(|err| err.to_string())?;
    let pending_marker_ref =
        adc_core::recorder_pending_marker_ref(&marker_id).map_err(|err| err.to_string())?;
    let result =
        adc_core::recorder_marker_result_for_queued(marker, incident_id, pending_marker_ref);
    adc_core::write_recorder_marker_result(&artifact_root, &result)
        .map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &result)
        .map_err(|err| format!("failed to serialize recorder marker freeze: {err}"))?;
    println!();
    Ok(())
}

fn recorder_incidents() -> Result<(), String> {
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let incidents = list_recorder_incident_ids(&artifact_root);
    let response = serde_json::json!({
        "schema_version": "obs.recorder_incident_list.v1",
        "incidents": incidents,
        "data_quality": {
            "dropped": false,
            "drop_count": 0,
            "throttled": false,
            "missing": [],
            "truncated": false,
            "clock_confidence": "medium",
            "notes": []
        }
    });
    serde_json::to_writer_pretty(std::io::stdout(), &response)
        .map_err(|err| format!("failed to serialize recorder incidents: {err}"))?;
    println!();
    Ok(())
}

fn recorder_export_dataset(args: &[String]) -> Result<(), String> {
    let selector = required_flag(args, "--selector")?;
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let incident_ids = list_recorder_incident_ids(&artifact_root);
    let mut windows = Vec::new();
    for incident_id in incident_ids {
        let incident_dir = artifact_root.join("recorder/incidents").join(&incident_id);
        let incident: serde_json::Value = read_json_file(&incident_dir.join("incident.json"))?;
        let window_ref = incident["frozen_window_ref"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let loss_report_ref = incident["loss_report_ref"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let coverage_ref = adc_core::recorder_incident_artifact_ref(&incident_id, "coverage.json")
            .map_err(|err| err.to_string())?;
        windows.push(serde_json::json!({
            "incident_id": incident_id,
            "window_type": "positive_incident",
            "window_ref": window_ref,
            "loss_report_ref": loss_report_ref,
            "coverage_ref": coverage_ref,
            "artifact_trust_preserved": true,
            "data_quality_preserved": true,
            "label_state": "weak_label"
        }));
    }
    let workload_profile = selector
        .strip_prefix("profile=")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("unknown");
    let selector_applied = false;
    let data_quality = if windows.is_empty() {
        serde_json::json!({
            "dropped": false,
            "drop_count": 0,
            "throttled": false,
            "missing": ["no recorder incidents matched selector"],
            "truncated": false,
            "clock_confidence": "medium",
            "notes": []
        })
    } else {
        serde_json::json!({
            "dropped": false,
            "drop_count": 0,
            "throttled": false,
            "missing": ["selector filtering is not implemented; all local incidents were included"],
            "truncated": false,
            "clock_confidence": "medium",
            "notes": ["dataset manifest preserves refs, loss reports, and data-quality metadata"]
        })
    };
    let response = serde_json::json!({
        "schema_version": "obs.dataset_manifest.v1",
        "dataset_id": format!("DS-{}", monotonic_now_ns()),
        "selector": selector,
        "selector_applied": selector_applied,
        "dataset_types": ["benchmark", "regression"],
        "generated_at_mono_ns": monotonic_now_ns(),
        "recorder_policy_version": "default-memory-ring-budget",
        "hardware_profile": "linux-edge",
        "workload_profile": workload_profile,
        "windows": windows,
        "redaction_policy": "artifact trust and existing redaction metadata are preserved",
        "sharing_policy": "local benchmark/regression export only; review before external sharing",
        "data_quality": data_quality
    });
    serde_json::to_writer_pretty(std::io::stdout(), &response)
        .map_err(|err| format!("failed to serialize recorder dataset manifest: {err}"))?;
    println!();
    Ok(())
}

fn recorder_incident_get(args: &[String]) -> Result<(), String> {
    let incident_id = required_flag(args, "--incident-id")?;
    adc_core::validate_recorder_file_segment(incident_id, "incident_id")
        .map_err(|err| err.to_string())?;
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let incident_dir = artifact_root.join("recorder/incidents").join(incident_id);
    let marker: Option<serde_json::Value> =
        read_optional_json_file(&incident_dir.join("marker.json"))?;
    let trigger_event: Option<serde_json::Value> =
        read_optional_json_file(&incident_dir.join("trigger_event.json"))?;
    let trigger_decision: Option<serde_json::Value> =
        read_optional_json_file(&incident_dir.join("trigger_decision.json"))?;
    let incident: serde_json::Value = read_json_file(&incident_dir.join("incident.json"))?;
    let frozen_window: serde_json::Value =
        read_json_file(&incident_dir.join("frozen_window.json"))?;
    let incident_ref = adc_core::recorder_incident_artifact_ref(incident_id, "incident.json")
        .map_err(|err| err.to_string())?;
    let frozen_window_ref =
        adc_core::recorder_incident_artifact_ref(incident_id, "frozen_window.json")
            .map_err(|err| err.to_string())?;
    let loss_report_ref = adc_core::recorder_incident_artifact_ref(incident_id, "loss_report.json")
        .map_err(|err| err.to_string())?;
    let coverage_ref = adc_core::recorder_incident_artifact_ref(incident_id, "coverage.json")
        .map_err(|err| err.to_string())?;
    let trigger_decision_ref = if trigger_decision.is_some() {
        Some(
            adc_core::recorder_incident_artifact_ref(incident_id, "trigger_decision.json")
                .map_err(|err| err.to_string())?,
        )
    } else {
        None
    };
    let samples_ref = adc_core::recorder_incident_artifact_ref(incident_id, "samples.jsonl")
        .map_err(|err| err.to_string())?;
    let response = serde_json::json!({
        "schema_version": "obs.recorder_incident_resolution.v1",
        "incident_id": incident_id,
        "incident_ref": incident_ref,
        "frozen_window_ref": frozen_window_ref,
        "loss_report_ref": loss_report_ref,
        "coverage_ref": coverage_ref,
        "trigger_decision_ref": trigger_decision_ref,
        "samples_ref": samples_ref,
        "marker": marker,
        "trigger_event": trigger_event,
        "trigger_decision": trigger_decision,
        "incident": incident,
        "frozen_window": frozen_window,
        "data_quality": {
            "dropped": false,
            "drop_count": 0,
            "throttled": false,
            "missing": [],
            "truncated": false,
            "clock_confidence": "medium",
            "notes": []
        }
    });
    serde_json::to_writer_pretty(std::io::stdout(), &response)
        .map_err(|err| format!("failed to serialize recorder incident: {err}"))?;
    println!();
    Ok(())
}

fn list_recorder_incident_ids(artifact_root: &Path) -> Vec<String> {
    let root = artifact_root.join("recorder/incidents");
    let mut incidents = Vec::new();
    if let Ok(entries) = fs::read_dir(&root) {
        for entry in entries.flatten() {
            let path = entry.path();
            let incident_id = entry.file_name().to_string_lossy().to_string();
            if path.join("incident.json").is_file()
                && adc_core::validate_recorder_file_segment(&incident_id, "incident_id").is_ok()
            {
                incidents.push(incident_id);
            }
        }
    }
    incidents.sort();
    incidents
}

fn read_optional_json_file(path: &Path) -> Result<Option<serde_json::Value>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    read_json_file(path).map(Some)
}

fn read_json_file(path: &Path) -> Result<serde_json::Value, String> {
    let bytes =
        fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|err| format!("failed to parse {}: {err}", path.display()))
}

fn monotonic_now_ns() -> u64 {
    fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|contents| contents.split_whitespace().next().map(str::to_string))
        .and_then(|seconds| seconds.parse::<f64>().ok())
        .map(|seconds| (seconds * 1_000_000_000.0) as u64)
        .unwrap_or(0)
}
