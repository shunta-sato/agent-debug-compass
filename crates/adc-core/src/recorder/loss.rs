use std::collections::BTreeMap;

use crate::{AdcError, AdcResult, DataQuality};

use super::{
    model::{CollectorLoss, LossReport, RecorderBudget, RecorderBufferStatus, RecorderSample},
    quality::medium_quality,
};

pub(super) fn loss_report_for_buffer_with_quality(
    window_id: &str,
    status: &RecorderBufferStatus,
    exported_samples: &[RecorderSample],
    mut loss_quality: DataQuality,
) -> LossReport {
    let mut collector_loss = Vec::new();
    let mut total_dropped = 0_u64;
    let exported_by_signal = recorded_counts_by_signal(exported_samples);
    for signal in &status.signals {
        let retained_before_freeze = signal.recorded_samples;
        let exported = exported_by_signal
            .get(&signal.signal_id)
            .copied()
            .unwrap_or(0);
        let truncated_by_freeze = retained_before_freeze.saturating_sub(exported);
        total_dropped = total_dropped.saturating_add(signal.dropped_samples);
        let mut reasons = Vec::new();
        if signal.dropped_samples > 0 {
            reasons.push("ring_capacity_drop_oldest".to_string());
        }
        if exported == 0 {
            reasons.push("collector_absent_or_no_samples".to_string());
        }
        if signal.data_quality.truncated || truncated_by_freeze > 0 {
            reasons.push("freeze_byte_budget_truncated".to_string());
            loss_quality.truncated = true;
        }
        if !signal.data_quality.missing.is_empty() {
            reasons.push("expected_signal_missing".to_string());
            loss_quality
                .missing
                .extend(signal.data_quality.missing.clone());
        }
        for missing in &signal.data_quality.missing {
            if missing.contains("battery_low policy")
                && !reasons.iter().any(|reason| reason == "battery_low_policy")
            {
                reasons.push("battery_low_policy".to_string());
            }
            if missing.contains("battery_critical policy")
                && !reasons
                    .iter()
                    .any(|reason| reason == "battery_critical_policy")
            {
                reasons.push("battery_critical_policy".to_string());
            }
        }
        collector_loss.push(CollectorLoss {
            collector_id: signal.signal_id.clone(),
            expected_samples: signal.expected_samples,
            recorded_samples: exported,
            retained_samples_before_freeze: retained_before_freeze,
            exported_samples: exported,
            truncated_samples_due_to_freeze_budget: truncated_by_freeze,
            dropped_samples: signal.dropped_samples,
            gap_ranges: signal.gap_ranges.clone(),
            collectors_degraded: if signal.degraded {
                vec![signal.signal_id.clone()]
            } else {
                Vec::new()
            },
            loss_reasons: reasons,
            loss_confidence: if signal.expected_samples.is_some() {
                "medium".to_string()
            } else {
                "unknown".to_string()
            },
        });
    }
    if total_dropped > 0 {
        loss_quality.dropped = true;
        loss_quality.drop_count = total_dropped;
        loss_quality
            .notes
            .push("memory ring dropped samples before freeze".to_string());
    }
    if collector_loss.is_empty() {
        loss_quality
            .missing
            .push("no retained recorder samples were available at freeze time".to_string());
    }
    LossReport {
        schema_version: "obs.loss_report.v1".to_string(),
        window_id: window_id.to_string(),
        collector_loss,
        data_quality: loss_quality,
    }
}

fn recorded_counts_by_signal(samples: &[RecorderSample]) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::new();
    for sample in samples {
        for signal in &sample.signals {
            *counts.entry(signal.signal_id.clone()).or_default() += 1;
        }
    }
    counts
}

pub(super) fn samples_within_freeze_budget(
    samples: &[RecorderSample],
    budget: &RecorderBudget,
) -> AdcResult<(Vec<RecorderSample>, DataQuality)> {
    let max_bytes = budget.max_freeze_bytes.min(budget.max_disk_bytes).max(1);
    let mut selected = Vec::new();
    let mut total_bytes = 0_u64;
    for sample in samples.iter().rev() {
        let line = serde_json::to_string(sample).map_err(|err| {
            AdcError::Artifact(format!("recorder sample serialization failed: {err}"))
        })?;
        let line_bytes = (line.len() + 1) as u64;
        if !selected.is_empty() && total_bytes.saturating_add(line_bytes) > max_bytes {
            break;
        }
        if selected.is_empty() && line_bytes > max_bytes {
            break;
        }
        total_bytes = total_bytes.saturating_add(line_bytes);
        selected.push(sample.clone());
    }
    selected.reverse();

    let mut data_quality = medium_quality();
    if selected.len() < samples.len() {
        data_quality.truncated = true;
        data_quality.notes.push(format!(
            "freeze samples truncated to max_freeze_bytes={} and max_disk_bytes={}",
            budget.max_freeze_bytes, budget.max_disk_bytes
        ));
    }
    if samples.is_empty() {
        data_quality
            .missing
            .push("no retained recorder samples were available at freeze time".to_string());
    }
    Ok((selected, data_quality))
}
