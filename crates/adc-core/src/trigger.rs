use serde::{Deserialize, Serialize};

use crate::{
    profile::{RuleType, TriggerRule},
    AdcError, AdcResult, DataQuality,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerInput {
    pub signal: String,
    pub value: Option<f64>,
    pub duration_sec: Option<u64>,
    pub text: Option<String>,
    pub severity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerEvaluation {
    pub trigger_name: String,
    pub matched: bool,
    pub reason: String,
    pub capture_profile: Option<String>,
    pub data_quality: DataQuality,
}

pub fn evaluate_trigger(rule: &TriggerRule, input: &TriggerInput) -> AdcResult<TriggerEvaluation> {
    let mut data_quality = DataQuality {
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };
    let matched = match rule.rule_type {
        RuleType::Threshold
        | RuleType::ThresholdDuration
        | RuleType::Delta
        | RuleType::DeltaRate => evaluate_numeric_rule(rule, input, &mut data_quality)?,
        RuleType::KmsgPattern => evaluate_kmsg_rule(rule, input, &mut data_quality),
        RuleType::MissingSample => input.value.is_none(),
        RuleType::Burst | RuleType::BaselineDeviation | RuleType::SequencePattern => {
            data_quality.notes.push(format!(
                "trigger type {:?} is parsed but requires a collector-specific evaluator",
                rule.rule_type
            ));
            false
        }
    };

    Ok(TriggerEvaluation {
        trigger_name: rule.name.clone(),
        matched,
        reason: if matched {
            matched_reason(rule, input)
        } else {
            format!("trigger {} did not match observed input", rule.name)
        },
        capture_profile: rule.capture_profile.clone(),
        data_quality,
    })
}

fn evaluate_numeric_rule(
    rule: &TriggerRule,
    input: &TriggerInput,
    data_quality: &mut DataQuality,
) -> AdcResult<bool> {
    if rule.signal.as_deref() != Some(input.signal.as_str()) {
        data_quality.notes.push(format!(
            "input signal {} did not match trigger signal {:?}",
            input.signal, rule.signal
        ));
        return Ok(false);
    }
    let actual = match input.value {
        Some(value) => value,
        None => {
            data_quality
                .missing
                .push("numeric trigger input value".to_string());
            return Ok(false);
        }
    };
    let threshold = rule.value.ok_or_else(|| {
        AdcError::ProfileValidation(format!("trigger {} missing numeric value", rule.name))
    })?;
    let op = rule.op.as_deref().unwrap_or(">");
    let numeric_match = match op {
        ">" => actual > threshold,
        ">=" => actual >= threshold,
        "<" => actual < threshold,
        "<=" => actual <= threshold,
        "==" => (actual - threshold).abs() < f64::EPSILON,
        other => {
            return Err(AdcError::ProfileValidation(format!(
                "unsupported trigger op: {other}"
            )))
        }
    };
    if !numeric_match {
        return Ok(false);
    }
    if rule.rule_type == RuleType::ThresholdDuration {
        let required = rule.duration_sec.unwrap_or(0);
        let observed = input.duration_sec.unwrap_or(0);
        return Ok(observed >= required);
    }
    Ok(true)
}

fn evaluate_kmsg_rule(
    rule: &TriggerRule,
    input: &TriggerInput,
    data_quality: &mut DataQuality,
) -> bool {
    if let Some(required) = &rule.severity_at_least {
        if !severity_at_least(input.severity.as_deref(), required) {
            data_quality.notes.push(format!(
                "severity {:?} is below required severity {required}",
                input.severity
            ));
            return false;
        }
    }
    let Some(text) = input.text.as_deref() else {
        data_quality.missing.push("kmsg trigger text".to_string());
        return false;
    };
    let text = text.to_ascii_lowercase();
    let matched = rule
        .patterns
        .iter()
        .any(|pattern| text.contains(&pattern.to_ascii_lowercase()));
    if !matched {
        data_quality
            .notes
            .push("no pattern matched observed kmsg text".to_string());
    }
    matched
}

fn severity_at_least(actual: Option<&str>, required: &str) -> bool {
    severity_rank(actual.unwrap_or("debug")) >= severity_rank(required)
}

fn severity_rank(severity: &str) -> u8 {
    match severity.to_ascii_lowercase().as_str() {
        "emerg" | "emergency" => 7,
        "alert" => 6,
        "crit" | "critical" => 5,
        "err" | "error" => 4,
        "warn" | "warning" => 3,
        "notice" => 2,
        "info" => 1,
        _ => 0,
    }
}

fn matched_reason(rule: &TriggerRule, input: &TriggerInput) -> String {
    match input.value {
        Some(value) => format!(
            "observed {} value {} matched trigger {}",
            input.signal, value, rule.name
        ),
        None => format!("observed input matched trigger {}", rule.name),
    }
}
