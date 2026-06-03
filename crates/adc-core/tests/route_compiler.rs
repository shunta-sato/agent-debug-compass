use adc_core::{
    compile_route_for_symptom, normalize_symptom, EvidenceFact, RouteCompileInput, SymptomKind,
};
use serde_json::json;

fn fact(fact_id: &str, value: serde_json::Value) -> EvidenceFact {
    EvidenceFact {
        fact_id: fact_id.to_string(),
        scope: "run".to_string(),
        target_id: None,
        source_ref: "artifact://raw/test.json".to_string(),
        value,
        data_quality: Default::default(),
        observed_at_monotonic_ns: None,
    }
}

#[test]
fn latency_symptom_selects_latency_route_and_reports_fact_gaps() {
    let symptom = normalize_symptom("timeout and high latency");
    let compiled = compile_route_for_symptom(RouteCompileInput {
        symptom,
        available_facts: vec![
            fact("signal.signal_line_count", json!(2)),
            fact("signal.has_signal_words", json!(true)),
        ],
        max_selected_packs: 3,
        target_ids: vec!["local".to_string()],
    });

    assert_eq!(compiled.schema_version, "obs.compiled_route.v1");
    assert_eq!(compiled.symptom.kind, SymptomKind::LatencyTimeout);
    assert_eq!(compiled.selected_packs[0].domain, "latency_timeouts");
    assert!(compiled
        .selected_packs
        .iter()
        .any(|pack| pack.domain == "service_health"));
    assert!(compiled
        .missing_fact_ids
        .iter()
        .any(|fact_id| fact_id == "resource.cpu_busy_percent"));
    assert!(compiled.rejected_packs.iter().any(|pack| {
        pack.domain == "thermal_power_edge" && pack.reason.contains("lower priority")
    }));
    assert!(compiled
        .selected_packs
        .iter()
        .all(|pack| pack.cause_neutral));
}

#[test]
fn unknown_symptom_uses_conservative_discovery_route() {
    let compiled = compile_route_for_symptom(RouteCompileInput {
        symptom: normalize_symptom("unclear failure"),
        available_facts: Vec::new(),
        max_selected_packs: 4,
        target_ids: vec!["pi5".to_string(), "example-target".to_string()],
    });

    assert_eq!(compiled.symptom.kind, SymptomKind::Unknown);
    assert!(compiled
        .selected_packs
        .iter()
        .any(|pack| pack.domain == "service_health"));
    assert!(compiled
        .selected_packs
        .iter()
        .any(|pack| pack.domain == "latency_timeouts"));
    assert_eq!(compiled.target_ids, vec!["pi5", "example-target"]);
    assert!(!compiled.missing_fact_ids.is_empty());
}
