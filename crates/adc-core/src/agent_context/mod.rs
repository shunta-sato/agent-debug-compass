use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    extract_evidence_facts_from_ref,
    fleet_semantics::{build_fleet_semantic_diff, FleetSemanticDiff},
    investigation_state::{
        build_investigation_session_state, BranchEvaluation, InvestigationSessionState,
        NextInvestigationAction, SessionStateInput,
    },
    read_evidence_index, search_events, AdcError, AdcResult, ArtifactManifest, DataQuality,
    EventEnvelope, EvidenceFact, EvidenceIndex, EvidenceWindowRef, FleetEvidence,
    FleetServiceInvestigationResult, FleetTargetEvidence, NextProbeOption, OverheadReport,
    RouteConditionExpr, SearchEventsQuery, ServiceInvestigationPack,
};

const AGENT_CONTEXT_SCHEMA_VERSION: &str = "obs.agent_context.v1";
const MAX_TIMELINE_EVENTS_FOR_CONTEXT: usize = 100;

mod ids;
mod refs;
mod render;
mod staging;

pub use ids::{latest_fleet_run_id, latest_run_id};
use refs::validate_relative_artifact_path;
pub use refs::{resolve_agent_ref, resolve_global_agent_ref};
pub use render::{
    render_agent_context_journald_jsonl, render_agent_context_markdown,
    render_agent_context_openmetrics, render_agent_context_otlp_json,
    render_agent_context_perfetto_json, render_investigation_route_markdown,
};
pub use staging::stage_agent_context_inputs;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContextRequest {
    pub run_id: String,
    pub service_name: Option<String>,
    pub max_markdown_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetAgentContextRequest {
    pub fleet_run_id: String,
    pub max_markdown_bytes: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentContextInputPaths {
    pub log_file: Option<PathBuf>,
    pub domain_events_file: Option<PathBuf>,
    pub config_file: Option<PathBuf>,
    pub service_name: Option<String>,
    pub otlp_file: Option<PathBuf>,
    pub journald_jsonl_file: Option<PathBuf>,
    pub perfetto_file: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentContext {
    pub schema_version: String,
    pub context_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fleet_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    pub target_dossier: AgentTargetDossier,
    pub primary_window: EvidenceWindowRef,
    pub evidence_index: EvidenceIndex,
    pub derived_facts: Vec<AgentContextFact>,
    pub recommended_refs: Vec<AgentContextRef>,
    pub playbook: AgentPlaybook,
    pub investigation_route: InvestigationRoute,
    pub raw_refs: BTreeMap<String, String>,
    pub next_probe_options: Vec<NextProbeOption>,
    pub information_debt: Vec<AgentContextDebt>,
    pub data_quality: DataQuality,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overhead: Option<AgentContextOverhead>,
    pub context_budget: AgentContextBudget,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvestigationStartRequest {
    pub run_id: Option<String>,
    pub fleet_run_id: Option<String>,
    pub service_name: Option<String>,
    pub inventory_path: Option<PathBuf>,
    pub max_journal_lines: Option<usize>,
    pub max_markdown_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvestigationStartPack {
    pub schema_version: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fleet_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_name: Option<String>,
    pub agent_context: Value,
    pub investigation_route: InvestigationRoute,
    pub raw_refs: BTreeMap<String, String>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvestigationContinuationRequest {
    pub run_id: Option<String>,
    pub fleet_run_id: Option<String>,
    pub service_name: Option<String>,
    pub route_id: Option<String>,
    pub session_id: Option<String>,
    pub current_step_id: String,
    pub open_ref_labels: Vec<String>,
    pub open_raw_refs: Vec<String>,
    pub max_context_bytes: usize,
    pub max_ref_lines: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationSessionRequest {
    pub run_id: Option<String>,
    pub fleet_run_id: Option<String>,
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationSessionCleanupRequest {
    pub run_id: Option<String>,
    pub fleet_run_id: Option<String>,
    pub max_sessions: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age_days: Option<u64>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationSessionCleanupReport {
    pub schema_version: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fleet_run_id: Option<String>,
    pub dry_run: bool,
    pub candidate_count: usize,
    pub deleted_count: usize,
    pub candidates: Vec<InvestigationSessionCleanupCandidate>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationSessionCleanupCandidate {
    pub path: String,
    pub reason: String,
    pub age_seconds: u64,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvestigationContinuationPack {
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
    pub current_step_id: String,
    pub opened_refs: Vec<OpenedRefSummary>,
    pub new_facts: Vec<InvestigationContinuationFact>,
    pub branch_evaluations: Vec<BranchEvaluation>,
    pub next_actions: Vec<NextInvestigationAction>,
    pub investigation_route: InvestigationRoute,
    pub raw_refs: BTreeMap<String, String>,
    pub data_quality: DataQuality,
    pub budget: InvestigationContinuationBudget,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenedRefSummary {
    pub label: String,
    pub raw_ref: String,
    pub ref_kind: String,
    pub content_type: String,
    pub summary: String,
    pub item_count: usize,
    pub truncated: bool,
    pub data_quality: DataQuality,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<EvidenceFact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationContinuationFact {
    pub kind: String,
    pub statement: String,
    pub raw_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationContinuationBudget {
    pub max_context_bytes: usize,
    pub returned_bytes: usize,
    pub opened_ref_count: usize,
    pub omitted_ref_count: usize,
    pub truncated_ref_count: usize,
}

struct BuiltContinuation {
    pack: InvestigationContinuationPack,
    session_state: InvestigationSessionState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentTargetDossier {
    pub target_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fleet_run_id: Option<String>,
    pub primary_window_id: String,
    pub primary_window_event_count: usize,
    pub artifact_refs: BTreeMap<String, String>,
    pub raw_ref_count: usize,
    pub capability_summary: BTreeMap<String, Value>,
    pub root_required: bool,
    pub raw_artifacts_are_ref_only: bool,
    pub redacted_artifacts: Vec<String>,
    pub data_quality: DataQuality,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overhead: Option<AgentContextOverhead>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentContextFact {
    pub fact_id: String,
    pub source: String,
    pub kind: String,
    pub window_id: String,
    pub statement: String,
    pub raw_ref: String,
    #[serde(default)]
    pub attributes: BTreeMap<String, Value>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentContextRef {
    pub label: String,
    pub raw_ref: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPlaybook {
    pub schema_version: String,
    pub playbook_id: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fleet_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_name: Option<String>,
    pub steps: Vec<AgentPlaybookStep>,
    pub data_quality: DataQuality,
    pub raw_refs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPlaybookStep {
    pub step_id: String,
    pub title: String,
    pub reason: String,
    pub expected_evidence: Vec<String>,
    pub required_privilege: String,
    pub estimated_cost: String,
    pub refs: Vec<AgentContextRef>,
    pub stop_condition: String,
    pub cause_neutral: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvestigationRoute {
    pub schema_version: String,
    pub route_id: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fleet_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_name: Option<String>,
    pub route_summary: Vec<String>,
    pub steps: Vec<InvestigationStep>,
    pub data_quality: DataQuality,
    pub raw_refs: BTreeMap<String, String>,
    pub budget: RouteBudget,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvestigationStep {
    pub step_id: String,
    pub title: String,
    pub purpose: String,
    pub expected_answer: String,
    pub refs: Vec<AgentContextRef>,
    pub branch_conditions: Vec<RouteBranchCondition>,
    pub stop_conditions: Vec<String>,
    pub required_privilege: String,
    pub estimated_cost: String,
    pub target_ids: Vec<String>,
    pub cause_neutral: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteBranchCondition {
    pub if_observed: String,
    pub next_step_id: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predicate: Option<RouteConditionExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteBudget {
    pub max_context_bytes: usize,
    pub returned_step_count: usize,
    pub omitted_step_count: usize,
    pub raw_ref_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentContextDebt {
    pub kind: String,
    pub description: String,
    pub impact: String,
    pub remediation_hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentContextOverhead {
    pub artifact_bytes: u64,
    pub event_count: u64,
    pub duration_ms: u64,
    pub throttled: bool,
    pub dropped: bool,
    pub drop_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentContextBudget {
    pub max_markdown_bytes: usize,
    pub rendered_bytes: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRefResolution {
    pub run_id: String,
    pub ref_uri: String,
    pub ref_kind: String,
    pub content_type: String,
    pub returned_lines: usize,
    pub total_lines: usize,
    pub truncated: bool,
    pub text: String,
    pub artifact_trust: crate::ArtifactTrust,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionEvidenceIndex {
    pub schema_version: String,
    pub run_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_run_id: Option<String>,
    pub total_artifact_bytes: u64,
    pub total_event_count: u64,
    pub runs: Vec<SessionRunEvidence>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionRunEvidence {
    pub run_id: String,
    pub target_id: String,
    pub capture_mode: String,
    pub window_id: String,
    pub event_count: u64,
    pub artifact_bytes: u64,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetAgentContext {
    pub schema_version: String,
    pub context_id: String,
    pub fleet_run_id: String,
    pub target_count: usize,
    pub captured_count: usize,
    pub failed_count: usize,
    pub target_matrix: Vec<FleetTargetEvidence>,
    pub target_summaries: Vec<FleetTargetContextSummary>,
    pub cross_target_summary: FleetCrossTargetSummary,
    pub failure_groups: Vec<FleetFailureGroup>,
    pub remediation_hints: Vec<String>,
    pub recommended_refs: Vec<AgentContextRef>,
    pub investigation_route: InvestigationRoute,
    pub raw_refs: BTreeMap<String, String>,
    pub information_debt: Vec<AgentContextDebt>,
    pub data_quality: DataQuality,
    pub context_budget: AgentContextBudget,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetCrossTargetSummary {
    pub captured_count: usize,
    pub failed_count: usize,
    pub total_event_count: u64,
    pub source_totals: Vec<FleetTargetSourceSummary>,
    pub targets_with_missing_data_quality: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetTargetContextSummary {
    pub target_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_window_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_count: Option<u64>,
    pub sources: Vec<FleetTargetSourceSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_dossier: Option<AgentTargetDossier>,
    pub top_leads: Vec<AgentContextRef>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetTargetSourceSummary {
    pub source: String,
    pub event_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetFailureGroup {
    pub failure_class: String,
    pub targets: Vec<String>,
    pub sample: String,
    pub next_action: String,
}

pub fn build_run_agent_context(
    artifact_root: impl AsRef<Path>,
    request: AgentContextRequest,
) -> AdcResult<AgentContext> {
    validate_segment(&request.run_id, "run_id")?;
    let artifact_root = artifact_root.as_ref();
    let evidence = read_evidence_index(artifact_root, &request.run_id)?;
    let events = read_context_events(artifact_root, &request.run_id)?;
    let manifest = read_manifest_optional(artifact_root, &request.run_id)?;
    let overhead = read_overhead_optional(artifact_root, &request.run_id)?;
    let mut derived_facts = derive_run_facts(artifact_root, &request.run_id, &evidence, &events)?;
    let mut raw_refs = evidence.raw_refs.clone();
    let artifact_facts = derive_optional_artifact_facts(
        artifact_root,
        &request.run_id,
        &evidence,
        derived_facts.len() + 1,
        &mut raw_refs,
    )?;
    derived_facts.extend(artifact_facts);
    sort_facts_by_salience(&mut derived_facts);
    let recommended_refs = recommend_refs(&evidence, &derived_facts);
    let route_service_name = request
        .service_name
        .as_deref()
        .or_else(|| service_name_from_facts(&derived_facts));
    let service_pack = route_service_name.and_then(|service_name| {
        read_service_investigation_pack_optional(artifact_root, service_name)
    });
    let playbook = build_agent_playbook(
        "run",
        Some(&evidence.run_id),
        evidence.fleet_run_id.as_deref(),
        &derived_facts,
        &recommended_refs,
        &evidence.data_quality,
    );
    let investigation_route = build_run_investigation_route(RunRouteInput {
        run_id: &evidence.run_id,
        fleet_run_id: evidence.fleet_run_id.as_deref(),
        target_id: Some(&evidence.target_id),
        route_service_name,
        derived_facts: &derived_facts,
        recommended_refs: &recommended_refs,
        service_pack: service_pack.as_ref(),
        data_quality: &evidence.data_quality,
        max_context_bytes: request.max_markdown_bytes,
    });
    let information_debt = evidence
        .information_debt
        .iter()
        .map(|debt| AgentContextDebt {
            kind: debt.kind.clone(),
            description: debt.description.clone(),
            impact: debt.impact.clone(),
            remediation_hint: remediation_hint_for(&debt.description),
        })
        .collect();
    let overhead_summary = overhead.map(|report| AgentContextOverhead {
        artifact_bytes: report.sample.artifact_bytes,
        event_count: report.sample.event_count,
        duration_ms: report.sample.duration_ms,
        throttled: report.decision.throttled,
        dropped: report.decision.dropped,
        drop_count: report.decision.drop_count,
    });
    let profile_id = manifest
        .as_ref()
        .map(|manifest| manifest.profile_id.clone())
        .or_else(|| events.first().map(|event| event.profile_id.clone()));
    let target_dossier = build_target_dossier(
        &evidence,
        profile_id.clone(),
        &raw_refs,
        &derived_facts,
        overhead_summary.clone(),
    );

    let mut context = AgentContext {
        schema_version: AGENT_CONTEXT_SCHEMA_VERSION.to_string(),
        context_id: format!("ctx-{}", request.run_id),
        run_id: Some(evidence.run_id.clone()),
        fleet_run_id: evidence.fleet_run_id.clone(),
        target_id: Some(evidence.target_id.clone()),
        profile_id,
        target_dossier,
        primary_window: evidence.primary_window.clone(),
        raw_refs,
        next_probe_options: evidence.next_probe_options.clone(),
        data_quality: evidence.data_quality.clone(),
        evidence_index: evidence,
        derived_facts,
        recommended_refs,
        playbook,
        investigation_route,
        information_debt,
        overhead: overhead_summary,
        context_budget: AgentContextBudget {
            max_markdown_bytes: request.max_markdown_bytes,
            rendered_bytes: 0,
            truncated: false,
        },
    };
    enforce_context_budget(&mut context)?;
    Ok(context)
}

pub fn build_session_evidence_index(
    artifact_root: impl AsRef<Path>,
) -> AdcResult<SessionEvidenceIndex> {
    let artifact_root = artifact_root.as_ref();
    let runs_root = artifact_root.join("runs");
    if !runs_root.exists() {
        return Ok(SessionEvidenceIndex {
            schema_version: "obs.session_evidence.v1".to_string(),
            run_count: 0,
            latest_run_id: None,
            total_artifact_bytes: 0,
            total_event_count: 0,
            runs: Vec::new(),
            data_quality: DataQuality {
                clock_confidence: "medium".to_string(),
                ..Default::default()
            },
        });
    }

    let mut runs = Vec::new();
    let mut data_quality = DataQuality {
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };
    for entry in fs::read_dir(&runs_root).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read runs directory {}: {err}",
            runs_root.display()
        ))
    })? {
        let entry = entry.map_err(|err| {
            AdcError::Artifact(format!("failed to read run directory entry: {err}"))
        })?;
        let Some(run_id) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if !entry.path().join("evidence_index.yaml").is_file() {
            continue;
        }
        match read_evidence_index(artifact_root, &run_id) {
            Ok(evidence) => {
                let overhead = read_overhead_optional(artifact_root, &run_id)?;
                let artifact_bytes = overhead
                    .as_ref()
                    .map(|report| report.sample.artifact_bytes)
                    .unwrap_or(0);
                runs.push(SessionRunEvidence {
                    run_id: run_id.clone(),
                    target_id: evidence.target_id,
                    capture_mode: evidence.capture_mode,
                    window_id: evidence.primary_window.window_id,
                    event_count: evidence.primary_window.event_count as u64,
                    artifact_bytes,
                    data_quality: evidence.data_quality,
                });
            }
            Err(err) => data_quality
                .missing
                .push(format!("run {run_id} evidence index unavailable: {err}")),
        }
    }
    runs.sort_by(|left, right| left.run_id.cmp(&right.run_id));
    let total_artifact_bytes = runs.iter().map(|run| run.artifact_bytes).sum();
    let total_event_count = runs.iter().map(|run| run.event_count).sum();
    Ok(SessionEvidenceIndex {
        schema_version: "obs.session_evidence.v1".to_string(),
        run_count: runs.len(),
        latest_run_id: latest_run_id(artifact_root)?,
        total_artifact_bytes,
        total_event_count,
        runs,
        data_quality,
    })
}

pub fn build_fleet_agent_context(
    artifact_root: impl AsRef<Path>,
    request: FleetAgentContextRequest,
) -> AdcResult<FleetAgentContext> {
    validate_segment(&request.fleet_run_id, "fleet_run_id")?;
    let artifact_root = artifact_root.as_ref();
    let evidence = read_fleet_evidence(artifact_root, &request.fleet_run_id)?;
    let information_debt = evidence
        .information_debt
        .iter()
        .map(|debt| AgentContextDebt {
            kind: debt.kind.clone(),
            description: debt.description.clone(),
            impact: debt.impact.clone(),
            remediation_hint: remediation_hint_for(&debt.description),
        })
        .collect::<Vec<_>>();
    let remediation_hints = evidence
        .information_debt
        .iter()
        .map(|debt| remediation_hint_for(&debt.description))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let recommended_refs = evidence
        .raw_refs
        .iter()
        .take(3)
        .map(|(label, raw_ref)| AgentContextRef {
            label: label.clone(),
            raw_ref: raw_ref.clone(),
            reason: "fleet target evidence ref".to_string(),
        })
        .collect::<Vec<_>>();
    let target_summaries = evidence
        .target_matrix
        .iter()
        .map(|target| build_fleet_target_context_summary(artifact_root, target))
        .collect::<Vec<_>>();
    let cross_target_summary = build_fleet_cross_target_summary(
        evidence.captured_count,
        evidence.failed_count,
        &target_summaries,
    );
    let failure_groups = build_fleet_failure_groups(&target_summaries);
    let fleet_service =
        read_fleet_service_investigation_optional(artifact_root, &request.fleet_run_id);
    let investigation_route = build_fleet_investigation_route(
        &request.fleet_run_id,
        &target_summaries,
        &recommended_refs,
        fleet_service.as_ref(),
        &evidence.data_quality,
        request.max_markdown_bytes,
    );

    Ok(FleetAgentContext {
        schema_version: "obs.agent_context.fleet.v1".to_string(),
        context_id: format!("ctx-fleet-{}", request.fleet_run_id),
        fleet_run_id: evidence.fleet_run_id,
        target_count: evidence.target_count,
        captured_count: evidence.captured_count,
        failed_count: evidence.failed_count,
        target_matrix: evidence.target_matrix,
        target_summaries,
        cross_target_summary,
        failure_groups,
        remediation_hints,
        recommended_refs,
        investigation_route,
        raw_refs: evidence.raw_refs,
        information_debt,
        data_quality: evidence.data_quality,
        context_budget: AgentContextBudget {
            max_markdown_bytes: request.max_markdown_bytes,
            rendered_bytes: 0,
            truncated: false,
        },
    })
}

pub fn start_investigation(
    artifact_root: impl AsRef<Path>,
    request: InvestigationStartRequest,
) -> AdcResult<InvestigationStartPack> {
    let artifact_root = artifact_root.as_ref();
    match (request.run_id, request.fleet_run_id) {
        (Some(run_id), None) => {
            let run_id = resolve_run_id_alias(artifact_root, run_id)?;
            if let Some(service_name) = &request.service_name {
                crate::investigate_service(
                    artifact_root,
                    crate::ServiceInvestigationRequest {
                        service_name: service_name.clone(),
                        max_journal_lines: request.max_journal_lines.unwrap_or(80),
                    },
                )?;
            }
            let context = build_run_agent_context(
                artifact_root,
                AgentContextRequest {
                    run_id: run_id.clone(),
                    service_name: request.service_name.clone(),
                    max_markdown_bytes: request.max_markdown_bytes,
                },
            )?;
            let route = context.investigation_route.clone();
            persist_run_investigation_route(artifact_root, &run_id, &route)?;
            Ok(InvestigationStartPack {
                schema_version: "obs.investigation_start.v1".to_string(),
                scope: "run".to_string(),
                run_id: Some(run_id),
                fleet_run_id: None,
                service_name: request.service_name,
                agent_context: compact_run_start_context(&context),
                investigation_route: route.clone(),
                raw_refs: route.raw_refs,
                data_quality: route.data_quality,
            })
        }
        (None, Some(fleet_run_id)) => {
            let fleet_run_id = resolve_fleet_run_id_alias(artifact_root, fleet_run_id)?;
            if let (Some(service_name), Some(inventory_path)) =
                (&request.service_name, request.inventory_path.as_ref())
            {
                crate::investigate_fleet_service(
                    artifact_root,
                    inventory_path,
                    crate::FleetServiceInvestigationOptions {
                        fleet_run_id: fleet_run_id.clone(),
                        service_name: service_name.clone(),
                        max_journal_lines: request.max_journal_lines.unwrap_or(80),
                    },
                )?;
            }
            let context = build_fleet_agent_context(
                artifact_root,
                FleetAgentContextRequest {
                    fleet_run_id: fleet_run_id.clone(),
                    max_markdown_bytes: request.max_markdown_bytes,
                },
            )?;
            let route = context.investigation_route.clone();
            persist_fleet_investigation_route(artifact_root, &fleet_run_id, &route)?;
            if let Some(service_result) =
                read_fleet_service_investigation_optional(artifact_root, &fleet_run_id)
            {
                let semantic_diff = build_fleet_semantic_diff(&service_result);
                persist_fleet_semantic_diff(artifact_root, &fleet_run_id, &semantic_diff)?;
            }
            Ok(InvestigationStartPack {
                schema_version: "obs.investigation_start.v1".to_string(),
                scope: "fleet".to_string(),
                run_id: None,
                fleet_run_id: Some(fleet_run_id),
                service_name: route.service_name.clone().or(request.service_name),
                agent_context: compact_fleet_start_context(&context),
                investigation_route: route.clone(),
                raw_refs: route.raw_refs,
                data_quality: route.data_quality,
            })
        }
        (None, None) => Err(AdcError::Artifact(
            "start investigation requires run_id or fleet_run_id".to_string(),
        )),
        (Some(_), Some(_)) => Err(AdcError::Artifact(
            "start investigation accepts only one of run_id or fleet_run_id".to_string(),
        )),
    }
}

pub fn continue_investigation(
    artifact_root: impl AsRef<Path>,
    request: InvestigationContinuationRequest,
) -> AdcResult<InvestigationContinuationPack> {
    let artifact_root = artifact_root.as_ref();
    match (request.run_id.clone(), request.fleet_run_id.clone()) {
        (Some(run_id), None) => {
            let run_id = resolve_run_id_alias(artifact_root, run_id)?;
            let context = build_run_agent_context(
                artifact_root,
                AgentContextRequest {
                    run_id: run_id.clone(),
                    service_name: request.service_name.clone(),
                    max_markdown_bytes: request.max_context_bytes,
                },
            )?;
            let route = read_run_route_optional(artifact_root, &run_id)?
                .unwrap_or_else(|| context.investigation_route.clone());
            let built = build_continuation_pack(
                artifact_root,
                "run",
                Some(run_id.clone()),
                None,
                request.service_name.clone(),
                route,
                &request,
            )?;
            persist_run_investigation_session(
                artifact_root,
                &run_id,
                &built.pack.session_id,
                &built.pack,
            )?;
            persist_run_investigation_session_state(
                artifact_root,
                &run_id,
                &built.pack.session_id,
                &built.session_state,
            )?;
            Ok(built.pack)
        }
        (None, Some(fleet_run_id)) => {
            let fleet_run_id = resolve_fleet_run_id_alias(artifact_root, fleet_run_id)?;
            let context = build_fleet_agent_context(
                artifact_root,
                FleetAgentContextRequest {
                    fleet_run_id: fleet_run_id.clone(),
                    max_markdown_bytes: request.max_context_bytes,
                },
            )?;
            let route = read_fleet_route_optional(artifact_root, &fleet_run_id)?
                .unwrap_or_else(|| context.investigation_route.clone());
            let service_name = request
                .service_name
                .clone()
                .or_else(|| route.service_name.clone());
            if let Some(service_result) =
                read_fleet_service_investigation_optional(artifact_root, &fleet_run_id)
            {
                let semantic_diff = build_fleet_semantic_diff(&service_result);
                persist_fleet_semantic_diff(artifact_root, &fleet_run_id, &semantic_diff)?;
            }
            let built = build_continuation_pack(
                artifact_root,
                "fleet",
                None,
                Some(fleet_run_id.clone()),
                service_name,
                route,
                &request,
            )?;
            persist_fleet_investigation_session(
                artifact_root,
                &fleet_run_id,
                &built.pack.session_id,
                &built.pack,
            )?;
            persist_fleet_investigation_session_state(
                artifact_root,
                &fleet_run_id,
                &built.pack.session_id,
                &built.session_state,
            )?;
            Ok(built.pack)
        }
        (None, None) => Err(AdcError::Artifact(
            "continue investigation requires run_id or fleet_run_id".to_string(),
        )),
        (Some(_), Some(_)) => Err(AdcError::Artifact(
            "continue investigation accepts only one of run_id or fleet_run_id".to_string(),
        )),
    }
}

pub fn get_investigation_session_state(
    artifact_root: impl AsRef<Path>,
    request: InvestigationSessionRequest,
) -> AdcResult<InvestigationSessionState> {
    let artifact_root = artifact_root.as_ref();
    validate_segment(&request.session_id, "session_id")?;
    match (request.run_id, request.fleet_run_id) {
        (Some(run_id), None) => {
            let run_id = resolve_run_id_alias(artifact_root, run_id)?;
            read_session_state_optional(
                artifact_root,
                "run",
                Some(&run_id),
                None,
                &request.session_id,
            )?
            .ok_or_else(|| {
                AdcError::Artifact(format!(
                    "investigation session state {} was not found for run {}",
                    request.session_id, run_id
                ))
            })
        }
        (None, Some(fleet_run_id)) => {
            let fleet_run_id = resolve_fleet_run_id_alias(artifact_root, fleet_run_id)?;
            read_session_state_optional(
                artifact_root,
                "fleet",
                None,
                Some(&fleet_run_id),
                &request.session_id,
            )?
            .ok_or_else(|| {
                AdcError::Artifact(format!(
                    "investigation session state {} was not found for fleet run {}",
                    request.session_id, fleet_run_id
                ))
            })
        }
        (None, None) => Err(AdcError::Artifact(
            "session lookup requires run_id or fleet_run_id".to_string(),
        )),
        (Some(_), Some(_)) => Err(AdcError::Artifact(
            "session lookup accepts only one of run_id or fleet_run_id".to_string(),
        )),
    }
}

pub fn cleanup_investigation_sessions(
    artifact_root: impl AsRef<Path>,
    request: InvestigationSessionCleanupRequest,
) -> AdcResult<InvestigationSessionCleanupReport> {
    let artifact_root = artifact_root.as_ref();
    match (request.run_id, request.fleet_run_id) {
        (Some(run_id), None) => {
            let run_id = resolve_run_id_alias(artifact_root, run_id)?;
            cleanup_session_dir(
                run_dir(artifact_root, &run_id).join("investigation_sessions"),
                "run",
                Some(run_id),
                None,
                request.max_sessions,
                request.max_age_days,
                request.dry_run,
            )
        }
        (None, Some(fleet_run_id)) => {
            let fleet_run_id = resolve_fleet_run_id_alias(artifact_root, fleet_run_id)?;
            cleanup_session_dir(
                artifact_root
                    .join("fleet_runs")
                    .join(&fleet_run_id)
                    .join("investigation_sessions"),
                "fleet",
                None,
                Some(fleet_run_id),
                request.max_sessions,
                request.max_age_days,
                request.dry_run,
            )
        }
        (None, None) => Err(AdcError::Artifact(
            "session cleanup requires run_id or fleet_run_id".to_string(),
        )),
        (Some(_), Some(_)) => Err(AdcError::Artifact(
            "session cleanup accepts only one of run_id or fleet_run_id".to_string(),
        )),
    }
}

fn compact_run_start_context(context: &AgentContext) -> Value {
    json!({
        "schema_version": context.schema_version,
        "context_id": context.context_id,
        "run_id": context.run_id,
        "fleet_run_id": context.fleet_run_id,
        "target_id": context.target_id,
        "profile_id": context.profile_id,
        "primary_window": {
            "window_id": context.primary_window.window_id,
            "event_count": context.primary_window.event_count,
        },
        "target_dossier": {
            "target_id": context.target_dossier.target_id,
            "profile_id": context.target_dossier.profile_id,
            "raw_ref_count": context.target_dossier.raw_ref_count,
            "root_required": context.target_dossier.root_required,
            "raw_artifacts_are_ref_only": context.target_dossier.raw_artifacts_are_ref_only,
            "data_quality": context.target_dossier.data_quality,
        },
        "derived_facts": context.derived_facts.iter().take(8).map(|fact| {
            json!({
                "kind": fact.kind,
                "statement": fact.statement,
                "raw_ref": fact.raw_ref,
            })
        }).collect::<Vec<_>>(),
        "recommended_refs": context.recommended_refs,
        "data_quality": context.data_quality,
        "context_budget": context.context_budget,
        "full_context_ref": context.raw_refs.get("agent_context_json").cloned(),
    })
}

fn compact_fleet_start_context(context: &FleetAgentContext) -> Value {
    json!({
        "schema_version": context.schema_version,
        "context_id": context.context_id,
        "fleet_run_id": context.fleet_run_id,
        "target_count": context.target_count,
        "captured_count": context.captured_count,
        "failed_count": context.failed_count,
        "target_matrix": context.target_matrix,
        "target_summaries": context.target_summaries.iter().map(|target| {
            json!({
                "target_id": target.target_id,
                "status": target.status,
                "run_id": target.run_id,
                "event_count": target.event_count,
                "evidence_ref": target.evidence_ref,
                "data_quality": target.data_quality,
            })
        }).collect::<Vec<_>>(),
        "failure_groups": context.failure_groups,
        "recommended_refs": context.recommended_refs,
        "data_quality": context.data_quality,
        "context_budget": context.context_budget,
    })
}

fn build_continuation_pack(
    artifact_root: &Path,
    scope: &str,
    run_id: Option<String>,
    fleet_run_id: Option<String>,
    service_name: Option<String>,
    route: InvestigationRoute,
    request: &InvestigationContinuationRequest,
) -> AdcResult<BuiltContinuation> {
    validate_segment(&request.current_step_id, "current_step_id")?;
    if let Some(route_id) = &request.route_id {
        validate_segment(route_id, "route_id")?;
        if route_id != &route.route_id {
            return Err(AdcError::Artifact(format!(
                "route_id {route_id} does not match persisted route {}",
                route.route_id
            )));
        }
    }
    let current_step = route
        .steps
        .iter()
        .find(|step| step.step_id == request.current_step_id)
        .ok_or_else(|| {
            AdcError::Artifact(format!(
                "route step {} was not found in {}",
                request.current_step_id, route.route_id
            ))
        })?;
    let session_id = request.session_id.clone().unwrap_or_else(|| {
        format!("S-{}-{}", route.route_id, request.current_step_id).replace(':', "-")
    });
    validate_segment(&session_id, "session_id")?;

    let requested_refs = continuation_refs(current_step, request);
    let mut opened_refs = Vec::new();
    let mut data_quality = DataQuality {
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };
    for reference in &requested_refs {
        match open_continuation_ref(
            artifact_root,
            run_id.as_deref(),
            fleet_run_id.as_deref(),
            reference,
            request.max_ref_lines,
        ) {
            Ok(summary) => {
                merge_data_quality(&mut data_quality, &summary.data_quality);
                opened_refs.push(summary);
            }
            Err(err) => {
                data_quality.missing.push(format!(
                    "{}: failed to open {}: {err}",
                    reference.label, reference.raw_ref
                ));
                opened_refs.push(OpenedRefSummary {
                    label: reference.label.clone(),
                    raw_ref: reference.raw_ref.clone(),
                    ref_kind: "unavailable".to_string(),
                    content_type: "text/plain".to_string(),
                    summary: "selected ref was unavailable; see data_quality".to_string(),
                    item_count: 0,
                    truncated: false,
                    data_quality: DataQuality {
                        clock_confidence: "medium".to_string(),
                        missing: vec![format!("selected ref unavailable: {}", reference.raw_ref)],
                        ..Default::default()
                    },
                    facts: Vec::new(),
                    text: None,
                });
            }
        }
    }

    let new_facts = opened_refs
        .iter()
        .map(|summary| InvestigationContinuationFact {
            kind: continuation_fact_kind(&summary.label, &summary.ref_kind),
            statement: summary.summary.clone(),
            raw_ref: summary.raw_ref.clone(),
        })
        .collect::<Vec<_>>();
    let mut next_route = route_after_step(&route, &request.current_step_id, &data_quality);
    next_route.route_summary.push(format!(
        "Continuation opened {} bounded ref(s) from {}.",
        opened_refs.len(),
        request.current_step_id
    ));
    let mut raw_refs = next_route.raw_refs.clone();
    for summary in &opened_refs {
        raw_refs.insert(summary.label.clone(), summary.raw_ref.clone());
    }
    match scope {
        "run" => {
            raw_refs.insert(
                "investigation_route".to_string(),
                "artifact://investigation_route.json".to_string(),
            );
            raw_refs.insert(
                "investigation_session".to_string(),
                format!("artifact://investigation_sessions/{session_id}.json"),
            );
            raw_refs.insert(
                "investigation_session_state".to_string(),
                format!("artifact://investigation_sessions/{session_id}.state.json"),
            );
        }
        "fleet" => {
            if let Some(fleet_run_id) = &fleet_run_id {
                raw_refs.insert(
                    "investigation_route".to_string(),
                    format!("artifact://fleet_runs/{fleet_run_id}/investigation_route.json"),
                );
                raw_refs.insert(
                    "investigation_session".to_string(),
                    format!(
                        "artifact://fleet_runs/{fleet_run_id}/investigation_sessions/{session_id}.json"
                    ),
                );
                raw_refs.insert(
                    "investigation_session_state".to_string(),
                    format!(
                        "artifact://fleet_runs/{fleet_run_id}/investigation_sessions/{session_id}.state.json"
                    ),
                );
                raw_refs.insert(
                    "fleet_semantic_diff".to_string(),
                    format!("artifact://fleet_runs/{fleet_run_id}/fleet_semantic_diff.json"),
                );
            }
        }
        _ => {}
    }
    next_route.raw_refs = raw_refs.clone();
    next_route.budget.raw_ref_count = next_route.raw_refs.len();
    next_route.budget.returned_step_count = next_route.steps.len();
    merge_data_quality(&mut next_route.data_quality, &data_quality);

    let truncated_ref_count = opened_refs
        .iter()
        .filter(|summary| summary.truncated)
        .count();
    let previous_state = read_session_state_optional(
        artifact_root,
        scope,
        run_id.as_deref(),
        fleet_run_id.as_deref(),
        &session_id,
    )?;
    let session_state = build_investigation_session_state(SessionStateInput {
        scope,
        run_id: run_id.as_deref(),
        fleet_run_id: fleet_run_id.as_deref(),
        service_name: service_name.as_deref(),
        route_id: &route.route_id,
        session_id: &session_id,
        current_step,
        remaining_route: &next_route,
        opened_refs: &opened_refs,
        new_facts: &new_facts,
        raw_refs: &raw_refs,
        data_quality: &data_quality,
        previous_state,
    });
    let branch_evaluations = session_state.branch_evaluations.clone();
    let next_actions = session_state.next_actions.clone();
    let mut pack = InvestigationContinuationPack {
        schema_version: "obs.investigation_continue.v1".to_string(),
        scope: scope.to_string(),
        run_id,
        fleet_run_id,
        service_name,
        route_id: route.route_id,
        session_id,
        current_step_id: request.current_step_id.clone(),
        opened_refs,
        new_facts,
        branch_evaluations,
        next_actions,
        investigation_route: next_route,
        raw_refs,
        data_quality,
        budget: InvestigationContinuationBudget {
            max_context_bytes: request.max_context_bytes,
            returned_bytes: 0,
            opened_ref_count: requested_refs.len(),
            omitted_ref_count: 0,
            truncated_ref_count,
        },
    };
    pack.budget.returned_bytes = serde_json::to_vec(&pack)
        .map_err(|err| AdcError::Artifact(format!("continuation serialization failed: {err}")))?
        .len();
    Ok(BuiltContinuation {
        pack,
        session_state,
    })
}

fn continuation_refs(
    step: &InvestigationStep,
    request: &InvestigationContinuationRequest,
) -> Vec<AgentContextRef> {
    let requested_labels = request
        .open_ref_labels
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut refs = step
        .refs
        .iter()
        .filter(|reference| {
            requested_labels.is_empty() || requested_labels.contains(&reference.label)
        })
        .cloned()
        .collect::<Vec<_>>();
    for raw_ref in &request.open_raw_refs {
        if refs.iter().any(|reference| &reference.raw_ref == raw_ref) {
            continue;
        }
        refs.push(AgentContextRef {
            label: label_for_raw_ref(raw_ref),
            raw_ref: raw_ref.clone(),
            reason: "explicit continuation ref".to_string(),
        });
    }
    refs
}

fn label_for_raw_ref(raw_ref: &str) -> String {
    raw_ref
        .rsplit('/')
        .next()
        .unwrap_or("ref")
        .trim_end_matches(".json")
        .trim_end_matches(".jsonl")
        .trim_end_matches(".yaml")
        .trim_end_matches(".txt")
        .replace('.', "_")
}

fn open_continuation_ref(
    artifact_root: &Path,
    run_id: Option<&str>,
    fleet_run_id: Option<&str>,
    reference: &AgentContextRef,
    limit: usize,
) -> AdcResult<OpenedRefSummary> {
    let resolution = if reference
        .raw_ref
        .starts_with("artifact://service_investigations/")
    {
        resolve_global_agent_ref(artifact_root, &reference.raw_ref, limit)?
    } else if reference.raw_ref.starts_with("artifact://fleet_runs/") {
        resolve_fleet_agent_ref(artifact_root, fleet_run_id, &reference.raw_ref, limit)?
    } else if let Some(run_id) = run_id {
        resolve_agent_ref(artifact_root, run_id, &reference.raw_ref, limit)?
    } else {
        return Err(AdcError::Artifact(format!(
            "ref {} requires a run_id or supported fleet ref",
            reference.raw_ref
        )));
    };
    let summary = summarize_ref_resolution(&reference.label, &resolution);
    let facts = extract_evidence_facts_from_ref(
        &reference.label,
        &reference.raw_ref,
        &resolution.ref_kind,
        &resolution.content_type,
        &resolution.text,
        &resolution.data_quality,
    );
    Ok(OpenedRefSummary {
        label: reference.label.clone(),
        raw_ref: reference.raw_ref.clone(),
        ref_kind: resolution.ref_kind,
        content_type: resolution.content_type,
        summary,
        item_count: resolution.returned_lines,
        truncated: resolution.truncated,
        data_quality: resolution.data_quality,
        facts,
        text: None,
    })
}

fn resolve_fleet_agent_ref(
    artifact_root: &Path,
    fleet_run_id: Option<&str>,
    ref_uri: &str,
    limit: usize,
) -> AdcResult<AgentRefResolution> {
    let Some(relative) = ref_uri.strip_prefix("artifact://") else {
        return Err(AdcError::Artifact(format!(
            "unsupported fleet artifact ref {ref_uri}"
        )));
    };
    validate_relative_artifact_path(relative)?;
    if let Some(fleet_run_id) = fleet_run_id {
        let expected_prefix = format!("fleet_runs/{fleet_run_id}/");
        if !relative.starts_with(&expected_prefix) {
            return Err(AdcError::Artifact(format!(
                "fleet ref {ref_uri} does not belong to fleet_run_id {fleet_run_id}"
            )));
        }
    }
    let path = artifact_root.join(relative);
    let contents = fs::read_to_string(&path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to resolve fleet artifact ref {} at {}: {err}",
            ref_uri,
            path.display()
        ))
    })?;
    let max_lines = limit.clamp(1, 1_000);
    let all_lines = contents.lines().map(str::to_string).collect::<Vec<_>>();
    let lines = all_lines
        .iter()
        .take(max_lines)
        .cloned()
        .collect::<Vec<_>>();
    let truncated = all_lines.len() > lines.len();
    let mut data_quality = DataQuality {
        truncated,
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };
    if truncated {
        data_quality.notes.push(format!(
            "fleet artifact ref returned {} of {} lines",
            lines.len(),
            all_lines.len()
        ));
    }
    let text = lines.join("\n");
    let data_quality_for_trust = data_quality.clone();
    Ok(AgentRefResolution {
        run_id: fleet_run_id.unwrap_or("fleet").to_string(),
        ref_uri: ref_uri.to_string(),
        ref_kind: "fleet_artifact".to_string(),
        content_type: content_type_for_path(&path),
        returned_lines: lines.len(),
        total_lines: all_lines.len(),
        truncated,
        artifact_trust: crate::classify_artifact_trust(
            ref_uri,
            crate::content_class_for_ref("fleet_artifact", &content_type_for_path(&path)),
            &text,
            &data_quality_for_trust,
        ),
        text,
        data_quality,
    })
}

fn content_type_for_path(path: &Path) -> String {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("json") => "application/json".to_string(),
        Some("jsonl") => "application/jsonl".to_string(),
        Some("yaml") | Some("yml") => "application/x-yaml".to_string(),
        Some("md") => "text/markdown".to_string(),
        _ => "text/plain".to_string(),
    }
}

fn summarize_ref_resolution(label: &str, resolution: &AgentRefResolution) -> String {
    if let Ok(state) = serde_json::from_str::<crate::ServiceStateSummary>(&resolution.text) {
        return format!(
            "Service {} state is {}/{} with availability={}.",
            state.service, state.active_state, state.sub_state, state.availability
        );
    }
    if label.contains("port_summary") || resolution.ref_uri.ends_with("/port_summary.json") {
        if let Ok(port) = serde_json::from_str::<crate::ServicePortSummary>(&resolution.text) {
            return format!(
                "Port summary availability={} socket_inode_count={} matched_socket_table_count={}.",
                port.availability,
                port.socket_inode_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                port.matched_socket_table_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            );
        }
    }
    if label.contains("process_summary") || resolution.ref_uri.ends_with("/process_summary.json") {
        if let Ok(process) = serde_json::from_str::<crate::ServiceProcessSummary>(&resolution.text)
        {
            return format!(
                "Process summary comm={} pid={} rss_kb={}.",
                process.comm.as_deref().unwrap_or("unknown"),
                process
                    .pid
                    .map(|pid| pid.to_string())
                    .unwrap_or_else(|| "none".to_string()),
                process
                    .rss_kb
                    .map(|rss| rss.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            );
        }
    }
    if let Ok(pack) = serde_json::from_str::<ServiceInvestigationPack>(&resolution.text) {
        return format!(
            "Service {} pack reports availability={} active_state={} journal_leads={} data_quality_missing={}.",
            pack.service_name,
            pack.service_state.availability,
            pack.service_state.active_state,
            pack.journal_summary.returned_lead_count,
            pack.data_quality.missing.len()
        );
    }
    if let Ok(leads) = serde_json::from_str::<Vec<crate::ServiceJournalLead>>(&resolution.text) {
        let mut severities = BTreeMap::<String, usize>::new();
        for lead in &leads {
            *severities.entry(lead.severity_hint.clone()).or_default() += 1;
        }
        return format!(
            "Journal leads include {} bounded item(s) with severity buckets {:?}.",
            leads.len(),
            severities
        );
    }
    let line_count = resolution.returned_lines;
    let signal_count = resolution
        .text
        .lines()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            ["error", "warn", "fail", "timeout", "denied"]
                .iter()
                .any(|needle| lower.contains(needle))
        })
        .count();
    if resolution.content_type.contains("json") {
        if let Ok(value) = serde_json::from_str::<Value>(&resolution.text) {
            let shape = match value {
                Value::Object(ref object) => format!("object_keys={}", object.len()),
                Value::Array(ref array) => format!("array_items={}", array.len()),
                _ => "scalar".to_string(),
            };
            return format!("JSON ref {label} opened as {shape}; returned_lines={line_count}.");
        }
    }
    format!("Text ref {label} returned {line_count} bounded line(s), including {signal_count} signal line(s).")
}

fn continuation_fact_kind(label: &str, ref_kind: &str) -> String {
    let normalized = label.replace('.', "_");
    if normalized.contains("service_state") {
        "opened_service_state".to_string()
    } else if normalized.contains("journal") {
        "opened_journal_leads".to_string()
    } else if normalized.contains("process") {
        "opened_process_summary".to_string()
    } else if normalized.contains("port") {
        "opened_port_summary".to_string()
    } else if normalized.contains("semantic_diff") {
        "opened_fleet_semantic_diff".to_string()
    } else {
        format!("opened_{ref_kind}")
    }
}

fn route_after_step(
    route: &InvestigationRoute,
    current_step_id: &str,
    data_quality: &DataQuality,
) -> InvestigationRoute {
    let mut next_route = route.clone();
    if let Some(index) = route
        .steps
        .iter()
        .position(|step| step.step_id == current_step_id)
    {
        next_route.steps = route.steps.iter().skip(index + 1).cloned().collect();
    }
    if next_route.steps.is_empty() && !data_quality.missing.is_empty() {
        next_route.steps.push(data_quality_route_step(
            "IR-DQ",
            "Review continuation data-quality gaps",
            data_quality,
            Vec::new(),
        ));
    }
    next_route.budget.returned_step_count = next_route.steps.len();
    next_route
}

fn persist_run_investigation_route(
    artifact_root: &Path,
    run_id: &str,
    route: &InvestigationRoute,
) -> AdcResult<()> {
    let run_dir = run_dir(artifact_root, run_id);
    write_json_pretty(&run_dir.join("investigation_route.json"), route)?;
    add_run_manifest_entry(
        artifact_root,
        run_id,
        "investigation_route.json",
        "investigation_route",
    )
}

fn persist_run_investigation_session(
    artifact_root: &Path,
    run_id: &str,
    session_id: &str,
    pack: &InvestigationContinuationPack,
) -> AdcResult<()> {
    validate_segment(session_id, "session_id")?;
    let relative_path = format!("investigation_sessions/{session_id}.json");
    let run_dir = run_dir(artifact_root, run_id);
    write_json_pretty(&run_dir.join(&relative_path), pack)?;
    add_run_manifest_entry(
        artifact_root,
        run_id,
        &relative_path,
        "investigation_session",
    )
}

fn persist_run_investigation_session_state(
    artifact_root: &Path,
    run_id: &str,
    session_id: &str,
    state: &InvestigationSessionState,
) -> AdcResult<()> {
    validate_segment(session_id, "session_id")?;
    let relative_path = format!("investigation_sessions/{session_id}.state.json");
    let run_dir = run_dir(artifact_root, run_id);
    write_json_pretty(&run_dir.join(&relative_path), state)?;
    add_run_manifest_entry(
        artifact_root,
        run_id,
        &relative_path,
        "investigation_session_state",
    )
}

fn persist_fleet_investigation_route(
    artifact_root: &Path,
    fleet_run_id: &str,
    route: &InvestigationRoute,
) -> AdcResult<()> {
    write_json_pretty(
        &artifact_root
            .join("fleet_runs")
            .join(fleet_run_id)
            .join("investigation_route.json"),
        route,
    )
}

fn persist_fleet_investigation_session(
    artifact_root: &Path,
    fleet_run_id: &str,
    session_id: &str,
    pack: &InvestigationContinuationPack,
) -> AdcResult<()> {
    validate_segment(session_id, "session_id")?;
    write_json_pretty(
        &artifact_root
            .join("fleet_runs")
            .join(fleet_run_id)
            .join("investigation_sessions")
            .join(format!("{session_id}.json")),
        pack,
    )
}

fn persist_fleet_investigation_session_state(
    artifact_root: &Path,
    fleet_run_id: &str,
    session_id: &str,
    state: &InvestigationSessionState,
) -> AdcResult<()> {
    validate_segment(session_id, "session_id")?;
    write_json_pretty(
        &artifact_root
            .join("fleet_runs")
            .join(fleet_run_id)
            .join("investigation_sessions")
            .join(format!("{session_id}.state.json")),
        state,
    )
}

fn persist_fleet_semantic_diff(
    artifact_root: &Path,
    fleet_run_id: &str,
    diff: &FleetSemanticDiff,
) -> AdcResult<()> {
    write_json_pretty(
        &artifact_root
            .join("fleet_runs")
            .join(fleet_run_id)
            .join("fleet_semantic_diff.json"),
        diff,
    )
}

fn read_session_state_optional(
    artifact_root: &Path,
    scope: &str,
    run_id: Option<&str>,
    fleet_run_id: Option<&str>,
    session_id: &str,
) -> AdcResult<Option<InvestigationSessionState>> {
    validate_segment(session_id, "session_id")?;
    let path = match scope {
        "run" => {
            let Some(run_id) = run_id else {
                return Ok(None);
            };
            run_dir(artifact_root, run_id)
                .join("investigation_sessions")
                .join(format!("{session_id}.state.json"))
        }
        "fleet" => {
            let Some(fleet_run_id) = fleet_run_id else {
                return Ok(None);
            };
            artifact_root
                .join("fleet_runs")
                .join(fleet_run_id)
                .join("investigation_sessions")
                .join(format!("{session_id}.state.json"))
        }
        _ => return Ok(None),
    };
    if !path.is_file() {
        return Ok(None);
    }
    read_json_file(&path).map(Some)
}

fn cleanup_session_dir(
    session_dir: PathBuf,
    scope: &str,
    run_id: Option<String>,
    fleet_run_id: Option<String>,
    max_sessions: usize,
    max_age_days: Option<u64>,
    dry_run: bool,
) -> AdcResult<InvestigationSessionCleanupReport> {
    let mut data_quality = DataQuality {
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };
    if !session_dir.is_dir() {
        data_quality.missing.push(format!(
            "session directory not found: {}",
            session_dir.display()
        ));
        return Ok(InvestigationSessionCleanupReport {
            schema_version: "obs.investigation_session_cleanup.v1".to_string(),
            scope: scope.to_string(),
            run_id,
            fleet_run_id,
            dry_run,
            candidate_count: 0,
            deleted_count: 0,
            candidates: Vec::new(),
            data_quality,
        });
    }
    let mut files = fs::read_dir(&session_dir)
        .map_err(|err| {
            AdcError::Artifact(format!(
                "failed to read session directory {}: {err}",
                session_dir.display()
            ))
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".json"))
        })
        .collect::<Vec<_>>();
    files.sort();
    let now = SystemTime::now();
    let removable_count = files.len().saturating_sub(max_sessions);
    let mut candidates = Vec::new();
    for path in files.iter().take(removable_count) {
        candidates.push(cleanup_candidate(
            path,
            now,
            format!("session file exceeds max_sessions={max_sessions}"),
            false,
        )?);
    }
    if let Some(max_age_days) = max_age_days {
        let max_age_seconds = max_age_days.saturating_mul(24 * 60 * 60);
        for path in &files {
            let candidate = cleanup_candidate(
                path,
                now,
                format!("session file age exceeds max_age_days={max_age_days}"),
                false,
            )?;
            if candidate.age_seconds >= max_age_seconds
                && !candidates
                    .iter()
                    .any(|existing| existing.path == candidate.path)
            {
                candidates.push(candidate);
            }
        }
    }
    let mut deleted_count = 0;
    if !dry_run {
        for candidate in &mut candidates {
            let path = PathBuf::from(&candidate.path);
            if path.parent() != Some(session_dir.as_path()) {
                return Err(AdcError::Artifact(format!(
                    "refusing to delete session path outside {}: {}",
                    session_dir.display(),
                    path.display()
                )));
            }
            fs::remove_file(&path).map_err(|err| {
                AdcError::Artifact(format!(
                    "failed to delete session file {}: {err}",
                    path.display()
                ))
            })?;
            candidate.deleted = true;
            deleted_count += 1;
        }
    }
    Ok(InvestigationSessionCleanupReport {
        schema_version: "obs.investigation_session_cleanup.v1".to_string(),
        scope: scope.to_string(),
        run_id,
        fleet_run_id,
        dry_run,
        candidate_count: candidates.len(),
        deleted_count,
        candidates,
        data_quality,
    })
}

fn cleanup_candidate(
    path: &Path,
    now: SystemTime,
    reason: String,
    deleted: bool,
) -> AdcResult<InvestigationSessionCleanupCandidate> {
    let modified = fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(UNIX_EPOCH);
    let age_seconds = now
        .duration_since(modified)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    Ok(InvestigationSessionCleanupCandidate {
        path: path.display().to_string(),
        reason,
        age_seconds,
        deleted,
    })
}

fn read_run_route_optional(
    artifact_root: &Path,
    run_id: &str,
) -> AdcResult<Option<InvestigationRoute>> {
    let path = run_dir(artifact_root, run_id).join("investigation_route.json");
    if !path.is_file() {
        return Ok(None);
    }
    read_json_file(&path).map(Some)
}

fn read_fleet_route_optional(
    artifact_root: &Path,
    fleet_run_id: &str,
) -> AdcResult<Option<InvestigationRoute>> {
    let path = artifact_root
        .join("fleet_runs")
        .join(fleet_run_id)
        .join("investigation_route.json");
    if !path.is_file() {
        return Ok(None);
    }
    read_json_file(&path).map(Some)
}

fn add_run_manifest_entry(
    artifact_root: &Path,
    run_id: &str,
    relative_path: &str,
    source: &str,
) -> AdcResult<()> {
    let manifest_path = run_dir(artifact_root, run_id).join("manifest.json");
    if !manifest_path.is_file() {
        return Ok(());
    }
    let mut manifest = ArtifactManifest::read_json(&manifest_path)?;
    manifest
        .artifacts
        .retain(|entry| entry.path != relative_path);
    manifest.add_file(run_dir(artifact_root, run_id), relative_path, source)?;
    manifest.write_json(&manifest_path)
}

fn write_json_pretty(path: &Path, value: &impl Serialize) -> AdcResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AdcError::Artifact(format!(
                "failed to create artifact directory {}: {err}",
                parent.display()
            ))
        })?;
    }
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|err| AdcError::Artifact(format!("json serialization failed: {err}")))?;
    fs::write(path, bytes)
        .map_err(|err| AdcError::Artifact(format!("failed to write {}: {err}", path.display())))
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> AdcResult<T> {
    let bytes = fs::read(path)
        .map_err(|err| AdcError::Artifact(format!("failed to read {}: {err}", path.display())))?;
    serde_json::from_slice(&bytes)
        .map_err(|err| AdcError::Artifact(format!("failed to parse {}: {err}", path.display())))
}

fn merge_data_quality(target: &mut DataQuality, source: &DataQuality) {
    target.dropped |= source.dropped;
    target.throttled |= source.throttled;
    target.truncated |= source.truncated;
    target.drop_count = target.drop_count.saturating_add(source.drop_count);
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
}

fn resolve_run_id_alias(artifact_root: &Path, run_id: String) -> AdcResult<String> {
    if run_id == "latest" {
        latest_run_id(artifact_root)?.ok_or_else(|| {
            AdcError::Artifact("no runs are available for run_id=latest".to_string())
        })
    } else {
        Ok(run_id)
    }
}

fn resolve_fleet_run_id_alias(artifact_root: &Path, fleet_run_id: String) -> AdcResult<String> {
    if fleet_run_id == "latest" {
        latest_fleet_run_id(artifact_root)?.ok_or_else(|| {
            AdcError::Artifact("no fleet runs are available for fleet_run_id=latest".to_string())
        })
    } else {
        Ok(fleet_run_id)
    }
}

fn build_fleet_cross_target_summary(
    captured_count: usize,
    failed_count: usize,
    target_summaries: &[FleetTargetContextSummary],
) -> FleetCrossTargetSummary {
    let total_event_count = target_summaries
        .iter()
        .filter_map(|target| target.event_count)
        .sum();
    let mut source_totals = BTreeMap::<String, u64>::new();
    let mut targets_with_missing_data_quality = Vec::new();
    for target in target_summaries {
        for source in &target.sources {
            let total = source_totals.entry(source.source.clone()).or_default();
            *total = total.saturating_add(source.event_count);
        }
        if !target.data_quality.missing.is_empty() {
            targets_with_missing_data_quality.push(target.target_id.clone());
        }
    }
    FleetCrossTargetSummary {
        captured_count,
        failed_count,
        total_event_count,
        source_totals: source_totals
            .into_iter()
            .map(|(source, event_count)| FleetTargetSourceSummary {
                source,
                event_count,
            })
            .collect(),
        targets_with_missing_data_quality,
    }
}

fn build_fleet_target_context_summary(
    artifact_root: &Path,
    target: &FleetTargetEvidence,
) -> FleetTargetContextSummary {
    let mut summary = FleetTargetContextSummary {
        target_id: target.target_id.clone(),
        status: target.status.clone(),
        run_id: target.run_id.clone(),
        profile_id: target.profile_id.clone(),
        evidence_ref: target.evidence_ref.clone(),
        primary_window_id: None,
        event_count: None,
        sources: Vec::new(),
        target_dossier: None,
        top_leads: Vec::new(),
        data_quality: target.data_quality.clone(),
    };
    let Some(evidence_ref) = target.evidence_ref.as_deref() else {
        return summary;
    };
    match read_evidence_index_from_artifact_ref(artifact_root, evidence_ref) {
        Ok(evidence) => {
            summary.primary_window_id = Some(evidence.primary_window.window_id.clone());
            summary.event_count = Some(evidence.primary_window.event_count as u64);
            summary.sources = evidence
                .observed_facts
                .iter()
                .map(|fact| FleetTargetSourceSummary {
                    source: fact.source.clone(),
                    event_count: fact
                        .attributes
                        .get("event_count")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                })
                .collect();
            let derived_facts = facts_from_evidence_index(&evidence);
            let mut raw_refs = evidence.raw_refs.clone();
            if let Some(evidence_ref) = &target.evidence_ref {
                raw_refs.insert("evidence_index".to_string(), evidence_ref.clone());
            }
            if let Some(artifact_ref) = &target.artifact_ref {
                raw_refs.insert("artifact".to_string(), artifact_ref.clone());
            }
            summary.top_leads = recommend_refs(&evidence, &derived_facts);
            summary.target_dossier = Some(build_target_dossier(
                &evidence,
                target.profile_id.clone(),
                &raw_refs,
                &derived_facts,
                None,
            ));
            merge_context_data_quality(&mut summary.data_quality, &evidence.data_quality);
        }
        Err(err) => {
            summary.data_quality.missing.push(format!(
                "target {} evidence summary unavailable: {err}",
                target.target_id
            ));
        }
    }
    summary
}

fn facts_from_evidence_index(evidence: &EvidenceIndex) -> Vec<AgentContextFact> {
    let mut facts = evidence
        .observed_facts
        .iter()
        .enumerate()
        .map(|(index, fact)| AgentContextFact {
            fact_id: format!("AFF{:03}", index + 1),
            source: fact.source.clone(),
            kind: "fleet_observed_fact".to_string(),
            window_id: fact.window_id.clone(),
            statement: fact.statement.clone(),
            raw_ref: fact.raw_ref.clone(),
            attributes: fact.attributes.clone(),
            data_quality: fact.data_quality.clone(),
        })
        .collect::<Vec<_>>();
    sort_facts_by_salience(&mut facts);
    facts
}

fn build_fleet_failure_groups(
    target_summaries: &[FleetTargetContextSummary],
) -> Vec<FleetFailureGroup> {
    let mut groups = BTreeMap::<String, FleetFailureGroup>::new();
    for target in target_summaries {
        if target.status == "captured" && target.data_quality.missing.is_empty() {
            continue;
        }
        let sample = target
            .data_quality
            .missing
            .first()
            .cloned()
            .unwrap_or_else(|| format!("target {} status {}", target.target_id, target.status));
        let failure_class = classify_fleet_failure(&target.status, &sample);
        let group = groups
            .entry(failure_class.clone())
            .or_insert_with(|| FleetFailureGroup {
                failure_class: failure_class.clone(),
                targets: Vec::new(),
                sample: sample.clone(),
                next_action: fleet_failure_next_action(&failure_class),
            });
        if !group.targets.contains(&target.target_id) {
            group.targets.push(target.target_id.clone());
        }
    }
    groups.into_values().collect()
}

fn classify_fleet_failure(status: &str, sample: &str) -> String {
    let lower = format!("{} {}", status, sample).to_ascii_lowercase();
    if lower.contains("permission_denied") || lower.contains("permission denied") {
        "permission_denied".to_string()
    } else if lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("auth")
        || lower.contains("401")
        || lower.contains("403")
    {
        "auth_failed".to_string()
    } else if lower.contains("unsupported") {
        "unsupported_transport".to_string()
    } else if lower.contains("collector") {
        "collector_failed".to_string()
    } else if lower.contains("artifact") || lower.contains("evidence summary unavailable") {
        "artifact_unavailable".to_string()
    } else if lower.contains("unreachable")
        || lower.contains("connection refused")
        || lower.contains("timed out")
        || lower.contains("timeout")
    {
        "unreachable".to_string()
    } else {
        status.to_string()
    }
}

fn fleet_failure_next_action(failure_class: &str) -> String {
    match failure_class {
        "permission_denied" => {
            "run fleet preflight for the affected targets and verify key/token permissions"
                .to_string()
        }
        "auth_failed" => {
            "rotate or re-enroll the managed MCP credentials, then rerun fleet preflight"
                .to_string()
        }
        "unsupported_transport" => {
            "change the inventory transport to local, mcp_stdio_over_ssh, or managed_mcp"
                .to_string()
        }
        "collector_failed" => {
            "open target data_quality and rerun with a lower-cost profile or supported collector"
                .to_string()
        }
        "artifact_unavailable" => {
            "verify target artifact refs and rerun fleet evidence retrieval for this target"
                .to_string()
        }
        "unreachable" => {
            "check target listener reachability, host/port, firewall, and managed MCP service status"
                .to_string()
        }
        _ => "inspect target data_quality and rerun the smallest bounded preflight".to_string(),
    }
}

fn read_evidence_index_from_artifact_ref(
    artifact_root: &Path,
    ref_uri: &str,
) -> AdcResult<EvidenceIndex> {
    let path = artifact_ref_path(artifact_root, ref_uri)?;
    let contents = fs::read_to_string(&path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read target evidence ref {} at {}: {err}",
            ref_uri,
            path.display()
        ))
    })?;
    yaml_serde::from_str(&contents).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to parse target evidence ref {} at {}: {err}",
            ref_uri,
            path.display()
        ))
    })
}

fn artifact_ref_path(artifact_root: &Path, ref_uri: &str) -> AdcResult<PathBuf> {
    let Some(rest) = ref_uri.strip_prefix("artifact://") else {
        return Err(AdcError::Artifact(format!(
            "unsupported artifact ref {ref_uri}; expected artifact://..."
        )));
    };
    let relative = Path::new(rest);
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(AdcError::Artifact(format!(
            "unsupported artifact ref path {ref_uri}"
        )));
    }
    Ok(artifact_root.join(relative))
}

fn merge_context_data_quality(target: &mut DataQuality, source: &DataQuality) {
    target.dropped |= source.dropped;
    target.throttled |= source.throttled;
    target.truncated |= source.truncated;
    target.drop_count = target.drop_count.saturating_add(source.drop_count);
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
}

fn read_context_events(artifact_root: &Path, run_id: &str) -> AdcResult<Vec<EventEnvelope>> {
    let result = search_events(
        artifact_root,
        run_id,
        &SearchEventsQuery {
            source: None,
            event_type: None,
            contains: None,
            limit: MAX_TIMELINE_EVENTS_FOR_CONTEXT,
        },
    )?;
    Ok(result.events)
}

fn read_manifest_optional(
    artifact_root: &Path,
    run_id: &str,
) -> AdcResult<Option<ArtifactManifest>> {
    let path = run_dir(artifact_root, run_id).join("manifest.json");
    if !path.is_file() {
        return Ok(None);
    }
    ArtifactManifest::read_json(path).map(Some)
}

fn read_overhead_optional(artifact_root: &Path, run_id: &str) -> AdcResult<Option<OverheadReport>> {
    let path = run_dir(artifact_root, run_id).join("overhead_report.json");
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read overhead report {}: {err}",
            path.display()
        ))
    })?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|err| AdcError::Artifact(format!("overhead report parse failed: {err}")))
}

fn build_target_dossier(
    evidence: &EvidenceIndex,
    profile_id: Option<String>,
    raw_refs: &BTreeMap<String, String>,
    derived_facts: &[AgentContextFact],
    overhead: Option<AgentContextOverhead>,
) -> AgentTargetDossier {
    let artifact_refs = raw_refs
        .iter()
        .filter(|(_, raw_ref)| !raw_ref.starts_with("artifact://raw/"))
        .map(|(key, raw_ref)| (key.clone(), raw_ref.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut capability_summary = BTreeMap::new();
    let mut root_required = false;
    for fact in derived_facts {
        if fact.kind == "kernel_optional_probe_snapshot" || fact.kind == "fd_thread_snapshot" {
            for (key, value) in &fact.attributes {
                capability_summary.insert(key.clone(), value.clone());
            }
        }
        if fact
            .attributes
            .get("root_required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            root_required = true;
        }
    }
    let mut redacted_artifacts = Vec::new();
    if derived_facts.iter().any(|fact| {
        fact.kind == "config_snapshot"
            && fact
                .attributes
                .get("redacted_count")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                > 0
    }) {
        redacted_artifacts.push("config".to_string());
    }

    AgentTargetDossier {
        target_id: evidence.target_id.clone(),
        profile_id,
        run_id: Some(evidence.run_id.clone()),
        fleet_run_id: evidence.fleet_run_id.clone(),
        primary_window_id: evidence.primary_window.window_id.clone(),
        primary_window_event_count: evidence.primary_window.event_count,
        artifact_refs,
        raw_ref_count: raw_refs.len(),
        capability_summary,
        root_required,
        raw_artifacts_are_ref_only: true,
        redacted_artifacts,
        data_quality: evidence.data_quality.clone(),
        overhead,
    }
}

fn enforce_context_budget(context: &mut AgentContext) -> AdcResult<()> {
    let max_bytes = context.context_budget.max_markdown_bytes;
    let mut rendered_len = render_agent_context_markdown(context)?.len();
    if rendered_len <= max_bytes {
        context.context_budget.rendered_bytes = rendered_len;
        return Ok(());
    }

    let original_fact_count = context.derived_facts.len();
    while rendered_len > max_bytes && context.derived_facts.len() > 1 {
        context.derived_facts.pop();
        context.recommended_refs = recommend_refs(&context.evidence_index, &context.derived_facts);
        rendered_len = render_agent_context_markdown(context)?.len();
    }
    while rendered_len > max_bytes && !context.next_probe_options.is_empty() {
        context.next_probe_options.pop();
        rendered_len = render_agent_context_markdown(context)?.len();
    }

    let omitted_fact_count = original_fact_count.saturating_sub(context.derived_facts.len());
    context.context_budget.rendered_bytes = rendered_len;
    context.context_budget.truncated = true;
    context.data_quality.truncated = true;
    context.data_quality.notes.push(format!(
        "agent context markdown reduced from {original_fact_count} to {} derived fact(s) to fit {max_bytes} bytes; omitted {omitted_fact_count}",
        context.derived_facts.len()
    ));
    Ok(())
}

fn derive_run_facts(
    artifact_root: &Path,
    run_id: &str,
    evidence: &EvidenceIndex,
    events: &[EventEnvelope],
) -> AdcResult<Vec<AgentContextFact>> {
    let mut facts = Vec::new();
    let cpu_samples = samples_for_source(artifact_root, run_id, events, "cpu")?;
    if let Some(fact) = derive_cpu_fact(evidence, &cpu_samples, facts.len() + 1) {
        facts.push(fact);
    }
    let memory_samples = samples_for_source(artifact_root, run_id, events, "memory")?;
    if let Some(fact) = derive_memory_fact(evidence, &memory_samples, facts.len() + 1) {
        facts.push(fact);
    }
    let network_samples = samples_for_source(artifact_root, run_id, events, "network")?;
    if let Some(fact) = derive_network_fact(evidence, &network_samples, facts.len() + 1) {
        facts.push(fact);
    }
    Ok(facts)
}

fn derive_optional_artifact_facts(
    artifact_root: &Path,
    run_id: &str,
    evidence: &EvidenceIndex,
    start_index: usize,
    raw_refs: &mut BTreeMap<String, String>,
) -> AdcResult<Vec<AgentContextFact>> {
    let raw_dir = run_dir(artifact_root, run_id).join("raw");
    let mut facts = Vec::new();
    if let Some(fact) = derive_log_fact(
        &raw_dir.join("app.log"),
        evidence,
        start_index + facts.len(),
        raw_refs,
    )? {
        facts.push(fact);
    }
    if let Some(fact) = derive_domain_events_fact(
        &raw_dir.join("domain_events.jsonl"),
        evidence,
        start_index + facts.len(),
        raw_refs,
    )? {
        facts.push(fact);
    }
    if let Some(fact) = derive_otlp_metrics_fact(
        &raw_dir.join("otlp_metrics.json"),
        evidence,
        start_index + facts.len(),
        raw_refs,
    )? {
        facts.push(fact);
    }
    if let Some(fact) = derive_journald_fact(
        &raw_dir.join("journald.jsonl"),
        evidence,
        start_index + facts.len(),
        raw_refs,
    )? {
        facts.push(fact);
    }
    if let Some(fact) = derive_perfetto_fact(
        &raw_dir.join("perfetto_trace.json"),
        evidence,
        start_index + facts.len(),
        raw_refs,
    )? {
        facts.push(fact);
    }
    if let Some(fact) = derive_config_fact(
        &raw_dir.join("config_redacted.txt"),
        evidence,
        start_index + facts.len(),
        raw_refs,
    )? {
        facts.push(fact);
    }
    if let Some(fact) = derive_service_state_fact(
        &raw_dir.join("service_state.json"),
        evidence,
        start_index + facts.len(),
        raw_refs,
    )? {
        facts.push(fact);
    }
    if let Some(fact) = derive_runtime_json_fact(
        &raw_dir.join("process_snapshot.json"),
        evidence,
        start_index + facts.len(),
        raw_refs,
        RuntimeFactSpec {
            source: "process",
            kind: "process_snapshot",
            raw_key: "process_snapshot",
            raw_ref: "artifact://raw/process_snapshot.json",
            count_key: "process_count",
            statement_label: "Process snapshot",
        },
    )? {
        facts.push(fact);
    }
    if let Some(fact) = derive_runtime_json_fact(
        &raw_dir.join("io_snapshot.json"),
        evidence,
        start_index + facts.len(),
        raw_refs,
        RuntimeFactSpec {
            source: "io",
            kind: "io_snapshot",
            raw_key: "io_snapshot",
            raw_ref: "artifact://raw/io_snapshot.json",
            count_key: "device_count",
            statement_label: "IO snapshot",
        },
    )? {
        facts.push(fact);
    }
    if let Some(fact) = derive_runtime_json_fact(
        &raw_dir.join("thermal_snapshot.json"),
        evidence,
        start_index + facts.len(),
        raw_refs,
        RuntimeFactSpec {
            source: "thermal",
            kind: "thermal_snapshot",
            raw_key: "thermal_snapshot",
            raw_ref: "artifact://raw/thermal_snapshot.json",
            count_key: "zone_count",
            statement_label: "Thermal snapshot",
        },
    )? {
        facts.push(fact);
    }
    if let Some(fact) = derive_fd_thread_snapshot_fact(
        &raw_dir.join("fd_thread_snapshot.json"),
        evidence,
        start_index + facts.len(),
        raw_refs,
    )? {
        facts.push(fact);
    }
    if let Some(fact) = derive_kernel_probe_snapshot_fact(
        &raw_dir.join("kernel_probe_snapshot.json"),
        evidence,
        start_index + facts.len(),
        raw_refs,
    )? {
        facts.push(fact);
    }
    Ok(facts)
}

fn derive_log_fact(
    path: &Path,
    evidence: &EvidenceIndex,
    index: usize,
    raw_refs: &mut BTreeMap<String, String>,
) -> AdcResult<Option<AgentContextFact>> {
    if !path.is_file() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read log artifact {}: {err}",
            path.display()
        ))
    })?;
    let lines = contents
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    let signal_lines = contents
        .lines()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("error")
                || lower.contains("warn")
                || lower.contains("timeout")
                || lower.contains("panic")
                || lower.contains("fail")
        })
        .count();
    let raw_ref = "artifact://raw/app.log".to_string();
    raw_refs.insert("app_log".to_string(), raw_ref.clone());
    let mut attributes = BTreeMap::new();
    attributes.insert("line_count".to_string(), json!(lines));
    attributes.insert("signal_line_count".to_string(), json!(signal_lines));
    Ok(Some(AgentContextFact {
        fact_id: format!("AF{:03}", index),
        source: "log".to_string(),
        kind: "log_error_slice".to_string(),
        window_id: evidence.primary_window.window_id.clone(),
        statement: format!(
            "Log slice contains {} signal line(s) across {} bounded line(s)",
            signal_lines, lines
        ),
        raw_ref,
        attributes,
        data_quality: evidence.data_quality.clone(),
    }))
}

fn derive_domain_events_fact(
    path: &Path,
    evidence: &EvidenceIndex,
    index: usize,
    raw_refs: &mut BTreeMap<String, String>,
) -> AdcResult<Option<AgentContextFact>> {
    if !path.is_file() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read domain event artifact {}: {err}",
            path.display()
        ))
    })?;
    let mut event_count = 0_usize;
    let mut event_types = Vec::new();
    for line in contents.lines().filter(|line| !line.trim().is_empty()) {
        event_count += 1;
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            if let Some(event_type) = value.get("event_type").and_then(Value::as_str) {
                if !event_types.iter().any(|known| known == event_type) && event_types.len() < 5 {
                    event_types.push(event_type.to_string());
                }
            }
        }
    }
    let raw_ref = "artifact://raw/domain_events.jsonl".to_string();
    raw_refs.insert("domain_events".to_string(), raw_ref.clone());
    let mut attributes = BTreeMap::new();
    attributes.insert("event_count".to_string(), json!(event_count));
    attributes.insert("event_types".to_string(), json!(event_types));
    Ok(Some(AgentContextFact {
        fact_id: format!("AF{:03}", index),
        source: "domain_event".to_string(),
        kind: "domain_event_count".to_string(),
        window_id: evidence.primary_window.window_id.clone(),
        statement: format!("Domain event adapter recorded {event_count} event(s)"),
        raw_ref,
        attributes,
        data_quality: evidence.data_quality.clone(),
    }))
}

fn derive_otlp_metrics_fact(
    path: &Path,
    evidence: &EvidenceIndex,
    index: usize,
    raw_refs: &mut BTreeMap<String, String>,
) -> AdcResult<Option<AgentContextFact>> {
    if !path.is_file() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read OTLP artifact {}: {err}",
            path.display()
        ))
    })?;
    let mut data_quality = evidence.data_quality.clone();
    let value = match serde_json::from_str::<Value>(&contents) {
        Ok(value) => value,
        Err(err) => {
            data_quality
                .missing
                .push(format!("otlp: json parse failed: {err}"));
            json!({})
        }
    };
    let metrics = otlp_metric_names(&value);
    let raw_ref = "artifact://raw/otlp_metrics.json".to_string();
    raw_refs.insert("otlp_metrics".to_string(), raw_ref.clone());
    let mut attributes = BTreeMap::new();
    attributes.insert("metric_count".to_string(), json!(metrics.len()));
    attributes.insert(
        "metric_names_sample".to_string(),
        json!(metrics.iter().take(5).cloned().collect::<Vec<_>>()),
    );
    Ok(Some(AgentContextFact {
        fact_id: format!("AF{:03}", index),
        source: "otlp".to_string(),
        kind: "otlp_metric_count".to_string(),
        window_id: evidence.primary_window.window_id.clone(),
        statement: format!(
            "OTLP adapter imported {} metric definition(s)",
            metrics.len()
        ),
        raw_ref,
        attributes,
        data_quality,
    }))
}

fn derive_journald_fact(
    path: &Path,
    evidence: &EvidenceIndex,
    index: usize,
    raw_refs: &mut BTreeMap<String, String>,
) -> AdcResult<Option<AgentContextFact>> {
    if !path.is_file() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read journald artifact {}: {err}",
            path.display()
        ))
    })?;
    let mut entry_count = 0_usize;
    let mut warning_or_error_count = 0_usize;
    for line in contents.lines().filter(|line| !line.trim().is_empty()) {
        entry_count += 1;
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            let priority = value
                .get("PRIORITY")
                .and_then(|priority| {
                    priority
                        .as_u64()
                        .or_else(|| priority.as_str().and_then(|text| text.parse::<u64>().ok()))
                })
                .unwrap_or(6);
            if priority <= 4 {
                warning_or_error_count += 1;
            }
        }
    }
    let raw_ref = "artifact://raw/journald.jsonl".to_string();
    raw_refs.insert("journald".to_string(), raw_ref.clone());
    let mut attributes = BTreeMap::new();
    attributes.insert("entry_count".to_string(), json!(entry_count));
    attributes.insert(
        "warning_or_error_count".to_string(),
        json!(warning_or_error_count),
    );
    Ok(Some(AgentContextFact {
        fact_id: format!("AF{:03}", index),
        source: "journald".to_string(),
        kind: "journald_entry_count".to_string(),
        window_id: evidence.primary_window.window_id.clone(),
        statement: format!(
            "journald adapter imported {entry_count} entrie(s), {warning_or_error_count} warning/error entrie(s)"
        ),
        raw_ref,
        attributes,
        data_quality: evidence.data_quality.clone(),
    }))
}

fn derive_perfetto_fact(
    path: &Path,
    evidence: &EvidenceIndex,
    index: usize,
    raw_refs: &mut BTreeMap<String, String>,
) -> AdcResult<Option<AgentContextFact>> {
    if !path.is_file() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read Perfetto artifact {}: {err}",
            path.display()
        ))
    })?;
    let mut data_quality = evidence.data_quality.clone();
    let value = match serde_json::from_str::<Value>(&contents) {
        Ok(value) => value,
        Err(err) => {
            data_quality
                .missing
                .push(format!("perfetto: json parse failed: {err}"));
            json!({})
        }
    };
    let trace_event_count = value
        .get("traceEvents")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let raw_ref = "artifact://raw/perfetto_trace.json".to_string();
    raw_refs.insert("perfetto_trace".to_string(), raw_ref.clone());
    let mut attributes = BTreeMap::new();
    attributes.insert("trace_event_count".to_string(), json!(trace_event_count));
    Ok(Some(AgentContextFact {
        fact_id: format!("AF{:03}", index),
        source: "perfetto".to_string(),
        kind: "perfetto_event_count".to_string(),
        window_id: evidence.primary_window.window_id.clone(),
        statement: format!("Perfetto adapter imported {trace_event_count} trace event(s)"),
        raw_ref,
        attributes,
        data_quality,
    }))
}

fn derive_config_fact(
    path: &Path,
    evidence: &EvidenceIndex,
    index: usize,
    raw_refs: &mut BTreeMap<String, String>,
) -> AdcResult<Option<AgentContextFact>> {
    if !path.is_file() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read config artifact {}: {err}",
            path.display()
        ))
    })?;
    let line_count = contents
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    let redacted_count = contents.matches("<redacted>").count();
    let raw_ref = "artifact://raw/config_redacted.txt".to_string();
    raw_refs.insert("config".to_string(), raw_ref.clone());
    let mut attributes = BTreeMap::new();
    attributes.insert("line_count".to_string(), json!(line_count));
    attributes.insert("redacted_count".to_string(), json!(redacted_count));
    Ok(Some(AgentContextFact {
        fact_id: format!("AF{:03}", index),
        source: "config".to_string(),
        kind: "config_snapshot".to_string(),
        window_id: evidence.primary_window.window_id.clone(),
        statement: format!(
            "Config snapshot has {} line(s) with {} redaction marker(s)",
            line_count, redacted_count
        ),
        raw_ref,
        attributes,
        data_quality: evidence.data_quality.clone(),
    }))
}

fn derive_service_state_fact(
    path: &Path,
    evidence: &EvidenceIndex,
    index: usize,
    raw_refs: &mut BTreeMap<String, String>,
) -> AdcResult<Option<AgentContextFact>> {
    if !path.is_file() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read service state artifact {}: {err}",
            path.display()
        ))
    })?;
    let value = serde_json::from_str::<Value>(&contents).unwrap_or_else(|_| json!({}));
    let service = value
        .get("service")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let availability = value
        .get("availability")
        .and_then(Value::as_str)
        .unwrap_or("available");
    let active_state = value
        .get("active_state")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let sub_state = value
        .get("sub_state")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let raw_ref = "artifact://raw/service_state.json".to_string();
    raw_refs.insert("service_state".to_string(), raw_ref.clone());
    let mut attributes = BTreeMap::new();
    attributes.insert("service".to_string(), json!(service));
    attributes.insert("availability".to_string(), json!(availability));
    attributes.insert("active_state".to_string(), json!(active_state));
    attributes.insert("sub_state".to_string(), json!(sub_state));
    Ok(Some(AgentContextFact {
        fact_id: format!("AF{:03}", index),
        source: "service_state".to_string(),
        kind: "service_state".to_string(),
        window_id: evidence.primary_window.window_id.clone(),
        statement: format!("Service {service} state is {active_state}/{sub_state}"),
        raw_ref,
        attributes,
        data_quality: evidence.data_quality.clone(),
    }))
}

struct RuntimeFactSpec<'a> {
    source: &'a str,
    kind: &'a str,
    raw_key: &'a str,
    raw_ref: &'a str,
    count_key: &'a str,
    statement_label: &'a str,
}

fn derive_runtime_json_fact(
    path: &Path,
    evidence: &EvidenceIndex,
    index: usize,
    raw_refs: &mut BTreeMap<String, String>,
    spec: RuntimeFactSpec<'_>,
) -> AdcResult<Option<AgentContextFact>> {
    if !path.is_file() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read runtime artifact {}: {err}",
            path.display()
        ))
    })?;
    let value = serde_json::from_str::<Value>(&contents).unwrap_or_else(|_| json!({}));
    let count = value
        .get(spec.count_key)
        .and_then(Value::as_u64)
        .unwrap_or(0);
    raw_refs.insert(spec.raw_key.to_string(), spec.raw_ref.to_string());
    let mut attributes = BTreeMap::new();
    attributes.insert(spec.count_key.to_string(), json!(count));
    Ok(Some(AgentContextFact {
        fact_id: format!("AF{:03}", index),
        source: spec.source.to_string(),
        kind: spec.kind.to_string(),
        window_id: evidence.primary_window.window_id.clone(),
        statement: format!("{} recorded {} item(s)", spec.statement_label, count),
        raw_ref: spec.raw_ref.to_string(),
        attributes,
        data_quality: evidence.data_quality.clone(),
    }))
}

fn derive_fd_thread_snapshot_fact(
    path: &Path,
    evidence: &EvidenceIndex,
    index: usize,
    raw_refs: &mut BTreeMap<String, String>,
) -> AdcResult<Option<AgentContextFact>> {
    if !path.is_file() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read fd/thread artifact {}: {err}",
            path.display()
        ))
    })?;
    let value = serde_json::from_str::<Value>(&contents).unwrap_or_else(|_| json!({}));
    let process_count = value
        .get("process_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let accessible_process_count = value
        .get("accessible_process_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let inaccessible_process_count = value
        .get("inaccessible_process_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total_fd_count = value
        .get("total_fd_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total_thread_count = value
        .get("total_thread_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let root_required = value
        .get("root_required")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let raw_ref = "artifact://raw/fd_thread_snapshot.json".to_string();
    raw_refs.insert("fd_thread_snapshot".to_string(), raw_ref.clone());
    let mut attributes = BTreeMap::new();
    attributes.insert("process_count".to_string(), json!(process_count));
    attributes.insert(
        "accessible_process_count".to_string(),
        json!(accessible_process_count),
    );
    attributes.insert(
        "inaccessible_process_count".to_string(),
        json!(inaccessible_process_count),
    );
    attributes.insert("total_fd_count".to_string(), json!(total_fd_count));
    attributes.insert("total_thread_count".to_string(), json!(total_thread_count));
    attributes.insert("root_required".to_string(), json!(root_required));
    Ok(Some(AgentContextFact {
        fact_id: format!("AF{:03}", index),
        source: "fd_thread".to_string(),
        kind: "fd_thread_snapshot".to_string(),
        window_id: evidence.primary_window.window_id.clone(),
        statement: format!(
            "FD/thread snapshot saw {total_fd_count} fd(s) and {total_thread_count} thread(s) across {accessible_process_count}/{process_count} readable process(es)"
        ),
        raw_ref,
        attributes,
        data_quality: evidence.data_quality.clone(),
    }))
}

fn derive_kernel_probe_snapshot_fact(
    path: &Path,
    evidence: &EvidenceIndex,
    index: usize,
    raw_refs: &mut BTreeMap<String, String>,
) -> AdcResult<Option<AgentContextFact>> {
    if !path.is_file() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read kernel probe artifact {}: {err}",
            path.display()
        ))
    })?;
    let value = serde_json::from_str::<Value>(&contents).unwrap_or_else(|_| json!({}));
    let ftrace_available = value
        .get("ftrace_available")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let perf_available = value
        .get("perf_available")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let kprobe_available = value
        .get("kprobe_available")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let ko_loaded = value
        .get("ko_loaded")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let ko_source_present = value
        .get("ko_source_present")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let root_required = value
        .get("root_required")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let data_quality = value
        .get("data_quality")
        .cloned()
        .and_then(|value| serde_json::from_value::<DataQuality>(value).ok())
        .unwrap_or_else(|| evidence.data_quality.clone());
    let raw_ref = "artifact://raw/kernel_probe_snapshot.json".to_string();
    raw_refs.insert("kernel_probe_snapshot".to_string(), raw_ref.clone());
    let mut attributes = BTreeMap::new();
    attributes.insert("ftrace_available".to_string(), json!(ftrace_available));
    attributes.insert("perf_available".to_string(), json!(perf_available));
    attributes.insert("kprobe_available".to_string(), json!(kprobe_available));
    attributes.insert("ko_loaded".to_string(), json!(ko_loaded));
    attributes.insert("ko_source_present".to_string(), json!(ko_source_present));
    attributes.insert("root_required".to_string(), json!(root_required));
    Ok(Some(AgentContextFact {
        fact_id: format!("AF{:03}", index),
        source: "kernel_probe".to_string(),
        kind: "kernel_optional_probe_snapshot".to_string(),
        window_id: evidence.primary_window.window_id.clone(),
        statement: format!(
            "Kernel optional probes: ftrace={ftrace_available} perf={perf_available} kprobe={kprobe_available} ko_loaded={ko_loaded}"
        ),
        raw_ref,
        attributes,
        data_quality,
    }))
}

#[derive(Debug, Clone)]
struct SamplePoint {
    time_mono_ns: Option<u64>,
    sample: Value,
    coverage_mode: &'static str,
}

fn derive_cpu_fact(
    evidence: &EvidenceIndex,
    samples: &[SamplePoint],
    index: usize,
) -> Option<AgentContextFact> {
    let first = samples.first()?;
    let last = samples.last()?;
    let first_total = sample_u64(&first.sample, "total_jiffies")?;
    let first_idle = sample_u64(&first.sample, "idle_jiffies")?;
    let last_total = sample_u64(&last.sample, "total_jiffies")?;
    let last_idle = sample_u64(&last.sample, "idle_jiffies")?;
    let total_delta = last_total.checked_sub(first_total)?;
    if total_delta == 0 {
        return None;
    }
    let idle_delta = last_idle.saturating_sub(first_idle);
    let busy_delta = total_delta.saturating_sub(idle_delta);
    let busy_percent = (busy_delta as f64 / total_delta as f64) * 100.0;
    let mut attributes = BTreeMap::new();
    attributes.insert("sample_count".to_string(), json!(samples.len()));
    attributes.insert("busy_percent_avg".to_string(), json!(round1(busy_percent)));
    attributes.insert("total_jiffies_delta".to_string(), json!(total_delta));
    attributes.insert("idle_jiffies_delta".to_string(), json!(idle_delta));
    add_coverage_attributes(&mut attributes, samples);
    Some(AgentContextFact {
        fact_id: format!("AF{:03}", index),
        source: "cpu".to_string(),
        kind: "cpu_busy_percent".to_string(),
        window_id: evidence.primary_window.window_id.clone(),
        statement: format!(
            "CPU busy averaged {:.1}% across {} sample(s)",
            busy_percent,
            samples.len()
        ),
        raw_ref: raw_ref(evidence, "cpu"),
        attributes,
        data_quality: evidence.data_quality.clone(),
    })
}

fn derive_memory_fact(
    evidence: &EvidenceIndex,
    samples: &[SamplePoint],
    index: usize,
) -> Option<AgentContextFact> {
    let first = samples.first()?;
    let last = samples.last()?;
    let first_available = sample_u64(&first.sample, "mem_available_kb")?;
    let last_available = sample_u64(&last.sample, "mem_available_kb")?;
    let total = sample_u64(&last.sample, "mem_total_kb").unwrap_or(0);
    let delta = last_available as i128 - first_available as i128;
    let mut attributes = BTreeMap::new();
    attributes.insert("sample_count".to_string(), json!(samples.len()));
    attributes.insert("mem_available_kb_start".to_string(), json!(first_available));
    attributes.insert("mem_available_kb_end".to_string(), json!(last_available));
    attributes.insert("mem_available_kb_delta".to_string(), json!(delta));
    add_coverage_attributes(&mut attributes, samples);
    if total > 0 {
        attributes.insert(
            "mem_available_percent_end".to_string(),
            json!(round1((last_available as f64 / total as f64) * 100.0)),
        );
    }
    Some(AgentContextFact {
        fact_id: format!("AF{:03}", index),
        source: "memory".to_string(),
        kind: "memory_available".to_string(),
        window_id: evidence.primary_window.window_id.clone(),
        statement: format!(
            "Memory available changed by {} KiB across {} sample(s)",
            delta,
            samples.len()
        ),
        raw_ref: raw_ref(evidence, "memory"),
        attributes,
        data_quality: evidence.data_quality.clone(),
    })
}

fn derive_network_fact(
    evidence: &EvidenceIndex,
    samples: &[SamplePoint],
    index: usize,
) -> Option<AgentContextFact> {
    let first = samples.first()?;
    let last = samples.last()?;
    let (first_rx, first_tx) = network_totals(&first.sample)?;
    let (last_rx, last_tx) = network_totals(&last.sample)?;
    let rx_delta = last_rx.saturating_sub(first_rx);
    let tx_delta = last_tx.saturating_sub(first_tx);
    let mut attributes = BTreeMap::new();
    attributes.insert("sample_count".to_string(), json!(samples.len()));
    attributes.insert("rx_bytes_delta".to_string(), json!(rx_delta));
    attributes.insert("tx_bytes_delta".to_string(), json!(tx_delta));
    add_coverage_attributes(&mut attributes, samples);
    Some(AgentContextFact {
        fact_id: format!("AF{:03}", index),
        source: "network".to_string(),
        kind: "network_bytes".to_string(),
        window_id: evidence.primary_window.window_id.clone(),
        statement: format!(
            "Network counters changed by rx={} bytes tx={} bytes across {} sample(s)",
            rx_delta,
            tx_delta,
            samples.len()
        ),
        raw_ref: raw_ref(evidence, "network"),
        attributes,
        data_quality: evidence.data_quality.clone(),
    })
}

fn samples_for_source(
    artifact_root: &Path,
    run_id: &str,
    events: &[EventEnvelope],
    source: &str,
) -> AdcResult<Vec<SamplePoint>> {
    let raw_samples = read_raw_sample_series(artifact_root, run_id, source)?;
    if !raw_samples.is_empty() {
        return Ok(raw_samples);
    }
    Ok(events
        .iter()
        .filter(|event| event.source == source)
        .filter_map(|event| event.payload.get("sample"))
        .cloned()
        .map(|sample| SamplePoint {
            time_mono_ns: None,
            sample,
            coverage_mode: "timeline_prefix",
        })
        .collect())
}

fn read_raw_sample_series(
    artifact_root: &Path,
    run_id: &str,
    source: &str,
) -> AdcResult<Vec<SamplePoint>> {
    let path = run_dir(artifact_root, run_id)
        .join("raw")
        .join(format!("{source}.jsonl"));
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let contents = fs::read_to_string(&path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read raw {source} series {}: {err}",
            path.display()
        ))
    })?;
    let mut samples = Vec::new();
    for (line_index, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str::<Value>(line).map_err(|err| {
            AdcError::Artifact(format!(
                "failed to parse raw {source} series {} line {}: {err}",
                path.display(),
                line_index + 1
            ))
        })?;
        let Some(sample) = value.get("sample").cloned() else {
            continue;
        };
        samples.push(SamplePoint {
            time_mono_ns: value.get("time_mono_ns").and_then(Value::as_u64),
            sample,
            coverage_mode: "raw_series_full",
        });
    }
    Ok(samples)
}

fn add_coverage_attributes(attributes: &mut BTreeMap<String, Value>, samples: &[SamplePoint]) {
    if let Some(first) = samples.first() {
        attributes.insert("coverage_mode".to_string(), json!(first.coverage_mode));
    }
    if let Some(start) = samples.first().and_then(|sample| sample.time_mono_ns) {
        attributes.insert("coverage_start_mono_ns".to_string(), json!(start));
    }
    if let Some(end) = samples.last().and_then(|sample| sample.time_mono_ns) {
        attributes.insert("coverage_end_mono_ns".to_string(), json!(end));
    }
}

fn sample_u64(sample: &Value, key: &str) -> Option<u64> {
    sample.get(key).and_then(Value::as_u64)
}

fn network_totals(sample: &Value) -> Option<(u64, u64)> {
    let interfaces = sample.get("interfaces")?.as_array()?;
    let mut rx = 0_u64;
    let mut tx = 0_u64;
    for interface in interfaces {
        rx = rx.saturating_add(
            interface
                .get("rx_bytes")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        );
        tx = tx.saturating_add(
            interface
                .get("tx_bytes")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        );
    }
    Some((rx, tx))
}

fn recommend_refs(
    evidence: &EvidenceIndex,
    derived_facts: &[AgentContextFact],
) -> Vec<AgentContextRef> {
    let mut refs = Vec::new();
    let mut ranked = derived_facts.iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        fact_salience_score(right)
            .cmp(&fact_salience_score(left))
            .then_with(|| left.fact_id.cmp(&right.fact_id))
    });
    for fact in ranked {
        if refs.iter().any(|reference: &AgentContextRef| {
            reference.raw_ref == fact.raw_ref || reference.label == fact.source
        }) {
            continue;
        }
        refs.push(AgentContextRef {
            label: fact.source.clone(),
            raw_ref: fact.raw_ref.clone(),
            reason: format!(
                "ranked by investigation salience {} for derived fact {}",
                fact_salience_score(fact),
                fact.kind
            ),
        });
        if refs.len() >= 3 {
            break;
        }
    }
    if refs.is_empty() {
        for (label, raw_ref) in evidence.raw_refs.iter().take(3) {
            refs.push(AgentContextRef {
                label: label.clone(),
                raw_ref: raw_ref.clone(),
                reason: "first bounded evidence ref".to_string(),
            });
        }
    }
    refs
}

struct RunRouteInput<'a> {
    run_id: &'a str,
    fleet_run_id: Option<&'a str>,
    target_id: Option<&'a str>,
    route_service_name: Option<&'a str>,
    derived_facts: &'a [AgentContextFact],
    recommended_refs: &'a [AgentContextRef],
    service_pack: Option<&'a ServiceInvestigationPack>,
    data_quality: &'a DataQuality,
    max_context_bytes: usize,
}

fn build_run_investigation_route(input: RunRouteInput<'_>) -> InvestigationRoute {
    let run_id = input.run_id;
    let derived_facts = input.derived_facts;
    let recommended_refs = input.recommended_refs;
    let service_pack = input.service_pack;
    let data_quality = input.data_quality;
    let service_name = input
        .route_service_name
        .or_else(|| service_name_from_facts(derived_facts))
        .map(str::to_string);
    let mut route_refs = recommended_refs
        .iter()
        .map(|reference| (reference.label.clone(), reference.raw_ref.clone()))
        .collect::<BTreeMap<_, _>>();
    if let Some(pack) = service_pack {
        for (label, raw_ref) in &pack.raw_refs {
            route_refs.insert(format!("service.{label}"), raw_ref.clone());
        }
    }
    let target_ids = input
        .target_id
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut steps = Vec::new();
    let service_state_ref = derived_facts
        .iter()
        .find(|fact| fact.kind == "service_state")
        .map(|fact| fact.raw_ref.clone())
        .or_else(|| {
            service_pack
                .and_then(|pack| pack.raw_refs.get("service_state"))
                .cloned()
        });
    if let Some(raw_ref) = service_state_ref {
        let mut refs = vec![AgentContextRef {
            label: "service_state".to_string(),
            raw_ref,
            reason: "route entrypoint for service availability and state".to_string(),
        }];
        if let Some(pack) = service_pack {
            for label in ["journal_leads", "process_summary", "port_summary"] {
                if let Some(raw_ref) = pack.raw_refs.get(label) {
                    refs.push(AgentContextRef {
                        label: format!("service.{label}"),
                        raw_ref: raw_ref.clone(),
                        reason: "bounded service investigation pack ref".to_string(),
                    });
                }
            }
        }
        steps.push(InvestigationStep {
            step_id: "IR001".to_string(),
            title: "Correlate service state with observed signals".to_string(),
            purpose: "Start from service availability, then compare log/domain/runtime evidence without widening to raw dumps.".to_string(),
            expected_answer: "A concise statement of service availability, sub-state, and whether bounded logs or domain markers describe the same time window.".to_string(),
            refs,
            branch_conditions: vec![
                RouteBranchCondition {
                    if_observed: "service availability is unavailable or state is unknown".to_string(),
                    next_step_id: "IR-DQ".to_string(),
                    reason: "unavailable service evidence changes the next action to data-quality repair before deeper inspection".to_string(),
                    predicate: Some(RouteConditionExpr::Any {
                        expressions: vec![
                            RouteConditionExpr::Eq {
                                fact_id: "service.availability".to_string(),
                                value: json!("unavailable"),
                            },
                            RouteConditionExpr::Eq {
                                fact_id: "service.availability".to_string(),
                                value: json!("unknown"),
                            },
                        ],
                    }),
                },
                RouteBranchCondition {
                    if_observed: "service is available and log/domain refs exist".to_string(),
                    next_step_id: "IR002".to_string(),
                    reason: "available service state can be correlated with bounded application/domain evidence".to_string(),
                    predicate: Some(RouteConditionExpr::Eq {
                        fact_id: "service.availability".to_string(),
                        value: json!("available"),
                    }),
                },
            ],
            stop_conditions: vec![
                "service state ref has been opened".to_string(),
                "service data_quality has been checked".to_string(),
            ],
            required_privilege: "none".to_string(),
            estimated_cost: "low".to_string(),
            target_ids: target_ids.clone(),
            cause_neutral: true,
        });
    }

    let signal_refs = route_refs_for_kinds(
        derived_facts,
        &[
            "log_error_slice",
            "domain_event_count",
            "journald_entry_count",
        ],
    );
    if !signal_refs.is_empty() {
        steps.push(InvestigationStep {
            step_id: "IR002".to_string(),
            title: "Open bounded log, journal, and domain markers".to_string(),
            purpose: "Identify operation names, timestamps, and domain markers that resource counters cannot provide.".to_string(),
            expected_answer: "A short list of bounded warning/error or domain markers with timestamps or line indexes.".to_string(),
            refs: signal_refs,
            branch_conditions: vec![
                RouteBranchCondition {
                    if_observed: "bounded log or domain markers name an operation".to_string(),
                    next_step_id: "IR003".to_string(),
                    reason: "named operations should be checked against runtime/resource windows".to_string(),
                    predicate: Some(RouteConditionExpr::Gte {
                        fact_id: "signal.signal_line_count".to_string(),
                        value: 1.0,
                    }),
                },
                RouteBranchCondition {
                    if_observed: "bounded log and domain refs are empty".to_string(),
                    next_step_id: "IR-DQ".to_string(),
                    reason: "missing signal refs should be treated as information debt, not absence of behavior".to_string(),
                    predicate: Some(RouteConditionExpr::Eq {
                        fact_id: "signal.has_signal_words".to_string(),
                        value: json!(false),
                    }),
                },
            ],
            stop_conditions: vec![
                "bounded signal refs have been opened".to_string(),
                "no full raw log dump was needed".to_string(),
            ],
            required_privilege: "none".to_string(),
            estimated_cost: "low".to_string(),
            target_ids: target_ids.clone(),
            cause_neutral: true,
        });
    }

    let runtime_refs = runtime_route_refs(recommended_refs, derived_facts);
    if !runtime_refs.is_empty() {
        steps.push(InvestigationStep {
            step_id: "IR003".to_string(),
            title: "Compare runtime counters in the primary window".to_string(),
            purpose: "Use bounded resource series after service and operation markers are known.".to_string(),
            expected_answer: "A compact resource-window observation such as CPU, memory, network, process, or thermal change with data_quality attached.".to_string(),
            refs: runtime_refs,
            branch_conditions: vec![
                RouteBranchCondition {
                    if_observed: "runtime series shows missing, dropped, or throttled data".to_string(),
                    next_step_id: "IR-DQ".to_string(),
                    reason: "data-quality gaps must be handled before widening collection".to_string(),
                    predicate: None,
                },
                RouteBranchCondition {
                    if_observed: "runtime refs are sufficient for the next bounded comparison".to_string(),
                    next_step_id: "IR-END".to_string(),
                    reason: "the Agent has enough compact evidence to decide the next ref to open".to_string(),
                    predicate: None,
                },
            ],
            stop_conditions: vec![
                "primary runtime refs have been reviewed".to_string(),
                "route remains within bounded refs".to_string(),
            ],
            required_privilege: "none".to_string(),
            estimated_cost: "low".to_string(),
            target_ids: target_ids.clone(),
            cause_neutral: true,
        });
    }
    if !data_quality.missing.is_empty() {
        steps.push(data_quality_route_step(
            "IR-DQ",
            "Review data-quality gaps before widening collection",
            data_quality,
            target_ids.clone(),
        ));
    }
    if steps.is_empty() {
        steps.push(fallback_route_step(recommended_refs, target_ids));
    }
    renumber_route_steps(&mut steps);
    let mut route_summary = vec![format!(
        "Route fuses {} evidence with bounded refs and data_quality.",
        route_signal_summary(derived_facts)
    )];
    if let Some(pack) = service_pack {
        route_summary.push(format!(
            "Service pack {} contributes {} journal lead(s) and {} data-quality gap(s).",
            pack.service_name,
            pack.journal_summary.returned_lead_count,
            pack.data_quality.missing.len()
        ));
    }
    route_from_parts(RouteBuildParts {
        scope: "run",
        route_id: format!("route-{run_id}"),
        run_id: Some(run_id.to_string()),
        fleet_run_id: input.fleet_run_id.map(str::to_string),
        service_name,
        route_summary,
        steps,
        data_quality: data_quality.clone(),
        raw_refs: route_refs,
        max_context_bytes: input.max_context_bytes,
    })
}

fn build_fleet_investigation_route(
    fleet_run_id: &str,
    target_summaries: &[FleetTargetContextSummary],
    recommended_refs: &[AgentContextRef],
    service_result: Option<&FleetServiceInvestigationResult>,
    data_quality: &DataQuality,
    max_context_bytes: usize,
) -> InvestigationRoute {
    let mut route_refs = recommended_refs
        .iter()
        .map(|reference| (reference.label.clone(), reference.raw_ref.clone()))
        .collect::<BTreeMap<_, _>>();
    let target_ids = target_summaries
        .iter()
        .map(|target| target.target_id.clone())
        .collect::<Vec<_>>();
    let mut route_summary = vec![format!(
        "Fleet route covers {} target(s), including {} captured and {} failed target(s).",
        target_summaries.len(),
        target_summaries
            .iter()
            .filter(|target| target.status == "captured")
            .count(),
        target_summaries
            .iter()
            .filter(|target| target.status != "captured")
            .count()
    )];
    let mut steps = vec![InvestigationStep {
        step_id: "IR001".to_string(),
        title: "Read the fleet target matrix and failure groups".to_string(),
        purpose: "Separate available target evidence from unreachable, permission, or collector gaps before opening per-target refs.".to_string(),
        expected_answer: "A target-by-target availability summary with captured targets and degraded targets named.".to_string(),
        refs: recommended_refs.iter().take(3).cloned().collect(),
        branch_conditions: vec![
            RouteBranchCondition {
                if_observed: "one or more targets failed or have missing data_quality".to_string(),
                next_step_id: "IR-DQ".to_string(),
                reason: "partial success must be preserved before comparing target behavior".to_string(),
                predicate: None,
            },
            RouteBranchCondition {
                if_observed: "at least one captured target has evidence refs".to_string(),
                next_step_id: "IR002".to_string(),
                reason: "captured target refs can be inspected without blocking on failed targets".to_string(),
                predicate: None,
            },
        ],
        stop_conditions: vec![
            "captured and failed target sets are identified".to_string(),
            "target data_quality has been checked".to_string(),
        ],
        required_privilege: "none".to_string(),
        estimated_cost: "low".to_string(),
        target_ids: target_ids.clone(),
        cause_neutral: true,
    }];

    if let Some(service_result) = service_result {
        for (label, raw_ref) in &service_result.raw_refs {
            route_refs.insert(format!("fleet_service.{label}"), raw_ref.clone());
        }
        let semantic_diff_ref =
            format!("artifact://fleet_runs/{fleet_run_id}/fleet_semantic_diff.json");
        route_refs.insert("fleet_semantic_diff".to_string(), semantic_diff_ref.clone());
        route_summary.push(format!(
            "Fleet service pack for {} captured {} of {} target(s).",
            service_result.service_name, service_result.captured_count, service_result.target_count
        ));
        let mut service_refs = vec![AgentContextRef {
            label: "fleet_semantic_diff".to_string(),
            raw_ref: semantic_diff_ref,
            reason: "typed semantic fleet service diff ref".to_string(),
        }];
        service_refs.extend(service_result.raw_refs.iter().map(|(label, raw_ref)| {
            AgentContextRef {
                label: format!("fleet_service.{label}"),
                raw_ref: raw_ref.clone(),
                reason: "fleet service investigation ref".to_string(),
            }
        }));
        steps.push(InvestigationStep {
            step_id: "IR002".to_string(),
            title: "Compare service investigation packs across targets".to_string(),
            purpose: "Use per-target service state, journal summary, and data_quality to find where evidence differs without inferring why.".to_string(),
            expected_answer: "A compact comparison of service availability, journal lead counts, and target-specific gaps.".to_string(),
            refs: service_refs,
            branch_conditions: vec![
                RouteBranchCondition {
                    if_observed: "service state differs across targets".to_string(),
                    next_step_id: "IR003".to_string(),
                    reason: "target-local evidence refs should be opened for the differing target set".to_string(),
                    predicate: Some(RouteConditionExpr::Gt {
                        fact_id: "fleet.semantic_diff.different_field_count".to_string(),
                        value: 0.0,
                    }),
                },
                RouteBranchCondition {
                    if_observed: "service pack is partial or has failed targets".to_string(),
                    next_step_id: "IR-DQ".to_string(),
                    reason: "failed service targets remain useful information debt".to_string(),
                    predicate: Some(RouteConditionExpr::Gt {
                        fact_id: "fleet.semantic_diff.partial_field_count".to_string(),
                        value: 0.0,
                    }),
                },
            ],
            stop_conditions: vec![
                "service comparison is summarized per target".to_string(),
                "failed service targets are recorded as data_quality".to_string(),
            ],
            required_privilege: "none".to_string(),
            estimated_cost: "low".to_string(),
            target_ids: service_result
                .targets
                .iter()
                .map(|target| target.target_id.clone())
                .collect(),
            cause_neutral: true,
        });
    }

    let captured_refs = recommended_refs.iter().take(3).cloned().collect::<Vec<_>>();
    if !captured_refs.is_empty() {
        steps.push(InvestigationStep {
            step_id: "IR003".to_string(),
            title: "Open captured target evidence refs only as needed".to_string(),
            purpose: "Keep the fleet investigation bounded by following one target evidence ref at a time.".to_string(),
            expected_answer: "A per-target evidence-window summary for the first captured target selected by the Agent.".to_string(),
            refs: captured_refs,
            branch_conditions: vec![RouteBranchCondition {
                if_observed: "target evidence has missing artifacts or stale data".to_string(),
                next_step_id: "IR-DQ".to_string(),
                reason: "artifact gaps should be handled as target-scoped data_quality".to_string(),
                predicate: None,
            }],
            stop_conditions: vec!["one captured target ref has been opened".to_string()],
            required_privilege: "none".to_string(),
            estimated_cost: "low".to_string(),
            target_ids: target_ids.clone(),
            cause_neutral: true,
        });
    }
    if !data_quality.missing.is_empty()
        || target_summaries
            .iter()
            .any(|target| !target.data_quality.missing.is_empty())
    {
        steps.push(data_quality_route_step(
            "IR-DQ",
            "Review fleet data-quality gaps",
            data_quality,
            target_ids,
        ));
    }
    renumber_route_steps(&mut steps);
    route_from_parts(RouteBuildParts {
        scope: "fleet",
        route_id: format!("route-{fleet_run_id}"),
        run_id: None,
        fleet_run_id: Some(fleet_run_id.to_string()),
        service_name: service_result.map(|result| result.service_name.clone()),
        route_summary,
        steps,
        data_quality: data_quality.clone(),
        raw_refs: route_refs,
        max_context_bytes,
    })
}

struct RouteBuildParts {
    scope: &'static str,
    route_id: String,
    run_id: Option<String>,
    fleet_run_id: Option<String>,
    service_name: Option<String>,
    route_summary: Vec<String>,
    steps: Vec<InvestigationStep>,
    data_quality: DataQuality,
    raw_refs: BTreeMap<String, String>,
    max_context_bytes: usize,
}

fn route_from_parts(parts: RouteBuildParts) -> InvestigationRoute {
    let returned_step_count = parts.steps.len();
    let raw_ref_count = parts.raw_refs.len();
    InvestigationRoute {
        schema_version: "obs.investigation_route.v1".to_string(),
        route_id: parts.route_id,
        scope: parts.scope.to_string(),
        run_id: parts.run_id,
        fleet_run_id: parts.fleet_run_id,
        service_name: parts.service_name,
        route_summary: parts.route_summary,
        steps: parts.steps,
        data_quality: parts.data_quality,
        raw_refs: parts.raw_refs,
        budget: RouteBudget {
            max_context_bytes: parts.max_context_bytes,
            returned_step_count,
            omitted_step_count: 0,
            raw_ref_count,
        },
    }
}

fn route_refs_for_kinds(
    derived_facts: &[AgentContextFact],
    kinds: &[&str],
) -> Vec<AgentContextRef> {
    let mut refs = Vec::new();
    for kind in kinds {
        if let Some(fact) = derived_facts.iter().find(|fact| fact.kind == *kind) {
            refs.push(AgentContextRef {
                label: fact.source.clone(),
                raw_ref: fact.raw_ref.clone(),
                reason: format!("route ref for {}", fact.kind),
            });
        }
    }
    refs
}

fn runtime_route_refs(
    recommended_refs: &[AgentContextRef],
    derived_facts: &[AgentContextFact],
) -> Vec<AgentContextRef> {
    let mut refs = recommended_refs
        .iter()
        .filter(|reference| {
            let text = format!("{} {}", reference.label, reference.raw_ref);
            ["cpu", "memory", "network", "process", "thermal"]
                .iter()
                .any(|needle| text.contains(needle))
        })
        .cloned()
        .collect::<Vec<_>>();
    for kind in [
        "cpu_busy_percent",
        "memory_available",
        "network_bytes",
        "process_snapshot",
        "io_snapshot",
        "thermal_snapshot",
    ] {
        if refs.len() >= 4 {
            break;
        }
        let Some(fact) = derived_facts.iter().find(|fact| fact.kind == kind) else {
            continue;
        };
        if refs
            .iter()
            .any(|reference| reference.raw_ref == fact.raw_ref)
        {
            continue;
        }
        refs.push(AgentContextRef {
            label: fact.source.clone(),
            raw_ref: fact.raw_ref.clone(),
            reason: format!("runtime route ref for {}", fact.kind),
        });
    }
    refs.truncate(4);
    refs
}

fn fallback_route_step(
    recommended_refs: &[AgentContextRef],
    target_ids: Vec<String>,
) -> InvestigationStep {
    InvestigationStep {
        step_id: "IR001".to_string(),
        title: "Open the highest-salience bounded ref".to_string(),
        purpose: "Start from the smallest available evidence ref when no richer route inputs exist."
            .to_string(),
        expected_answer: "A compact note describing what the bounded ref contains and whether data_quality changes interpretation.".to_string(),
        refs: recommended_refs.iter().take(1).cloned().collect(),
        branch_conditions: vec![RouteBranchCondition {
            if_observed: "the bounded ref is empty or unavailable".to_string(),
            next_step_id: "IR-DQ".to_string(),
            reason: "missing primary evidence should become information debt".to_string(),
            predicate: None,
        }],
        stop_conditions: vec!["one bounded ref has been opened".to_string()],
        required_privilege: "none".to_string(),
        estimated_cost: "low".to_string(),
        target_ids,
        cause_neutral: true,
    }
}

fn data_quality_route_step(
    step_id: &str,
    title: &str,
    data_quality: &DataQuality,
    target_ids: Vec<String>,
) -> InvestigationStep {
    InvestigationStep {
        step_id: step_id.to_string(),
        title: title.to_string(),
        purpose: "Make missing, dropped, throttled, or unavailable evidence explicit before broadening observation.".to_string(),
        expected_answer: format!(
            "{} missing item(s), dropped={}, throttled={}, truncated={}",
            data_quality.missing.len(),
            data_quality.dropped,
            data_quality.throttled,
            data_quality.truncated
        ),
        refs: Vec::new(),
        branch_conditions: vec![RouteBranchCondition {
            if_observed: "data_quality gaps block the next bounded ref".to_string(),
            next_step_id: "IR-END".to_string(),
            reason: "the route should stop and repair access or collector setup before widening"
                .to_string(),
            predicate: None,
        }],
        stop_conditions: vec!["data_quality has been recorded as investigation debt".to_string()],
        required_privilege: "none".to_string(),
        estimated_cost: "low".to_string(),
        target_ids,
        cause_neutral: true,
    }
}

fn renumber_route_steps(steps: &mut [InvestigationStep]) {
    let mut index = 1;
    for step in steps {
        if step.step_id == "IR-DQ" {
            continue;
        }
        step.step_id = format!("IR{index:03}");
        index += 1;
    }
}

fn route_signal_summary(derived_facts: &[AgentContextFact]) -> String {
    let mut signals = Vec::new();
    if derived_facts
        .iter()
        .any(|fact| fact.kind == "service_state")
    {
        signals.push("service");
    }
    if derived_facts
        .iter()
        .any(|fact| fact.kind == "log_error_slice")
    {
        signals.push("log");
    }
    if derived_facts
        .iter()
        .any(|fact| fact.kind == "domain_event_count")
    {
        signals.push("domain");
    }
    if derived_facts.iter().any(|fact| {
        matches!(
            fact.kind.as_str(),
            "cpu_busy_percent" | "memory_available_kb" | "network_delta_bytes"
        )
    }) {
        signals.push("runtime");
    }
    if signals.is_empty() {
        "available".to_string()
    } else {
        signals.join("/")
    }
}

fn service_name_from_facts(derived_facts: &[AgentContextFact]) -> Option<&str> {
    derived_facts.iter().find_map(|fact| {
        fact.attributes
            .get("service")
            .and_then(Value::as_str)
            .filter(|service| *service != "unknown")
    })
}

fn read_service_investigation_pack_optional(
    artifact_root: &Path,
    service_name: &str,
) -> Option<ServiceInvestigationPack> {
    let path = artifact_root
        .join("service_investigations")
        .join(service_name)
        .join("service_investigation.json");
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn read_fleet_service_investigation_optional(
    artifact_root: &Path,
    fleet_run_id: &str,
) -> Option<FleetServiceInvestigationResult> {
    let path = artifact_root
        .join("fleet_runs")
        .join(fleet_run_id)
        .join("service_investigation.json");
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn build_agent_playbook(
    scope: &str,
    run_id: Option<&str>,
    fleet_run_id: Option<&str>,
    derived_facts: &[AgentContextFact],
    recommended_refs: &[AgentContextRef],
    data_quality: &DataQuality,
) -> AgentPlaybook {
    let mut steps = Vec::new();
    let service_name = derived_facts.iter().find_map(|fact| {
        fact.attributes
            .get("service")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    push_playbook_step_for_kind(
        &mut steps,
        derived_facts,
        "service_state",
        "Open service state and process evidence",
        "Service state is a high-signal starting point for correlating runtime status with logs and resource observations.",
        vec!["service state".to_string(), "process summary".to_string(), "data_quality".to_string()],
        "service evidence is available or explicitly unavailable",
    );
    push_playbook_step_for_kind(
        &mut steps,
        derived_facts,
        "log_error_slice",
        "Inspect bounded application log signals",
        "Log warning/error lines provide the fastest cause-neutral route to the failing operation without dumping full logs.",
        vec!["bounded log lines".to_string(), "error/warning count".to_string()],
        "the relevant log ref has been opened or the log slice is empty",
    );
    push_playbook_step_for_kind(
        &mut steps,
        derived_facts,
        "journald_entry_count",
        "Check journald warning and error entries",
        "System journal entries can confirm service-level failures and stale evidence boundaries.",
        vec![
            "journald leads".to_string(),
            "journal recency metadata".to_string(),
        ],
        "journal leads are reviewed with their time basis",
    );
    push_playbook_step_for_kind(
        &mut steps,
        derived_facts,
        "domain_event_count",
        "Inspect domain event markers",
        "Domain events name the application-specific operation that resource counters cannot explain by themselves.",
        vec!["domain events".to_string(), "event type distribution".to_string()],
        "domain event refs are opened or no domain markers exist",
    );
    if steps.len() < 2 {
        for reference in recommended_refs {
            if steps.len() >= 2 {
                break;
            }
            steps.push(AgentPlaybookStep {
                step_id: format!("AP{:03}", steps.len() + 1),
                title: format!("Open {} evidence ref", reference.label),
                reason: "Keeps investigation bounded by opening one high-salience ref before widening collection.".to_string(),
                expected_evidence: vec![reference.label.clone()],
                required_privilege: "none".to_string(),
                estimated_cost: "low".to_string(),
                refs: vec![reference.clone()],
                stop_condition: "the ref has been opened and data_quality was checked".to_string(),
                cause_neutral: true,
            });
        }
    }
    steps.truncate(5);
    for (index, step) in steps.iter_mut().enumerate() {
        step.step_id = format!("AP{:03}", index + 1);
    }
    AgentPlaybook {
        schema_version: "obs.agent_playbook.v1".to_string(),
        playbook_id: run_id
            .map(|id| format!("playbook-{id}"))
            .or_else(|| fleet_run_id.map(|id| format!("playbook-{id}")))
            .unwrap_or_else(|| "playbook-context".to_string()),
        scope: scope.to_string(),
        run_id: run_id.map(str::to_string),
        fleet_run_id: fleet_run_id.map(str::to_string),
        service_name,
        steps,
        data_quality: data_quality.clone(),
        raw_refs: recommended_refs
            .iter()
            .map(|reference| (reference.label.clone(), reference.raw_ref.clone()))
            .collect(),
    }
}

fn push_playbook_step_for_kind(
    steps: &mut Vec<AgentPlaybookStep>,
    derived_facts: &[AgentContextFact],
    kind: &str,
    title: &str,
    reason: &str,
    expected_evidence: Vec<String>,
    stop_condition: &str,
) {
    if steps.len() >= 5 {
        return;
    }
    let Some(fact) = derived_facts.iter().find(|fact| fact.kind == kind) else {
        return;
    };
    steps.push(AgentPlaybookStep {
        step_id: String::new(),
        title: title.to_string(),
        reason: reason.to_string(),
        expected_evidence,
        required_privilege: "none".to_string(),
        estimated_cost: "low".to_string(),
        refs: vec![AgentContextRef {
            label: fact.source.clone(),
            raw_ref: fact.raw_ref.clone(),
            reason: format!("playbook step from salient fact {}", fact.kind),
        }],
        stop_condition: stop_condition.to_string(),
        cause_neutral: true,
    });
}

fn sort_facts_by_salience(facts: &mut [AgentContextFact]) {
    facts.sort_by(|left, right| {
        fact_salience_score(right)
            .cmp(&fact_salience_score(left))
            .then_with(|| left.fact_id.cmp(&right.fact_id))
    });
}

fn fact_salience_score(fact: &AgentContextFact) -> i64 {
    let missing_bonus = if fact.data_quality.missing.is_empty() {
        0
    } else {
        20
    };
    match fact.kind.as_str() {
        "service_state" => {
            let availability = fact
                .attributes
                .get("availability")
                .and_then(Value::as_str)
                .unwrap_or("available");
            let active_state = fact
                .attributes
                .get("active_state")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            if availability == "unavailable" {
                35 + missing_bonus
            } else if active_state == "active" {
                60 + missing_bonus
            } else {
                100 + missing_bonus
            }
        }
        "log_error_slice" => {
            let signal_lines = fact
                .attributes
                .get("signal_line_count")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            80 + (signal_lines.min(10) as i64 * 2) + missing_bonus
        }
        "journald_entry_count" => {
            let warning_or_error_count = fact
                .attributes
                .get("warning_or_error_count")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            70 + (warning_or_error_count.min(10) as i64 * 2) + missing_bonus
        }
        "domain_event_count" => 65 + missing_bonus,
        "perfetto_event_count" => 55 + missing_bonus,
        "kernel_optional_probe_snapshot" => 50 + missing_bonus,
        "fd_thread_snapshot" => 45 + missing_bonus,
        "config_snapshot" => 40 + missing_bonus,
        "cpu_busy_percent" => 30 + missing_bonus,
        "memory_available" => 25 + missing_bonus,
        "network_bytes" => 20 + missing_bonus,
        _ => 10 + missing_bonus,
    }
}

fn remediation_hint_for(description: &str) -> String {
    let lower = description.to_ascii_lowercase();
    if lower.contains("permission_denied") || lower.contains("permission denied") {
        "run fleet preflight, verify SSH BatchMode credentials, remote PATH, and target state directory".to_string()
    } else if lower.contains("unsupported") || lower.contains("not supported") {
        "unsupported transport: update inventory transport or add a supported adapter before rerun"
            .to_string()
    } else if lower.contains("collector") {
        "check target capability/profile and rerun with a supported low-cost probe".to_string()
    } else if lower.contains("truncated") {
        "open the explicit raw ref with a bounded raw-slice request".to_string()
    } else {
        "treat this source as unavailable and prefer corroborating evidence from other refs"
            .to_string()
    }
}

fn raw_ref(evidence: &EvidenceIndex, source: &str) -> String {
    evidence
        .raw_refs
        .get(source)
        .cloned()
        .unwrap_or_else(|| "artifact://timeline.jsonl".to_string())
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn otlp_metric_names(value: &Value) -> Vec<String> {
    value
        .get("resourceMetrics")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|resource_metric| {
            resource_metric
                .get("scopeMetrics")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .flat_map(|scope_metric| {
            scope_metric
                .get("metrics")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(|metric| {
            metric
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect()
}

fn run_dir(artifact_root: &Path, run_id: &str) -> PathBuf {
    artifact_root.join("runs").join(run_id)
}

fn read_fleet_evidence(artifact_root: &Path, fleet_run_id: &str) -> AdcResult<FleetEvidence> {
    let path = artifact_root
        .join("fleet_runs")
        .join(fleet_run_id)
        .join("fleet_evidence.yaml");
    let contents = fs::read_to_string(&path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read fleet evidence {}: {err}",
            path.display()
        ))
    })?;
    yaml_serde::from_str(&contents)
        .map_err(|err| AdcError::Artifact(format!("fleet evidence yaml parse failed: {err}")))
}

fn validate_segment(value: &str, label: &str) -> AdcResult<()> {
    if value.trim().is_empty() || value.contains('/') || value.contains('\\') {
        return Err(AdcError::Artifact(format!(
            "{label} must be a single relative path segment"
        )));
    }
    Ok(())
}
