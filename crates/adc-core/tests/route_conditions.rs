use adc_core::{
    evaluate_route_condition, ConditionStatus, EvidenceFact, RouteConditionExpr,
    RouteConditionInput,
};
use serde_json::json;

fn fact(fact_id: &str, value: serde_json::Value) -> EvidenceFact {
    EvidenceFact {
        fact_id: fact_id.to_string(),
        scope: "run".to_string(),
        target_id: None,
        source_ref: "artifact://test/ref.json".to_string(),
        value,
        data_quality: adc_core::DataQuality {
            clock_confidence: "medium".to_string(),
            ..Default::default()
        },
        observed_at_monotonic_ns: None,
    }
}

#[test]
fn typed_eq_does_not_match_unavailable_by_substring() {
    let facts = vec![
        fact("service.availability", json!("unavailable")),
        fact("port.availability", json!("unavailable")),
    ];

    let evaluation = evaluate_route_condition(RouteConditionInput {
        condition_id: "C001",
        expression: &RouteConditionExpr::Eq {
            fact_id: "service.availability".to_string(),
            value: json!("available"),
        },
        facts: &facts,
    });

    assert_eq!(evaluation.status, ConditionStatus::NotMatched);
    assert!(evaluation.matched_facts.is_empty());
    assert!(evaluation.missing_fact_ids.is_empty());
}

#[test]
fn typed_eq_keeps_service_and_port_availability_distinct() {
    let facts = vec![
        fact("service.availability", json!("available")),
        fact("port.availability", json!("unavailable")),
    ];

    let evaluation = evaluate_route_condition(RouteConditionInput {
        condition_id: "C002",
        expression: &RouteConditionExpr::Eq {
            fact_id: "service.availability".to_string(),
            value: json!("available"),
        },
        facts: &facts,
    });

    assert_eq!(evaluation.status, ConditionStatus::Matched);
    assert_eq!(evaluation.matched_facts.len(), 1);
    assert_eq!(evaluation.matched_facts[0].fact_id, "service.availability");
    assert_eq!(evaluation.matched_facts[0].value, json!("available"));
}

#[test]
fn missing_required_fact_is_unknown_with_fact_id() {
    let evaluation = evaluate_route_condition(RouteConditionInput {
        condition_id: "C003",
        expression: &RouteConditionExpr::Gte {
            fact_id: "resource.cpu_busy_percent".to_string(),
            value: 90.0,
        },
        facts: &[],
    });

    assert_eq!(evaluation.status, ConditionStatus::Unknown);
    assert_eq!(
        evaluation.missing_fact_ids,
        vec!["resource.cpu_busy_percent".to_string()]
    );
}

#[test]
fn bucket_count_gte_matches_typed_journal_bucket() {
    let facts = vec![fact(
        "journal.severity_buckets",
        json!({
            "error": 2,
            "warning": 1,
        }),
    )];

    let evaluation = evaluate_route_condition(RouteConditionInput {
        condition_id: "C004",
        expression: &RouteConditionExpr::BucketCountGte {
            fact_id: "journal.severity_buckets".to_string(),
            key: "error".to_string(),
            value: 2,
        },
        facts: &facts,
    });

    assert_eq!(evaluation.status, ConditionStatus::Matched);
    assert_eq!(
        evaluation.matched_facts[0].fact_id,
        "journal.severity_buckets"
    );
}
