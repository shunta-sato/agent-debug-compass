use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    DataQuality, FleetSemanticDiff, ServiceInvestigationPack, ServiceJournalLead,
    ServicePortSummary, ServiceProcessSummary, ServiceStateSummary,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceFact {
    pub fact_id: String,
    #[serde(default = "default_scope", skip_serializing_if = "is_default_scope")]
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    pub source_ref: String,
    pub value: Value,
    #[serde(default, skip_serializing_if = "is_default_data_quality")]
    pub data_quality: DataQuality,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_at_monotonic_ns: Option<u64>,
}

fn default_scope() -> String {
    "run".to_string()
}

fn is_default_scope(scope: &str) -> bool {
    scope == "run"
}

fn is_default_data_quality(data_quality: &DataQuality) -> bool {
    !data_quality.dropped
        && data_quality.drop_count == 0
        && !data_quality.throttled
        && data_quality.missing.is_empty()
        && !data_quality.truncated
        && data_quality.notes.is_empty()
        && matches!(
            data_quality.clock_confidence,
            crate::ClockConfidence::Medium | crate::ClockConfidence::Unknown
        )
}

pub fn extract_evidence_facts_from_ref(
    label: &str,
    raw_ref: &str,
    ref_kind: &str,
    content_type: &str,
    text: &str,
    data_quality: &DataQuality,
) -> Vec<EvidenceFact> {
    if let Ok(state) = serde_json::from_str::<ServiceStateSummary>(text) {
        return service_state_facts(raw_ref, data_quality, &state);
    }
    if label.contains("port_summary") || raw_ref.ends_with("/port_summary.json") {
        if let Ok(port) = serde_json::from_str::<ServicePortSummary>(text) {
            return port_summary_facts(raw_ref, data_quality, &port);
        }
    }
    if label.contains("process_summary") || raw_ref.ends_with("/process_summary.json") {
        if let Ok(process) = serde_json::from_str::<ServiceProcessSummary>(text) {
            return process_summary_facts(raw_ref, data_quality, &process);
        }
    }
    if let Ok(pack) = serde_json::from_str::<ServiceInvestigationPack>(text) {
        return service_pack_facts(raw_ref, data_quality, &pack);
    }
    if let Ok(leads) = serde_json::from_str::<Vec<ServiceJournalLead>>(text) {
        return journal_lead_facts(raw_ref, data_quality, &leads);
    }
    if let Ok(diff) = serde_json::from_str::<FleetSemanticDiff>(text) {
        return fleet_semantic_facts(raw_ref, data_quality, &diff);
    }
    if is_cpu_ref(label, raw_ref) {
        if let Some(facts) = cpu_resource_facts(raw_ref, data_quality, text) {
            return facts;
        }
    }
    if is_memory_ref(label, raw_ref) {
        if let Some(facts) = memory_resource_facts(raw_ref, data_quality, text) {
            return facts;
        }
    }
    if is_network_ref(label, raw_ref) {
        if let Some(facts) = network_resource_facts(raw_ref, data_quality, text) {
            return facts;
        }
    }
    if is_thermal_ref(label, raw_ref) {
        if let Some(facts) = thermal_resource_facts(raw_ref, data_quality, text) {
            return facts;
        }
    }
    if is_io_ref(label, raw_ref) {
        if let Some(facts) = io_resource_facts(raw_ref, data_quality, text) {
            return facts;
        }
    }
    if is_config_ref(label, raw_ref) {
        return config_facts(raw_ref, data_quality, text);
    }
    if is_kernel_probe_ref(label, raw_ref) {
        if let Some(facts) = kernel_probe_facts(raw_ref, data_quality, text) {
            return facts;
        }
    }
    text_signal_facts(label, raw_ref, ref_kind, content_type, text, data_quality)
}

fn service_state_facts(
    raw_ref: &str,
    data_quality: &DataQuality,
    state: &ServiceStateSummary,
) -> Vec<EvidenceFact> {
    vec![
        fact(raw_ref, data_quality, "service.name", json!(state.service)),
        fact(
            raw_ref,
            data_quality,
            "service.availability",
            json!(state.availability),
        ),
        fact(
            raw_ref,
            data_quality,
            "service.active_state",
            json!(state.active_state),
        ),
        fact(
            raw_ref,
            data_quality,
            "service.sub_state",
            json!(state.sub_state),
        ),
        fact(
            raw_ref,
            data_quality,
            "service.main_pid_present",
            json!(state.main_pid.is_some()),
        ),
    ]
}

fn port_summary_facts(
    raw_ref: &str,
    data_quality: &DataQuality,
    port: &ServicePortSummary,
) -> Vec<EvidenceFact> {
    vec![
        fact(
            raw_ref,
            data_quality,
            "port.availability",
            json!(port.availability),
        ),
        fact(
            raw_ref,
            data_quality,
            "port.socket_inode_count",
            json!(port.socket_inode_count),
        ),
        fact(
            raw_ref,
            data_quality,
            "port.matched_socket_table_count",
            json!(port.matched_socket_table_count),
        ),
        fact(
            raw_ref,
            data_quality,
            "port.unavailable_reason_present",
            json!(port.unavailable_reason.is_some()),
        ),
    ]
}

fn process_summary_facts(
    raw_ref: &str,
    data_quality: &DataQuality,
    process: &ServiceProcessSummary,
) -> Vec<EvidenceFact> {
    vec![
        fact(
            raw_ref,
            data_quality,
            "process.pid_present",
            json!(process.pid.is_some()),
        ),
        fact(raw_ref, data_quality, "process.comm", json!(process.comm)),
        fact(
            raw_ref,
            data_quality,
            "process.rss_kb",
            json!(process.rss_kb),
        ),
    ]
}

fn service_pack_facts(
    raw_ref: &str,
    data_quality: &DataQuality,
    pack: &ServiceInvestigationPack,
) -> Vec<EvidenceFact> {
    let mut facts = service_state_facts(raw_ref, data_quality, &pack.service_state);
    facts.extend(process_summary_facts(
        raw_ref,
        data_quality,
        &pack.process_summary,
    ));
    facts.extend(port_summary_facts(
        raw_ref,
        data_quality,
        &pack.port_summary,
    ));
    facts.extend(journal_lead_facts(
        raw_ref,
        data_quality,
        &pack.journal_leads,
    ));
    facts.push(fact(
        raw_ref,
        data_quality,
        "journal.returned_lead_count",
        json!(pack.journal_summary.returned_lead_count),
    ));
    facts.push(fact(
        raw_ref,
        data_quality,
        "data_quality.missing_count",
        json!(pack.data_quality.missing.len()),
    ));
    facts
}

fn journal_lead_facts(
    raw_ref: &str,
    data_quality: &DataQuality,
    leads: &[ServiceJournalLead],
) -> Vec<EvidenceFact> {
    let mut buckets = std::collections::BTreeMap::<String, usize>::new();
    for lead in leads {
        *buckets.entry(lead.severity_hint.clone()).or_default() += 1;
    }
    vec![
        fact(
            raw_ref,
            data_quality,
            "journal.returned_lead_count",
            json!(leads.len()),
        ),
        fact(
            raw_ref,
            data_quality,
            "journal.severity_buckets",
            json!(buckets),
        ),
    ]
}

fn fleet_semantic_facts(
    raw_ref: &str,
    data_quality: &DataQuality,
    diff: &FleetSemanticDiff,
) -> Vec<EvidenceFact> {
    let different_count = diff
        .field_diffs
        .iter()
        .filter(|field| field.status == "different")
        .count();
    let partial_count = diff
        .field_diffs
        .iter()
        .filter(|field| field.status == "partial")
        .count();
    vec![
        fact(
            raw_ref,
            data_quality,
            "fleet.semantic_diff.different_field_count",
            json!(different_count),
        ),
        fact(
            raw_ref,
            data_quality,
            "fleet.semantic_diff.partial_field_count",
            json!(partial_count),
        ),
        fact(
            raw_ref,
            data_quality,
            "fleet.target_count",
            json!(diff.target_count),
        ),
    ]
}

fn cpu_resource_facts(
    raw_ref: &str,
    data_quality: &DataQuality,
    text: &str,
) -> Option<Vec<EvidenceFact>> {
    let samples = parse_jsonl_values(text);
    let first = samples.first()?.get("sample")?;
    let last = samples.last()?.get("sample")?;
    let first_total = value_u64(first, "total_jiffies")?;
    let first_idle = value_u64(first, "idle_jiffies")?;
    let last_total = value_u64(last, "total_jiffies")?;
    let last_idle = value_u64(last, "idle_jiffies")?;
    let total_delta = last_total.checked_sub(first_total)?;
    if total_delta == 0 {
        return None;
    }
    let idle_delta = last_idle.saturating_sub(first_idle);
    let busy_delta = total_delta.saturating_sub(idle_delta);
    let busy_percent = ((busy_delta as f64 / total_delta as f64) * 1000.0).round() / 10.0;
    Some(vec![
        fact(
            raw_ref,
            data_quality,
            "resource.cpu_busy_percent",
            json!(busy_percent),
        ),
        fact(
            raw_ref,
            data_quality,
            "resource.cpu_sample_count",
            json!(samples.len()),
        ),
    ])
}

fn memory_resource_facts(
    raw_ref: &str,
    data_quality: &DataQuality,
    text: &str,
) -> Option<Vec<EvidenceFact>> {
    let last = last_json_value(text)?;
    let sample = last.get("sample").unwrap_or(&last);
    let available_kb = value_u64(sample, "mem_available_kb")?;
    let mut facts = vec![fact(
        raw_ref,
        data_quality,
        "resource.memory_available_bytes",
        json!(available_kb.saturating_mul(1024)),
    )];
    if let Some(total_kb) = value_u64(sample, "mem_total_kb") {
        facts.push(fact(
            raw_ref,
            data_quality,
            "resource.memory_total_bytes",
            json!(total_kb.saturating_mul(1024)),
        ));
    }
    Some(facts)
}

fn network_resource_facts(
    raw_ref: &str,
    data_quality: &DataQuality,
    text: &str,
) -> Option<Vec<EvidenceFact>> {
    let last = last_json_value(text)?;
    let sample = last.get("sample").unwrap_or(&last);
    let interfaces = sample.get("interfaces")?.as_array()?;
    let mut rx_bytes = 0_u64;
    let mut tx_bytes = 0_u64;
    for iface in interfaces {
        rx_bytes = rx_bytes.saturating_add(value_u64(iface, "rx_bytes").unwrap_or(0));
        tx_bytes = tx_bytes.saturating_add(value_u64(iface, "tx_bytes").unwrap_or(0));
    }
    Some(vec![
        fact(
            raw_ref,
            data_quality,
            "resource.network_rx_bytes",
            json!(rx_bytes),
        ),
        fact(
            raw_ref,
            data_quality,
            "resource.network_tx_bytes",
            json!(tx_bytes),
        ),
        fact(
            raw_ref,
            data_quality,
            "resource.network_interface_count",
            json!(interfaces.len()),
        ),
    ])
}

fn thermal_resource_facts(
    raw_ref: &str,
    data_quality: &DataQuality,
    text: &str,
) -> Option<Vec<EvidenceFact>> {
    let value = serde_json::from_str::<Value>(text).ok()?;
    let zones = value.get("zones")?.as_array()?;
    let max_temp = zones
        .iter()
        .filter_map(|zone| value_i64(zone, "temp_millicelsius"))
        .max()?;
    let throttled_state = value
        .get("throttled_state")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    Some(vec![
        fact(
            raw_ref,
            data_quality,
            "resource.thermal_millidegree_c",
            json!(max_temp),
        ),
        fact(
            raw_ref,
            data_quality,
            "resource.throttled_state",
            json!(throttled_state),
        ),
        fact(
            raw_ref,
            data_quality,
            "resource.thermal_zone_count",
            json!(zones.len()),
        ),
    ])
}

fn io_resource_facts(
    raw_ref: &str,
    data_quality: &DataQuality,
    text: &str,
) -> Option<Vec<EvidenceFact>> {
    let value = serde_json::from_str::<Value>(text).ok()?;
    let mut facts = Vec::new();
    if let Some(read_bytes) =
        value_u64(&value, "io_read_bytes").or_else(|| value_u64(&value, "read_bytes"))
    {
        facts.push(fact(
            raw_ref,
            data_quality,
            "resource.io_read_bytes",
            json!(read_bytes),
        ));
    }
    if let Some(write_bytes) =
        value_u64(&value, "io_write_bytes").or_else(|| value_u64(&value, "write_bytes"))
    {
        facts.push(fact(
            raw_ref,
            data_quality,
            "resource.io_write_bytes",
            json!(write_bytes),
        ));
    }
    if let Some(device_count) = value_u64(&value, "device_count") {
        facts.push(fact(
            raw_ref,
            data_quality,
            "resource.io_device_count",
            json!(device_count),
        ));
    }
    if facts.is_empty() {
        None
    } else {
        Some(facts)
    }
}

fn config_facts(raw_ref: &str, data_quality: &DataQuality, text: &str) -> Vec<EvidenceFact> {
    let redacted_marker_count = text.matches("<redacted>").count();
    vec![
        fact(
            raw_ref,
            data_quality,
            "config.redacted_diff_present",
            json!(!text.trim().is_empty()),
        ),
        fact(
            raw_ref,
            data_quality,
            "config.redacted_marker_count",
            json!(redacted_marker_count),
        ),
    ]
}

fn kernel_probe_facts(
    raw_ref: &str,
    data_quality: &DataQuality,
    text: &str,
) -> Option<Vec<EvidenceFact>> {
    let value = serde_json::from_str::<Value>(text).ok()?;
    Some(vec![
        fact(
            raw_ref,
            data_quality,
            "kernel.ftrace_available",
            json!(value_bool(&value, "ftrace_available").unwrap_or(false)),
        ),
        fact(
            raw_ref,
            data_quality,
            "kernel.perf_available",
            json!(value_bool(&value, "perf_available").unwrap_or(false)),
        ),
        fact(
            raw_ref,
            data_quality,
            "kernel.kprobe_available",
            json!(value_bool(&value, "kprobe_available").unwrap_or(false)),
        ),
    ])
}

fn text_signal_facts(
    _label: &str,
    raw_ref: &str,
    _ref_kind: &str,
    _content_type: &str,
    text: &str,
    data_quality: &DataQuality,
) -> Vec<EvidenceFact> {
    let signal_line_count = text
        .lines()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            ["error", "warn", "fail", "timeout", "denied", "marker"]
                .iter()
                .any(|needle| lower.contains(needle))
        })
        .count();
    vec![
        fact(
            raw_ref,
            data_quality,
            "signal.returned_lines",
            json!(text.lines().count()),
        ),
        fact(
            raw_ref,
            data_quality,
            "signal.signal_line_count",
            json!(signal_line_count),
        ),
        fact(
            raw_ref,
            data_quality,
            "signal.has_signal_words",
            json!(signal_line_count > 0),
        ),
    ]
}

fn fact(raw_ref: &str, data_quality: &DataQuality, fact_id: &str, value: Value) -> EvidenceFact {
    EvidenceFact {
        fact_id: fact_id.to_string(),
        scope: "run".to_string(),
        target_id: None,
        source_ref: raw_ref.to_string(),
        value,
        data_quality: data_quality.clone(),
        observed_at_monotonic_ns: None,
    }
}

fn is_cpu_ref(label: &str, raw_ref: &str) -> bool {
    label == "cpu" || raw_ref.ends_with("/cpu.jsonl")
}

fn is_memory_ref(label: &str, raw_ref: &str) -> bool {
    label == "memory" || raw_ref.ends_with("/memory.jsonl")
}

fn is_network_ref(label: &str, raw_ref: &str) -> bool {
    label == "network" || raw_ref.ends_with("/network.jsonl")
}

fn is_thermal_ref(label: &str, raw_ref: &str) -> bool {
    label.contains("thermal") || raw_ref.ends_with("/thermal_snapshot.json")
}

fn is_io_ref(label: &str, raw_ref: &str) -> bool {
    label.contains("io") || raw_ref.ends_with("/io_snapshot.json")
}

fn is_config_ref(label: &str, raw_ref: &str) -> bool {
    label.contains("config") || raw_ref.ends_with("/config_redacted.txt")
}

fn is_kernel_probe_ref(label: &str, raw_ref: &str) -> bool {
    label.contains("kernel") || raw_ref.ends_with("/kernel_probe_snapshot.json")
}

fn parse_jsonl_values(text: &str) -> Vec<Value> {
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect()
}

fn last_json_value(text: &str) -> Option<Value> {
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        return Some(value);
    }
    parse_jsonl_values(text).into_iter().last()
}

fn value_u64(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

fn value_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

fn value_bool(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}
