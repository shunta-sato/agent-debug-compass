use adc_core::profile::{RuleType, TriggerHysteresis, TriggerRule};
use adc_core::{
    default_recorder_budget, evaluate_trigger, recorder_default_budget_status,
    trigger_decision_for_budget_refusal, trigger_decision_for_rule,
    trigger_decision_with_runtime_state, ClockConfidence, DataQuality, ExpectedSamplesBasis,
    RecorderCoverageConfidence, RecorderCoverageState, RecorderSignalCoverage,
    TriggerBudgetDecision, TriggerDecisionOutcome, TriggerDecisionReason, TriggerInput,
    TriggerRuntimeState,
};

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
        cooldown_ms: None,
        hysteresis: None,
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
        cooldown_ms: None,
        hysteresis: None,
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

#[test]
fn trigger_decision_skips_missing_coverage_instead_of_reporting_not_fired() {
    let rule = threshold_rule(
        "memory_pressure_low",
        "memory.available_percent",
        "<=",
        20.0,
    );
    let missing_coverage = coverage("memory.summary", RecorderCoverageState::Missing);

    let decision = trigger_decision_for_rule(
        "default-symptom-trigger-policy",
        &rule,
        Some(&TriggerInput {
            signal: "memory.available_percent".to_string(),
            value: Some(12.5),
            duration_sec: None,
            text: None,
            severity: None,
        }),
        Some(&missing_coverage),
        None,
        None,
    )
    .expect("trigger decision");

    assert_eq!(
        decision.decision,
        TriggerDecisionOutcome::SkippedMissingCoverage
    );
    assert_eq!(
        decision.decision_reason,
        TriggerDecisionReason::CoverageMissing
    );
    assert_eq!(decision.coverage_state, RecorderCoverageState::Missing);
    assert_ne!(decision.decision, TriggerDecisionOutcome::NotFired);
    assert!(decision
        .data_quality
        .missing
        .iter()
        .any(|missing| missing.contains("memory.summary")));
}

#[test]
fn trigger_decision_fires_only_when_covered_threshold_crosses() {
    let rule = threshold_rule(
        "memory_pressure_low",
        "memory.available_percent",
        "<=",
        20.0,
    );
    let covered = coverage("memory.summary", RecorderCoverageState::Covered);

    let fired = trigger_decision_for_rule(
        "default-symptom-trigger-policy",
        &rule,
        Some(&TriggerInput {
            signal: "memory.available_percent".to_string(),
            value: Some(12.5),
            duration_sec: None,
            text: None,
            severity: None,
        }),
        Some(&covered),
        Some("artifact://recorder/incidents/INC-001/coverage.json".to_string()),
        Some("INC-001".to_string()),
    )
    .expect("fired decision");

    assert_eq!(fired.decision, TriggerDecisionOutcome::Fired);
    assert_eq!(
        fired.decision_reason,
        TriggerDecisionReason::ThresholdCrossed
    );
    assert_eq!(
        fired.budget_decision,
        adc_core::TriggerBudgetDecision::Accepted
    );
    assert_eq!(fired.incident_id.as_deref(), Some("INC-001"));
    assert_eq!(
        fired.trigger_event_ref.as_deref(),
        Some("artifact://recorder/incidents/INC-001/trigger_event.json")
    );

    let not_fired = trigger_decision_for_rule(
        "default-symptom-trigger-policy",
        &rule,
        Some(&TriggerInput {
            signal: "memory.available_percent".to_string(),
            value: Some(45.0),
            duration_sec: None,
            text: None,
            severity: None,
        }),
        Some(&covered),
        None,
        None,
    )
    .expect("not fired decision");

    assert_eq!(not_fired.decision, TriggerDecisionOutcome::NotFired);
    assert_eq!(
        not_fired.decision_reason,
        TriggerDecisionReason::ThresholdNotCrossed
    );
    assert!(not_fired.incident_id.is_none());
    assert!(not_fired.trigger_event_ref.is_none());
}

#[test]
fn trigger_decision_records_budget_exhaustion_as_skip() {
    let rule = threshold_rule(
        "memory_pressure_low",
        "memory.available_percent",
        "<=",
        20.0,
    );
    let covered = coverage("memory.summary", RecorderCoverageState::Covered);
    let mut budget = default_recorder_budget();
    budget.max_frozen_incidents = 0;
    let budget_status = recorder_default_budget_status(&budget, 0);

    let decision = trigger_decision_for_budget_refusal(
        "default-symptom-trigger-policy",
        &rule,
        Some(&TriggerInput {
            signal: "memory.available_percent".to_string(),
            value: Some(12.5),
            duration_sec: None,
            text: None,
            severity: None,
        }),
        Some(&covered),
        Some("artifact://recorder/incidents/INC-001/coverage.json".to_string()),
        &budget_status,
    )
    .expect("budget skip decision");

    assert_eq!(
        decision.decision,
        TriggerDecisionOutcome::SkippedBudgetExhausted
    );
    assert_eq!(
        decision.decision_reason,
        TriggerDecisionReason::BudgetExhausted
    );
    assert_eq!(
        decision.budget_decision,
        TriggerBudgetDecision::SkippedBudgetExhausted
    );
    assert_eq!(decision.coverage_state, RecorderCoverageState::Covered);
    assert!(decision.incident_id.is_none());
    assert!(decision.trigger_event_ref.is_none());
    assert!(decision.data_quality.throttled);
}

#[test]
fn trigger_runtime_state_suppresses_repeated_fire_inside_cooldown() {
    let mut rule = threshold_rule(
        "memory_pressure_low",
        "memory.available_percent",
        "<=",
        20.0,
    );
    rule.cooldown_ms = Some(30_000);
    let covered = coverage("memory.summary", RecorderCoverageState::Covered);
    let input = TriggerInput {
        signal: "memory.available_percent".to_string(),
        value: Some(12.5),
        duration_sec: None,
        text: None,
        severity: None,
    };
    let mut state = TriggerRuntimeState::default();

    let first = trigger_decision_with_runtime_state(
        "default-symptom-trigger-policy",
        &rule,
        Some(&input),
        Some(&covered),
        None,
        1_000_000_000,
        &mut state,
    )
    .expect("first trigger");
    let second = trigger_decision_with_runtime_state(
        "default-symptom-trigger-policy",
        &rule,
        Some(&input),
        Some(&covered),
        None,
        2_000_000_000,
        &mut state,
    )
    .expect("second trigger");

    assert_eq!(first.decision, TriggerDecisionOutcome::Fired);
    assert_eq!(second.decision, TriggerDecisionOutcome::SuppressedCooldown);
    assert_eq!(
        second.decision_reason,
        TriggerDecisionReason::CooldownActive
    );
    assert!(second.incident_id.is_none());
}

#[test]
fn trigger_runtime_state_suppresses_until_hysteresis_clears() {
    let mut rule = threshold_rule("cpu_pressure_high", "cpu.total_percent", ">=", 85.0);
    rule.hysteresis = Some(TriggerHysteresis {
        clear_below: Some(75.0),
        min_clear_duration_ms: Some(0),
    });
    let covered = coverage("cpu.summary", RecorderCoverageState::Covered);
    let mut state = TriggerRuntimeState::default();

    let first = trigger_decision_with_runtime_state(
        "default-symptom-trigger-policy",
        &rule,
        Some(&TriggerInput {
            signal: "cpu.total_percent".to_string(),
            value: Some(91.0),
            duration_sec: None,
            text: None,
            severity: None,
        }),
        Some(&covered),
        None,
        1_000_000_000,
        &mut state,
    )
    .expect("first trigger");
    let second = trigger_decision_with_runtime_state(
        "default-symptom-trigger-policy",
        &rule,
        Some(&TriggerInput {
            signal: "cpu.total_percent".to_string(),
            value: Some(88.0),
            duration_sec: None,
            text: None,
            severity: None,
        }),
        Some(&covered),
        None,
        40_000_000_000,
        &mut state,
    )
    .expect("hysteresis suppressed trigger");

    assert_eq!(first.decision, TriggerDecisionOutcome::Fired);
    assert_eq!(
        second.decision,
        TriggerDecisionOutcome::SuppressedHysteresis
    );
    assert_eq!(
        second.decision_reason,
        TriggerDecisionReason::HysteresisNotCleared
    );
}

fn threshold_rule(name: &str, signal: &str, op: &str, value: f64) -> TriggerRule {
    TriggerRule {
        name: name.to_string(),
        rule_type: RuleType::Threshold,
        signal: Some(signal.to_string()),
        op: Some(op.to_string()),
        value: Some(value),
        duration_sec: None,
        capture_profile: None,
        severity_at_least: None,
        patterns: Vec::new(),
        cooldown_ms: None,
        hysteresis: None,
    }
}

fn coverage(signal_id: &str, coverage_state: RecorderCoverageState) -> RecorderSignalCoverage {
    RecorderSignalCoverage {
        signal_id: signal_id.to_string(),
        expected: true,
        coverage_state,
        coverage_confidence: RecorderCoverageConfidence::Medium,
        configured_interval_ms: 10,
        effective_interval_ms: 63,
        expected_samples_configured: Some(7),
        expected_samples_budgeted: Some(1),
        expected_samples: Some(1),
        expected_samples_basis: ExpectedSamplesBasis::BudgetedRecorderInterval,
        retained_samples_before_freeze: 0,
        exported_samples: 0,
        dropped_samples: 0,
        truncated_samples_due_to_freeze_budget: 0,
        loss_report_ref: "artifact://recorder/incidents/INC-001/loss_report.json".to_string(),
        loss_collector_id: signal_id.to_string(),
        loss_reasons: Vec::new(),
        capability_status: adc_core::CapabilityStatus::Unknown,
        data_quality: DataQuality {
            clock_confidence: ClockConfidence::Medium,
            ..Default::default()
        },
    }
}
