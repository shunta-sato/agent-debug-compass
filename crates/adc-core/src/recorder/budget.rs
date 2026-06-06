use std::{fs, path::Path};

use super::{
    model::{
        RecorderAdmissionDecision, RecorderAdmissionRefusalReason, RecorderBudget,
        RecorderBudgetStatus, RecorderIncidentCountScope, RetainedArtifactBytesEstimateScope,
    },
    quality::medium_quality,
    validation::validate_recorder_file_segment,
};

pub fn recorder_ring_capacity_for_budget(budget: &RecorderBudget) -> usize {
    const ESTIMATED_SAMPLE_BYTES: u64 = 256;
    let retention_seconds = budget.max_retention_ms.saturating_add(999) / 1000;
    let capacity_by_rate = budget
        .max_samples_per_second
        .saturating_mul(retention_seconds.max(1));
    let capacity_by_memory = budget.max_memory_bytes / ESTIMATED_SAMPLE_BYTES;
    capacity_by_rate.min(capacity_by_memory).max(1) as usize
}

pub fn recorder_default_budget_status(
    budget: &RecorderBudget,
    frozen_incidents_this_run: u64,
) -> RecorderBudgetStatus {
    recorder_budget_status_from_counts(
        budget,
        frozen_incidents_this_run,
        RecorderBudgetInventorySummary::default(),
    )
}

pub fn recorder_incident_budget_status(
    artifact_root: impl AsRef<Path>,
    budget: &RecorderBudget,
    frozen_incidents_this_run: u64,
) -> RecorderBudgetStatus {
    let incidents_dir = artifact_root.as_ref().join("recorder/incidents");
    let Ok(root_metadata) = fs::symlink_metadata(&incidents_dir) else {
        return recorder_default_budget_status(budget, frozen_incidents_this_run);
    };
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return recorder_budget_status_from_counts(
            budget,
            frozen_incidents_this_run,
            RecorderBudgetInventorySummary {
                retained_artifact_bytes_estimate_scope: RetainedArtifactBytesEstimateScope::Unknown,
                malformed_entry_count: 1,
                inventory_refusal_reason: Some(
                    RecorderAdmissionRefusalReason::IncidentInventoryUnreliable,
                ),
                ..Default::default()
            },
        );
    }

    let entries = match fs::read_dir(&incidents_dir) {
        Ok(entries) => entries,
        Err(_) => {
            return recorder_budget_status_from_counts(
                budget,
                frozen_incidents_this_run,
                RecorderBudgetInventorySummary {
                    retained_artifact_bytes_estimate_scope:
                        RetainedArtifactBytesEstimateScope::Unknown,
                    unreadable_entry_count: 1,
                    inventory_refusal_reason: Some(
                        RecorderAdmissionRefusalReason::IncidentInventoryUnreadable,
                    ),
                    ..Default::default()
                },
            )
        }
    };

    let mut valid_count = 0_u64;
    let mut retained_bytes = 0_u64;
    let mut malformed_count = 0_u64;
    let mut unreadable_count = 0_u64;
    let mut inventory_truncated = false;
    let decision_limit = budget.max_frozen_incidents.saturating_add(1);

    for entry in entries {
        let Ok(entry) = entry else {
            unreadable_count = unreadable_count.saturating_add(1);
            continue;
        };
        let file_name = entry.file_name();
        let Some(incident_id) = file_name.to_str() else {
            malformed_count = malformed_count.saturating_add(1);
            continue;
        };
        if validate_recorder_file_segment(incident_id, "incident_id").is_err() {
            malformed_count = malformed_count.saturating_add(1);
            continue;
        }
        let incident_dir = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&incident_dir) else {
            unreadable_count = unreadable_count.saturating_add(1);
            continue;
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            malformed_count = malformed_count.saturating_add(1);
            continue;
        }

        let incident_json = incident_dir.join("incident.json");
        match validate_regular_incident_json(&incident_json) {
            Ok(()) => {
                valid_count = valid_count.saturating_add(1);
                retained_bytes = retained_bytes
                    .saturating_add(recorder_incident_known_artifact_bytes(&incident_dir));
                if valid_count >= decision_limit {
                    inventory_truncated = true;
                    break;
                }
            }
            Err(IncidentInventoryEntryError::Malformed) => {
                malformed_count = malformed_count.saturating_add(1);
            }
            Err(IncidentInventoryEntryError::Unreadable) => {
                unreadable_count = unreadable_count.saturating_add(1);
            }
        }
    }

    let estimate_scope = if inventory_truncated {
        RetainedArtifactBytesEstimateScope::CountedIncidentsOnly
    } else {
        RetainedArtifactBytesEstimateScope::FullInventory
    };
    let refusal_reason = if malformed_count > 0 || unreadable_count > 0 {
        Some(RecorderAdmissionRefusalReason::IncidentInventoryUnreliable)
    } else {
        None
    };

    recorder_budget_status_from_counts(
        budget,
        frozen_incidents_this_run,
        RecorderBudgetInventorySummary {
            existing_frozen_incidents: valid_count,
            retained_artifact_bytes_estimate: retained_bytes,
            retained_artifact_bytes_estimate_scope: estimate_scope,
            inventory_truncated,
            malformed_entry_count: malformed_count,
            unreadable_entry_count: unreadable_count,
            inventory_refusal_reason: refusal_reason,
        },
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecorderBudgetInventorySummary {
    existing_frozen_incidents: u64,
    retained_artifact_bytes_estimate: u64,
    retained_artifact_bytes_estimate_scope: RetainedArtifactBytesEstimateScope,
    inventory_truncated: bool,
    malformed_entry_count: u64,
    unreadable_entry_count: u64,
    inventory_refusal_reason: Option<RecorderAdmissionRefusalReason>,
}

impl Default for RecorderBudgetInventorySummary {
    fn default() -> Self {
        Self {
            existing_frozen_incidents: 0,
            retained_artifact_bytes_estimate: 0,
            retained_artifact_bytes_estimate_scope:
                RetainedArtifactBytesEstimateScope::FullInventory,
            inventory_truncated: false,
            malformed_entry_count: 0,
            unreadable_entry_count: 0,
            inventory_refusal_reason: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IncidentInventoryEntryError {
    Malformed,
    Unreadable,
}

fn validate_regular_incident_json(path: &Path) -> Result<(), IncidentInventoryEntryError> {
    let metadata = fs::symlink_metadata(path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            IncidentInventoryEntryError::Malformed
        } else {
            IncidentInventoryEntryError::Unreadable
        }
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(IncidentInventoryEntryError::Malformed);
    }
    let bytes = fs::read(path).map_err(|_| IncidentInventoryEntryError::Unreadable)?;
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| IncidentInventoryEntryError::Malformed)?;
    if json.get("schema_version").and_then(|value| value.as_str())
        != Some("obs.recorder_incident.v1")
    {
        return Err(IncidentInventoryEntryError::Malformed);
    }
    Ok(())
}

fn recorder_incident_known_artifact_bytes(incident_dir: &Path) -> u64 {
    [
        "incident.json",
        "frozen_window.json",
        "loss_report.json",
        "samples.jsonl",
        "marker.json",
        "trigger_event.json",
    ]
    .iter()
    .filter_map(|file_name| {
        let path = incident_dir.join(file_name);
        let metadata = fs::symlink_metadata(path).ok()?;
        if metadata.is_file() && !metadata.file_type().is_symlink() {
            Some(metadata.len())
        } else {
            None
        }
    })
    .sum()
}

fn recorder_budget_status_from_counts(
    budget: &RecorderBudget,
    frozen_incidents_this_run: u64,
    inventory: RecorderBudgetInventorySummary,
) -> RecorderBudgetStatus {
    let remaining = budget
        .max_frozen_incidents
        .saturating_sub(inventory.existing_frozen_incidents);
    let (admission_decision, admission_refusal_reason) =
        if let Some(reason) = inventory.inventory_refusal_reason {
            (RecorderAdmissionDecision::UnknownFailClosed, Some(reason))
        } else if inventory.existing_frozen_incidents >= budget.max_frozen_incidents {
            (
                RecorderAdmissionDecision::Refuse,
                Some(RecorderAdmissionRefusalReason::MaxFrozenIncidentsExceeded),
            )
        } else {
            (RecorderAdmissionDecision::Accept, None)
        };

    let mut data_quality = medium_quality();
    if inventory.inventory_truncated {
        data_quality.truncated = true;
        data_quality.notes.push(format!(
            "recorder incident inventory stopped after max_frozen_incidents+1={}",
            budget.max_frozen_incidents.saturating_add(1)
        ));
    }
    if inventory.retained_artifact_bytes_estimate_scope
        == RetainedArtifactBytesEstimateScope::CountedIncidentsOnly
    {
        data_quality.notes.push(
            "retained_artifact_bytes_estimate is partial for counted incidents only".to_string(),
        );
    }
    if inventory.malformed_entry_count > 0 || inventory.unreadable_entry_count > 0 {
        data_quality.throttled = true;
        data_quality.missing.push(
            "recorder incident inventory is unreliable; freeze admission failed closed".to_string(),
        );
        data_quality.notes.push(format!(
            "malformed incident entries: {}; unreadable incident entries: {}",
            inventory.malformed_entry_count, inventory.unreadable_entry_count
        ));
    }
    data_quality.notes.push(
        "existing_frozen_incidents includes current-run materialized incidents; frozen_incidents_this_run is informational"
            .to_string(),
    );

    RecorderBudgetStatus {
        schema_version: "obs.recorder_budget_status.v1".to_string(),
        budget_id: budget.budget_id.clone(),
        incident_count_scope: RecorderIncidentCountScope::ArtifactRoot,
        admission_decision,
        admission_refusal_reason,
        max_frozen_incidents: budget.max_frozen_incidents,
        existing_frozen_incidents: inventory.existing_frozen_incidents,
        frozen_incidents_this_run,
        remaining_frozen_incidents: remaining,
        current_run_included_in_existing: true,
        max_disk_bytes: budget.max_disk_bytes,
        retained_artifact_bytes_estimate: inventory.retained_artifact_bytes_estimate,
        retained_artifact_bytes_estimate_scope: inventory.retained_artifact_bytes_estimate_scope,
        inventory_truncated: inventory.inventory_truncated,
        malformed_entry_count: inventory.malformed_entry_count,
        unreadable_entry_count: inventory.unreadable_entry_count,
        data_quality,
    }
}

pub fn default_recorder_budget() -> RecorderBudget {
    RecorderBudget {
        schema_version: "obs.recorder_budget.v1".to_string(),
        budget_id: "default-memory-ring-budget".to_string(),
        max_memory_bytes: 8 * 1024 * 1024,
        max_samples_per_second: 16,
        max_collectors: 8,
        max_frozen_incidents: 4,
        max_freeze_bytes: 1024 * 1024,
        max_post_window_ms: 30_000,
        max_retention_ms: 60_000,
        max_status_write_interval_ms: 5_000,
        max_ref_lines: 1000,
        max_cpu_percent: 5.0,
        max_disk_bytes: 4 * 1024 * 1024,
        collector_priority: vec![
            "adc.self_overhead".to_string(),
            "cpu.summary".to_string(),
            "memory.summary".to_string(),
        ],
        degradation_policies: vec![
            "drop_oldest".to_string(),
            "downsample".to_string(),
            "partial_freeze".to_string(),
        ],
        data_quality: medium_quality(),
    }
}
