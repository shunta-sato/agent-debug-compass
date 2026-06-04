use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    agent_context::{
        AgentContextRef, InvestigationContinuationFact, InvestigationRoute, InvestigationStep,
        OpenedRefSummary, RouteBranchCondition,
    },
    evaluate_route_condition, DataQuality, EvidenceFact, RouteConditionInput,
};

const SESSION_STATE_SCHEMA_VERSION: &str = "obs.investigation_session_state.v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvestigationSessionState {
    pub schema_version: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fleet_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_name: Option<String>,
    pub route_id: String,
    pub session_id: String,
    pub completed_steps: Vec<String>,
    pub completed_refs: Vec<CompletedInvestigationRef>,
    pub facts: Vec<InvestigationContinuationFact>,
    pub unknowns: Vec<String>,
    #[serde(default)]
    pub compact_summary: Vec<String>,
    pub branch_evaluations: Vec<BranchEvaluation>,
    pub next_actions: Vec<NextInvestigationAction>,
    pub raw_refs: BTreeMap<String, String>,
    pub data_quality: DataQuality,
    pub retention_policy: SessionRetentionPolicy,
    pub budget: InvestigationSessionBudget,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletedInvestigationRef {
    pub label: String,
    pub raw_ref: String,
    pub ref_kind: String,
    pub summary: String,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchEvaluation {
    pub step_id: String,
    pub condition: RouteBranchCondition,
    pub status: String,
    pub matched_facts: Vec<String>,
    #[serde(default)]
    pub missing_fact_ids: Vec<String>,
    pub missing_evidence: Vec<String>,
    pub next_step_id: String,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NextInvestigationAction {
    pub action_id: String,
    pub next_step_id: String,
    pub title: String,
    pub reason: String,
    pub expected_answer: String,
    pub refs: Vec<AgentContextRef>,
    pub target_ids: Vec<String>,
    pub cost: String,
    pub required_privilege: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRetentionPolicy {
    pub max_sessions_per_scope: usize,
    pub max_age_days: usize,
    pub cleanup_mode: String,
}

impl Default for SessionRetentionPolicy {
    fn default() -> Self {
        Self {
            max_sessions_per_scope: 64,
            max_age_days: 14,
            cleanup_mode: "manual_dry_run_first".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationSessionBudget {
    pub returned_bytes: usize,
    pub completed_ref_count: usize,
    pub fact_count: usize,
    pub branch_evaluation_count: usize,
    pub next_action_count: usize,
}

pub struct SessionStateInput<'a> {
    pub scope: &'a str,
    pub run_id: Option<&'a str>,
    pub fleet_run_id: Option<&'a str>,
    pub service_name: Option<&'a str>,
    pub route_id: &'a str,
    pub session_id: &'a str,
    pub current_step: &'a InvestigationStep,
    pub remaining_route: &'a InvestigationRoute,
    pub opened_refs: &'a [OpenedRefSummary],
    pub new_facts: &'a [InvestigationContinuationFact],
    pub raw_refs: &'a BTreeMap<String, String>,
    pub data_quality: &'a DataQuality,
    pub previous_state: Option<InvestigationSessionState>,
}

pub fn build_investigation_session_state(
    input: SessionStateInput<'_>,
) -> InvestigationSessionState {
    let branch_evaluations = evaluate_branches(
        input.current_step,
        input.remaining_route,
        input.opened_refs,
        input.new_facts,
        input.data_quality,
    );
    let next_actions = next_actions_from_evaluations(
        input.remaining_route,
        &branch_evaluations,
        input.data_quality,
    );

    let mut state = match input.previous_state.clone() {
        Some(state) => state,
        None => empty_state(&input, &branch_evaluations, &next_actions),
    };
    push_unique(
        &mut state.completed_steps,
        input.current_step.step_id.clone(),
    );
    for opened in input.opened_refs {
        push_unique_ref(
            &mut state.completed_refs,
            CompletedInvestigationRef {
                label: opened.label.clone(),
                raw_ref: opened.raw_ref.clone(),
                ref_kind: opened.ref_kind.clone(),
                summary: opened.summary.clone(),
                data_quality: opened.data_quality.clone(),
            },
        );
    }
    for fact in input.new_facts {
        push_unique_fact(&mut state.facts, fact.clone());
    }
    for branch in &branch_evaluations {
        push_unique_branch(&mut state.branch_evaluations, branch.clone());
        if branch.status == "unknown" {
            push_unique(
                &mut state.unknowns,
                format!(
                    "{} -> {}: {}",
                    branch.step_id, branch.next_step_id, branch.condition.if_observed
                ),
            );
        }
    }
    state.next_actions = next_actions;
    state.raw_refs.extend(input.raw_refs.clone());
    merge_data_quality(&mut state.data_quality, input.data_quality);
    state.compact_summary = compact_session_summary(&state);
    state.retention_policy = SessionRetentionPolicy::default();
    state.budget = session_budget(&state);
    state
}

fn empty_state(
    input: &SessionStateInput<'_>,
    branch_evaluations: &[BranchEvaluation],
    next_actions: &[NextInvestigationAction],
) -> InvestigationSessionState {
    InvestigationSessionState {
        schema_version: SESSION_STATE_SCHEMA_VERSION.to_string(),
        scope: input.scope.to_string(),
        run_id: input.run_id.map(str::to_string),
        fleet_run_id: input.fleet_run_id.map(str::to_string),
        service_name: input.service_name.map(str::to_string),
        route_id: input.route_id.to_string(),
        session_id: input.session_id.to_string(),
        completed_steps: Vec::new(),
        completed_refs: Vec::new(),
        facts: Vec::new(),
        unknowns: branch_evaluations
            .iter()
            .filter(|branch| branch.status == "unknown")
            .map(|branch| {
                format!(
                    "{} -> {}: {}",
                    branch.step_id, branch.next_step_id, branch.condition.if_observed
                )
            })
            .collect(),
        compact_summary: Vec::new(),
        branch_evaluations: branch_evaluations.to_vec(),
        next_actions: next_actions.to_vec(),
        raw_refs: input.raw_refs.clone(),
        data_quality: input.data_quality.clone(),
        retention_policy: SessionRetentionPolicy::default(),
        budget: InvestigationSessionBudget {
            returned_bytes: 0,
            completed_ref_count: 0,
            fact_count: 0,
            branch_evaluation_count: branch_evaluations.len(),
            next_action_count: next_actions.len(),
        },
    }
}

fn compact_session_summary(state: &InvestigationSessionState) -> Vec<String> {
    let mut summary = vec![format!(
        "completed_steps={} completed_refs={} facts={} unknowns={} next_actions={}",
        state.completed_steps.len(),
        state.completed_refs.len(),
        state.facts.len(),
        state.unknowns.len(),
        state.next_actions.len()
    )];
    if let Some(action) = state.next_actions.first() {
        summary.push(format!(
            "next_action={} step={} refs={}",
            action.action_id,
            action.next_step_id,
            action.refs.len()
        ));
    }
    if !state.data_quality.missing.is_empty() {
        summary.push(format!(
            "data_quality_missing={}",
            state.data_quality.missing.len()
        ));
    }
    summary
}

fn evaluate_branches(
    step: &InvestigationStep,
    remaining_route: &InvestigationRoute,
    opened_refs: &[OpenedRefSummary],
    new_facts: &[InvestigationContinuationFact],
    data_quality: &DataQuality,
) -> Vec<BranchEvaluation> {
    step.branch_conditions
        .iter()
        .map(|condition| {
            evaluate_branch(
                step,
                remaining_route,
                condition,
                opened_refs,
                new_facts,
                data_quality,
            )
        })
        .collect()
}

fn evaluate_branch(
    step: &InvestigationStep,
    remaining_route: &InvestigationRoute,
    condition: &RouteBranchCondition,
    opened_refs: &[OpenedRefSummary],
    new_facts: &[InvestigationContinuationFact],
    data_quality: &DataQuality,
) -> BranchEvaluation {
    if let Some(predicate) = &condition.predicate {
        let facts = opened_ref_facts(opened_refs);
        let evaluation = evaluate_route_condition(RouteConditionInput {
            condition_id: &format!("{}:{}", step.step_id, condition.next_step_id),
            expression: predicate,
            facts: &facts,
        });
        let mut branch_quality = evaluation.data_quality.clone();
        merge_data_quality(&mut branch_quality, data_quality);
        return BranchEvaluation {
            step_id: step.step_id.clone(),
            condition: condition.clone(),
            status: evaluation.status.as_str().to_string(),
            matched_facts: evaluation
                .matched_facts
                .iter()
                .take(5)
                .map(render_evidence_fact)
                .collect(),
            missing_fact_ids: evaluation.missing_fact_ids.clone(),
            missing_evidence: evaluation
                .missing_fact_ids
                .iter()
                .map(|fact_id| format!("missing typed fact: {fact_id}"))
                .collect(),
            next_step_id: condition.next_step_id.clone(),
            data_quality: branch_quality,
        };
    }

    let condition_text = condition.if_observed.to_ascii_lowercase();
    let evidence_text = evidence_text(opened_refs, new_facts);
    let missing_evidence = missing_evidence(opened_refs, data_quality);
    let has_missing = !missing_evidence.is_empty();
    let service_state_text = service_state_text(opened_refs, new_facts);
    let service_available = service_state_text.contains("availability=available");
    let service_unavailable = service_state_text.contains("availability=unavailable")
        || evidence_text.contains("state is unknown")
        || service_state_text.contains("availability=unknown");
    let has_remaining_refs = remaining_route
        .steps
        .iter()
        .any(|step| !step.refs.is_empty());
    let has_signal_words = ["error", "warn", "fail", "timeout", "denied", "marker"]
        .iter()
        .any(|needle| evidence_text.contains(needle));
    let has_opened_refs = !opened_refs.is_empty();

    let (status, matched_facts, missing) = if condition_text.contains("unavailable")
        || condition_text.contains("unknown")
    {
        if service_unavailable {
            (
                "matched",
                matching_summaries(opened_refs, &["unavailable", "unknown", "missing"]),
                missing_evidence,
            )
        } else if service_available {
            (
                "not_matched",
                matching_summaries(opened_refs, &["available"]),
                Vec::new(),
            )
        } else if has_missing {
            ("unknown", Vec::new(), missing_evidence)
        } else {
            (
                "unknown",
                Vec::new(),
                inferred_missing(opened_refs, missing_evidence),
            )
        }
    } else if condition_text.contains("available") && condition_text.contains("refs exist") {
        if service_available && has_remaining_refs {
            (
                "matched",
                matching_summaries(opened_refs, &["available"]),
                Vec::new(),
            )
        } else if service_unavailable {
            (
                "not_matched",
                matching_summaries(opened_refs, &["unavailable", "unknown"]),
                missing_evidence,
            )
        } else {
            (
                "unknown",
                Vec::new(),
                inferred_missing(opened_refs, missing_evidence),
            )
        }
    } else if condition_text.contains("failed")
        || condition_text.contains("missing data_quality")
        || condition_text.contains("partial")
    {
        if has_missing
            || evidence_text.contains("partial")
            || evidence_text.contains("unreachable")
            || evidence_text.contains("collector_failed")
        {
            (
                "matched",
                matching_summaries(opened_refs, &["partial", "unreachable", "missing"]),
                missing_evidence,
            )
        } else if has_opened_refs {
            (
                "not_matched",
                matching_summaries(opened_refs, &["available", "captured"]),
                Vec::new(),
            )
        } else {
            (
                "unknown",
                Vec::new(),
                inferred_missing(opened_refs, missing_evidence),
            )
        }
    } else if condition_text.contains("captured target") && condition_text.contains("evidence refs")
    {
        if has_remaining_refs || evidence_text.contains("captured") {
            (
                "matched",
                matching_summaries(opened_refs, &["captured"]),
                Vec::new(),
            )
        } else {
            (
                "unknown",
                Vec::new(),
                inferred_missing(opened_refs, missing_evidence),
            )
        }
    } else if condition_text.contains("differs") || condition_text.contains("different") {
        if evidence_text.contains("different") || evidence_text.contains("partial") {
            (
                "matched",
                matching_summaries(opened_refs, &["different", "partial"]),
                missing_evidence,
            )
        } else if evidence_text.contains("same") {
            (
                "not_matched",
                matching_summaries(opened_refs, &["same"]),
                Vec::new(),
            )
        } else {
            (
                "unknown",
                Vec::new(),
                inferred_missing(opened_refs, missing_evidence),
            )
        }
    } else if condition_text.contains("bounded log") || condition_text.contains("domain markers") {
        if has_signal_words {
            (
                "matched",
                matching_summaries(opened_refs, &["error", "warn", "fail", "timeout", "marker"]),
                Vec::new(),
            )
        } else if has_opened_refs {
            (
                "not_matched",
                matching_summaries(opened_refs, &["returned"]),
                Vec::new(),
            )
        } else {
            (
                "unknown",
                Vec::new(),
                inferred_missing(opened_refs, missing_evidence),
            )
        }
    } else if condition_text.contains("empty") {
        if has_opened_refs && opened_refs.iter().all(|opened| opened.item_count == 0) {
            ("matched", Vec::new(), Vec::new())
        } else if has_opened_refs {
            (
                "not_matched",
                matching_summaries(opened_refs, &["returned"]),
                Vec::new(),
            )
        } else {
            (
                "unknown",
                Vec::new(),
                inferred_missing(opened_refs, missing_evidence),
            )
        }
    } else if has_missing {
        ("unknown", Vec::new(), missing_evidence)
    } else if has_opened_refs {
        (
            "not_matched",
            matching_summaries(opened_refs, &[""]),
            Vec::new(),
        )
    } else {
        (
            "unknown",
            Vec::new(),
            inferred_missing(opened_refs, missing_evidence),
        )
    };

    BranchEvaluation {
        step_id: step.step_id.clone(),
        condition: condition.clone(),
        status: status.to_string(),
        matched_facts,
        missing_fact_ids: Vec::new(),
        missing_evidence: missing,
        next_step_id: condition.next_step_id.clone(),
        data_quality: data_quality.clone(),
    }
}

fn opened_ref_facts(opened_refs: &[OpenedRefSummary]) -> Vec<EvidenceFact> {
    opened_refs
        .iter()
        .flat_map(|opened| opened.facts.iter().cloned())
        .collect()
}

fn render_evidence_fact(fact: &EvidenceFact) -> String {
    match &fact.target_id {
        Some(target_id) => format!(
            "{}[{}]={} from {}",
            fact.fact_id, target_id, fact.value, fact.source_ref
        ),
        None => format!("{}={} from {}", fact.fact_id, fact.value, fact.source_ref),
    }
}

fn service_state_text(
    opened_refs: &[OpenedRefSummary],
    new_facts: &[InvestigationContinuationFact],
) -> String {
    let mut text = String::new();
    for opened in opened_refs {
        let label = opened.label.to_ascii_lowercase();
        if label.contains("service_state") || label.contains("service_investigation") {
            text.push_str(&opened.summary.to_ascii_lowercase());
            text.push(' ');
        }
    }
    for fact in new_facts {
        if fact.kind == "opened_service_state" || fact.kind == "opened_service_investigation" {
            text.push_str(&fact.statement.to_ascii_lowercase());
            text.push(' ');
        }
    }
    text
}

fn next_actions_from_evaluations(
    remaining_route: &InvestigationRoute,
    branch_evaluations: &[BranchEvaluation],
    data_quality: &DataQuality,
) -> Vec<NextInvestigationAction> {
    let mut selected_step_ids = branch_evaluations
        .iter()
        .filter(|branch| branch.status == "matched")
        .map(|branch| branch.next_step_id.clone())
        .collect::<Vec<_>>();
    if selected_step_ids.is_empty() {
        selected_step_ids.extend(
            branch_evaluations
                .iter()
                .filter(|branch| branch.status == "unknown")
                .map(|branch| branch.next_step_id.clone()),
        );
    }
    if selected_step_ids.is_empty() {
        selected_step_ids.extend(
            remaining_route
                .steps
                .iter()
                .map(|step| step.step_id.clone()),
        );
    }
    let known_step_ids = remaining_route
        .steps
        .iter()
        .map(|step| step.step_id.clone())
        .collect::<BTreeSet<_>>();
    let mut actions = Vec::new();
    for step_id in selected_step_ids {
        if let Some(step) = remaining_route
            .steps
            .iter()
            .find(|step| step.step_id == step_id)
        {
            let index = actions.len() + 1;
            let action = action_for_step(index, step, None);
            push_unique_action(&mut actions, action);
        } else if step_id == "IR-DQ" || !known_step_ids.contains(&step_id) {
            let index = actions.len() + 1;
            let action = data_quality_action(index, &step_id, data_quality);
            push_unique_action(&mut actions, action);
        }
        if actions.len() >= 5 {
            break;
        }
    }
    if actions.is_empty() {
        for step in remaining_route.steps.iter().take(5) {
            let index = actions.len() + 1;
            let action = action_for_step(index, step, None);
            push_unique_action(&mut actions, action);
        }
    }
    actions
}

fn action_for_step(
    index: usize,
    step: &InvestigationStep,
    reason_override: Option<String>,
) -> NextInvestigationAction {
    NextInvestigationAction {
        action_id: format!("IA{index:03}"),
        next_step_id: step.step_id.clone(),
        title: step.title.clone(),
        reason: reason_override.unwrap_or_else(|| step.purpose.clone()),
        expected_answer: step.expected_answer.clone(),
        refs: step.refs.iter().take(5).cloned().collect(),
        target_ids: step.target_ids.clone(),
        cost: step.estimated_cost.clone(),
        required_privilege: step.required_privilege.clone(),
    }
}

fn data_quality_action(
    index: usize,
    next_step_id: &str,
    data_quality: &DataQuality,
) -> NextInvestigationAction {
    let refs = Vec::new();
    let reason = if data_quality.missing.is_empty() {
        "Review data_quality before widening observation.".to_string()
    } else {
        format!(
            "Resolve {} data_quality gap(s) before treating missing evidence as absence.",
            data_quality.missing.len()
        )
    };
    NextInvestigationAction {
        action_id: format!("IA{index:03}"),
        next_step_id: next_step_id.to_string(),
        title: "Review investigation data_quality gaps".to_string(),
        reason,
        expected_answer:
            "A compact list of missing, unavailable, truncated, or permission-limited evidence."
                .to_string(),
        refs,
        target_ids: Vec::new(),
        cost: "low".to_string(),
        required_privilege: "none".to_string(),
    }
}

fn evidence_text(
    opened_refs: &[OpenedRefSummary],
    new_facts: &[InvestigationContinuationFact],
) -> String {
    let mut text = String::new();
    for opened in opened_refs {
        text.push_str(&opened.label.to_ascii_lowercase());
        text.push(' ');
        text.push_str(&opened.ref_kind.to_ascii_lowercase());
        text.push(' ');
        text.push_str(&opened.summary.to_ascii_lowercase());
        text.push(' ');
    }
    for fact in new_facts {
        text.push_str(&fact.kind.to_ascii_lowercase());
        text.push(' ');
        text.push_str(&fact.statement.to_ascii_lowercase());
        text.push(' ');
    }
    text
}

fn missing_evidence(opened_refs: &[OpenedRefSummary], data_quality: &DataQuality) -> Vec<String> {
    let mut missing = data_quality.missing.clone();
    for opened in opened_refs {
        if opened.ref_kind == "unavailable" {
            missing.push(format!("{} unavailable: {}", opened.label, opened.raw_ref));
        }
        missing.extend(opened.data_quality.missing.clone());
    }
    missing.sort();
    missing.dedup();
    missing
}

fn inferred_missing(opened_refs: &[OpenedRefSummary], mut missing: Vec<String>) -> Vec<String> {
    if missing.is_empty() && opened_refs.is_empty() {
        missing.push("no bounded refs were opened for this branch condition".to_string());
    }
    missing
}

fn matching_summaries(opened_refs: &[OpenedRefSummary], needles: &[&str]) -> Vec<String> {
    opened_refs
        .iter()
        .filter_map(|opened| {
            let summary = opened.summary.to_ascii_lowercase();
            if needles.is_empty()
                || needles
                    .iter()
                    .any(|needle| summary_matches(&summary, needle))
            {
                Some(opened.summary.clone())
            } else {
                None
            }
        })
        .take(5)
        .collect()
}

fn summary_matches(summary: &str, needle: &str) -> bool {
    match needle {
        "" => true,
        "available" => {
            summary.contains("availability=available") || summary.contains("is available")
        }
        "unavailable" => {
            summary.contains("availability=unavailable") || summary.contains("unavailable")
        }
        _ => summary.contains(needle),
    }
}

fn session_budget(state: &InvestigationSessionState) -> InvestigationSessionBudget {
    let mut budget = InvestigationSessionBudget {
        returned_bytes: 0,
        completed_ref_count: state.completed_refs.len(),
        fact_count: state.facts.len(),
        branch_evaluation_count: state.branch_evaluations.len(),
        next_action_count: state.next_actions.len(),
    };
    budget.returned_bytes = serde_json::to_vec(state)
        .map(|bytes| bytes.len())
        .unwrap_or(0);
    budget
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn push_unique_ref(values: &mut Vec<CompletedInvestigationRef>, value: CompletedInvestigationRef) {
    if !values
        .iter()
        .any(|existing| existing.label == value.label && existing.raw_ref == value.raw_ref)
    {
        values.push(value);
    }
}

fn push_unique_fact(
    values: &mut Vec<InvestigationContinuationFact>,
    value: InvestigationContinuationFact,
) {
    if !values.iter().any(|existing| {
        existing.kind == value.kind
            && existing.raw_ref == value.raw_ref
            && existing.statement == value.statement
    }) {
        values.push(value);
    }
}

fn push_unique_branch(values: &mut Vec<BranchEvaluation>, value: BranchEvaluation) {
    if !values.iter().any(|existing| {
        existing.step_id == value.step_id
            && existing.next_step_id == value.next_step_id
            && existing.condition.if_observed == value.condition.if_observed
    }) {
        values.push(value);
    }
}

fn push_unique_action(values: &mut Vec<NextInvestigationAction>, value: NextInvestigationAction) {
    if !values
        .iter()
        .any(|existing| existing.next_step_id == value.next_step_id)
    {
        values.push(value);
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
