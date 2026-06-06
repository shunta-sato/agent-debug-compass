use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{AdcError, AdcResult, ClockConfidence, DataQuality};
use crate::{
    TriggerBudgetDecision, TriggerDecision, TriggerDecisionOutcome, TriggerDecisionReason,
    TriggerKind,
};

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
pub enum RecorderIncidentCountScope {
    ArtifactRoot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecorderAdmissionDecision {
    Accept,
    Refuse,
    UnknownFailClosed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecorderAdmissionRefusalReason {
    MaxFrozenIncidentsExceeded,
    IncidentInventoryUnreadable,
    IncidentInventoryUnreliable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecorderFreezeDecisionSource {
    TriggerPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecorderFreezeDecisionOutcome {
    Refused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetainedArtifactBytesEstimateScope {
    CountedIncidentsOnly,
    FullInventory,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecorderBudgetStatus {
    pub schema_version: String,
    pub budget_id: String,
    pub incident_count_scope: RecorderIncidentCountScope,
    pub admission_decision: RecorderAdmissionDecision,
    pub admission_refusal_reason: Option<RecorderAdmissionRefusalReason>,
    pub max_frozen_incidents: u64,
    pub existing_frozen_incidents: u64,
    pub frozen_incidents_this_run: u64,
    pub remaining_frozen_incidents: u64,
    pub current_run_included_in_existing: bool,
    pub max_disk_bytes: u64,
    pub retained_artifact_bytes_estimate: u64,
    pub retained_artifact_bytes_estimate_scope: RetainedArtifactBytesEstimateScope,
    pub inventory_truncated: bool,
    pub malformed_entry_count: u64,
    pub unreadable_entry_count: u64,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecorderFreezeDecision {
    pub schema_version: String,
    pub source: RecorderFreezeDecisionSource,
    pub decision: RecorderFreezeDecisionOutcome,
    pub reason: RecorderAdmissionRefusalReason,
    pub budget_status: RecorderBudgetStatus,
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
    pub budget_status: RecorderBudgetStatus,
    pub overhead: RecorderOverhead,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecorderStatusInput {
    pub target_id: String,
    pub active_profile: Option<String>,
    pub previous_state: Option<String>,
    pub recorder_state: String,
    pub buffer_status: RecorderBufferStatus,
    pub budget: RecorderBudget,
    pub budget_status: RecorderBudgetStatus,
    pub overhead: RecorderOverhead,
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
    pub budget_status: Option<RecorderBudgetStatus>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecorderLayer {
    App,
    Middleware,
    Os,
    Kernel,
    Driver,
    Hardware,
    Network,
    AdcSelf,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecorderCoverageState {
    Covered,
    Partial,
    Missing,
    Unavailable,
    NotExpected,
    Degraded,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecorderCoverageConfidence {
    Unknown,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedSamplesBasis {
    ConfiguredProfileInterval,
    BudgetedRecorderInterval,
    Inferred,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecorderExpectedSignal {
    pub schema_version: String,
    pub signal_id: String,
    pub collector_id: String,
    pub layer: RecorderLayer,
    pub configured_interval_ms: u64,
    pub effective_interval_ms: u64,
    pub required_capability: Option<String>,
    pub capability_status: crate::CapabilityStatus,
    pub required_privilege: String,
    pub cost_tier: String,
    pub priority: String,
    pub expected_samples: Option<u64>,
    pub expected: bool,
    pub expectation_source: String,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecorderSignalCoverage {
    pub signal_id: String,
    pub expected: bool,
    pub coverage_state: RecorderCoverageState,
    pub coverage_confidence: RecorderCoverageConfidence,
    pub configured_interval_ms: u64,
    pub effective_interval_ms: u64,
    pub expected_samples_configured: Option<u64>,
    pub expected_samples_budgeted: Option<u64>,
    pub expected_samples: Option<u64>,
    pub expected_samples_basis: ExpectedSamplesBasis,
    pub retained_samples_before_freeze: u64,
    pub exported_samples: u64,
    pub dropped_samples: u64,
    pub truncated_samples_due_to_freeze_budget: u64,
    pub loss_report_ref: String,
    pub loss_collector_id: String,
    pub loss_reasons: Vec<String>,
    pub capability_status: crate::CapabilityStatus,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecorderCoverageSummary {
    pub expected_signal_count: u64,
    pub covered_signal_count: u64,
    pub missing_signal_count: u64,
    pub partial_signal_count: u64,
    pub unavailable_signal_count: u64,
    pub overall_coverage_percent: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecorderObservationCoverage {
    pub schema_version: String,
    pub target_id: String,
    pub incident_id: String,
    pub window_id: String,
    pub time_range_mono_ns: TimeRange,
    pub coverage_scope: String,
    pub expected_signals: Vec<RecorderExpectedSignal>,
    pub signals: Vec<RecorderSignalCoverage>,
    pub summary: RecorderCoverageSummary,
    pub loss_report_ref: String,
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

pub fn recorder_trigger_decision_dir(artifact_root: impl AsRef<Path>) -> PathBuf {
    artifact_root.as_ref().join("recorder/trigger-decisions")
}

pub fn recorder_pending_marker_ref(marker_id: &str) -> AdcResult<String> {
    validate_recorder_file_segment(marker_id, "marker_id")?;
    Ok(format!(
        "artifact://recorder/markers/pending/{marker_id}.json"
    ))
}

pub fn recorder_trigger_decision_ref(decision_id: &str) -> AdcResult<String> {
    validate_recorder_file_segment(decision_id, "decision_id")?;
    Ok(format!(
        "artifact://recorder/trigger-decisions/{decision_id}.json"
    ))
}

pub fn recorder_incident_artifact_ref(incident_id: &str, artifact_name: &str) -> AdcResult<String> {
    validate_recorder_file_segment(incident_id, "incident_id")?;
    match artifact_name {
        "incident.json"
        | "frozen_window.json"
        | "coverage.json"
        | "loss_report.json"
        | "samples.jsonl"
        | "marker.json"
        | "trigger_event.json"
        | "trigger_decision.json" => Ok(format!(
            "artifact://recorder/incidents/{incident_id}/{artifact_name}"
        )),
        _ => Err(AdcError::Artifact(format!(
            "unsupported recorder incident artifact {artifact_name}"
        ))),
    }
}

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
        budget_status: None,
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
        budget_status: None,
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
        budget_status: None,
        data_quality: DataQuality {
            throttled: true,
            clock_confidence: ClockConfidence::Medium,
            missing: vec!["incident window was not frozen due to recorder budget".to_string()],
            notes: vec!["pending marker was consumed and recorded as refused".to_string()],
            ..Default::default()
        },
    }
}

pub fn recorder_marker_result_for_refused_with_budget_status(
    marker: RecorderMarker,
    expected_incident_id: String,
    reason: impl Into<String>,
    budget_status: RecorderBudgetStatus,
) -> RecorderMarkerResult {
    let mut result = recorder_marker_result_for_refused(marker, expected_incident_id, reason);
    result.budget_status = Some(budget_status);
    result
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
    expected_signals: BTreeMap<String, RecorderExpectedSignal>,
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
        Self::with_expected_signal_model(
            target_id,
            capacity,
            retention_ms,
            expected_signal_ids
                .into_iter()
                .map(|signal_id| recorder_expected_signal_for_id(&signal_id, 1000)),
        )
    }

    pub fn with_expected_signal_model(
        target_id: impl Into<String>,
        capacity: usize,
        retention_ms: u64,
        expected_signals: impl IntoIterator<Item = RecorderExpectedSignal>,
    ) -> Self {
        Self {
            target_id: target_id.into(),
            capacity: capacity.max(1),
            retention_ms,
            samples: Vec::new(),
            dropped_by_signal: BTreeMap::new(),
            expected_signals: expected_signals
                .into_iter()
                .map(|signal| (signal.signal_id.clone(), signal))
                .collect(),
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

    pub fn expected_signals(&self) -> Vec<RecorderExpectedSignal> {
        self.expected_signals.values().cloned().collect()
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
            .chain(self.expected_signals.keys())
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
                self.expected_signals.contains_key(&signal_id) && recorded == 0 && dropped == 0;
            let mut signal_quality = data_quality_for_drop_count(dropped);
            if expected_but_absent {
                signal_quality.missing.push(format!(
                    "expected recorder signal {signal_id} has no retained samples"
                ));
                buffer_quality.missing.push(format!(
                    "expected recorder signal {signal_id} has no retained samples"
                ));
            }
            let configured_interval_ms = self
                .expected_signals
                .get(&signal_id)
                .map(|signal| signal.configured_interval_ms)
                .unwrap_or(1000);
            signals.push(RecorderSignalStatus {
                signal_id,
                configured_interval_ms,
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
    artifact_refs.insert(
        "observation_coverage".to_string(),
        format!("artifact://recorder/incidents/{incident_id}/coverage.json"),
    );
    let time_range = TimeRange { start, end };
    let expected_signals = ring.expected_signals();
    let coverage = observation_coverage_for_freeze(
        CoverageBuildContext {
            incident_id,
            window_id,
            target_id: &buffer_status.target_id,
            time_range: time_range.clone(),
        },
        &buffer_status,
        &expected_signals,
        &loss_report,
        budget,
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
        time_range_mono_ns: time_range,
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
    write_json(&incident_dir.join("coverage.json"), &coverage)?;
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
    freeze_recorder_trigger_with_decision(
        artifact_root,
        RecorderTriggerFreezeRequest {
            incident_id,
            window_id,
            trigger_name,
            trigger_time_mono_ns,
            trigger_decision: None,
        },
        ring,
        budget,
    )
}

#[derive(Debug)]
pub struct RecorderTriggerFreezeRequest<'a> {
    pub incident_id: &'a str,
    pub window_id: &'a str,
    pub trigger_name: &'a str,
    pub trigger_time_mono_ns: u64,
    pub trigger_decision: Option<TriggerDecision>,
}

pub fn freeze_recorder_trigger_with_decision(
    artifact_root: impl AsRef<Path>,
    request: RecorderTriggerFreezeRequest<'_>,
    ring: &RecorderRing,
    budget: &RecorderBudget,
) -> AdcResult<RecorderTriggerFreeze> {
    let incident_id = request.incident_id;
    let window_id = request.window_id;
    let trigger_name = request.trigger_name;
    let trigger_time_mono_ns = request.trigger_time_mono_ns;
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
        "trigger_decision".to_string(),
        format!("artifact://recorder/incidents/{incident_id}/trigger_decision.json"),
    );
    artifact_refs.insert(
        "samples".to_string(),
        format!("artifact://recorder/incidents/{incident_id}/samples.jsonl"),
    );
    artifact_refs.insert(
        "loss_report".to_string(),
        format!("artifact://recorder/incidents/{incident_id}/loss_report.json"),
    );
    artifact_refs.insert(
        "observation_coverage".to_string(),
        format!("artifact://recorder/incidents/{incident_id}/coverage.json"),
    );
    let time_range = TimeRange { start, end };
    let expected_signals = ring.expected_signals();
    let coverage = observation_coverage_for_freeze(
        CoverageBuildContext {
            incident_id,
            window_id,
            target_id: &buffer_status.target_id,
            time_range: time_range.clone(),
        },
        &buffer_status,
        &expected_signals,
        &loss_report,
        budget,
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
        time_range_mono_ns: time_range,
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

    let coverage_ref = format!("artifact://recorder/incidents/{incident_id}/coverage.json");
    let trigger_event_ref =
        format!("artifact://recorder/incidents/{incident_id}/trigger_event.json");
    let trigger_decision_ref =
        format!("artifact://recorder/incidents/{incident_id}/trigger_decision.json");
    let decision = request.trigger_decision.unwrap_or_else(|| {
        default_trigger_decision_for_freeze(
            incident_id,
            trigger_name,
            &coverage_ref,
            &trigger_event_ref,
        )
    });
    write_json(
        &incident_dir.join("trigger_event.json"),
        &serde_json::json!({
            "schema_version": "obs.recorder_trigger_event.v1",
            "trigger_name": trigger_name,
            "trigger_time_mono_ns": trigger_time_mono_ns,
            "agent_contract": "preservation_reason_only",
            "root_cause_claim": false,
            "trigger_decision_ref": trigger_decision_ref,
            "coverage_ref": coverage_ref,
            "data_quality": medium_quality()
        }),
    )?;
    write_json(&incident_dir.join("trigger_decision.json"), &decision)?;
    write_jsonl(&incident_dir.join("samples.jsonl"), &freeze_samples)?;
    write_json(&incident_dir.join("loss_report.json"), &loss_report)?;
    write_json(&incident_dir.join("coverage.json"), &coverage)?;
    write_json(&incident_dir.join("frozen_window.json"), &frozen_window)?;
    write_json(&incident_dir.join("incident.json"), &incident)?;

    Ok(RecorderTriggerFreeze {
        incident,
        frozen_window,
        run_dir: incident_dir,
    })
}

fn default_trigger_decision_for_freeze(
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

struct CoverageBuildContext<'a> {
    incident_id: &'a str,
    window_id: &'a str,
    target_id: &'a str,
    time_range: TimeRange,
}

fn observation_coverage_for_freeze(
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
    recorder_status_from_input(RecorderStatusInput {
        target_id,
        active_profile: active_profile.map(str::to_string),
        previous_state: previous_state.map(str::to_string),
        recorder_state: recorder_state.to_string(),
        buffer_status,
        budget_status: recorder_default_budget_status(&budget, 0),
        budget,
        overhead,
    })
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
    let budget_status = recorder_default_budget_status(&budget, 0);
    recorder_status_from_input(RecorderStatusInput {
        target_id: target_id.into(),
        active_profile: active_profile.map(str::to_string),
        previous_state: previous_state.map(str::to_string),
        recorder_state: recorder_state.to_string(),
        buffer_status,
        budget,
        budget_status,
        overhead,
    })
}

pub fn recorder_status_from_input(input: RecorderStatusInput) -> RecorderStatus {
    let dropped = input.buffer_status.data_quality.drop_count;
    RecorderStatus {
        schema_version: "obs.recorder_status.v1".to_string(),
        target_id: input.target_id,
        recorder_state: RecorderState::parse(&input.recorder_state),
        previous_state: input.previous_state.as_deref().map(RecorderState::parse),
        active_profile: input.active_profile.clone(),
        armed: input.active_profile.is_some(),
        storage: RecorderStorageStatus {
            storage_mode: "memory_ring".to_string(),
            volatile: true,
            survives_daemon_restart: false,
            survives_target_reboot: false,
            survives_power_loss: false,
        },
        buffer_status: input.buffer_status,
        budget: input.budget,
        budget_status: input.budget_status,
        overhead: input.overhead,
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
