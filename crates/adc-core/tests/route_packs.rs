use std::collections::BTreeSet;

use adc_core::default_route_packs;

#[test]
fn default_route_packs_cover_world_class_bug_domains() {
    let packs = default_route_packs();
    let domains = packs
        .iter()
        .map(|pack| pack.domain.as_str())
        .collect::<BTreeSet<_>>();

    for domain in [
        "service_health",
        "latency_timeouts",
        "memory_growth",
        "cpu_saturation",
        "network_degradation",
        "disk_io_pressure",
        "config_deploy_drift",
        "thermal_power_edge",
    ] {
        assert!(
            domains.contains(domain),
            "missing route pack domain {domain}"
        );
    }
    assert!(packs.iter().all(|pack| pack.cause_neutral));
    assert!(packs.iter().all(|pack| !pack.required_facts.is_empty()));
    assert!(packs.iter().all(|pack| pack.budget_hint.max_steps > 0));
}

#[test]
fn route_pack_text_stays_cause_neutral() {
    let serialized = serde_json::to_string(&default_route_packs()).expect("route packs json");
    let lower = serialized.to_ascii_lowercase();

    assert!(!lower.contains("root cause"));
    assert!(!lower.contains("likely cause"));
    assert!(!lower.contains("confidence score"));
    assert!(!lower.contains("remediation"));
}
