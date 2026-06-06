use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{DataQuality, TriggerDecision};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecorderPowerSource {
    AcPower,
    Battery,
    External,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecorderBatteryState {
    Charging,
    Discharging,
    Full,
    Low,
    Critical,
    Absent,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecorderResourceScope {
    ServiceRun,
    CurrentStatusSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecorderPowerMode {
    Unknown,
    AcPower,
    BatteryNormal,
    BatteryLow,
    BatteryCritical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecorderDegradationReason {
    BatteryLow,
    BatteryCritical,
    MemoryBudget,
    SampleRateBudget,
    UnknownPowerConservativePolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecorderDegradationAction {
    Downsample,
    ShortenRetention,
    DropLowPrioritySignal,
    ReduceStatusWriteRate,
    RefuseFreeze,
    PartialFreeze,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecorderDegradationDecision {
    pub schema_version: String,
    pub decision_id: String,
    pub reason: RecorderDegradationReason,
    pub actions: Vec<RecorderDegradationAction>,
    pub affected_signals: Vec<String>,
    pub preserved_signals: Vec<String>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecorderPowerPolicyMode {
    pub mode: RecorderPowerMode,
    pub max_samples_per_second: u64,
    pub max_retention_ms: u64,
    pub low_priority_signals_enabled: bool,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecorderPowerPolicy {
    pub schema_version: String,
    pub policy_id: String,
    pub modes: Vec<RecorderPowerPolicyMode>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecorderResourceStatus {
    pub schema_version: String,
    pub target_id: String,
    pub resource_scope: RecorderResourceScope,
    pub power_source: RecorderPowerSource,
    pub battery_state: RecorderBatteryState,
    pub battery_percent: Option<f64>,
    pub policy_mode: RecorderPowerMode,
    pub estimated_ring_memory_bytes: u64,
    pub max_memory_bytes: u64,
    pub wakeup_rate_hz: Option<f64>,
    pub recorder_loop_rate_hz: Option<f64>,
    pub recorder_sample_rate_hz: Option<f64>,
    pub status_write_rate_hz: Option<f64>,
    pub continuous_ring_disk_write_bytes: u64,
    pub status_write_bytes: u64,
    pub frozen_artifact_write_bytes: u64,
    pub network_upload_bytes: u64,
    pub degradation_decisions: Vec<RecorderDegradationDecision>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecorderPowerSnapshot {
    pub power_source: RecorderPowerSource,
    pub battery_state: RecorderBatteryState,
    pub battery_percent: Option<f64>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RecorderResourceRates {
    pub recorder_loop_rate_hz: Option<f64>,
    pub recorder_sample_rate_hz: Option<f64>,
    pub status_write_rate_hz: Option<f64>,
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
    pub resource_status: RecorderResourceStatus,
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
    pub resource_status: Option<RecorderResourceStatus>,
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

pub struct RecorderTriggerFreezeRequest<'a> {
    pub incident_id: &'a str,
    pub window_id: &'a str,
    pub trigger_name: &'a str,
    pub trigger_time_mono_ns: u64,
    pub trigger_decision: Option<TriggerDecision>,
}
