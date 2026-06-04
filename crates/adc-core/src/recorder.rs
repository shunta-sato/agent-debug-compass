use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{ClockConfidence, DataQuality};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecorderState {
    Disabled,
    Armed,
    Recording,
    Degraded,
    Freezing,
    Frozen,
    OverBudget,
    Error,
}

impl RecorderState {
    pub fn parse(value: &str) -> Self {
        match value {
            "disabled" => Self::Disabled,
            "armed" => Self::Armed,
            "degraded" => Self::Degraded,
            "freezing" => Self::Freezing,
            "frozen" => Self::Frozen,
            "over_budget" => Self::OverBudget,
            "error" => Self::Error,
            _ => Self::Recording,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecorderSignalSample {
    pub signal_id: String,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecorderSample {
    pub time_mono_ns: u64,
    pub signals: Vec<RecorderSignalSample>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecorderTimeRange {
    pub start: Option<u64>,
    pub end: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecorderGapRange {
    pub start_mono_ns: u64,
    pub end_mono_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecorderSignalStatus {
    pub signal_id: String,
    pub configured_interval_ms: u64,
    pub expected_samples: Option<u64>,
    pub recorded_samples: u64,
    pub dropped_samples: u64,
    pub gap_ranges: Vec<RecorderGapRange>,
    pub degraded: bool,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecorderBufferStatus {
    pub schema_version: String,
    pub target_id: String,
    pub storage_mode: String,
    pub volatile: bool,
    pub survives_daemon_restart: bool,
    pub survives_target_reboot: bool,
    pub survives_power_loss: bool,
    pub retention_ms: u64,
    pub current_retained_range_mono_ns: RecorderTimeRange,
    pub signals: Vec<RecorderSignalStatus>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecorderBudget {
    pub schema_version: String,
    pub budget_id: String,
    pub max_memory_bytes: u64,
    pub max_samples_per_second: u64,
    pub max_collectors: u64,
    pub max_frozen_incidents: u64,
    pub max_freeze_bytes: u64,
    pub max_post_window_ms: u64,
    pub max_retention_ms: u64,
    pub max_ref_lines: u64,
    pub max_cpu_percent: f64,
    pub max_disk_bytes: u64,
    pub collector_priority: Vec<String>,
    pub degradation_policies: Vec<String>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecorderOverhead {
    pub schema_version: String,
    pub target_id: String,
    pub cpu_percent: Option<f64>,
    pub memory_bytes: Option<u64>,
    pub disk_write_bytes: u64,
    pub artifact_bytes: u64,
    pub wakeup_rate_hz: Option<f64>,
    pub self_samples_dropped: u64,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecorderStorageStatus {
    pub storage_mode: String,
    pub volatile: bool,
    pub survives_daemon_restart: bool,
    pub survives_target_reboot: bool,
    pub survives_power_loss: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecorderStatus {
    pub schema_version: String,
    pub target_id: String,
    pub recorder_state: RecorderState,
    pub previous_state: Option<RecorderState>,
    pub active_profile: Option<String>,
    pub armed: bool,
    pub storage: RecorderStorageStatus,
    pub buffer_status: RecorderBufferStatus,
    pub budget: RecorderBudget,
    pub overhead: RecorderOverhead,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone)]
pub struct RecorderRing {
    target_id: String,
    capacity: usize,
    retention_ms: u64,
    samples: Vec<RecorderSample>,
    dropped_by_signal: BTreeMap<String, u64>,
}

impl RecorderRing {
    pub fn new(target_id: impl Into<String>, capacity: usize, retention_ms: u64) -> Self {
        Self {
            target_id: target_id.into(),
            capacity: capacity.max(1),
            retention_ms,
            samples: Vec::new(),
            dropped_by_signal: BTreeMap::new(),
        }
    }

    pub fn push(&mut self, sample: RecorderSample) {
        while self.samples.len() >= self.capacity {
            if let Some(removed) = self.samples.first().cloned() {
                for signal in removed.signals {
                    *self.dropped_by_signal.entry(signal.signal_id).or_default() += 1;
                }
            }
            self.samples.remove(0);
        }
        self.samples.push(sample);
    }

    pub fn status(&self) -> RecorderBufferStatus {
        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        for sample in &self.samples {
            for signal in &sample.signals {
                *counts.entry(signal.signal_id.clone()).or_default() += 1;
            }
        }
        for (signal_id, dropped) in &self.dropped_by_signal {
            counts.entry(signal_id.clone()).or_insert(*dropped);
        }

        let mut signals = Vec::new();
        let mut total_dropped = 0_u64;
        for (signal_id, recorded) in counts {
            let dropped = self.dropped_by_signal.get(&signal_id).copied().unwrap_or(0);
            total_dropped = total_dropped.saturating_add(dropped);
            signals.push(RecorderSignalStatus {
                signal_id,
                configured_interval_ms: 1000,
                expected_samples: Some(recorded.saturating_add(dropped)),
                recorded_samples: recorded,
                dropped_samples: dropped,
                gap_ranges: Vec::new(),
                degraded: dropped > 0,
                data_quality: data_quality_for_drop_count(dropped),
            });
        }

        RecorderBufferStatus {
            schema_version: "obs.recorder_buffer_status.v1".to_string(),
            target_id: self.target_id.clone(),
            storage_mode: "memory_ring".to_string(),
            volatile: true,
            survives_daemon_restart: false,
            survives_target_reboot: false,
            survives_power_loss: false,
            retention_ms: self.retention_ms,
            current_retained_range_mono_ns: RecorderTimeRange {
                start: self.samples.first().map(|sample| sample.time_mono_ns),
                end: self.samples.last().map(|sample| sample.time_mono_ns),
            },
            signals,
            data_quality: data_quality_for_drop_count(total_dropped),
        }
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

pub fn recorder_status_for(
    target_id: impl Into<String>,
    active_profile: Option<&str>,
    previous_state: &str,
    recorder_state: &str,
    buffer_status: RecorderBufferStatus,
    budget: RecorderBudget,
) -> RecorderStatus {
    let target_id = target_id.into();
    let dropped = buffer_status.data_quality.drop_count;
    RecorderStatus {
        schema_version: "obs.recorder_status.v1".to_string(),
        target_id: target_id.clone(),
        recorder_state: RecorderState::parse(recorder_state),
        previous_state: Some(RecorderState::parse(previous_state)),
        active_profile: active_profile.map(str::to_string),
        armed: active_profile.is_some(),
        storage: RecorderStorageStatus {
            storage_mode: "memory_ring".to_string(),
            volatile: true,
            survives_daemon_restart: false,
            survives_target_reboot: false,
            survives_power_loss: false,
        },
        buffer_status,
        budget,
        overhead: RecorderOverhead {
            schema_version: "obs.recorder_overhead.v1".to_string(),
            target_id,
            cpu_percent: None,
            memory_bytes: None,
            disk_write_bytes: 0,
            artifact_bytes: 0,
            wakeup_rate_hz: None,
            self_samples_dropped: dropped,
            data_quality: data_quality_for_drop_count(dropped),
        },
        data_quality: data_quality_for_drop_count(dropped),
    }
}

fn medium_quality() -> DataQuality {
    DataQuality {
        clock_confidence: ClockConfidence::Medium,
        ..Default::default()
    }
}

fn data_quality_for_drop_count(drop_count: u64) -> DataQuality {
    let mut data_quality = medium_quality();
    if drop_count > 0 {
        data_quality.dropped = true;
        data_quality.drop_count = drop_count;
        data_quality
            .notes
            .push("memory ring dropped oldest samples".to_string());
    }
    data_quality
}
