use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafeProbePack {
    pub probe_pack_id: String,
    pub title: String,
    pub required_privilege: String,
    pub expected_cost: String,
    pub timeout_ms: u64,
    pub capability_requirements: Vec<String>,
    pub emitted_fact_ids: Vec<String>,
    pub suggested_refs: Vec<String>,
    pub failure_contract: String,
    pub cause_neutral: bool,
}

pub fn default_safe_probe_packs() -> Vec<SafeProbePack> {
    vec![
        probe_pack(ProbePackSpec {
            probe_pack_id: "baseline-observe",
            title: "Run bounded baseline observation",
            required_privilege: "none",
            expected_cost: "low",
            timeout_ms: 5_000,
            capability_requirements: &[],
            emitted_fact_ids: &[
                "resource.cpu_busy_percent",
                "resource.memory_available_bytes",
                "resource.network_rx_bytes",
                "resource.network_tx_bytes",
            ],
            suggested_refs: &["series.cpu", "series.memory", "series.network"],
        }),
        probe_pack(ProbePackSpec {
            probe_pack_id: "service-context",
            title: "Collect bounded service context",
            required_privilege: "none",
            expected_cost: "low",
            timeout_ms: 3_000,
            capability_requirements: &["systemd"],
            emitted_fact_ids: &[
                "service.availability",
                "service.active_state",
                "service.sub_state",
                "port.availability",
                "journal.severity_buckets",
                "process.pid_present",
                "process.rss_kb",
            ],
            suggested_refs: &[
                "service_state",
                "service.port_summary",
                "service.journal_leads",
            ],
        }),
        probe_pack(ProbePackSpec {
            probe_pack_id: "thermal-power-snapshot",
            title: "Collect thermal and optional power state",
            required_privilege: "none",
            expected_cost: "low",
            timeout_ms: 2_000,
            capability_requirements: &["thermal_sysfs"],
            emitted_fact_ids: &["resource.thermal_millidegree_c", "resource.throttled_state"],
            suggested_refs: &["thermal_snapshot", "kernel_optional_probe_snapshot"],
        }),
        probe_pack(ProbePackSpec {
            probe_pack_id: "io-config-snapshot",
            title: "Collect IO and redacted configuration evidence",
            required_privilege: "none",
            expected_cost: "low",
            timeout_ms: 2_000,
            capability_requirements: &[],
            emitted_fact_ids: &[
                "resource.io_read_bytes",
                "resource.io_write_bytes",
                "config.redacted_diff_present",
            ],
            suggested_refs: &["io_snapshot", "config_redacted"],
        }),
        probe_pack(ProbePackSpec {
            probe_pack_id: "kernel-optional-readiness",
            title: "Inspect optional kernel probe readiness",
            required_privilege: "none",
            expected_cost: "low",
            timeout_ms: 2_000,
            capability_requirements: &["tracefs_or_perf_optional"],
            emitted_fact_ids: &[
                "kernel.ftrace_available",
                "kernel.perf_available",
                "kernel.kprobe_available",
            ],
            suggested_refs: &["kernel_optional_probe_snapshot"],
        }),
    ]
}

pub fn safe_probe_packs_for_missing_facts(missing_fact_ids: &[String]) -> Vec<SafeProbePack> {
    let wanted = missing_fact_ids.iter().cloned().collect::<BTreeSet<_>>();
    let mut selected = default_safe_probe_packs()
        .into_iter()
        .filter(|pack| {
            pack.emitted_fact_ids
                .iter()
                .any(|fact_id| wanted.contains(fact_id))
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        selected.push(
            default_safe_probe_packs()
                .into_iter()
                .next()
                .expect("baseline probe pack exists"),
        );
    }
    selected
}

struct ProbePackSpec<'a> {
    probe_pack_id: &'a str,
    title: &'a str,
    required_privilege: &'a str,
    expected_cost: &'a str,
    timeout_ms: u64,
    capability_requirements: &'a [&'a str],
    emitted_fact_ids: &'a [&'a str],
    suggested_refs: &'a [&'a str],
}

fn probe_pack(spec: ProbePackSpec<'_>) -> SafeProbePack {
    SafeProbePack {
        probe_pack_id: spec.probe_pack_id.to_string(),
        title: spec.title.to_string(),
        required_privilege: spec.required_privilege.to_string(),
        expected_cost: spec.expected_cost.to_string(),
        timeout_ms: spec.timeout_ms,
        capability_requirements: spec
            .capability_requirements
            .iter()
            .map(|value| value.to_string())
            .collect(),
        emitted_fact_ids: spec
            .emitted_fact_ids
            .iter()
            .map(|value| value.to_string())
            .collect(),
        suggested_refs: spec
            .suggested_refs
            .iter()
            .map(|value| value.to_string())
            .collect(),
        failure_contract:
            "return partial context and record unavailable facts in data_quality; do not infer cause"
                .to_string(),
        cause_neutral: true,
    }
}
