use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{AdcError, AdcResult};

use super::{
    budget::recorder_default_budget_status,
    io::write_json,
    model::{
        RecorderBudget, RecorderBufferStatus, RecorderOverhead, RecorderState, RecorderStatus,
        RecorderStatusInput, RecorderStorageStatus,
    },
    overhead::default_recorder_overhead,
    quality::data_quality_for_drop_count,
    resource::recorder_resource_status_for_overhead,
};

pub fn recorder_status_path(artifact_root: impl AsRef<Path>) -> PathBuf {
    artifact_root.as_ref().join("recorder/status.json")
}

pub fn write_recorder_status_artifact(
    artifact_root: impl AsRef<Path>,
    status: &RecorderStatus,
) -> AdcResult<PathBuf> {
    let path = recorder_status_path(artifact_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AdcError::Artifact(format!(
                "failed to create recorder status directory {}: {err}",
                parent.display()
            ))
        })?;
    }
    write_json(&path, status)?;
    Ok(path)
}

pub fn read_recorder_status_artifact(artifact_root: impl AsRef<Path>) -> AdcResult<RecorderStatus> {
    let path = recorder_status_path(artifact_root);
    let bytes = fs::read(&path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read recorder status {}: {err}",
            path.display()
        ))
    })?;
    serde_json::from_slice(&bytes).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to parse recorder status {}: {err}",
            path.display()
        ))
    })
}

pub fn recorder_status_for(
    target_id: impl Into<String>,
    active_profile: Option<&str>,
    previous_state: Option<&str>,
    recorder_state: &str,
    buffer_status: RecorderBufferStatus,
    budget: RecorderBudget,
) -> RecorderStatus {
    let target_id = target_id.into();
    let dropped = buffer_status.data_quality.drop_count;
    let overhead = default_recorder_overhead(&target_id, &buffer_status, dropped);
    recorder_status_from_input(RecorderStatusInput {
        target_id,
        active_profile: active_profile.map(str::to_string),
        previous_state: previous_state.map(str::to_string),
        recorder_state: recorder_state.to_string(),
        buffer_status,
        budget_status: recorder_default_budget_status(&budget, 0),
        budget,
        overhead,
        resource_status: None,
    })
}

pub fn recorder_status_for_with_overhead(
    target_id: impl Into<String>,
    active_profile: Option<&str>,
    previous_state: Option<&str>,
    recorder_state: &str,
    buffer_status: RecorderBufferStatus,
    budget: RecorderBudget,
    overhead: RecorderOverhead,
) -> RecorderStatus {
    let budget_status = recorder_default_budget_status(&budget, 0);
    recorder_status_from_input(RecorderStatusInput {
        target_id: target_id.into(),
        active_profile: active_profile.map(str::to_string),
        previous_state: previous_state.map(str::to_string),
        recorder_state: recorder_state.to_string(),
        buffer_status,
        budget,
        budget_status,
        overhead,
        resource_status: None,
    })
}

pub fn recorder_status_from_input(input: RecorderStatusInput) -> RecorderStatus {
    let dropped = input.buffer_status.data_quality.drop_count;
    let resource_status = input.resource_status.unwrap_or_else(|| {
        recorder_resource_status_for_overhead(
            input.target_id.clone(),
            &input.budget,
            &input.overhead,
            None,
        )
    });
    RecorderStatus {
        schema_version: "obs.recorder_status.v1".to_string(),
        target_id: input.target_id,
        recorder_state: RecorderState::parse(&input.recorder_state),
        previous_state: input.previous_state.as_deref().map(RecorderState::parse),
        active_profile: input.active_profile.clone(),
        armed: input.active_profile.is_some(),
        storage: RecorderStorageStatus {
            storage_mode: "memory_ring".to_string(),
            volatile: true,
            survives_daemon_restart: false,
            survives_target_reboot: false,
            survives_power_loss: false,
        },
        buffer_status: input.buffer_status,
        budget: input.budget,
        budget_status: input.budget_status,
        overhead: input.overhead,
        resource_status,
        data_quality: data_quality_for_drop_count(dropped),
    }
}
