use std::{fs, path::Path};

use crate::{AdcError, AdcResult};

pub fn latest_run_id(artifact_root: impl AsRef<Path>) -> AdcResult<Option<String>> {
    latest_manifest_backed_id(
        artifact_root.as_ref().join("runs"),
        "manifest.json",
        "runs directory",
    )
}

pub fn latest_fleet_run_id(artifact_root: impl AsRef<Path>) -> AdcResult<Option<String>> {
    latest_manifest_backed_id(
        artifact_root.as_ref().join("fleet_runs"),
        "fleet_evidence.yaml",
        "fleet runs directory",
    )
}

fn latest_manifest_backed_id(
    root: impl AsRef<Path>,
    marker_file: &str,
    directory_label: &str,
) -> AdcResult<Option<String>> {
    let root = root.as_ref();
    if !root.exists() {
        return Ok(None);
    }

    let mut candidates = Vec::new();
    for entry in fs::read_dir(root).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read {directory_label} {}: {err}",
            root.display()
        ))
    })? {
        let entry = entry.map_err(|err| {
            AdcError::Artifact(format!("failed to read {directory_label} entry: {err}"))
        })?;
        let marker_path = entry.path().join(marker_file);
        if !marker_path.is_file() {
            continue;
        }
        let Some(id) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let modified = marker_path
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok();
        candidates.push((modified, id));
    }

    candidates.sort_by(|(left_time, left_id), (right_time, right_id)| {
        left_time
            .cmp(right_time)
            .then_with(|| left_id.cmp(right_id))
    });
    Ok(candidates.into_iter().last().map(|(_, id)| id))
}
