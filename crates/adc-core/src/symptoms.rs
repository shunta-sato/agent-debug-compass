use serde::{Deserialize, Serialize};

use crate::DataQuality;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymptomKind {
    ServiceUnhealthy,
    LatencyTimeout,
    MemoryGrowth,
    CpuSaturation,
    NetworkDegradation,
    DiskIoPressure,
    ConfigDrift,
    ThermalPower,
    SensorGap,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedSymptom {
    pub schema_version: String,
    pub original: String,
    pub normalized: String,
    pub kind: SymptomKind,
    pub parser_quality: String,
    pub summary: String,
    pub data_quality: DataQuality,
}

pub fn normalize_symptom(input: &str) -> NormalizedSymptom {
    let original = input.trim().to_string();
    let lower = original.to_ascii_lowercase();
    let (kind, normalized, parser_quality) = if matches_exact(&lower, "service_unhealthy") {
        (SymptomKind::ServiceUnhealthy, "service_unhealthy", "exact")
    } else if matches_exact(&lower, "latency_timeout") {
        (SymptomKind::LatencyTimeout, "latency_timeout", "exact")
    } else if matches_exact(&lower, "memory_growth") {
        (SymptomKind::MemoryGrowth, "memory_growth", "exact")
    } else if matches_exact(&lower, "cpu_saturation") {
        (SymptomKind::CpuSaturation, "cpu_saturation", "exact")
    } else if matches_exact(&lower, "network_degradation") {
        (
            SymptomKind::NetworkDegradation,
            "network_degradation",
            "exact",
        )
    } else if matches_exact(&lower, "disk_io_pressure") {
        (SymptomKind::DiskIoPressure, "disk_io_pressure", "exact")
    } else if matches_exact(&lower, "config_drift") {
        (SymptomKind::ConfigDrift, "config_drift", "exact")
    } else if matches_exact(&lower, "thermal_power") {
        (SymptomKind::ThermalPower, "thermal_power", "exact")
    } else if matches_exact(&lower, "sensor_gap") {
        (SymptomKind::SensorGap, "sensor_gap", "exact")
    } else if contains_any(
        &lower,
        &[
            "latency",
            "timeout",
            "timing out",
            "slow",
            "p99",
            "deadline",
        ],
    ) {
        (SymptomKind::LatencyTimeout, "latency_timeout", "alias")
    } else if contains_any(&lower, &["memory", "rss", "oom", "leak", "pressure"]) {
        (SymptomKind::MemoryGrowth, "memory_growth", "alias")
    } else if contains_any(&lower, &["cpu", "runqueue", "load average", "saturation"]) {
        (SymptomKind::CpuSaturation, "cpu_saturation", "alias")
    } else if contains_any(
        &lower,
        &[
            "network",
            "packet",
            "loss",
            "interface",
            "drops",
            "rx",
            "tx",
        ],
    ) {
        (
            SymptomKind::NetworkDegradation,
            "network_degradation",
            "alias",
        )
    } else if contains_any(&lower, &["disk", "io", "i/o", "storage", "read stall"]) {
        (SymptomKind::DiskIoPressure, "disk_io_pressure", "alias")
    } else if contains_any(
        &lower,
        &["config", "deploy", "version", "drift", "rollback"],
    ) {
        (SymptomKind::ConfigDrift, "config_drift", "alias")
    } else if contains_any(
        &lower,
        &["thermal", "hot", "throttle", "throttling", "power"],
    ) {
        (SymptomKind::ThermalPower, "thermal_power", "alias")
    } else if contains_any(&lower, &["sensor", "frame", "gap", "dropped frame"]) {
        (SymptomKind::SensorGap, "sensor_gap", "alias")
    } else if contains_any(
        &lower,
        &[
            "service",
            "down",
            "unhealthy",
            "crash",
            "restart",
            "inactive",
        ],
    ) {
        (SymptomKind::ServiceUnhealthy, "service_unhealthy", "alias")
    } else {
        (SymptomKind::Unknown, "unknown", "unknown")
    };

    let mut data_quality = DataQuality {
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };
    if kind == SymptomKind::Unknown {
        data_quality.missing.push(format!(
            "unrecognized symptom: {}",
            empty_as_unknown(&original)
        ));
    }

    NormalizedSymptom {
        schema_version: "obs.normalized_symptom.v1".to_string(),
        original,
        normalized: normalized.to_string(),
        kind,
        parser_quality: parser_quality.to_string(),
        summary: format!("Symptom normalized to {normalized} for cause-neutral route selection."),
        data_quality,
    }
}

fn matches_exact(input: &str, expected: &str) -> bool {
    let normalized = input.replace([' ', '-'], "_");
    normalized == expected
}

fn contains_any(input: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| input.contains(needle))
}

fn empty_as_unknown(input: &str) -> &str {
    if input.is_empty() {
        "<empty>"
    } else {
        input
    }
}
