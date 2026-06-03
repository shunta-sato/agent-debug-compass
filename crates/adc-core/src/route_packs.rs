use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutePack {
    pub pack_id: String,
    pub domain: String,
    pub title: String,
    pub scope: String,
    pub required_facts: Vec<String>,
    pub suggested_refs: Vec<String>,
    pub stop_conditions: Vec<String>,
    pub budget_hint: RoutePackBudgetHint,
    pub cause_neutral: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutePackBudgetHint {
    pub max_steps: usize,
    pub max_refs_per_step: usize,
    pub expected_cost: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutePackRegistry {
    pub schema_version: String,
    pub pack_count: usize,
    pub packs: Vec<RoutePack>,
}

pub fn default_route_packs() -> Vec<RoutePack> {
    vec![
        pack(
            "service-health",
            "service_health",
            "Service health and availability",
            &[
                "service.availability",
                "service.active_state",
                "service.sub_state",
                "port.availability",
                "journal.severity_buckets",
            ],
            &[
                "service_state",
                "service.port_summary",
                "service.journal_leads",
            ],
        ),
        pack(
            "latency-timeouts",
            "latency_timeouts",
            "Latency and timeout markers",
            &[
                "signal.signal_line_count",
                "signal.has_signal_words",
                "journal.severity_buckets",
                "resource.cpu_busy_percent",
            ],
            &["log", "domain_event", "window.primary", "series.cpu"],
        ),
        pack(
            "memory-growth",
            "memory_growth",
            "Memory growth and pressure",
            &[
                "resource.memory_available_bytes",
                "process.rss_kb",
                "signal.signal_line_count",
            ],
            &[
                "series.memory",
                "process_snapshot",
                "service.process_summary",
            ],
        ),
        pack(
            "cpu-saturation",
            "cpu_saturation",
            "CPU saturation and scheduling pressure",
            &[
                "resource.cpu_busy_percent",
                "process.pid_present",
                "signal.signal_line_count",
            ],
            &["series.cpu", "process_snapshot", "window.primary"],
        ),
        pack(
            "network-degradation",
            "network_degradation",
            "Network degradation and interface deltas",
            &[
                "resource.network_rx_bytes",
                "resource.network_tx_bytes",
                "signal.signal_line_count",
            ],
            &["series.network", "raw.net_dev", "window.primary"],
        ),
        pack(
            "disk-io-pressure",
            "disk_io_pressure",
            "Disk and IO pressure",
            &[
                "resource.io_read_bytes",
                "resource.io_write_bytes",
                "signal.signal_line_count",
            ],
            &["io_snapshot", "window.primary"],
        ),
        pack(
            "config-deploy-drift",
            "config_deploy_drift",
            "Configuration and deploy drift",
            &[
                "config.redacted_diff_present",
                "signal.signal_line_count",
                "data_quality.missing_count",
            ],
            &["config_redacted", "domain_event", "manifest"],
        ),
        pack(
            "thermal-power-edge",
            "thermal_power_edge",
            "Thermal and power edge degradation",
            &[
                "resource.thermal_millidegree_c",
                "resource.throttled_state",
                "signal.signal_line_count",
            ],
            &[
                "thermal_snapshot",
                "kernel_optional_probe_snapshot",
                "window.primary",
            ],
        ),
    ]
}

pub fn default_route_pack_registry() -> RoutePackRegistry {
    let packs = default_route_packs();
    RoutePackRegistry {
        schema_version: "obs.route_pack_registry.v1".to_string(),
        pack_count: packs.len(),
        packs,
    }
}

fn pack(
    pack_id: &str,
    domain: &str,
    title: &str,
    required_facts: &[&str],
    suggested_refs: &[&str],
) -> RoutePack {
    RoutePack {
        pack_id: pack_id.to_string(),
        domain: domain.to_string(),
        title: title.to_string(),
        scope: "run_or_fleet".to_string(),
        required_facts: required_facts.iter().map(|fact| fact.to_string()).collect(),
        suggested_refs: suggested_refs
            .iter()
            .map(|reference| reference.to_string())
            .collect(),
        stop_conditions: vec![
            "typed facts have been evaluated".to_string(),
            "data_quality gaps remain explicit".to_string(),
            "raw artifacts remain ref-only".to_string(),
        ],
        budget_hint: RoutePackBudgetHint {
            max_steps: 3,
            max_refs_per_step: 4,
            expected_cost: "low".to_string(),
        },
        cause_neutral: true,
    }
}
