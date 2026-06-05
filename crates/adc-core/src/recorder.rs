use std::{
    collections::{BTreeMap, BTreeSet},
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
            "recording" => Self::Recording,
            "degraded" => Self::Degraded,
            "freezing" => Self::Freezing,
            "frozen" => Self::Frozen,
            "over_budget" => Self::OverBudget,
            "error" => Self::Error,
            _ => Self::Error,
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
    pub max_status_write_interval_ms: u64,
    pub max_ref_lines: u64,
    pub max_cpu_percent: f64,
    pub max_disk_bytes: u64,
    pub collector_priority: Vec<String>,
    pub degradation_policies: Vec<String>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecorderOverheadScope {
    ServiceRun,
    CurrentStatusSnapshot,
    Incident,
    ArtifactRootTotal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecorderOverhead {
    pub schema_version: String,
    pub target_id: String,
    pub overhead_scope: RecorderOverheadScope,
    pub since_mono_ns: Option<u64>,
    pub through_mono_ns: Option<u64>,
    pub cpu_percent: Option<f64>,
    pub memory_bytes: Option<u64>,
    pub disk_write_bytes: u64,
    pub artifact_bytes: u64,
    pub status_write_bytes: u64,
    pub frozen_artifact_bytes: u64,
    pub samples_jsonl_bytes: u64,
    pub incident_count: u64,
    pub estimated_memory_ring_bytes: u64,
    pub wakeup_rate_hz: Option<f64>,
    pub self_samples_dropped: u64,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecorderOverheadAccounting {
    pub overhead_scope: RecorderOverheadScope,
    pub since_mono_ns: Option<u64>,
    pub through_mono_ns: Option<u64>,
    pub disk_write_bytes: u64,
    pub artifact_bytes: u64,
    pub status_write_bytes: u64,
    pub frozen_artifact_bytes: u64,
    pub samples_jsonl_bytes: u64,
    pub incident_count: u64,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecorderMarkerResult {
    pub schema_version: String,
    pub marker: RecorderMarker,
    pub status: String,
    pub reason: String,
    pub pending_marker_ref: Option<String>,
    pub incident_ref: Option<String>,
    pub expected_incident_id: String,
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
    pub retained_samples_before_freeze: u64,
    pub exported_samples: u64,
    pub truncated_samples_due_to_freeze_budget: u64,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecorderTriggerFreeze {
    pub incident: RecorderIncident,
    pub frozen_window: RecorderFrozenWindow,
    pub run_dir: PathBuf,
}

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

pub fn recorder_pending_marker_dir(artifact_root: impl AsRef<Path>) -> PathBuf {
    artifact_root.as_ref().join("recorder/markers/pending")
}

pub fn recorder_marker_result_dir(artifact_root: impl AsRef<Path>) -> PathBuf {
    artifact_root.as_ref().join("recorder/markers/results")
}

pub fn recorder_pending_marker_ref(marker_id: &str) -> AdcResult<String> {
    validate_recorder_file_segment(marker_id, "marker_id")?;
    Ok(format!(
        "artifact://recorder/markers/pending/{marker_id}.json"
    ))
}

pub fn recorder_incident_artifact_ref(incident_id: &str, artifact_name: &str) -> AdcResult<String> {
    validate_recorder_file_segment(incident_id, "incident_id")?;
    match artifact_name {
        "incident.json" | "frozen_window.json" | "loss_report.json" | "samples.jsonl"
        | "marker.json" | "trigger_event.json" => Ok(format!(
            "artifact://recorder/incidents/{incident_id}/{artifact_name}"
        )),
        _ => Err(AdcError::Artifact(format!(
            "unsupported recorder incident artifact {artifact_name}"
        ))),
    }
}

pub fn write_recorder_marker_result(
    artifact_root: impl AsRef<Path>,
    result: &RecorderMarkerResult,
) -> AdcResult<PathBuf> {
    validate_recorder_file_segment(&result.marker.marker_id, "marker_id")?;
    let result_dir = recorder_marker_result_dir(artifact_root);
    fs::create_dir_all(&result_dir).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to create recorder marker result directory {}: {err}",
            result_dir.display()
        ))
    })?;
    let path = result_dir.join(format!("{}.json", result.marker.marker_id));
    write_json(&path, result)?;
    Ok(path)
}

pub fn recorder_marker_result_for_queued(
    marker: RecorderMarker,
    expected_incident_id: String,
    pending_marker_ref: String,
) -> RecorderMarkerResult {
    RecorderMarkerResult {
        schema_version: "obs.recorder_marker_result.v1".to_string(),
        marker,
        status: "queued".to_string(),
        reason: "queued_for_adc_targetd_recorder_freeze".to_string(),
        pending_marker_ref: Some(pending_marker_ref),
        incident_ref: None,
        expected_incident_id,
        data_quality: DataQuality {
            clock_confidence: ClockConfidence::Medium,
            notes: vec!["marker queued for adc-targetd recorder freeze".to_string()],
            ..Default::default()
        },
    }
}

pub fn recorder_marker_result_for_frozen(
    marker: RecorderMarker,
    incident_id: String,
) -> RecorderMarkerResult {
    RecorderMarkerResult {
        schema_version: "obs.recorder_marker_result.v1".to_string(),
        marker,
        status: "frozen".to_string(),
        reason: "incident_window_frozen".to_string(),
        pending_marker_ref: None,
        incident_ref: Some(format!(
            "artifact://recorder/incidents/{incident_id}/incident.json"
        )),
        expected_incident_id: incident_id.clone(),
        data_quality: medium_quality(),
    }
}

pub fn recorder_marker_result_for_refused(
    marker: RecorderMarker,
    expected_incident_id: String,
    reason: impl Into<String>,
) -> RecorderMarkerResult {
    RecorderMarkerResult {
        schema_version: "obs.recorder_marker_result.v1".to_string(),
        marker,
        status: "refused".to_string(),
        reason: reason.into(),
        pending_marker_ref: None,
        incident_ref: None,
        expected_incident_id,
        data_quality: DataQuality {
            throttled: true,
            clock_confidence: ClockConfidence::Medium,
            missing: vec!["incident window was not frozen due to recorder budget".to_string()],
            notes: vec!["pending marker was consumed and recorded as refused".to_string()],
            ..Default::default()
        },
    }
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
    expected_signal_ids: BTreeSet<String>,
    throttled_sample_count: u64,
    throttle_notes: BTreeSet<String>,
}

impl RecorderRing {
    pub fn new(target_id: impl Into<String>, capacity: usize, retention_ms: u64) -> Self {
        Self::with_expected_signals(
            target_id,
            capacity,
            retention_ms,
            std::iter::empty::<String>(),
        )
    }

    pub fn with_expected_signals(
        target_id: impl Into<String>,
        capacity: usize,
        retention_ms: u64,
        expected_signal_ids: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            target_id: target_id.into(),
            capacity: capacity.max(1),
            retention_ms,
            samples: Vec::new(),
            dropped_by_signal: BTreeMap::new(),
            expected_signal_ids: expected_signal_ids.into_iter().collect(),
            throttled_sample_count: 0,
            throttle_notes: BTreeSet::new(),
        }
    }

    pub fn push(&mut self, sample: RecorderSample) {
        while self.samples.len() >= self.capacity {
            self.drop_oldest_sample();
        }
        self.samples.push(sample);
        self.evict_expired_samples();
    }

    pub fn samples(&self) -> &[RecorderSample] {
        &self.samples
    }

    pub fn record_throttled_sample(&mut self, note: impl Into<String>) {
        self.throttled_sample_count = self.throttled_sample_count.saturating_add(1);
        self.throttle_notes.insert(note.into());
    }

    pub fn status(&self) -> RecorderBufferStatus {
        let mut recorded_by_signal: BTreeMap<String, u64> = BTreeMap::new();
        for sample in &self.samples {
            for signal in &sample.signals {
                *recorded_by_signal
                    .entry(signal.signal_id.clone())
                    .or_default() += 1;
            }
        }
        let signal_ids = recorded_by_signal
            .keys()
            .chain(self.dropped_by_signal.keys())
            .chain(self.expected_signal_ids.iter())
            .cloned()
            .collect::<BTreeSet<_>>();

        let mut signals = Vec::new();
        let mut total_dropped = 0_u64;
        let mut buffer_quality = data_quality_for_drop_count(0);
        for signal_id in signal_ids {
            let recorded = recorded_by_signal.get(&signal_id).copied().unwrap_or(0);
            let dropped = self.dropped_by_signal.get(&signal_id).copied().unwrap_or(0);
            total_dropped = total_dropped.saturating_add(dropped);
            let expected_but_absent =
                self.expected_signal_ids.contains(&signal_id) && recorded == 0 && dropped == 0;
            let mut signal_quality = data_quality_for_drop_count(dropped);
            if expected_but_absent {
                signal_quality.missing.push(format!(
                    "expected recorder signal {signal_id} has no retained samples"
                ));
                buffer_quality.missing.push(format!(
                    "expected recorder signal {signal_id} has no retained samples"
                ));
            }
            signals.push(RecorderSignalStatus {
                signal_id,
                configured_interval_ms: 1000,
                expected_samples: Some(recorded.saturating_add(dropped)),
                recorded_samples: recorded,
                dropped_samples: dropped,
                gap_ranges: Vec::new(),
                degraded: dropped > 0 || expected_but_absent,
                data_quality: signal_quality,
            });
        }
        let dropped_quality = data_quality_for_drop_count(total_dropped);
        if dropped_quality.dropped {
            buffer_quality.dropped = true;
            buffer_quality.drop_count = dropped_quality.drop_count;
            buffer_quality.notes.extend(dropped_quality.notes);
        }
        if self.throttled_sample_count > 0 {
            buffer_quality.throttled = true;
            buffer_quality
                .notes
                .extend(self.throttle_notes.iter().cloned());
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
            data_quality: buffer_quality,
        }
    }

    fn drop_oldest_sample(&mut self) {
        if self.samples.is_empty() {
            return;
        }
        let removed = self.samples.remove(0);
        for signal in removed.signals {
            *self.dropped_by_signal.entry(signal.signal_id).or_default() += 1;
        }
    }

    fn evict_expired_samples(&mut self) {
        let Some(newest_time) = self.samples.last().map(|sample| sample.time_mono_ns) else {
            return;
        };
        let retention_ns = self.retention_ms.saturating_mul(1_000_000);
        let cutoff = newest_time.saturating_sub(retention_ns);
        while self
            .samples
            .first()
            .is_some_and(|sample| sample.time_mono_ns < cutoff)
        {
            self.drop_oldest_sample();
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecorderSampleRateGovernor {
    min_interval_ns: u64,
    last_sample_mono_ns: Option<u64>,
}

impl RecorderSampleRateGovernor {
    pub fn new(max_samples_per_second: u64) -> Self {
        let min_interval_ns = if max_samples_per_second == 0 {
            u64::MAX
        } else {
            1_000_000_000_u64 / max_samples_per_second
        };
        Self {
            min_interval_ns: min_interval_ns.max(1),
            last_sample_mono_ns: None,
        }
    }

    pub fn should_record(&mut self, now_mono_ns: u64) -> bool {
        let Some(last) = self.last_sample_mono_ns else {
            self.last_sample_mono_ns = Some(now_mono_ns);
            return true;
        };
        if now_mono_ns.saturating_sub(last) < self.min_interval_ns {
            return false;
        }
        self.last_sample_mono_ns = Some(now_mono_ns);
        true
    }
}

#[derive(Debug, Clone)]
pub struct RecorderStatusWriteGovernor {
    interval_ns: u64,
    last_write_mono_ns: Option<u64>,
}

impl RecorderStatusWriteGovernor {
    pub fn new(max_status_write_interval_ms: u64) -> Self {
        Self {
            interval_ns: max_status_write_interval_ms
                .max(1)
                .saturating_mul(1_000_000),
            last_write_mono_ns: None,
        }
    }

    pub fn should_write(&mut self, now_mono_ns: u64, force: bool) -> bool {
        if force {
            self.last_write_mono_ns = Some(now_mono_ns);
            return true;
        }
        let Some(last) = self.last_write_mono_ns else {
            self.last_write_mono_ns = Some(now_mono_ns);
            return true;
        };
        if now_mono_ns.saturating_sub(last) < self.interval_ns {
            return false;
        }
        self.last_write_mono_ns = Some(now_mono_ns);
        true
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
    validate_recorder_file_segment(incident_id, "incident_id")?;
    validate_recorder_file_segment(window_id, "window_id")?;
    validate_recorder_file_segment(&marker.marker_id, "marker_id")?;
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
    let (freeze_samples, sample_quality) = samples_within_freeze_budget(ring.samples(), budget)?;
    let loss_report = loss_report_for_buffer_with_quality(
        window_id,
        &buffer_status,
        &freeze_samples,
        sample_quality,
    );
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
            survives_target_reboot: false,
            bounded_by: vec![
                "max_freeze_bytes".to_string(),
                "max_disk_bytes".to_string(),
                "max_frozen_incidents".to_string(),
                "retention_policy".to_string(),
                "target_reboot_survival_storage_dependent".to_string(),
                "write_durability_best_effort_no_fsync".to_string(),
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
    write_jsonl(&incident_dir.join("samples.jsonl"), &freeze_samples)?;
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

pub fn freeze_recorder_trigger(
    artifact_root: impl AsRef<Path>,
    incident_id: &str,
    window_id: &str,
    trigger_name: &str,
    trigger_time_mono_ns: u64,
    ring: &RecorderRing,
    budget: &RecorderBudget,
) -> AdcResult<RecorderTriggerFreeze> {
    validate_recorder_file_segment(incident_id, "incident_id")?;
    validate_recorder_file_segment(window_id, "window_id")?;
    validate_preservation_reason_name(trigger_name)?;
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
    let (freeze_samples, sample_quality) = samples_within_freeze_budget(ring.samples(), budget)?;
    let loss_report = loss_report_for_buffer_with_quality(
        window_id,
        &buffer_status,
        &freeze_samples,
        sample_quality,
    );
    let start = ring
        .samples()
        .first()
        .map(|sample| sample.time_mono_ns)
        .unwrap_or(trigger_time_mono_ns);
    let end = ring
        .samples()
        .last()
        .map(|sample| sample.time_mono_ns)
        .unwrap_or(trigger_time_mono_ns);

    let mut artifact_refs = BTreeMap::new();
    artifact_refs.insert(
        "trigger_event".to_string(),
        format!("artifact://recorder/incidents/{incident_id}/trigger_event.json"),
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
        marker_id: None,
        freeze_reason: "trigger_policy".to_string(),
        preservation_reason: PreservationReason {
            kind: "trigger_policy".to_string(),
            name: trigger_name.to_string(),
        },
        time_range_mono_ns: TimeRange { start, end },
        pre_window_ms: budget.max_retention_ms,
        post_window_ms: 0,
        persistence: FrozenWindowPersistence {
            persistence_mode: "bounded_artifact_bundle".to_string(),
            survives_daemon_restart: true,
            survives_target_reboot: false,
            bounded_by: vec![
                "max_freeze_bytes".to_string(),
                "max_disk_bytes".to_string(),
                "max_frozen_incidents".to_string(),
                "retention_policy".to_string(),
                "target_reboot_survival_storage_dependent".to_string(),
                "write_durability_best_effort_no_fsync".to_string(),
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
        marker_id: None,
        freeze_reason: "trigger_policy".to_string(),
        frozen_window_ref: Some(format!(
            "artifact://recorder/incidents/{incident_id}/frozen_window.json"
        )),
        loss_report_ref: Some(format!(
            "artifact://recorder/incidents/{incident_id}/loss_report.json"
        )),
        created_at_mono_ns: trigger_time_mono_ns,
        updated_at_mono_ns: trigger_time_mono_ns,
        data_quality: frozen_window.data_quality.clone(),
    };

    write_json(
        &incident_dir.join("trigger_event.json"),
        &serde_json::json!({
            "schema_version": "obs.recorder_trigger_event.v1",
            "trigger_name": trigger_name,
            "trigger_time_mono_ns": trigger_time_mono_ns,
            "agent_contract": "preservation_reason_only",
            "root_cause_claim": false,
            "data_quality": medium_quality()
        }),
    )?;
    write_jsonl(&incident_dir.join("samples.jsonl"), &freeze_samples)?;
    write_json(&incident_dir.join("loss_report.json"), &loss_report)?;
    write_json(&incident_dir.join("frozen_window.json"), &frozen_window)?;
    write_json(&incident_dir.join("incident.json"), &incident)?;

    Ok(RecorderTriggerFreeze {
        incident,
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

fn validate_preservation_reason_name(name: &str) -> AdcResult<()> {
    let normalized = name.to_ascii_lowercase().replace(['_', '-'], " ");
    let forbidden = [
        "root cause",
        "caused",
        "cause detected",
        "driver bug",
        "bad firmware",
    ];
    if forbidden.iter().any(|needle| normalized.contains(needle)) {
        return Err(AdcError::Artifact(format!(
            "recorder trigger name must be symptom/event oriented, not a root-cause claim: {name}"
        )));
    }
    Ok(())
}

fn loss_report_for_buffer_with_quality(
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

fn samples_within_freeze_budget(
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

pub fn validate_recorder_file_segment(value: &str, label: &str) -> AdcResult<()> {
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

pub fn recorder_ring_capacity_for_budget(budget: &RecorderBudget) -> usize {
    const ESTIMATED_SAMPLE_BYTES: u64 = 256;
    let retention_seconds = budget.max_retention_ms.saturating_add(999) / 1000;
    let capacity_by_rate = budget
        .max_samples_per_second
        .saturating_mul(retention_seconds.max(1));
    let capacity_by_memory = budget.max_memory_bytes / ESTIMATED_SAMPLE_BYTES;
    capacity_by_rate.min(capacity_by_memory).max(1) as usize
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
    recorder_status_for_with_overhead(
        target_id,
        active_profile,
        previous_state,
        recorder_state,
        buffer_status,
        budget,
        overhead,
    )
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
    let target_id = target_id.into();
    let dropped = buffer_status.data_quality.drop_count;
    RecorderStatus {
        schema_version: "obs.recorder_status.v1".to_string(),
        target_id: target_id.clone(),
        recorder_state: RecorderState::parse(recorder_state),
        previous_state: previous_state.map(RecorderState::parse),
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
        overhead,
        data_quality: data_quality_for_drop_count(dropped),
    }
}

pub fn recorder_overhead_for_service_run(
    target_id: impl Into<String>,
    buffer_status: &RecorderBufferStatus,
    accounting: RecorderOverheadAccounting,
) -> RecorderOverhead {
    let target_id = target_id.into();
    let dropped = buffer_status.data_quality.drop_count;
    RecorderOverhead {
        schema_version: "obs.recorder_overhead.v1".to_string(),
        target_id,
        overhead_scope: accounting.overhead_scope,
        since_mono_ns: accounting.since_mono_ns,
        through_mono_ns: accounting.through_mono_ns,
        cpu_percent: None,
        memory_bytes: None,
        disk_write_bytes: accounting.disk_write_bytes,
        artifact_bytes: accounting.artifact_bytes,
        status_write_bytes: accounting.status_write_bytes,
        frozen_artifact_bytes: accounting.frozen_artifact_bytes,
        samples_jsonl_bytes: accounting.samples_jsonl_bytes,
        incident_count: accounting.incident_count,
        estimated_memory_ring_bytes: estimated_recorder_memory_bytes(buffer_status),
        wakeup_rate_hz: None,
        self_samples_dropped: dropped,
        data_quality: recorder_overhead_data_quality(dropped),
    }
}

fn default_recorder_overhead(
    target_id: &str,
    buffer_status: &RecorderBufferStatus,
    self_samples_dropped: u64,
) -> RecorderOverhead {
    RecorderOverhead {
        schema_version: "obs.recorder_overhead.v1".to_string(),
        target_id: target_id.to_string(),
        overhead_scope: RecorderOverheadScope::CurrentStatusSnapshot,
        since_mono_ns: buffer_status.current_retained_range_mono_ns.start,
        through_mono_ns: buffer_status.current_retained_range_mono_ns.end,
        cpu_percent: None,
        memory_bytes: None,
        disk_write_bytes: 0,
        artifact_bytes: 0,
        status_write_bytes: 0,
        frozen_artifact_bytes: 0,
        samples_jsonl_bytes: 0,
        incident_count: 0,
        estimated_memory_ring_bytes: estimated_recorder_memory_bytes(buffer_status),
        wakeup_rate_hz: None,
        self_samples_dropped,
        data_quality: recorder_overhead_data_quality(self_samples_dropped),
    }
}

fn estimated_recorder_memory_bytes(buffer_status: &RecorderBufferStatus) -> u64 {
    const ESTIMATED_SAMPLE_BYTES: u64 = 256;
    let retained_samples = buffer_status
        .signals
        .iter()
        .map(|signal| signal.recorded_samples)
        .sum::<u64>();
    retained_samples.saturating_mul(ESTIMATED_SAMPLE_BYTES)
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

fn recorder_overhead_data_quality(drop_count: u64) -> DataQuality {
    let mut data_quality = data_quality_for_drop_count(drop_count);
    data_quality
        .missing
        .push("recorder CPU and memory overhead are not measured in this MVP".to_string());
    data_quality.notes.push(
        "disk_write_bytes and status_write_bytes are cumulative for the service-run scope"
            .to_string(),
    );
    data_quality.notes.push(
        "artifact_bytes is a write-path retained-size estimate and may overcount overwritten marker/status artifacts in this MVP"
            .to_string(),
    );
    data_quality
        .notes
        .push("status_write_bytes excludes the current status artifact write".to_string());
    data_quality
        .notes
        .push("estimated_memory_ring_bytes uses a fixed per-sample estimate".to_string());
    data_quality
}
