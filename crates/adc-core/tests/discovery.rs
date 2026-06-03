use adc_core::discover_same_network_targets_from_neighbors;

#[test]
fn discovery_filters_neighbor_table_to_safe_same_network_candidates() {
    let neighbors = r#"
198.51.100.21 dev eth0 lladdr aa:bb:cc:00:00:01 REACHABLE
198.51.100.22 dev eth0 lladdr aa:bb:cc:00:00:02 STALE
198.51.100.99 dev eth0 FAILED
203.0.113.30 dev eth0 lladdr aa:bb:cc:00:00:03 REACHABLE
"#;

    let result = discover_same_network_targets_from_neighbors("198.51.100.0/24", neighbors)
        .expect("discover candidates");

    assert_eq!(result.schema_version, "obs.discovery.v2");
    assert_eq!(result.network_cidr, "198.51.100.0/24");
    assert_eq!(result.candidate_count, 2);
    assert_eq!(result.candidates[0].target_id, "target-198-51-100-21");
    assert_eq!(result.candidates[0].host, "198.51.100.21");
    assert_eq!(result.candidates[0].transport, "mcp_stdio_over_ssh");
    assert_eq!(result.candidates[0].confidence, "medium");
    assert!(result
        .data_quality
        .missing
        .iter()
        .any(|missing| missing.contains("198.51.100.99")));

    let rendered = serde_json::to_string(&result).expect("render discovery");
    assert!(!rendered.contains("aa:bb:cc"));
}

#[test]
fn discovery_rejects_broad_or_invalid_network_input() {
    let broad = discover_same_network_targets_from_neighbors("198.51.0.0/16", "")
        .expect_err("broad discovery must be rejected");
    assert!(broad.to_string().contains("/24 or narrower"));

    let invalid = discover_same_network_targets_from_neighbors("not-a-cidr", "")
        .expect_err("invalid cidr must be rejected");
    assert!(invalid.to_string().contains("CIDR"));
}
