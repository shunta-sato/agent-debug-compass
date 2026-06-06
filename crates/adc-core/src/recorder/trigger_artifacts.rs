use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    AdcError, AdcResult, ClockConfidence, DataQuality, TriggerBudgetDecision, TriggerDecision,
    TriggerDecisionOutcome, TriggerDecisionReason, TriggerKind,
};

use super::{
    io::write_json,
    model::{
        RecorderAdmissionRefusalReason, RecorderBudgetStatus, RecorderCoverageConfidence,
        RecorderCoverageState, RecorderFreezeDecision, RecorderFreezeDecisionOutcome,
        RecorderFreezeDecisionSource,
    },
    refs::recorder_trigger_decision_dir,
    validation::validate_recorder_file_segment,
};

pub fn write_recorder_trigger_decision(
    artifact_root: impl AsRef<Path>,
    decision: &TriggerDecision,
) -> AdcResult<PathBuf> {
    validate_recorder_file_segment(&decision.decision_id, "decision_id")?;
    let decision_dir = recorder_trigger_decision_dir(artifact_root);
    fs::create_dir_all(&decision_dir).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to create recorder trigger decision directory {}: {err}",
            decision_dir.display()
        ))
    })?;
    let path = decision_dir.join(format!("{}.json", decision.decision_id));
    write_json(&path, decision)?;
    Ok(path)
}

pub fn recorder_freeze_decision_for_refused_trigger(
    budget_status: RecorderBudgetStatus,
) -> RecorderFreezeDecision {
    let reason = budget_status
        .admission_refusal_reason
        .unwrap_or(RecorderAdmissionRefusalReason::IncidentInventoryUnreliable);
    let mut data_quality = budget_status.data_quality.clone();
    data_quality.throttled = true;
    if !data_quality
        .notes
        .iter()
        .any(|note| note == "trigger recorder freeze was skipped due to budget admission")
    {
        data_quality
            .notes
            .push("trigger recorder freeze was skipped due to budget admission".to_string());
    }
    RecorderFreezeDecision {
        schema_version: "obs.recorder_freeze_decision.v1".to_string(),
        source: RecorderFreezeDecisionSource::TriggerPolicy,
        decision: RecorderFreezeDecisionOutcome::Refused,
        reason,
        budget_status,
        data_quality,
    }
}

pub(super) fn default_trigger_decision_for_freeze(
    incident_id: &str,
    trigger_name: &str,
    coverage_ref: &str,
    trigger_event_ref: &str,
) -> TriggerDecision {
    TriggerDecision {
        schema_version: "obs.trigger_decision.v1".to_string(),
        decision_id: format!("TD-{incident_id}"),
        policy_id: "legacy-profile-trigger-policy".to_string(),
        rule_id: format!("{trigger_name}_v1"),
        trigger_name: trigger_name.to_string(),
        trigger_kind: TriggerKind::BurstCount,
        decision: TriggerDecisionOutcome::Fired,
        decision_reason: TriggerDecisionReason::BurstCountCrossed,
        signal_id: "kmsg.message".to_string(),
        coverage_signal_id: "kmsg.cursor".to_string(),
        observed_value: null_option_f64(),
        threshold: null_option_f64(),
        coverage_state: RecorderCoverageState::Unknown,
        coverage_confidence: RecorderCoverageConfidence::Unknown,
        coverage_ref: Some(coverage_ref.to_string()),
        budget_decision: TriggerBudgetDecision::Accepted,
        budget_status_ref: None,
        incident_id: Some(incident_id.to_string()),
        trigger_event_ref: Some(trigger_event_ref.to_string()),
        root_cause_claim: false,
        data_quality: DataQuality {
            clock_confidence: ClockConfidence::Medium,
            notes: vec![
                "default trigger decision was synthesized by trigger freeze helper".to_string(),
            ],
            ..Default::default()
        },
    }
}

fn null_option_f64() -> Option<f64> {
    None
}
