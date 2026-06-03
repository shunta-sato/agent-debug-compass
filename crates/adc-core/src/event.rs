use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub run_id: String,
    pub source: String,
    pub event_type: String,
    pub time_mono_ns: u64,
    pub time_range_ns: TimeRangeNs,
    pub clock_source: ClockSource,
    pub collector_id: String,
    pub profile_id: String,
    pub payload: serde_json::Value,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRangeNs {
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClockSource {
    #[serde(rename = "CLOCK_MONOTONIC")]
    Monotonic,
    #[serde(rename = "CLOCK_REALTIME")]
    Realtime,
    #[serde(rename = "collector_clock")]
    CollectorClock,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataQuality {
    pub dropped: bool,
    pub drop_count: u64,
    pub throttled: bool,
    pub missing: Vec<String>,
    pub truncated: bool,
    pub clock_confidence: String,
    pub notes: Vec<String>,
}

impl Default for DataQuality {
    fn default() -> Self {
        Self {
            dropped: false,
            drop_count: 0,
            throttled: false,
            missing: Vec::new(),
            truncated: false,
            clock_confidence: "unknown".to_string(),
            notes: Vec::new(),
        }
    }
}
