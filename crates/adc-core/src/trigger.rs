use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    profile::{RuleType, TriggerRule},
    AdcError, AdcResult, DataQuality, RecorderBudgetStatus, RecorderCoverageConfidence,
    RecorderCoverageState, RecorderSignalCoverage,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    Threshold,
    Delta,
    BurstCount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerDecisionOutcome {
    Fired,
    NotFired,
    SuppressedCooldown,
    SuppressedHysteresis,
    SuppressedStorm,
    SkippedMissingCoverage,
    SkippedBudgetExhausted,
    SkippedPolicyInvalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerDecisionReason {
    ThresholdCrossed,
    ThresholdNotCrossed,
    DeltaCrossed,
    DeltaNotCrossed,
    BurstCountCrossed,
    BurstCountNotCrossed,
    CoverageMissing,
    CoverageUnavailable,
    CooldownActive,
    HysteresisNotCleared,
    StormSuppressed,
    BudgetExhausted,
    PolicyInvalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerBudgetDecision {
    Accepted,
    SkippedBudgetExhausted,
    NotRequired,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerDecision {
    pub schema_version: String,
    pub decision_id: String,
    pub policy_id: String,
    pub rule_id: String,
    pub trigger_name: String,
    pub trigger_kind: TriggerKind,
    pub decision: TriggerDecisionOutcome,
    pub decision_reason: TriggerDecisionReason,
    pub signal_id: String,
    pub coverage_signal_id: String,
    pub observed_value: Option<f64>,
    pub threshold: Option<f64>,
    pub coverage_state: RecorderCoverageState,
    pub coverage_confidence: RecorderCoverageConfidence,
    pub coverage_ref: Option<String>,
    pub budget_decision: TriggerBudgetDecision,
    pub budget_status_ref: Option<String>,
    pub incident_id: Option<String>,
    pub trigger_event_ref: Option<String>,
    pub root_cause_claim: bool,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerPolicy {
    pub schema_version: String,
    pub policy_id: String,
    pub policy_scope: String,
    pub rules: Vec<TriggerPolicyRule>,
    pub default_decision: String,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerPolicyRule {
    pub rule_id: String,
    pub trigger_name: String,
    pub trigger_kind: TriggerKind,
    pub signal_id: String,
    pub coverage_signal_id: String,
    pub operator: Option<String>,
    pub threshold: Option<f64>,
    pub delta: Option<f64>,
    pub burst_count: Option<u64>,
    pub burst_window_ms: Option<u64>,
    pub min_coverage_state: RecorderCoverageState,
    pub required_signal_state: String,
    pub cooldown_ms: u64,
    pub hysteresis: Option<TriggerPolicyHysteresis>,
    pub freeze_profile: String,
    pub root_cause_claim: bool,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerPolicyHysteresis {
    pub clear_below: Option<f64>,
    pub min_clear_duration_ms: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TriggerRuntimeState {
    last_fired_mono_ns_by_rule: BTreeMap<String, u64>,
}

pub fn trigger_policy_for_profile(policy_id: &str, profile: &crate::Profile) -> TriggerPolicy {
    TriggerPolicy {
        schema_version: "obs.trigger_policy.v1".to_string(),
        policy_id: policy_id.to_string(),
        policy_scope: "recorder".to_string(),
        rules: profile
            .triggers
            .iter()
            .map(trigger_policy_rule_for_profile_rule)
            .collect(),
        default_decision: "evaluate".to_string(),
        data_quality: DataQuality {
            clock_confidence: crate::ClockConfidence::Medium,
            ..Default::default()
        },
    }
}

pub fn evaluate_trigger(rule: &TriggerRule, input: &TriggerInput) -> AdcResult<TriggerEvaluation> {
    let mut data_quality = DataQuality {
        clock_confidence: crate::ClockConfidence::Medium,
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

pub fn trigger_decision_for_rule(
    policy_id: &str,
    rule: &TriggerRule,
    input: Option<&TriggerInput>,
    coverage: Option<&RecorderSignalCoverage>,
    coverage_ref: Option<String>,
    incident_id: Option<String>,
) -> AdcResult<TriggerDecision> {
    let signal_id = rule
        .signal
        .clone()
        .or_else(|| input.map(|input| input.signal.clone()))
        .unwrap_or_else(|| "unknown".to_string());
    let coverage_signal_id = coverage
        .map(|coverage| coverage.signal_id.clone())
        .unwrap_or_else(|| coverage_signal_id_for_trigger_signal(&signal_id));
    let coverage_state = coverage
        .map(|coverage| coverage.coverage_state)
        .unwrap_or(RecorderCoverageState::Unknown);
    let coverage_confidence = coverage
        .map(|coverage| coverage.coverage_confidence)
        .unwrap_or(RecorderCoverageConfidence::Unknown);
    let mut data_quality = coverage
        .map(|coverage| coverage.data_quality.clone())
        .unwrap_or_else(|| DataQuality {
            clock_confidence: crate::ClockConfidence::Medium,
            ..Default::default()
        });

    if matches!(
        coverage_state,
        RecorderCoverageState::Missing
            | RecorderCoverageState::Unavailable
            | RecorderCoverageState::Unknown
    ) {
        let reason = if matches!(coverage_state, RecorderCoverageState::Unavailable) {
            TriggerDecisionReason::CoverageUnavailable
        } else {
            TriggerDecisionReason::CoverageMissing
        };
        let missing =
            format!("{coverage_signal_id} coverage is not sufficient for trigger evaluation");
        if !data_quality.missing.iter().any(|item| item == &missing) {
            data_quality.missing.push(missing);
        }
        return Ok(trigger_decision(TriggerDecisionInput {
            policy_id,
            rule,
            signal_id,
            coverage_signal_id,
            observed_value: input.and_then(|input| input.value),
            coverage_state,
            coverage_confidence,
            coverage_ref,
            incident_id,
            decision: TriggerDecisionOutcome::SkippedMissingCoverage,
            decision_reason: reason,
            budget_decision: TriggerBudgetDecision::NotRequired,
            data_quality,
        }));
    }

    let Some(input) = input else {
        data_quality
            .missing
            .push("trigger input was unavailable".to_string());
        return Ok(trigger_decision(TriggerDecisionInput {
            policy_id,
            rule,
            signal_id,
            coverage_signal_id,
            observed_value: None,
            coverage_state,
            coverage_confidence,
            coverage_ref,
            incident_id,
            decision: TriggerDecisionOutcome::SkippedPolicyInvalid,
            decision_reason: TriggerDecisionReason::PolicyInvalid,
            budget_decision: TriggerBudgetDecision::NotRequired,
            data_quality,
        }));
    };
    let evaluation = evaluate_trigger(rule, input)?;
    data_quality.notes.extend(evaluation.data_quality.notes);
    data_quality.missing.extend(evaluation.data_quality.missing);
    if evaluation.matched {
        Ok(trigger_decision(TriggerDecisionInput {
            policy_id,
            rule,
            signal_id,
            coverage_signal_id,
            observed_value: input.value,
            coverage_state,
            coverage_confidence,
            coverage_ref,
            incident_id,
            decision: TriggerDecisionOutcome::Fired,
            decision_reason: fired_reason_for_rule(rule),
            budget_decision: TriggerBudgetDecision::Accepted,
            data_quality,
        }))
    } else {
        Ok(trigger_decision(TriggerDecisionInput {
            policy_id,
            rule,
            signal_id,
            coverage_signal_id,
            observed_value: input.value,
            coverage_state,
            coverage_confidence,
            coverage_ref,
            incident_id: None,
            decision: TriggerDecisionOutcome::NotFired,
            decision_reason: not_fired_reason_for_rule(rule),
            budget_decision: TriggerBudgetDecision::NotRequired,
            data_quality,
        }))
    }
}

fn trigger_policy_rule_for_profile_rule(rule: &TriggerRule) -> TriggerPolicyRule {
    let signal_id = rule
        .signal
        .clone()
        .unwrap_or_else(|| "kmsg.message".to_string());
    let trigger_kind = trigger_kind_for_rule(rule);
    TriggerPolicyRule {
        rule_id: format!("{}_v1", rule.name),
        trigger_name: rule.name.clone(),
        trigger_kind,
        coverage_signal_id: coverage_signal_id_for_trigger_signal(&signal_id),
        signal_id,
        operator: rule.op.clone(),
        threshold: if matches!(trigger_kind, TriggerKind::Threshold) {
            rule.value
        } else {
            None
        },
        delta: if matches!(trigger_kind, TriggerKind::Delta) {
            rule.value
        } else {
            None
        },
        burst_count: if matches!(trigger_kind, TriggerKind::BurstCount) {
            Some(rule.value.unwrap_or(1.0).max(1.0) as u64)
        } else {
            None
        },
        burst_window_ms: if matches!(trigger_kind, TriggerKind::BurstCount) {
            Some(rule.duration_sec.unwrap_or(1).max(1).saturating_mul(1000))
        } else {
            None
        },
        min_coverage_state: RecorderCoverageState::Partial,
        required_signal_state: "covered_or_partial".to_string(),
        cooldown_ms: rule.cooldown_ms.unwrap_or(0),
        hysteresis: rule
            .hysteresis
            .as_ref()
            .map(|hysteresis| TriggerPolicyHysteresis {
                clear_below: hysteresis.clear_below,
                min_clear_duration_ms: hysteresis.min_clear_duration_ms.unwrap_or(0),
            }),
        freeze_profile: rule
            .capture_profile
            .clone()
            .unwrap_or_else(|| "default_pre_window".to_string()),
        root_cause_claim: false,
        data_quality: DataQuality {
            clock_confidence: crate::ClockConfidence::Medium,
            ..Default::default()
        },
    }
}

pub fn trigger_decision_for_budget_refusal(
    policy_id: &str,
    rule: &TriggerRule,
    input: Option<&TriggerInput>,
    coverage: Option<&RecorderSignalCoverage>,
    coverage_ref: Option<String>,
    budget_status: &RecorderBudgetStatus,
) -> AdcResult<TriggerDecision> {
    let signal_id = rule
        .signal
        .clone()
        .or_else(|| input.map(|input| input.signal.clone()))
        .unwrap_or_else(|| "unknown".to_string());
    let coverage_signal_id = coverage
        .map(|coverage| coverage.signal_id.clone())
        .unwrap_or_else(|| coverage_signal_id_for_trigger_signal(&signal_id));
    let coverage_state = coverage
        .map(|coverage| coverage.coverage_state)
        .unwrap_or(RecorderCoverageState::Unknown);
    let coverage_confidence = coverage
        .map(|coverage| coverage.coverage_confidence)
        .unwrap_or(RecorderCoverageConfidence::Unknown);
    let mut data_quality = budget_status.data_quality.clone();
    data_quality.throttled = true;
    if !data_quality
        .notes
        .iter()
        .any(|note| note == "trigger preservation skipped due to recorder budget admission")
    {
        data_quality
            .notes
            .push("trigger preservation skipped due to recorder budget admission".to_string());
    }
    Ok(trigger_decision(TriggerDecisionInput {
        policy_id,
        rule,
        signal_id,
        coverage_signal_id,
        observed_value: input.and_then(|input| input.value),
        coverage_state,
        coverage_confidence,
        coverage_ref,
        incident_id: None,
        decision: TriggerDecisionOutcome::SkippedBudgetExhausted,
        decision_reason: TriggerDecisionReason::BudgetExhausted,
        budget_decision: TriggerBudgetDecision::SkippedBudgetExhausted,
        data_quality,
    }))
}

pub fn trigger_decision_with_runtime_state(
    policy_id: &str,
    rule: &TriggerRule,
    input: Option<&TriggerInput>,
    coverage: Option<&RecorderSignalCoverage>,
    coverage_ref: Option<String>,
    now_mono_ns: u64,
    state: &mut TriggerRuntimeState,
) -> AdcResult<TriggerDecision> {
    let mut decision =
        trigger_decision_for_rule(policy_id, rule, input, coverage, coverage_ref, None)?;
    if decision.decision != TriggerDecisionOutcome::Fired {
        return Ok(decision);
    }

    if let Some(last_fired) = state.last_fired_mono_ns_by_rule.get(&rule.name).copied() {
        if let Some(cooldown_ms) = rule.cooldown_ms {
            let elapsed_ns = now_mono_ns.saturating_sub(last_fired);
            if elapsed_ns < cooldown_ms.saturating_mul(1_000_000) {
                decision.decision = TriggerDecisionOutcome::SuppressedCooldown;
                decision.decision_reason = TriggerDecisionReason::CooldownActive;
                decision.budget_decision = TriggerBudgetDecision::NotRequired;
                decision.incident_id = None;
                decision.trigger_event_ref = None;
                decision
                    .data_quality
                    .notes
                    .push("trigger fire suppressed by service-run cooldown".to_string());
                return Ok(decision);
            }
        }
        if let Some(hysteresis) = &rule.hysteresis {
            if let (Some(clear_below), Some(value)) =
                (hysteresis.clear_below, decision.observed_value)
            {
                if value >= clear_below {
                    decision.decision = TriggerDecisionOutcome::SuppressedHysteresis;
                    decision.decision_reason = TriggerDecisionReason::HysteresisNotCleared;
                    decision.budget_decision = TriggerBudgetDecision::NotRequired;
                    decision.incident_id = None;
                    decision.trigger_event_ref = None;
                    decision.data_quality.notes.push(
                        "trigger fire suppressed until hysteresis clear condition".to_string(),
                    );
                    return Ok(decision);
                }
            }
        }
    }

    state
        .last_fired_mono_ns_by_rule
        .insert(rule.name.clone(), now_mono_ns);
    Ok(decision)
}

struct TriggerDecisionInput<'a> {
    policy_id: &'a str,
    rule: &'a TriggerRule,
    signal_id: String,
    coverage_signal_id: String,
    observed_value: Option<f64>,
    coverage_state: RecorderCoverageState,
    coverage_confidence: RecorderCoverageConfidence,
    coverage_ref: Option<String>,
    incident_id: Option<String>,
    decision: TriggerDecisionOutcome,
    decision_reason: TriggerDecisionReason,
    budget_decision: TriggerBudgetDecision,
    data_quality: DataQuality,
}

fn trigger_decision(input: TriggerDecisionInput<'_>) -> TriggerDecision {
    let trigger_event_ref = input.incident_id.as_ref().map(|incident_id| {
        format!("artifact://recorder/incidents/{incident_id}/trigger_event.json")
    });
    TriggerDecision {
        schema_version: "obs.trigger_decision.v1".to_string(),
        decision_id: format!("TD-{}-{}", input.policy_id, input.rule.name),
        policy_id: input.policy_id.to_string(),
        rule_id: format!("{}_v1", input.rule.name),
        trigger_name: input.rule.name.clone(),
        trigger_kind: trigger_kind_for_rule(input.rule),
        decision: input.decision,
        decision_reason: input.decision_reason,
        signal_id: input.signal_id,
        coverage_signal_id: input.coverage_signal_id,
        observed_value: input.observed_value,
        threshold: input.rule.value,
        coverage_state: input.coverage_state,
        coverage_confidence: input.coverage_confidence,
        coverage_ref: input.coverage_ref,
        budget_decision: input.budget_decision,
        budget_status_ref: None,
        incident_id: input.incident_id,
        trigger_event_ref,
        root_cause_claim: false,
        data_quality: input.data_quality,
    }
}

pub fn coverage_signal_id_for_trigger_signal(signal: &str) -> String {
    match signal {
        "cpu.total_percent" => "cpu.summary",
        "memory.available_percent" | "memory.available_delta_kb" => "memory.summary",
        "network.total_delta_bytes" => "network.counters",
        "kmsg.message" | "kmsg" => "kmsg.cursor",
        _ => signal,
    }
    .to_string()
}

fn trigger_kind_for_rule(rule: &TriggerRule) -> TriggerKind {
    match rule.rule_type {
        RuleType::Delta | RuleType::DeltaRate => TriggerKind::Delta,
        RuleType::Burst | RuleType::KmsgPattern | RuleType::SequencePattern => {
            TriggerKind::BurstCount
        }
        RuleType::Threshold
        | RuleType::ThresholdDuration
        | RuleType::MissingSample
        | RuleType::BaselineDeviation => TriggerKind::Threshold,
    }
}

fn fired_reason_for_rule(rule: &TriggerRule) -> TriggerDecisionReason {
    match trigger_kind_for_rule(rule) {
        TriggerKind::Threshold => TriggerDecisionReason::ThresholdCrossed,
        TriggerKind::Delta => TriggerDecisionReason::DeltaCrossed,
        TriggerKind::BurstCount => TriggerDecisionReason::BurstCountCrossed,
    }
}

fn not_fired_reason_for_rule(rule: &TriggerRule) -> TriggerDecisionReason {
    match trigger_kind_for_rule(rule) {
        TriggerKind::Threshold => TriggerDecisionReason::ThresholdNotCrossed,
        TriggerKind::Delta => TriggerDecisionReason::DeltaNotCrossed,
        TriggerKind::BurstCount => TriggerDecisionReason::BurstCountNotCrossed,
    }
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
