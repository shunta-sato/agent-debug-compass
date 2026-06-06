use std::collections::BTreeMap;

use crate::DataQuality;

use super::{
    model::{
        CollectorLoss, ExpectedSamplesBasis, LossReport, RecorderBudget, RecorderBufferStatus,
        RecorderCoverageConfidence, RecorderCoverageState, RecorderCoverageSummary,
        RecorderExpectedSignal, RecorderLayer, RecorderObservationCoverage, RecorderSignalCoverage,
        RecorderSignalStatus, TimeRange,
    },
    quality::medium_quality,
};

pub(super) struct CoverageBuildContext<'a> {
    pub(super) incident_id: &'a str,
    pub(super) window_id: &'a str,
    pub(super) target_id: &'a str,
    pub(super) time_range: TimeRange,
}

pub(super) fn observation_coverage_for_freeze(
    context: CoverageBuildContext<'_>,
    status: &RecorderBufferStatus,
    expected_signal_model: &[RecorderExpectedSignal],
    loss_report: &LossReport,
    budget: &RecorderBudget,
) -> RecorderObservationCoverage {
    let loss_report_ref = format!(
        "artifact://recorder/incidents/{}/loss_report.json",
        context.incident_id
    );
    let loss_by_signal = loss_report
        .collector_loss
        .iter()
        .map(|loss| (loss.collector_id.clone(), loss))
        .collect::<BTreeMap<_, _>>();
    let expected_by_signal = expected_signal_model
        .iter()
        .map(|signal| (signal.signal_id.clone(), signal))
        .collect::<BTreeMap<_, _>>();
    let mut expected_signals = Vec::new();
    let mut signals = Vec::new();
    let mut coverage_quality = medium_quality();

    for signal_status in &status.signals {
        let expected_signal = expected_signal_for_status(
            signal_status,
            &context.time_range,
            budget,
            expected_by_signal.get(&signal_status.signal_id).copied(),
        );
        let Some(loss) = loss_by_signal.get(&signal_status.signal_id).copied() else {
            coverage_quality.missing.push(format!(
                "loss_report has no collector_loss entry for expected signal {}",
                signal_status.signal_id
            ));
            continue;
        };
        let coverage_state = coverage_state_for_loss(loss, expected_signal.capability_status);
        let mut signal_quality = signal_status.data_quality.clone();
        if matches!(coverage_state, RecorderCoverageState::Missing) {
            let note = format!(
                "{} expected by active recorder profile but has no exported samples",
                signal_status.signal_id
            );
            if !signal_quality
                .missing
                .iter()
                .any(|existing| existing == &note)
            {
                signal_quality.missing.push(note.clone());
            }
            if !coverage_quality
                .missing
                .iter()
                .any(|existing| existing == &note)
            {
                coverage_quality.missing.push(note);
            }
        }
        if loss.truncated_samples_due_to_freeze_budget > 0 {
            signal_quality.truncated = true;
            coverage_quality.truncated = true;
        }
        if signal_status.dropped_samples > 0 {
            signal_quality.dropped = true;
            signal_quality.drop_count = signal_status.dropped_samples;
            coverage_quality.dropped = true;
            coverage_quality.drop_count = coverage_quality
                .drop_count
                .saturating_add(signal_status.dropped_samples);
        }
        if signal_status.data_quality.throttled {
            signal_quality.throttled = true;
            coverage_quality.throttled = true;
        }
        let mut loss_reasons = loss.loss_reasons.clone();
        if matches!(coverage_state, RecorderCoverageState::Unavailable)
            && !loss_reasons
                .iter()
                .any(|reason| reason == "required_capability_unavailable")
        {
            loss_reasons.push("required_capability_unavailable".to_string());
        }

        let coverage_confidence = if expected_signal.expected_samples.is_some() {
            RecorderCoverageConfidence::Medium
        } else {
            RecorderCoverageConfidence::Unknown
        };
        signals.push(RecorderSignalCoverage {
            signal_id: signal_status.signal_id.clone(),
            expected: true,
            coverage_state,
            coverage_confidence,
            configured_interval_ms: expected_signal.configured_interval_ms,
            effective_interval_ms: expected_signal.effective_interval_ms,
            expected_samples_configured: expected_samples_for_interval(
                &context.time_range,
                expected_signal.configured_interval_ms,
            ),
            expected_samples_budgeted: expected_signal.expected_samples,
            expected_samples: expected_signal.expected_samples,
            expected_samples_basis: ExpectedSamplesBasis::BudgetedRecorderInterval,
            retained_samples_before_freeze: loss.retained_samples_before_freeze,
            exported_samples: loss.exported_samples,
            dropped_samples: loss.dropped_samples,
            truncated_samples_due_to_freeze_budget: loss.truncated_samples_due_to_freeze_budget,
            loss_report_ref: loss_report_ref.clone(),
            loss_collector_id: loss.collector_id.clone(),
            loss_reasons,
            capability_status: expected_signal.capability_status,
            data_quality: signal_quality,
        });
        expected_signals.push(expected_signal);
    }

    let summary = coverage_summary(&signals);
    RecorderObservationCoverage {
        schema_version: "obs.recorder_observation_coverage.v1".to_string(),
        target_id: context.target_id.to_string(),
        incident_id: context.incident_id.to_string(),
        window_id: context.window_id.to_string(),
        time_range_mono_ns: context.time_range,
        coverage_scope: "frozen_incident".to_string(),
        expected_signals,
        signals,
        summary,
        loss_report_ref,
        data_quality: coverage_quality,
    }
}

fn expected_signal_for_status(
    signal_status: &RecorderSignalStatus,
    time_range: &TimeRange,
    budget: &RecorderBudget,
    model: Option<&RecorderExpectedSignal>,
) -> RecorderExpectedSignal {
    let mut expected = model
        .cloned()
        .unwrap_or_else(|| recorder_expected_signal_for_id(&signal_status.signal_id, 1000));
    let configured_interval_ms = model
        .map(|signal| signal.configured_interval_ms)
        .unwrap_or(signal_status.configured_interval_ms)
        .max(1);
    let effective_interval_ms = configured_interval_ms.max(effective_recorder_interval_ms(
        budget.max_samples_per_second,
    ));
    let expected_samples = expected_samples_for_interval(time_range, effective_interval_ms);
    let mut data_quality = merge_data_quality(&expected.data_quality, &signal_status.data_quality);
    data_quality.notes.push(format!(
        "effective_interval_ms uses recorder sample-rate budget max_samples_per_second={}",
        budget.max_samples_per_second
    ));
    if expected_samples.is_none() {
        data_quality
            .missing
            .push("expected sample count could not be derived".to_string());
    }
    expected.configured_interval_ms = configured_interval_ms;
    expected.effective_interval_ms = effective_interval_ms;
    expected.expected_samples = expected_samples;
    expected.data_quality = data_quality;
    expected
}

fn merge_data_quality(left: &DataQuality, right: &DataQuality) -> DataQuality {
    let mut merged = left.clone();
    merged.dropped |= right.dropped;
    merged.drop_count = merged.drop_count.saturating_add(right.drop_count);
    merged.throttled |= right.throttled;
    merged.truncated |= right.truncated;
    merged.missing.extend(right.missing.clone());
    merged.notes.extend(right.notes.clone());
    merged
}

fn expected_signal_metadata(signal_id: &str) -> (String, RecorderLayer, Option<&'static str>) {
    match signal_id {
        "cpu.summary" => (
            "cpu".to_string(),
            RecorderLayer::Os,
            Some("linux.procfs.cpu"),
        ),
        "memory.summary" => (
            "memory".to_string(),
            RecorderLayer::Os,
            Some("linux.procfs.meminfo"),
        ),
        "network.counters" => (
            "network".to_string(),
            RecorderLayer::Network,
            Some("linux.procfs.net_dev"),
        ),
        "kmsg.cursor" => (
            "kmsg".to_string(),
            RecorderLayer::Kernel,
            Some("linux.kmsg_or_fixture"),
        ),
        "thermal.zone" => (
            "thermal".to_string(),
            RecorderLayer::Hardware,
            Some("linux.sysfs.thermal_zone"),
        ),
        "cpufreq.summary" => (
            "cpufreq".to_string(),
            RecorderLayer::Hardware,
            Some("linux.sysfs.cpufreq"),
        ),
        "process.topN" => (
            "process".to_string(),
            RecorderLayer::Os,
            Some("linux.procfs.process"),
        ),
        other => (
            other.split('.').next().unwrap_or("unknown").to_string(),
            RecorderLayer::Unknown,
            None,
        ),
    }
}

fn effective_recorder_interval_ms(max_samples_per_second: u64) -> u64 {
    if max_samples_per_second == 0 {
        return 1000;
    }
    1000_u64.div_ceil(max_samples_per_second).max(1)
}

fn expected_samples_for_interval(time_range: &TimeRange, interval_ms: u64) -> Option<u64> {
    if interval_ms == 0 {
        return None;
    }
    let duration_ms = time_range
        .end
        .saturating_sub(time_range.start)
        .saturating_div(1_000_000);
    Some(duration_ms.saturating_div(interval_ms).saturating_add(1))
}

fn coverage_state_for_loss(
    loss: &CollectorLoss,
    capability_status: crate::CapabilityStatus,
) -> RecorderCoverageState {
    if matches!(
        capability_status,
        crate::CapabilityStatus::Unavailable
            | crate::CapabilityStatus::RequiresPrivilege
            | crate::CapabilityStatus::Unsafe
    ) {
        return RecorderCoverageState::Unavailable;
    }
    if loss.exported_samples == 0 {
        return RecorderCoverageState::Missing;
    }
    if loss.dropped_samples > 0
        || loss.truncated_samples_due_to_freeze_budget > 0
        || !loss.collectors_degraded.is_empty()
    {
        return RecorderCoverageState::Partial;
    }
    RecorderCoverageState::Covered
}

fn coverage_summary(signals: &[RecorderSignalCoverage]) -> RecorderCoverageSummary {
    let expected_signal_count = signals.iter().filter(|signal| signal.expected).count() as u64;
    let covered_signal_count = signals
        .iter()
        .filter(|signal| signal.coverage_state == RecorderCoverageState::Covered)
        .count() as u64;
    let missing_signal_count = signals
        .iter()
        .filter(|signal| signal.coverage_state == RecorderCoverageState::Missing)
        .count() as u64;
    let partial_signal_count = signals
        .iter()
        .filter(|signal| signal.coverage_state == RecorderCoverageState::Partial)
        .count() as u64;
    let unavailable_signal_count = signals
        .iter()
        .filter(|signal| signal.coverage_state == RecorderCoverageState::Unavailable)
        .count() as u64;
    let overall_coverage_percent = if expected_signal_count == 0 {
        0.0
    } else {
        (covered_signal_count as f64 / expected_signal_count as f64) * 100.0
    };
    RecorderCoverageSummary {
        expected_signal_count,
        covered_signal_count,
        missing_signal_count,
        partial_signal_count,
        unavailable_signal_count,
        overall_coverage_percent,
    }
}

pub fn recorder_expected_signal_for_id(
    signal_id: &str,
    configured_interval_ms: u64,
) -> RecorderExpectedSignal {
    let (collector_id, layer, capability) = expected_signal_metadata(signal_id);
    RecorderExpectedSignal {
        schema_version: "obs.recorder_expected_signal.v1".to_string(),
        signal_id: signal_id.to_string(),
        collector_id,
        layer,
        configured_interval_ms: configured_interval_ms.max(1),
        effective_interval_ms: configured_interval_ms.max(1),
        required_capability: capability.map(str::to_string),
        capability_status: crate::CapabilityStatus::Unknown,
        required_privilege: "none".to_string(),
        cost_tier: "always_on_low".to_string(),
        priority: "medium".to_string(),
        expected_samples: None,
        expected: true,
        expectation_source: "profile.always_on.collectors".to_string(),
        data_quality: medium_quality(),
    }
}

pub fn recorder_expected_signals_for_collectors(
    collectors: &[String],
    configured_interval_ms: u64,
) -> Vec<RecorderExpectedSignal> {
    let mut signals = Vec::new();
    for collector in collectors {
        let Some(signal_id) = signal_id_for_collector(collector) else {
            continue;
        };
        signals.push(recorder_expected_signal_for_id(
            signal_id,
            configured_interval_ms,
        ));
    }
    signals.sort_by(|left, right| left.signal_id.cmp(&right.signal_id));
    signals.dedup_by(|left, right| left.signal_id == right.signal_id);
    signals
}

fn signal_id_for_collector(collector: &str) -> Option<&'static str> {
    match collector {
        "cpu" => Some("cpu.summary"),
        "memory" => Some("memory.summary"),
        "network" => Some("network.counters"),
        "kmsg" => Some("kmsg.cursor"),
        "thermal" => Some("thermal.zone"),
        "cpufreq" => Some("cpufreq.summary"),
        "process" => Some("process.topN"),
        _ => None,
    }
}
