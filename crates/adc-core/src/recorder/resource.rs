use std::{
    env, fs,
    path::{Path, PathBuf},
};

use crate::{ClockConfidence, DataQuality};

use super::{
    model::{
        RecorderBatteryState, RecorderBudget, RecorderDegradationAction,
        RecorderDegradationDecision, RecorderDegradationReason, RecorderOverhead,
        RecorderPowerMode, RecorderPowerPolicy, RecorderPowerPolicyMode, RecorderPowerSnapshot,
        RecorderPowerSource, RecorderResourceRates, RecorderResourceScope, RecorderResourceStatus,
    },
    quality::medium_quality,
};

pub fn recorder_power_snapshot_from_sysfs(
    power_supply_root: impl AsRef<Path>,
) -> RecorderPowerSnapshot {
    let root = power_supply_root.as_ref();
    let mut snapshot = unknown_power_snapshot();
    let Ok(entries) = fs::read_dir(root) else {
        snapshot
            .data_quality
            .missing
            .push("battery state is unavailable on this target".to_string());
        snapshot.data_quality.notes.push(format!(
            "power supply root {} is unavailable",
            root.display()
        ));
        return snapshot;
    };

    let mut saw_supply = false;
    let mut saw_ac = false;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        saw_supply = true;
        let supply_type = read_trimmed(path.join("type")).unwrap_or_default();
        if supply_type.eq_ignore_ascii_case("battery") {
            snapshot.power_source = RecorderPowerSource::Battery;
            snapshot.battery_state = battery_state_from_status(
                read_trimmed(path.join("status"))
                    .as_deref()
                    .unwrap_or("unknown"),
            );
            snapshot.battery_percent =
                read_trimmed(path.join("capacity")).and_then(|value| value.parse::<f64>().ok());
            snapshot.data_quality = medium_quality();
            return snapshot;
        }
        if matches!(
            supply_type.to_ascii_lowercase().as_str(),
            "mains" | "ac" | "usb" | "usb_c" | "usb-c"
        ) {
            saw_ac = true;
        }
    }

    if saw_ac {
        snapshot.power_source = RecorderPowerSource::AcPower;
        snapshot.battery_state = RecorderBatteryState::Absent;
        snapshot.data_quality = medium_quality();
        snapshot
            .data_quality
            .missing
            .push("battery state is unavailable on this target".to_string());
        return snapshot;
    }

    if !saw_supply {
        snapshot
            .data_quality
            .missing
            .push("battery state is unavailable on this target".to_string());
        snapshot
            .data_quality
            .notes
            .push("no power supplies were found under sysfs".to_string());
        return snapshot;
    }

    snapshot
        .data_quality
        .missing
        .push("battery state is unavailable on this target".to_string());
    snapshot
        .data_quality
        .notes
        .push("power supplies were present but no battery or AC state was recognized".to_string());
    snapshot
}

pub fn recorder_power_mode_override_from_env() -> Option<RecorderPowerMode> {
    env::var("ADC_RECORDER_POWER_MODE")
        .ok()
        .and_then(|value| recorder_power_mode_from_str(&value))
}

pub fn recorder_power_mode_for_snapshot(
    snapshot: &RecorderPowerSnapshot,
    override_mode: Option<RecorderPowerMode>,
) -> RecorderPowerMode {
    if let Some(mode) = override_mode {
        return mode;
    }
    match (snapshot.power_source, snapshot.battery_state) {
        (_, RecorderBatteryState::Critical) => RecorderPowerMode::BatteryCritical,
        (_, RecorderBatteryState::Low) => RecorderPowerMode::BatteryLow,
        (RecorderPowerSource::Battery, _) => RecorderPowerMode::BatteryNormal,
        (RecorderPowerSource::AcPower, _) | (RecorderPowerSource::External, _) => {
            RecorderPowerMode::AcPower
        }
        _ => RecorderPowerMode::Unknown,
    }
}

pub fn default_recorder_power_policy() -> RecorderPowerPolicy {
    RecorderPowerPolicy {
        schema_version: "obs.recorder_power_policy.v1".to_string(),
        policy_id: "default-battery-safe-recorder-policy".to_string(),
        modes: vec![
            policy_mode(RecorderPowerMode::Unknown, 16, 60_000, true, &[]),
            policy_mode(RecorderPowerMode::AcPower, 16, 60_000, true, &[]),
            policy_mode(
                RecorderPowerMode::BatteryNormal,
                4,
                30_000,
                true,
                &["battery_normal reduces recorder sampling rate"],
            ),
            policy_mode(
                RecorderPowerMode::BatteryLow,
                1,
                10_000,
                false,
                &["battery_low preserves only high-priority recorder signals"],
            ),
            policy_mode(
                RecorderPowerMode::BatteryCritical,
                1,
                5_000,
                false,
                &["battery_critical keeps minimum recorder evidence only"],
            ),
        ],
        data_quality: medium_quality(),
    }
}

pub fn recorder_budget_for_power_mode(
    budget: &RecorderBudget,
    mode: RecorderPowerMode,
) -> RecorderBudget {
    let policy = default_recorder_power_policy();
    let Some(policy_mode) = policy.modes.iter().find(|candidate| candidate.mode == mode) else {
        return budget.clone();
    };
    let mut adjusted = budget.clone();
    adjusted.max_samples_per_second = adjusted
        .max_samples_per_second
        .min(policy_mode.max_samples_per_second.max(1));
    adjusted.max_retention_ms = adjusted
        .max_retention_ms
        .min(policy_mode.max_retention_ms.max(1));
    if adjusted.max_samples_per_second != budget.max_samples_per_second
        || adjusted.max_retention_ms != budget.max_retention_ms
        || !policy_mode.low_priority_signals_enabled
    {
        adjusted.data_quality.throttled = true;
        adjusted.data_quality.notes.push(format!(
            "recorder resource policy {:?} adjusted sample/retention budget",
            mode
        ));
    }
    adjusted
}

pub fn recorder_degradation_decisions_for_power_mode(
    mode: RecorderPowerMode,
) -> Vec<RecorderDegradationDecision> {
    match mode {
        RecorderPowerMode::BatteryLow => vec![recorder_degradation_decision(
            "RD-battery-low",
            RecorderDegradationReason::BatteryLow,
            vec![
                RecorderDegradationAction::Downsample,
                RecorderDegradationAction::DropLowPrioritySignal,
                RecorderDegradationAction::ShortenRetention,
            ],
            low_priority_signals(),
        )],
        RecorderPowerMode::BatteryCritical => vec![recorder_degradation_decision(
            "RD-battery-critical",
            RecorderDegradationReason::BatteryCritical,
            vec![
                RecorderDegradationAction::Downsample,
                RecorderDegradationAction::DropLowPrioritySignal,
                RecorderDegradationAction::ShortenRetention,
            ],
            low_priority_signals(),
        )],
        _ => Vec::new(),
    }
}

pub fn recorder_resource_status_for_overhead(
    target_id: impl Into<String>,
    budget: &RecorderBudget,
    overhead: &RecorderOverhead,
    policy_mode: Option<RecorderPowerMode>,
) -> RecorderResourceStatus {
    let snapshot = unknown_power_snapshot_with_missing();
    recorder_resource_status_for_service_run(
        target_id,
        budget,
        overhead,
        snapshot,
        policy_mode.unwrap_or(RecorderPowerMode::Unknown),
        RecorderResourceRates::default(),
        Vec::new(),
    )
}

pub fn recorder_resource_status_for_service_run(
    target_id: impl Into<String>,
    budget: &RecorderBudget,
    overhead: &RecorderOverhead,
    power_snapshot: RecorderPowerSnapshot,
    policy_mode: RecorderPowerMode,
    rates: RecorderResourceRates,
    degradation_decisions: Vec<RecorderDegradationDecision>,
) -> RecorderResourceStatus {
    let mut data_quality = power_snapshot.data_quality.clone();
    data_quality.notes.push(
        "continuous recorder ring is memory-backed and performs no continuous disk writes"
            .to_string(),
    );
    if overhead.wakeup_rate_hz.is_none() {
        data_quality
            .missing
            .push("recorder wakeup rate is not measured in this MVP".to_string());
    }
    if overhead.estimated_memory_ring_bytes > budget.max_memory_bytes {
        data_quality.throttled = true;
        data_quality.missing.push(format!(
            "estimated recorder memory {} exceeds max_memory_bytes {}",
            overhead.estimated_memory_ring_bytes, budget.max_memory_bytes
        ));
    }
    for decision in &degradation_decisions {
        data_quality.throttled |= decision.data_quality.throttled;
        data_quality
            .missing
            .extend(decision.data_quality.missing.clone());
        data_quality
            .notes
            .extend(decision.data_quality.notes.clone());
    }

    RecorderResourceStatus {
        schema_version: "obs.recorder_resource_status.v1".to_string(),
        target_id: target_id.into(),
        resource_scope: RecorderResourceScope::ServiceRun,
        power_source: power_snapshot.power_source,
        battery_state: power_snapshot.battery_state,
        battery_percent: power_snapshot.battery_percent,
        policy_mode,
        estimated_ring_memory_bytes: overhead.estimated_memory_ring_bytes,
        max_memory_bytes: budget.max_memory_bytes,
        wakeup_rate_hz: overhead.wakeup_rate_hz,
        recorder_loop_rate_hz: rates.recorder_loop_rate_hz,
        recorder_sample_rate_hz: rates.recorder_sample_rate_hz,
        status_write_rate_hz: rates.status_write_rate_hz,
        continuous_ring_disk_write_bytes: 0,
        status_write_bytes: overhead.status_write_bytes,
        frozen_artifact_write_bytes: overhead.frozen_artifact_bytes,
        network_upload_bytes: 0,
        degradation_decisions,
        data_quality,
    }
}

pub fn collector_enabled_by_power_mode(collector: &str, mode: RecorderPowerMode) -> bool {
    if matches!(
        mode,
        RecorderPowerMode::BatteryLow | RecorderPowerMode::BatteryCritical
    ) && matches!(collector, "network" | "process")
    {
        return false;
    }
    true
}

pub fn annotate_expected_signal_for_power_mode(
    signal_id: &str,
    mode: RecorderPowerMode,
    data_quality: &mut DataQuality,
) {
    if !signal_disabled_by_power_mode(signal_id, mode) {
        return;
    }
    data_quality.throttled = true;
    data_quality.missing.push(format!(
        "{signal_id} disabled by {} policy",
        power_mode_label(mode)
    ));
    data_quality
        .notes
        .push("resource policy disabled a low-priority recorder signal".to_string());
}

pub fn resource_policy_loss_reason_for_signal(
    signal_id: &str,
    mode: RecorderPowerMode,
) -> Option<&'static str> {
    if signal_disabled_by_power_mode(signal_id, mode) {
        return Some(match mode {
            RecorderPowerMode::BatteryLow => "battery_low_policy",
            RecorderPowerMode::BatteryCritical => "battery_critical_policy",
            _ => "resource_policy",
        });
    }
    None
}

fn recorder_power_mode_from_str(value: &str) -> Option<RecorderPowerMode> {
    match value.trim() {
        "unknown" => Some(RecorderPowerMode::Unknown),
        "ac_power" => Some(RecorderPowerMode::AcPower),
        "battery_normal" => Some(RecorderPowerMode::BatteryNormal),
        "battery_low" => Some(RecorderPowerMode::BatteryLow),
        "battery_critical" => Some(RecorderPowerMode::BatteryCritical),
        _ => None,
    }
}

fn unknown_power_snapshot() -> RecorderPowerSnapshot {
    RecorderPowerSnapshot {
        power_source: RecorderPowerSource::Unknown,
        battery_state: RecorderBatteryState::Unknown,
        battery_percent: None,
        data_quality: DataQuality {
            clock_confidence: ClockConfidence::Medium,
            ..Default::default()
        },
    }
}

fn unknown_power_snapshot_with_missing() -> RecorderPowerSnapshot {
    let mut snapshot = unknown_power_snapshot();
    snapshot
        .data_quality
        .missing
        .push("battery state is unavailable on this target".to_string());
    snapshot
}

fn read_trimmed(path: PathBuf) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn battery_state_from_status(status: &str) -> RecorderBatteryState {
    match status.to_ascii_lowercase().as_str() {
        "charging" => RecorderBatteryState::Charging,
        "discharging" => RecorderBatteryState::Discharging,
        "full" => RecorderBatteryState::Full,
        "low" => RecorderBatteryState::Low,
        "critical" => RecorderBatteryState::Critical,
        _ => RecorderBatteryState::Unknown,
    }
}

fn policy_mode(
    mode: RecorderPowerMode,
    max_samples_per_second: u64,
    max_retention_ms: u64,
    low_priority_signals_enabled: bool,
    notes: &[&str],
) -> RecorderPowerPolicyMode {
    let mut data_quality = medium_quality();
    if max_samples_per_second < 16 || !low_priority_signals_enabled {
        data_quality.throttled = true;
    }
    data_quality
        .notes
        .extend(notes.iter().map(|note| (*note).to_string()));
    RecorderPowerPolicyMode {
        mode,
        max_samples_per_second,
        max_retention_ms,
        low_priority_signals_enabled,
        data_quality,
    }
}

fn recorder_degradation_decision(
    decision_id: &str,
    reason: RecorderDegradationReason,
    actions: Vec<RecorderDegradationAction>,
    affected_signals: Vec<String>,
) -> RecorderDegradationDecision {
    let mut data_quality = medium_quality();
    data_quality.throttled = true;
    for signal in &affected_signals {
        data_quality
            .missing
            .push(format!("{signal} disabled by resource policy"));
    }
    data_quality.notes.push(
        "resource policy degraded low-priority recorder signals before collection".to_string(),
    );
    RecorderDegradationDecision {
        schema_version: "obs.recorder_degradation_decision.v1".to_string(),
        decision_id: decision_id.to_string(),
        reason,
        actions,
        affected_signals,
        preserved_signals: vec![
            "adc.self_overhead".to_string(),
            "cpu.summary".to_string(),
            "memory.summary".to_string(),
        ],
        data_quality,
    }
}

fn low_priority_signals() -> Vec<String> {
    vec!["network.counters".to_string(), "process.topN".to_string()]
}

fn signal_disabled_by_power_mode(signal_id: &str, mode: RecorderPowerMode) -> bool {
    matches!(
        mode,
        RecorderPowerMode::BatteryLow | RecorderPowerMode::BatteryCritical
    ) && matches!(signal_id, "network.counters" | "process.topN")
}

fn power_mode_label(mode: RecorderPowerMode) -> &'static str {
    match mode {
        RecorderPowerMode::Unknown => "unknown",
        RecorderPowerMode::AcPower => "ac_power",
        RecorderPowerMode::BatteryNormal => "battery_normal",
        RecorderPowerMode::BatteryLow => "battery_low",
        RecorderPowerMode::BatteryCritical => "battery_critical",
    }
}
