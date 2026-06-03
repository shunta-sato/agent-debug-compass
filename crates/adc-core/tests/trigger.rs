use adc_core::profile::{RuleType, TriggerRule};
use adc_core::{evaluate_trigger, TriggerInput};

#[test]
fn threshold_duration_trigger_matches_observed_signal_without_cause_claim() {
    let rule = TriggerRule {
        name: "cpu_sustained_high".to_string(),
        rule_type: RuleType::ThresholdDuration,
        signal: Some("cpu.total_percent".to_string()),
        op: Some(">".to_string()),
        value: Some(85.0),
        duration_sec: Some(5),
        capture_profile: Some("perf_short".to_string()),
        severity_at_least: None,
        patterns: Vec::new(),
    };

    let result = evaluate_trigger(
        &rule,
        &TriggerInput {
            signal: "cpu.total_percent".to_string(),
            value: Some(91.0),
            duration_sec: Some(8),
            text: None,
            severity: None,
        },
    )
    .expect("trigger evaluates");

    assert!(result.matched);
    assert_eq!(result.trigger_name, "cpu_sustained_high");
    assert_eq!(result.capture_profile.as_deref(), Some("perf_short"));
    assert!(result.reason.contains("observed"));
    assert!(!result.reason.contains("root cause"));
}

#[test]
fn kmsg_pattern_trigger_requires_matching_text() {
    let rule = TriggerRule {
        name: "kmsg_warning".to_string(),
        rule_type: RuleType::KmsgPattern,
        signal: None,
        op: None,
        value: None,
        duration_sec: None,
        capture_profile: None,
        severity_at_least: Some("warning".to_string()),
        patterns: vec!["undervoltage".to_string()],
    };

    let result = evaluate_trigger(
        &rule,
        &TriggerInput {
            signal: "kmsg".to_string(),
            value: None,
            duration_sec: None,
            text: Some("thermal throttling observed".to_string()),
            severity: Some("warning".to_string()),
        },
    )
    .expect("trigger evaluates");

    assert!(!result.matched);
    assert!(result
        .data_quality
        .notes
        .iter()
        .any(|note| note.contains("no pattern")));
}
