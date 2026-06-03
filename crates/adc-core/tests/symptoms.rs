use adc_core::{normalize_symptom, SymptomKind};

#[test]
fn normalizes_common_symptom_aliases_without_cause_claims() {
    let latency = normalize_symptom("requests are timing out with high latency");
    assert_eq!(latency.kind, SymptomKind::LatencyTimeout);
    assert_eq!(latency.normalized, "latency_timeout");
    assert_eq!(latency.parser_quality, "alias");
    assert!(!latency.summary.to_ascii_lowercase().contains("root cause"));

    let thermal = normalize_symptom("pi is hot and throttling under load");
    assert_eq!(thermal.kind, SymptomKind::ThermalPower);
    assert_eq!(thermal.normalized, "thermal_power");
}

#[test]
fn unknown_symptom_stays_explicit_information_debt() {
    let symptom = normalize_symptom("something weird happened");

    assert_eq!(symptom.kind, SymptomKind::Unknown);
    assert_eq!(symptom.normalized, "unknown");
    assert!(symptom
        .data_quality
        .missing
        .iter()
        .any(|missing| missing.contains("unrecognized symptom")));
}
