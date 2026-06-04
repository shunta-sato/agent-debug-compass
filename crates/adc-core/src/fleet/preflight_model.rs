use super::{bounded_text, FleetPreflightCheck, FleetPreflightTarget, FleetTargetConfig};
use crate::DataQuality;

impl FleetPreflightCheck {
    pub(super) fn ok(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: "ok".to_string(),
            detail: None,
        }
    }

    pub(super) fn failed(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: "error".to_string(),
            detail: Some(bounded_text(&detail.into())),
        }
    }
}

pub(super) fn ready_target(
    target: &FleetTargetConfig,
    checks: Vec<FleetPreflightCheck>,
) -> FleetPreflightTarget {
    FleetPreflightTarget {
        target_id: target.id.clone(),
        transport: target.transport.clone(),
        status: "ready".to_string(),
        host: target.host.clone(),
        profile_id: Some(super::profile_id_for_target(target)),
        checks,
        data_quality: DataQuality {
            clock_confidence: crate::ClockConfidence::Medium,
            ..Default::default()
        },
    }
}

pub(super) fn failed_target(
    target: &FleetTargetConfig,
    status: impl Into<String>,
    message: impl Into<String>,
    checks: Vec<FleetPreflightCheck>,
) -> FleetPreflightTarget {
    let status = status.into();
    FleetPreflightTarget {
        target_id: target.id.clone(),
        transport: target.transport.clone(),
        status: status.clone(),
        host: target.host.clone(),
        profile_id: target
            .profile
            .clone()
            .or_else(|| Some(super::profile_id_for_target(target))),
        checks,
        data_quality: DataQuality {
            missing: vec![format!("{}: {}", status, message.into())],
            clock_confidence: crate::ClockConfidence::Medium,
            ..Default::default()
        },
    }
}

pub(super) fn next_actions(status: &str) -> Vec<String> {
    match status {
        "ready" => vec![
            "run fleet snapshot or observe with a bounded duration".to_string(),
            "inspect fleet agent context for target-specific evidence refs".to_string(),
        ],
        "degraded" => vec![
            "run fleet observe for ready targets if partial evidence is useful".to_string(),
            "fix failed target preflight checks before rerunning full fleet capture".to_string(),
            "inspect data_quality.missing for unreachable, permission, binary, or artifact issues"
                .to_string(),
        ],
        _ => vec![
            "fix fleet inventory or target access before capture".to_string(),
            "rerun preflight after correcting inventory, credentials, or target setup".to_string(),
        ],
    }
}
