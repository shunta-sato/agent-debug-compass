use serde::{Deserialize, Serialize};

use crate::DataQuality;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverheadBudget {
    pub max_artifact_bytes: u64,
    pub max_events: u64,
    pub max_duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverheadSample {
    pub artifact_bytes: u64,
    pub event_count: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverheadDecision {
    pub capture_level: String,
    pub throttled: bool,
    pub dropped: bool,
    pub drop_count: u64,
    pub reasons: Vec<String>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverheadReport {
    pub budget: OverheadBudget,
    pub sample: OverheadSample,
    pub decision: OverheadDecision,
}

impl Default for OverheadBudget {
    fn default() -> Self {
        Self {
            max_artifact_bytes: 512 * 1024 * 1024,
            max_events: 100_000,
            max_duration_ms: 30_000,
        }
    }
}

pub fn evaluate_overhead(budget: &OverheadBudget, sample: &OverheadSample) -> OverheadDecision {
    let mut reasons = Vec::new();
    if sample.artifact_bytes > budget.max_artifact_bytes {
        reasons.push(format!(
            "artifact budget exceeded: {} > {} bytes",
            sample.artifact_bytes, budget.max_artifact_bytes
        ));
    }
    if sample.event_count > budget.max_events {
        reasons.push(format!(
            "event budget exceeded: {} > {} events",
            sample.event_count, budget.max_events
        ));
    }
    if sample.duration_ms > budget.max_duration_ms {
        reasons.push(format!(
            "duration budget exceeded: {} > {} ms",
            sample.duration_ms, budget.max_duration_ms
        ));
    }

    let throttled = !reasons.is_empty();
    let mut data_quality = DataQuality {
        throttled,
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };
    data_quality.notes = reasons.clone();

    OverheadDecision {
        capture_level: if throttled { "degraded" } else { "normal" }.to_string(),
        throttled,
        dropped: false,
        drop_count: 0,
        reasons,
        data_quality,
    }
}

pub fn build_overhead_report(budget: OverheadBudget, sample: OverheadSample) -> OverheadReport {
    let decision = evaluate_overhead(&budget, &sample);
    OverheadReport {
        budget,
        sample,
        decision,
    }
}
