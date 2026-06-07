use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use crate::{AdcError, AdcResult, CapabilityStatus, ClockConfidence, DataQuality};

use super::{
    model::{RecorderGapRange, TimeRange},
    quality::medium_quality,
};

const MAX_LOG_EVENTS_RETAINED: usize = 1_000;
const MAX_LOG_BYTES_PER_DRAIN: u64 = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecorderLogEvent {
    pub schema_version: String,
    pub source_id: String,
    pub source_kind: String,
    pub time_mono_ns: u64,
    pub severity: String,
    pub message: String,
    pub line_index: u64,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecorderLogSourceStatus {
    pub schema_version: String,
    pub target_id: String,
    pub source_id: String,
    pub source_kind: String,
    pub source_ref: String,
    pub cursor_mode: String,
    pub cursor_token: Option<String>,
    pub cursor_confidence: String,
    pub continuity_state: String,
    pub capability_status: CapabilityStatus,
    pub permission_status: String,
    pub last_observed_mono_ns: Option<u64>,
    pub events_observed: u64,
    pub events_exported: u64,
    pub events_truncated: u64,
    pub rotation_detected: bool,
    pub blackout_ranges: Vec<RecorderGapRange>,
    pub artifact_refs: BTreeMap<String, String>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecorderBlackoutReport {
    pub schema_version: String,
    pub target_id: String,
    pub incident_id: String,
    pub window_id: String,
    pub time_range_mono_ns: TimeRange,
    pub blackout_detected: bool,
    pub sources: Vec<RecorderLogSourceStatus>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecorderLogSnapshot {
    pub source_status: RecorderLogSourceStatus,
    pub events: Vec<RecorderLogEvent>,
}

#[derive(Debug, Clone)]
pub struct AppendOnlyLogCursor {
    source_id: String,
    source_kind: String,
    source_ref: String,
    path: PathBuf,
    offset: u64,
    identity: Option<FileIdentity>,
    events_observed: u64,
    events_truncated: u64,
    rotation_detected: bool,
    blackout_ranges: Vec<RecorderGapRange>,
    retained_events: Vec<RecorderLogEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    dev: u64,
    ino: u64,
}

impl AppendOnlyLogCursor {
    pub fn new_app_log(path: impl Into<PathBuf>) -> Self {
        Self {
            source_id: "app_log".to_string(),
            source_kind: "append_file".to_string(),
            source_ref: "target://logs/app_log".to_string(),
            path: path.into(),
            offset: 0,
            identity: None,
            events_observed: 0,
            events_truncated: 0,
            rotation_detected: false,
            blackout_ranges: Vec::new(),
            retained_events: Vec::new(),
        }
    }

    pub fn poll(&mut self, target_id: &str, time_mono_ns: u64) -> RecorderLogSnapshot {
        match self.poll_inner(time_mono_ns) {
            Ok(status) => RecorderLogSnapshot {
                source_status: self.status(target_id, status, time_mono_ns, medium_quality()),
                events: self.retained_events.clone(),
            },
            Err(err) => {
                let mut quality = DataQuality {
                    clock_confidence: ClockConfidence::Medium,
                    ..Default::default()
                };
                quality
                    .missing
                    .push(format!("{} source unavailable: {err}", self.source_id));
                RecorderLogSnapshot {
                    source_status: self.status(target_id, "unavailable", time_mono_ns, quality),
                    events: self.retained_events.clone(),
                }
            }
        }
    }

    fn poll_inner(&mut self, time_mono_ns: u64) -> AdcResult<&'static str> {
        let metadata = std::fs::metadata(&self.path).map_err(|err| {
            AdcError::Artifact(format!(
                "failed to stat log source {}: {err}",
                self.source_id
            ))
        })?;
        let identity = file_identity(&metadata);
        let size = metadata.len();
        let mut state = "silent_with_continuity";
        if self.identity.is_some_and(|current| current != identity) || size < self.offset {
            self.rotation_detected = true;
            self.blackout_ranges.push(RecorderGapRange {
                start_mono_ns: time_mono_ns,
                end_mono_ns: time_mono_ns,
            });
            self.offset = 0;
            state = "rotated";
        }
        self.identity = Some(identity);
        if size > self.offset {
            let available = size.saturating_sub(self.offset);
            let read_len = available.min(MAX_LOG_BYTES_PER_DRAIN);
            let mut file = File::open(&self.path).map_err(|err| {
                AdcError::Artifact(format!(
                    "failed to open log source {}: {err}",
                    self.source_id
                ))
            })?;
            file.seek(SeekFrom::Start(self.offset)).map_err(|err| {
                AdcError::Artifact(format!(
                    "failed to seek log source {}: {err}",
                    self.source_id
                ))
            })?;
            let mut bytes = vec![0; read_len as usize];
            let read = file.read(&mut bytes).map_err(|err| {
                AdcError::Artifact(format!(
                    "failed to read log source {}: {err}",
                    self.source_id
                ))
            })?;
            bytes.truncate(read);
            self.offset = self.offset.saturating_add(read as u64);
            if available > read as u64 {
                self.events_truncated = self.events_truncated.saturating_add(1);
                state = "gap_detected";
            } else if state != "rotated" {
                state = "continuous";
            }
            let text = String::from_utf8_lossy(&bytes);
            for line in text.lines().filter(|line| !line.trim().is_empty()) {
                self.events_observed = self.events_observed.saturating_add(1);
                self.retained_events.push(RecorderLogEvent {
                    schema_version: "obs.recorder_log_event.v1".to_string(),
                    source_id: self.source_id.clone(),
                    source_kind: self.source_kind.clone(),
                    time_mono_ns,
                    severity: severity_for_log_line(line).to_string(),
                    message: line.trim().to_string(),
                    line_index: self.events_observed,
                    data_quality: medium_quality(),
                });
            }
            if self.retained_events.len() > MAX_LOG_EVENTS_RETAINED {
                let excess = self.retained_events.len() - MAX_LOG_EVENTS_RETAINED;
                self.retained_events.drain(0..excess);
                self.events_truncated = self.events_truncated.saturating_add(excess as u64);
            }
        }
        Ok(state)
    }

    fn status(
        &self,
        target_id: &str,
        continuity_state: &str,
        time_mono_ns: u64,
        mut data_quality: DataQuality,
    ) -> RecorderLogSourceStatus {
        if self.rotation_detected {
            data_quality.truncated = true;
            data_quality
                .notes
                .push("log source rotation or truncation was detected".to_string());
        }
        if self.events_truncated > 0 {
            data_quality.truncated = true;
            data_quality.notes.push(format!(
                "{} log event(s) were truncated by log cursor budget",
                self.events_truncated
            ));
        }
        RecorderLogSourceStatus {
            schema_version: "obs.recorder_log_source_status.v1".to_string(),
            target_id: target_id.to_string(),
            source_id: self.source_id.clone(),
            source_kind: self.source_kind.clone(),
            source_ref: self.source_ref.clone(),
            cursor_mode: "append_offset_fallback".to_string(),
            cursor_token: Some(self.offset.to_string()),
            cursor_confidence: "medium".to_string(),
            continuity_state: continuity_state.to_string(),
            capability_status: CapabilityStatus::Supported,
            permission_status: "readable".to_string(),
            last_observed_mono_ns: Some(time_mono_ns),
            events_observed: self.events_observed,
            events_exported: self.retained_events.len() as u64,
            events_truncated: self.events_truncated,
            rotation_detected: self.rotation_detected,
            blackout_ranges: self.blackout_ranges.clone(),
            artifact_refs: BTreeMap::new(),
            data_quality,
        }
    }
}

pub fn blackout_report_for_log_snapshot(
    target_id: &str,
    incident_id: &str,
    window_id: &str,
    time_range: TimeRange,
    snapshot: &RecorderLogSnapshot,
) -> RecorderBlackoutReport {
    let blackout_detected = matches!(
        snapshot.source_status.continuity_state.as_str(),
        "gap_detected" | "rotated" | "unavailable" | "permission_denied"
    ) || !snapshot.source_status.blackout_ranges.is_empty();
    let mut data_quality = medium_quality();
    if blackout_detected {
        data_quality.truncated = true;
        data_quality
            .notes
            .push("one or more expected log sources had a continuity gap".to_string());
    }
    RecorderBlackoutReport {
        schema_version: "obs.recorder_blackout_report.v1".to_string(),
        target_id: target_id.to_string(),
        incident_id: incident_id.to_string(),
        window_id: window_id.to_string(),
        time_range_mono_ns: time_range,
        blackout_detected,
        sources: vec![snapshot.source_status.clone()],
        data_quality,
    }
}

pub fn unavailable_app_log_snapshot(
    target_id: &str,
    time_mono_ns: u64,
    reason: impl Into<String>,
) -> RecorderLogSnapshot {
    let mut data_quality = DataQuality {
        clock_confidence: ClockConfidence::Medium,
        ..Default::default()
    };
    data_quality.missing.push(reason.into());
    RecorderLogSnapshot {
        source_status: RecorderLogSourceStatus {
            schema_version: "obs.recorder_log_source_status.v1".to_string(),
            target_id: target_id.to_string(),
            source_id: "app_log".to_string(),
            source_kind: "append_file".to_string(),
            source_ref: "target://logs/app_log".to_string(),
            cursor_mode: "unknown".to_string(),
            cursor_token: None,
            cursor_confidence: "unknown".to_string(),
            continuity_state: "unavailable".to_string(),
            capability_status: CapabilityStatus::Unknown,
            permission_status: "unavailable".to_string(),
            last_observed_mono_ns: Some(time_mono_ns),
            events_observed: 0,
            events_exported: 0,
            events_truncated: 0,
            rotation_detected: false,
            blackout_ranges: vec![RecorderGapRange {
                start_mono_ns: time_mono_ns,
                end_mono_ns: time_mono_ns,
            }],
            artifact_refs: BTreeMap::new(),
            data_quality,
        },
        events: Vec::new(),
    }
}

pub fn log_source_status_has_continuity(status: &RecorderLogSourceStatus) -> bool {
    matches!(
        status.continuity_state.as_str(),
        "continuous" | "silent_with_continuity"
    )
}

fn severity_for_log_line(line: &str) -> &'static str {
    let lower = line.to_ascii_lowercase();
    if lower.contains("error") {
        "error"
    } else if lower.contains("warn") {
        "warning"
    } else {
        "info"
    }
}

fn file_identity(metadata: &std::fs::Metadata) -> FileIdentity {
    #[cfg(unix)]
    {
        FileIdentity {
            dev: metadata.dev(),
            ino: metadata.ino(),
        }
    }
    #[cfg(not(unix))]
    {
        FileIdentity {
            dev: 0,
            ino: metadata.len(),
        }
    }
}
