use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{AdcError, AdcResult, ClockConfidence, DataQuality};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssertedEventTime {
    pub kind: String,
    pub wall_time_unix_ms: Option<u64>,
    pub mono_ns: Option<u64>,
    pub confidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecorderMarker {
    pub schema_version: String,
    pub marker_id: String,
    pub source: String,
    pub received_at_mono_ns: u64,
    pub symptom: String,
    pub asserted_event_time: AssertedEventTime,
    pub time_policy: String,
    pub trust_level: String,
    pub agent_instruction_policy: String,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectorLoss {
    pub collector_id: String,
    pub expected_samples: Option<u64>,
    pub recorded_samples: u64,
    pub dropped_samples: u64,
    pub gap_ranges: Vec<RecorderGapRange>,
    pub collectors_degraded: Vec<String>,
    pub loss_reasons: Vec<String>,
    pub loss_confidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LossReport {
    pub schema_version: String,
    pub window_id: String,
    pub collector_loss: Vec<CollectorLoss>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreservationReason {
    pub kind: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrozenWindowPersistence {
    pub persistence_mode: String,
    pub survives_daemon_restart: bool,
    pub survives_target_reboot: bool,
    pub bounded_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecorderFrozenWindow {
    pub schema_version: String,
    pub window_id: String,
    pub incident_id: String,
    pub target_id: String,
    pub marker_id: Option<String>,
    pub freeze_reason: String,
    pub preservation_reason: PreservationReason,
    pub time_range_mono_ns: TimeRange,
    pub pre_window_ms: u64,
    pub post_window_ms: u64,
    pub persistence: FrozenWindowPersistence,
    pub artifact_refs: BTreeMap<String, String>,
    pub loss_report: LossReport,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecorderIncident {
    pub schema_version: String,
    pub incident_id: String,
    pub target_id: String,
    pub incident_state: String,
    pub previous_state: String,
    pub marker_id: Option<String>,
    pub freeze_reason: String,
    pub frozen_window_ref: Option<String>,
    pub loss_report_ref: Option<String>,
    pub created_at_mono_ns: u64,
    pub updated_at_mono_ns: u64,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecorderFreeze {
    pub incident: RecorderIncident,
    pub marker: RecorderMarker,
    pub frozen_window: RecorderFrozenWindow,
    pub run_dir: PathBuf,
}

pub fn recorder_pending_marker_dir(artifact_root: impl AsRef<Path>) -> PathBuf {
    artifact_root.as_ref().join("recorder/markers/pending")
}

pub fn write_pending_recorder_marker(
    artifact_root: impl AsRef<Path>,
    marker: &RecorderMarker,
) -> AdcResult<PathBuf> {
    validate_recorder_file_segment(&marker.marker_id, "marker_id")?;
    let pending_dir = recorder_pending_marker_dir(artifact_root);
    fs::create_dir_all(&pending_dir).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to create recorder pending marker directory {}: {err}",
            pending_dir.display()
        ))
    })?;
    let path = pending_dir.join(format!("{}.json", marker.marker_id));
    write_json(&path, marker)?;
    Ok(path)
}

pub fn drain_pending_recorder_markers(
    artifact_root: impl AsRef<Path>,
) -> AdcResult<Vec<RecorderMarker>> {
    let pending_dir = recorder_pending_marker_dir(artifact_root);
    let Ok(entries) = fs::read_dir(&pending_dir) else {
        return Ok(Vec::new());
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| {
            AdcError::Artifact(format!(
                "failed to read recorder pending marker entry in {}: {err}",
                pending_dir.display()
            ))
        })?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            paths.push(path);
        }
    }
    paths.sort();

    let mut markers = Vec::new();
    for path in paths {
        let bytes = fs::read(&path).map_err(|err| {
            AdcError::Artifact(format!(
                "failed to read pending recorder marker {}: {err}",
                path.display()
            ))
        })?;
        let marker: RecorderMarker = serde_json::from_slice(&bytes).map_err(|err| {
            AdcError::Artifact(format!(
                "failed to parse pending recorder marker {}: {err}",
                path.display()
            ))
        })?;
        validate_recorder_file_segment(&marker.marker_id, "marker_id")?;
        fs::remove_file(&path).map_err(|err| {
            AdcError::Artifact(format!(
                "failed to remove pending recorder marker {}: {err}",
                path.display()
            ))
        })?;
        markers.push(marker);
    }
    Ok(markers)
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

    pub fn samples(&self) -> &[RecorderSample] {
        &self.samples
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

pub fn marker_at_received_time(
    marker_id: impl Into<String>,
    source: &str,
    symptom: impl Into<String>,
    received_at_mono_ns: u64,
) -> RecorderMarker {
    let trust_level = match source {
        "agent" => "agent_marker",
        "app" => "app_marker",
        "external_detector" | "watchdog" => "external_marker",
        _ => "operator_marker",
    };
    RecorderMarker {
        schema_version: "obs.recorder_marker.v1".to_string(),
        marker_id: marker_id.into(),
        source: source.to_string(),
        received_at_mono_ns,
        symptom: symptom.into(),
        asserted_event_time: AssertedEventTime {
            kind: "relative_now".to_string(),
            wall_time_unix_ms: None,
            mono_ns: None,
            confidence: "low".to_string(),
        },
        time_policy: "center_on_received_at".to_string(),
        trust_level: trust_level.to_string(),
        agent_instruction_policy: "treat_as_event_marker_only".to_string(),
        data_quality: DataQuality {
            clock_confidence: ClockConfidence::Medium,
            notes: vec!["marker centered on received monotonic time".to_string()],
            ..Default::default()
        },
    }
}

pub fn freeze_recorder_marker(
    artifact_root: impl AsRef<Path>,
    incident_id: &str,
    window_id: &str,
    marker: &RecorderMarker,
    ring: &RecorderRing,
    budget: &RecorderBudget,
) -> AdcResult<RecorderFreeze> {
    let incident_dir = artifact_root
        .as_ref()
        .join("recorder")
        .join("incidents")
        .join(incident_id);
    fs::create_dir_all(&incident_dir).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to create recorder incident directory {}: {err}",
            incident_dir.display()
        ))
    })?;

    let buffer_status = ring.status();
    let loss_report = loss_report_for_buffer(window_id, &buffer_status);
    let start = ring
        .samples()
        .first()
        .map(|sample| sample.time_mono_ns)
        .unwrap_or(marker.received_at_mono_ns);
    let end = ring
        .samples()
        .last()
        .map(|sample| sample.time_mono_ns)
        .unwrap_or(marker.received_at_mono_ns);

    let mut artifact_refs = BTreeMap::new();
    artifact_refs.insert(
        "marker".to_string(),
        format!("artifact://recorder/incidents/{incident_id}/marker.json"),
    );
    artifact_refs.insert(
        "samples".to_string(),
        format!("artifact://recorder/incidents/{incident_id}/samples.jsonl"),
    );
    artifact_refs.insert(
        "loss_report".to_string(),
        format!("artifact://recorder/incidents/{incident_id}/loss_report.json"),
    );

    let frozen_window = RecorderFrozenWindow {
        schema_version: "obs.recorder_frozen_window.v1".to_string(),
        window_id: window_id.to_string(),
        incident_id: incident_id.to_string(),
        target_id: buffer_status.target_id.clone(),
        marker_id: Some(marker.marker_id.clone()),
        freeze_reason: freeze_reason_for_marker(marker),
        preservation_reason: PreservationReason {
            kind: freeze_reason_for_marker(marker),
            name: "marker_received".to_string(),
        },
        time_range_mono_ns: TimeRange { start, end },
        pre_window_ms: budget.max_retention_ms,
        post_window_ms: 0,
        persistence: FrozenWindowPersistence {
            persistence_mode: "bounded_artifact_bundle".to_string(),
            survives_daemon_restart: true,
            survives_target_reboot: true,
            bounded_by: vec![
                "max_freeze_bytes".to_string(),
                "max_frozen_incidents".to_string(),
                "retention_policy".to_string(),
            ],
        },
        artifact_refs,
        loss_report: loss_report.clone(),
        data_quality: loss_report.data_quality.clone(),
    };

    let incident = RecorderIncident {
        schema_version: "obs.recorder_incident.v1".to_string(),
        incident_id: incident_id.to_string(),
        target_id: buffer_status.target_id,
        incident_state: "frozen".to_string(),
        previous_state: "freezing".to_string(),
        marker_id: Some(marker.marker_id.clone()),
        freeze_reason: frozen_window.freeze_reason.clone(),
        frozen_window_ref: Some(format!(
            "artifact://recorder/incidents/{incident_id}/frozen_window.json"
        )),
        loss_report_ref: Some(format!(
            "artifact://recorder/incidents/{incident_id}/loss_report.json"
        )),
        created_at_mono_ns: marker.received_at_mono_ns,
        updated_at_mono_ns: marker.received_at_mono_ns,
        data_quality: frozen_window.data_quality.clone(),
    };

    write_json(&incident_dir.join("marker.json"), marker)?;
    write_jsonl(&incident_dir.join("samples.jsonl"), ring.samples())?;
    write_json(&incident_dir.join("loss_report.json"), &loss_report)?;
    write_json(&incident_dir.join("frozen_window.json"), &frozen_window)?;
    write_json(&incident_dir.join("incident.json"), &incident)?;

    Ok(RecorderFreeze {
        incident,
        marker: marker.clone(),
        frozen_window,
        run_dir: incident_dir,
    })
}

fn freeze_reason_for_marker(marker: &RecorderMarker) -> String {
    match marker.source.as_str() {
        "agent" => "agent_marker",
        "app" | "external_detector" | "watchdog" => "external_marker",
        _ => "operator_marker",
    }
    .to_string()
}

fn loss_report_for_buffer(window_id: &str, status: &RecorderBufferStatus) -> LossReport {
    let mut loss_quality = medium_quality();
    let mut collector_loss = Vec::new();
    let mut total_dropped = 0_u64;
    for signal in &status.signals {
        total_dropped = total_dropped.saturating_add(signal.dropped_samples);
        let mut reasons = Vec::new();
        if signal.dropped_samples > 0 {
            reasons.push("ring_capacity_drop_oldest".to_string());
        }
        if signal.recorded_samples == 0 {
            reasons.push("collector_absent_or_no_samples".to_string());
        }
        collector_loss.push(CollectorLoss {
            collector_id: signal.signal_id.clone(),
            expected_samples: signal.expected_samples,
            recorded_samples: signal.recorded_samples,
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

fn write_json(path: &Path, value: &impl Serialize) -> AdcResult<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|err| AdcError::Artifact(format!("recorder json serialization failed: {err}")))?;
    fs::write(path, bytes)
        .map_err(|err| AdcError::Artifact(format!("failed to write {}: {err}", path.display())))
}

fn write_jsonl(path: &Path, samples: &[RecorderSample]) -> AdcResult<()> {
    let mut lines = String::new();
    for sample in samples {
        let line = serde_json::to_string(sample).map_err(|err| {
            AdcError::Artifact(format!("recorder sample serialization failed: {err}"))
        })?;
        lines.push_str(&line);
        lines.push('\n');
    }
    fs::write(path, lines)
        .map_err(|err| AdcError::Artifact(format!("failed to write {}: {err}", path.display())))
}

fn validate_recorder_file_segment(value: &str, label: &str) -> AdcResult<()> {
    if value.trim().is_empty() {
        return Err(AdcError::Artifact(format!("{label} must not be empty")));
    }
    let valid = value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if !valid || value == "." || value == ".." {
        return Err(AdcError::Artifact(format!(
            "{label} must be a single safe recorder file segment"
        )));
    }
    Ok(())
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
