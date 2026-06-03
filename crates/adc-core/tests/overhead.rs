use adc_core::{evaluate_overhead, OverheadBudget, OverheadSample};

#[test]
fn overhead_guard_throttles_when_artifact_budget_is_exceeded() {
    let budget = OverheadBudget {
        max_artifact_bytes: 10,
        max_events: 10,
        max_duration_ms: 1_000,
    };
    let sample = OverheadSample {
        artifact_bytes: 11,
        event_count: 3,
        duration_ms: 25,
    };

    let decision = evaluate_overhead(&budget, &sample);

    assert!(decision.throttled);
    assert_eq!(decision.capture_level, "degraded");
    assert!(decision
        .data_quality
        .notes
        .iter()
        .any(|note| note.contains("artifact budget exceeded")));
}
