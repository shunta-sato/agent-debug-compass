use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{DataQuality, EvidenceFact};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RouteConditionExpr {
    Eq {
        fact_id: String,
        value: Value,
    },
    Neq {
        fact_id: String,
        value: Value,
    },
    Exists {
        fact_id: String,
    },
    Missing {
        fact_id: String,
    },
    Gt {
        fact_id: String,
        value: f64,
    },
    Gte {
        fact_id: String,
        value: f64,
    },
    Lt {
        fact_id: String,
        value: f64,
    },
    Lte {
        fact_id: String,
        value: f64,
    },
    ContainsKey {
        fact_id: String,
        key: String,
    },
    BucketCountGte {
        fact_id: String,
        key: String,
        value: u64,
    },
    Any {
        expressions: Vec<RouteConditionExpr>,
    },
    All {
        expressions: Vec<RouteConditionExpr>,
    },
    Not {
        expression: Box<RouteConditionExpr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionStatus {
    Matched,
    NotMatched,
    Unknown,
}

impl ConditionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Matched => "matched",
            Self::NotMatched => "not_matched",
            Self::Unknown => "unknown",
        }
    }
}

pub struct RouteConditionInput<'a> {
    pub condition_id: &'a str,
    pub expression: &'a RouteConditionExpr,
    pub facts: &'a [EvidenceFact],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteConditionEvaluation {
    pub condition_id: String,
    pub status: ConditionStatus,
    pub matched_facts: Vec<EvidenceFact>,
    pub missing_fact_ids: Vec<String>,
    pub data_quality: DataQuality,
    pub explanation: String,
}

pub fn evaluate_route_condition(input: RouteConditionInput<'_>) -> RouteConditionEvaluation {
    let mut evaluation = evaluate_expr(input.condition_id, input.expression, input.facts);
    evaluation.missing_fact_ids.sort();
    evaluation.missing_fact_ids.dedup();
    evaluation
}

fn evaluate_expr(
    condition_id: &str,
    expression: &RouteConditionExpr,
    facts: &[EvidenceFact],
) -> RouteConditionEvaluation {
    match expression {
        RouteConditionExpr::Eq { fact_id, value } => {
            evaluate_fact_match(condition_id, facts, fact_id, |fact| &fact.value == value)
        }
        RouteConditionExpr::Neq { fact_id, value } => {
            evaluate_fact_match(condition_id, facts, fact_id, |fact| &fact.value != value)
        }
        RouteConditionExpr::Exists { fact_id } => {
            evaluate_fact_match(condition_id, facts, fact_id, |_| true)
        }
        RouteConditionExpr::Missing { fact_id } => {
            let matching = facts_for_id(facts, fact_id);
            if matching.is_empty() {
                unknown(condition_id, vec![fact_id.clone()])
            } else if matching
                .iter()
                .any(|fact| !fact.data_quality.missing.is_empty())
            {
                matched(condition_id, matching)
            } else {
                not_matched(condition_id)
            }
        }
        RouteConditionExpr::Gt { fact_id, value } => {
            evaluate_numeric(condition_id, facts, fact_id, |actual| actual > *value)
        }
        RouteConditionExpr::Gte { fact_id, value } => {
            evaluate_numeric(condition_id, facts, fact_id, |actual| actual >= *value)
        }
        RouteConditionExpr::Lt { fact_id, value } => {
            evaluate_numeric(condition_id, facts, fact_id, |actual| actual < *value)
        }
        RouteConditionExpr::Lte { fact_id, value } => {
            evaluate_numeric(condition_id, facts, fact_id, |actual| actual <= *value)
        }
        RouteConditionExpr::ContainsKey { fact_id, key } => {
            evaluate_fact_match(condition_id, facts, fact_id, |fact| {
                fact.value
                    .as_object()
                    .is_some_and(|object| object.contains_key(key))
            })
        }
        RouteConditionExpr::BucketCountGte {
            fact_id,
            key,
            value,
        } => evaluate_fact_match(condition_id, facts, fact_id, |fact| {
            fact.value
                .get(key)
                .and_then(Value::as_u64)
                .is_some_and(|actual| actual >= *value)
        }),
        RouteConditionExpr::Any { expressions } => evaluate_any(condition_id, expressions, facts),
        RouteConditionExpr::All { expressions } => evaluate_all(condition_id, expressions, facts),
        RouteConditionExpr::Not { expression } => {
            let mut inner = evaluate_expr(condition_id, expression, facts);
            inner.status = match inner.status {
                ConditionStatus::Matched => ConditionStatus::NotMatched,
                ConditionStatus::NotMatched => ConditionStatus::Matched,
                ConditionStatus::Unknown => ConditionStatus::Unknown,
            };
            inner.explanation = format!("not({})", inner.explanation);
            inner
        }
    }
}

fn evaluate_any(
    condition_id: &str,
    expressions: &[RouteConditionExpr],
    facts: &[EvidenceFact],
) -> RouteConditionEvaluation {
    let evaluations = expressions
        .iter()
        .map(|expression| evaluate_expr(condition_id, expression, facts))
        .collect::<Vec<_>>();
    if evaluations
        .iter()
        .any(|evaluation| evaluation.status == ConditionStatus::Matched)
    {
        combine(condition_id, ConditionStatus::Matched, evaluations)
    } else if evaluations
        .iter()
        .any(|evaluation| evaluation.status == ConditionStatus::Unknown)
    {
        combine(condition_id, ConditionStatus::Unknown, evaluations)
    } else {
        combine(condition_id, ConditionStatus::NotMatched, evaluations)
    }
}

fn evaluate_all(
    condition_id: &str,
    expressions: &[RouteConditionExpr],
    facts: &[EvidenceFact],
) -> RouteConditionEvaluation {
    let evaluations = expressions
        .iter()
        .map(|expression| evaluate_expr(condition_id, expression, facts))
        .collect::<Vec<_>>();
    if evaluations
        .iter()
        .any(|evaluation| evaluation.status == ConditionStatus::NotMatched)
    {
        combine(condition_id, ConditionStatus::NotMatched, evaluations)
    } else if evaluations
        .iter()
        .any(|evaluation| evaluation.status == ConditionStatus::Unknown)
    {
        combine(condition_id, ConditionStatus::Unknown, evaluations)
    } else {
        combine(condition_id, ConditionStatus::Matched, evaluations)
    }
}

fn combine(
    condition_id: &str,
    status: ConditionStatus,
    evaluations: Vec<RouteConditionEvaluation>,
) -> RouteConditionEvaluation {
    let mut matched_facts = Vec::new();
    let mut missing_fact_ids = Vec::new();
    let mut data_quality = DataQuality {
        clock_confidence: crate::ClockConfidence::Medium,
        ..Default::default()
    };
    for evaluation in evaluations {
        matched_facts.extend(evaluation.matched_facts);
        missing_fact_ids.extend(evaluation.missing_fact_ids);
        merge_data_quality(&mut data_quality, &evaluation.data_quality);
    }
    RouteConditionEvaluation {
        condition_id: condition_id.to_string(),
        status,
        matched_facts,
        missing_fact_ids,
        data_quality,
        explanation: status.as_str().to_string(),
    }
}

fn evaluate_fact_match(
    condition_id: &str,
    facts: &[EvidenceFact],
    fact_id: &str,
    predicate: impl Fn(&EvidenceFact) -> bool,
) -> RouteConditionEvaluation {
    let matching = facts_for_id(facts, fact_id);
    if matching.is_empty() {
        return unknown(condition_id, vec![fact_id.to_string()]);
    }
    let matched_facts = matching
        .iter()
        .filter(|fact| predicate(fact))
        .cloned()
        .collect::<Vec<_>>();
    if matched_facts.is_empty() {
        not_matched(condition_id)
    } else {
        matched(condition_id, matched_facts)
    }
}

fn evaluate_numeric(
    condition_id: &str,
    facts: &[EvidenceFact],
    fact_id: &str,
    predicate: impl Fn(f64) -> bool,
) -> RouteConditionEvaluation {
    evaluate_fact_match(condition_id, facts, fact_id, |fact| {
        fact.value.as_f64().is_some_and(&predicate)
    })
}

fn facts_for_id(facts: &[EvidenceFact], fact_id: &str) -> Vec<EvidenceFact> {
    facts
        .iter()
        .filter(|fact| fact.fact_id == fact_id)
        .cloned()
        .collect()
}

fn matched(condition_id: &str, matched_facts: Vec<EvidenceFact>) -> RouteConditionEvaluation {
    let mut data_quality = DataQuality {
        clock_confidence: crate::ClockConfidence::Medium,
        ..Default::default()
    };
    for fact in &matched_facts {
        merge_data_quality(&mut data_quality, &fact.data_quality);
    }
    RouteConditionEvaluation {
        condition_id: condition_id.to_string(),
        status: ConditionStatus::Matched,
        matched_facts,
        missing_fact_ids: Vec::new(),
        data_quality,
        explanation: "predicate matched typed fact(s)".to_string(),
    }
}

fn not_matched(condition_id: &str) -> RouteConditionEvaluation {
    RouteConditionEvaluation {
        condition_id: condition_id.to_string(),
        status: ConditionStatus::NotMatched,
        matched_facts: Vec::new(),
        missing_fact_ids: Vec::new(),
        data_quality: DataQuality {
            clock_confidence: crate::ClockConfidence::Medium,
            ..Default::default()
        },
        explanation: "typed fact(s) were present but predicate did not match".to_string(),
    }
}

fn unknown(condition_id: &str, missing_fact_ids: Vec<String>) -> RouteConditionEvaluation {
    RouteConditionEvaluation {
        condition_id: condition_id.to_string(),
        status: ConditionStatus::Unknown,
        matched_facts: Vec::new(),
        missing_fact_ids,
        data_quality: DataQuality {
            clock_confidence: crate::ClockConfidence::Medium,
            ..Default::default()
        },
        explanation: "required typed fact(s) were missing".to_string(),
    }
}

fn merge_data_quality(target: &mut DataQuality, source: &DataQuality) {
    target.dropped |= source.dropped;
    target.throttled |= source.throttled;
    target.truncated |= source.truncated;
    target.drop_count += source.drop_count;
    for missing in &source.missing {
        if !target.missing.contains(missing) {
            target.missing.push(missing.clone());
        }
    }
    for note in &source.notes {
        if !target.notes.contains(note) {
            target.notes.push(note.clone());
        }
    }
    if target.clock_confidence == crate::ClockConfidence::Unknown
        && source.clock_confidence != crate::ClockConfidence::Unknown
    {
        target.clock_confidence = source.clock_confidence;
    }
}
