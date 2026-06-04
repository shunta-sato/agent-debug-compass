use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    build_fleet_agent_context, build_run_agent_context, compile_route_for_symptom,
    extract_evidence_facts_from_ref, normalize_symptom, resolve_agent_ref,
    resolve_global_agent_ref, safe_probe_packs_for_missing_facts, start_investigation, AdcError,
    AdcResult, AgentContextRef, AgentRefResolution, CompiledInvestigationRoute, DataQuality,
    EvidenceFact, EvidenceGraph, FleetAgentContextRequest, HypothesisSet, InvestigationRoute,
    InvestigationStartRequest, ProbePlan, RouteCompileInput, SafeProbePack, SafetyPolicy,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymptomInvestigationRequest {
    pub run_id: Option<String>,
    pub fleet_run_id: Option<String>,
    pub service_name: Option<String>,
    pub inventory_path: Option<PathBuf>,
    pub symptom: String,
    pub max_journal_lines: Option<usize>,
    pub max_markdown_bytes: usize,
    pub max_context_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymptomContextPack {
    pub schema_version: String,
    pub context_id: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fleet_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_name: Option<String>,
    pub symptom: crate::NormalizedSymptom,
    pub target_summary: SymptomTargetSummary,
    pub agent_context: Value,
    pub investigation_route: InvestigationRoute,
    pub compiled_route: CompiledInvestigationRoute,
    pub facts: Vec<EvidenceFact>,
    pub missing_fact_ids: Vec<String>,
    pub recommended_refs: Vec<AgentContextRef>,
    pub next_safe_probes: Vec<SafeProbePack>,
    pub hypothesis_set: HypothesisSet,
    pub evidence_graph: EvidenceGraph,
    pub probe_plan: ProbePlan,
    pub safety_policy: SafetyPolicy,
    pub budget: SymptomContextBudget,
    pub raw_refs: BTreeMap<String, String>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymptomTargetSummary {
    pub target_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub captured_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_count: Option<usize>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymptomContextBudget {
    pub max_context_bytes: usize,
    pub returned_bytes: usize,
    pub fact_count: usize,
    pub missing_fact_count: usize,
    pub selected_pack_count: usize,
    pub truncated: bool,
}

pub fn investigate_bug(
    artifact_root: impl AsRef<Path>,
    request: SymptomInvestigationRequest,
) -> AdcResult<SymptomContextPack> {
    let artifact_root = artifact_root.as_ref();
    match (request.run_id.clone(), request.fleet_run_id.clone()) {
        (Some(_), Some(_)) => Err(AdcError::Artifact(
            "symptom investigation accepts only one of run_id or fleet_run_id".to_string(),
        )),
        (None, None) => Err(AdcError::Artifact(
            "symptom investigation requires run_id or fleet_run_id".to_string(),
        )),
        (Some(run_id), None) => investigate_run_bug(artifact_root, request, run_id),
        (None, Some(fleet_run_id)) => investigate_fleet_bug(artifact_root, request, fleet_run_id),
    }
}

fn investigate_run_bug(
    artifact_root: &Path,
    request: SymptomInvestigationRequest,
    run_id: String,
) -> AdcResult<SymptomContextPack> {
    let symptom = normalize_symptom(&request.symptom);
    let start_pack = start_investigation(
        artifact_root,
        InvestigationStartRequest {
            run_id: Some(run_id),
            fleet_run_id: None,
            service_name: request.service_name.clone(),
            inventory_path: None,
            max_journal_lines: request.max_journal_lines,
            max_markdown_bytes: request.max_markdown_bytes,
        },
    )?;
    let run_id = start_pack
        .run_id
        .clone()
        .ok_or_else(|| AdcError::Artifact("run investigation did not return run_id".to_string()))?;
    let context = build_run_agent_context(
        artifact_root,
        crate::AgentContextRequest {
            run_id: run_id.clone(),
            service_name: request.service_name.clone(),
            max_markdown_bytes: request.max_markdown_bytes,
        },
    )?;
    let target_id = context
        .target_id
        .clone()
        .unwrap_or_else(|| "local".to_string());
    let mut raw_refs = context.raw_refs.clone();
    raw_refs.extend(start_pack.raw_refs.clone());
    let (facts, ref_quality) = collect_facts_from_refs(
        artifact_root,
        Some(&run_id),
        None,
        Some(&target_id),
        &raw_refs,
    );
    let target_ids = vec![target_id.clone()];
    let compiled_route = compile_route_for_symptom(RouteCompileInput {
        symptom: symptom.clone(),
        available_facts: facts.clone(),
        max_selected_packs: 4,
        target_ids: target_ids.clone(),
    });
    let mut data_quality = context.data_quality.clone();
    merge_data_quality(&mut data_quality, &start_pack.data_quality);
    merge_data_quality(&mut data_quality, &compiled_route.data_quality);
    merge_data_quality(&mut data_quality, &ref_quality);
    let missing_fact_ids = compiled_route.missing_fact_ids.clone();
    let next_safe_probes = safe_probe_packs_for_missing_facts(&missing_fact_ids);
    let target_summary = SymptomTargetSummary {
        target_ids,
        captured_count: Some(1),
        failed_count: Some(0),
        data_quality: context.target_dossier.data_quality.clone(),
    };
    let agent_context = json!({
        "schema_version": context.schema_version,
        "context_id": context.context_id,
        "run_id": context.run_id,
        "target_id": context.target_id,
        "profile_id": context.profile_id,
        "derived_fact_count": context.derived_facts.len(),
        "recommended_refs": context.recommended_refs,
        "context_budget": context.context_budget,
        "full_context_ref": context.raw_refs.get("agent_context_json").cloned(),
    });
    let pack = build_pack(BuildPackInput {
        scope: "run",
        run_id: Some(run_id.clone()),
        fleet_run_id: None,
        service_name: request.service_name,
        symptom,
        target_summary,
        agent_context,
        investigation_route: start_pack.investigation_route,
        compiled_route,
        facts,
        missing_fact_ids,
        recommended_refs: context.recommended_refs,
        next_safe_probes,
        max_context_bytes: request.max_context_bytes,
        raw_refs,
        data_quality,
    })?;
    persist_symptom_context(artifact_root, "runs", &run_id, &pack)?;
    Ok(pack)
}

fn investigate_fleet_bug(
    artifact_root: &Path,
    request: SymptomInvestigationRequest,
    fleet_run_id: String,
) -> AdcResult<SymptomContextPack> {
    let symptom = normalize_symptom(&request.symptom);
    let start_pack = start_investigation(
        artifact_root,
        InvestigationStartRequest {
            run_id: None,
            fleet_run_id: Some(fleet_run_id),
            service_name: request.service_name.clone(),
            inventory_path: request.inventory_path.clone(),
            max_journal_lines: request.max_journal_lines,
            max_markdown_bytes: request.max_markdown_bytes,
        },
    )?;
    let fleet_run_id = start_pack.fleet_run_id.clone().ok_or_else(|| {
        AdcError::Artifact("fleet investigation did not return fleet_run_id".to_string())
    })?;
    let context = build_fleet_agent_context(
        artifact_root,
        FleetAgentContextRequest {
            fleet_run_id: fleet_run_id.clone(),
            max_markdown_bytes: request.max_markdown_bytes,
        },
    )?;
    let mut raw_refs = context.raw_refs.clone();
    raw_refs.extend(start_pack.raw_refs.clone());
    let (facts, ref_quality) =
        collect_facts_from_refs(artifact_root, None, Some(&fleet_run_id), None, &raw_refs);
    let target_ids = context
        .target_matrix
        .iter()
        .map(|target| target.target_id.clone())
        .collect::<Vec<_>>();
    let compiled_route = compile_route_for_symptom(RouteCompileInput {
        symptom: symptom.clone(),
        available_facts: facts.clone(),
        max_selected_packs: 4,
        target_ids: target_ids.clone(),
    });
    let mut data_quality = context.data_quality.clone();
    merge_data_quality(&mut data_quality, &start_pack.data_quality);
    merge_data_quality(&mut data_quality, &compiled_route.data_quality);
    merge_data_quality(&mut data_quality, &ref_quality);
    let missing_fact_ids = compiled_route.missing_fact_ids.clone();
    let next_safe_probes = safe_probe_packs_for_missing_facts(&missing_fact_ids);
    let target_summary = SymptomTargetSummary {
        target_ids,
        captured_count: Some(context.captured_count),
        failed_count: Some(context.failed_count),
        data_quality: context.data_quality.clone(),
    };
    let agent_context = json!({
        "schema_version": context.schema_version,
        "context_id": context.context_id,
        "fleet_run_id": context.fleet_run_id,
        "target_count": context.target_count,
        "captured_count": context.captured_count,
        "failed_count": context.failed_count,
        "target_matrix": context.target_matrix,
        "recommended_refs": context.recommended_refs,
        "context_budget": context.context_budget,
    });
    let pack = build_pack(BuildPackInput {
        scope: "fleet",
        run_id: None,
        fleet_run_id: Some(fleet_run_id.clone()),
        service_name: request.service_name,
        symptom,
        target_summary,
        agent_context,
        investigation_route: start_pack.investigation_route,
        compiled_route,
        facts,
        missing_fact_ids,
        recommended_refs: context.recommended_refs,
        next_safe_probes,
        max_context_bytes: request.max_context_bytes,
        raw_refs,
        data_quality,
    })?;
    persist_symptom_context(artifact_root, "fleet_runs", &fleet_run_id, &pack)?;
    Ok(pack)
}

struct BuildPackInput {
    scope: &'static str,
    run_id: Option<String>,
    fleet_run_id: Option<String>,
    service_name: Option<String>,
    symptom: crate::NormalizedSymptom,
    target_summary: SymptomTargetSummary,
    agent_context: Value,
    investigation_route: InvestigationRoute,
    compiled_route: CompiledInvestigationRoute,
    facts: Vec<EvidenceFact>,
    missing_fact_ids: Vec<String>,
    recommended_refs: Vec<AgentContextRef>,
    next_safe_probes: Vec<SafeProbePack>,
    max_context_bytes: usize,
    raw_refs: BTreeMap<String, String>,
    data_quality: DataQuality,
}

fn build_pack(input: BuildPackInput) -> AdcResult<SymptomContextPack> {
    let context_id = input
        .run_id
        .as_ref()
        .map(|run_id| format!("symptom-context-{run_id}"))
        .or_else(|| {
            input
                .fleet_run_id
                .as_ref()
                .map(|fleet_run_id| format!("symptom-context-{fleet_run_id}"))
        })
        .unwrap_or_else(|| "symptom-context".to_string());
    let contracts = crate::investigation_contracts_for(
        input.scope,
        input.run_id.as_deref(),
        input.fleet_run_id.as_deref(),
        &input.symptom,
        &input.compiled_route,
        &input.next_safe_probes,
        &input.data_quality,
    );
    let mut pack = SymptomContextPack {
        schema_version: "obs.symptom_context.v1".to_string(),
        context_id,
        scope: input.scope.to_string(),
        run_id: input.run_id,
        fleet_run_id: input.fleet_run_id,
        service_name: input.service_name,
        symptom: input.symptom,
        target_summary: input.target_summary,
        agent_context: input.agent_context,
        investigation_route: input.investigation_route,
        compiled_route: input.compiled_route,
        facts: input.facts,
        missing_fact_ids: input.missing_fact_ids,
        recommended_refs: input.recommended_refs,
        next_safe_probes: input.next_safe_probes,
        hypothesis_set: contracts.hypothesis_set,
        evidence_graph: contracts.evidence_graph,
        probe_plan: contracts.probe_plan,
        safety_policy: contracts.safety_policy,
        budget: SymptomContextBudget {
            max_context_bytes: input.max_context_bytes,
            returned_bytes: 0,
            fact_count: 0,
            missing_fact_count: 0,
            selected_pack_count: 0,
            truncated: false,
        },
        raw_refs: input.raw_refs,
        data_quality: input.data_quality,
    };
    pack.budget.fact_count = pack.facts.len();
    pack.budget.missing_fact_count = pack.missing_fact_ids.len();
    pack.budget.selected_pack_count = pack.compiled_route.selected_packs.len();
    enforce_symptom_budget(&mut pack)?;
    Ok(pack)
}

fn enforce_symptom_budget(pack: &mut SymptomContextPack) -> AdcResult<()> {
    let rendered = serde_json::to_vec(pack).map_err(|err| {
        AdcError::Artifact(format!("symptom context json serialization failed: {err}"))
    })?;
    pack.budget.returned_bytes = rendered.len();
    while pack.budget.returned_bytes > pack.budget.max_context_bytes && pack.facts.len() > 1 {
        pack.facts.pop();
        pack.budget.truncated = true;
        pack.budget.fact_count = pack.facts.len();
        let rendered = serde_json::to_vec(pack).map_err(|err| {
            AdcError::Artifact(format!("symptom context json serialization failed: {err}"))
        })?;
        pack.budget.returned_bytes = rendered.len();
    }
    if pack.budget.truncated {
        pack.data_quality.truncated = true;
        pack.data_quality
            .notes
            .push("symptom context facts were truncated to fit budget".to_string());
    }
    Ok(())
}

fn collect_facts_from_refs(
    artifact_root: &Path,
    run_id: Option<&str>,
    fleet_run_id: Option<&str>,
    target_id: Option<&str>,
    raw_refs: &BTreeMap<String, String>,
) -> (Vec<EvidenceFact>, DataQuality) {
    let mut facts = Vec::new();
    let mut data_quality = DataQuality {
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };
    let mut seen = BTreeSet::new();
    for (label, raw_ref) in raw_refs {
        let resolution = if raw_ref.starts_with("artifact://service_investigations/") {
            resolve_global_agent_ref(artifact_root, raw_ref, 1_000)
        } else if raw_ref.starts_with("artifact://fleet_runs/") {
            resolve_fleet_ref(artifact_root, fleet_run_id, raw_ref, 1_000)
        } else if let Some(run_id) = run_id {
            resolve_agent_ref(artifact_root, run_id, raw_ref, 1_000)
        } else {
            Err(AdcError::Artifact(format!(
                "ref {raw_ref} requires a run_id or supported fleet/global ref"
            )))
        };
        match resolution {
            Ok(resolution) => {
                let inferred_target_id = target_id
                    .map(str::to_string)
                    .or_else(|| target_id_from_label(label));
                for mut fact in extract_evidence_facts_from_ref(
                    label,
                    raw_ref,
                    &resolution.ref_kind,
                    &resolution.content_type,
                    &resolution.text,
                    &resolution.data_quality,
                ) {
                    fact.scope = if fleet_run_id.is_some() {
                        "fleet".to_string()
                    } else {
                        "run".to_string()
                    };
                    fact.target_id = inferred_target_id.clone();
                    let key = format!(
                        "{}|{}|{}",
                        fact.fact_id,
                        fact.source_ref,
                        fact.target_id.as_deref().unwrap_or("")
                    );
                    if seen.insert(key) {
                        facts.push(fact);
                    }
                }
                merge_data_quality(&mut data_quality, &resolution.data_quality);
            }
            Err(err) => {
                data_quality
                    .missing
                    .push(format!("fact ref unavailable: {raw_ref}: {err}"));
            }
        }
    }
    (facts, data_quality)
}

fn resolve_fleet_ref(
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
    let ref_kind = ref_kind(ref_uri).to_string();
    let content_type = content_type(ref_uri).to_string();
    let text = lines.join("\n");
    let artifact_trust = crate::classify_artifact_trust(
        ref_uri,
        crate::content_class_for_ref(&ref_kind, &content_type),
        &text,
        &data_quality,
    );
    Ok(AgentRefResolution {
        run_id: fleet_run_id.unwrap_or("fleet").to_string(),
        ref_uri: ref_uri.to_string(),
        ref_kind,
        content_type,
        returned_lines: lines.len(),
        total_lines: all_lines.len(),
        truncated,
        text,
        artifact_trust,
        data_quality,
    })
}

fn persist_symptom_context(
    artifact_root: &Path,
    scope_dir: &str,
    id: &str,
    pack: &SymptomContextPack,
) -> AdcResult<()> {
    validate_segment(id, "symptom context id")?;
    let dir = artifact_root.join(scope_dir).join(id);
    fs::create_dir_all(&dir)
        .map_err(|err| AdcError::Artifact(format!("failed to create {}: {err}", dir.display())))?;
    write_json(&dir.join("symptom_context.json"), pack)?;
    write_json(&dir.join("compiled_route.json"), &pack.compiled_route)?;
    write_json(
        &dir.join("route_compiler_decisions.json"),
        &json!({
            "schema_version": "obs.route_compiler_decisions.v1",
            "compiler_id": pack.compiled_route.compiler_id,
            "selected_packs": pack.compiled_route.selected_packs,
            "rejected_packs": pack.compiled_route.rejected_packs,
            "data_quality": pack.compiled_route.data_quality,
        }),
    )?;
    write_json(
        &dir.join("fact_gap_report.json"),
        &json!({
            "schema_version": "obs.fact_gap_report.v1",
            "missing_fact_ids": pack.missing_fact_ids,
            "next_safe_probes": pack.next_safe_probes,
            "data_quality": pack.data_quality,
        }),
    )?;
    write_json(&dir.join("hypothesis_set.json"), &pack.hypothesis_set)?;
    write_json(&dir.join("evidence_graph.json"), &pack.evidence_graph)?;
    write_json(&dir.join("probe_plan.json"), &pack.probe_plan)?;
    write_json(&dir.join("safety_policy.json"), &pack.safety_policy)
}

fn write_json(path: &Path, value: &impl Serialize) -> AdcResult<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|err| AdcError::Artifact(format!("json serialization failed: {err}")))?;
    fs::write(path, bytes)
        .map_err(|err| AdcError::Artifact(format!("failed to write {}: {err}", path.display())))
}

fn merge_data_quality(target: &mut DataQuality, source: &DataQuality) {
    target.dropped |= source.dropped;
    target.drop_count = target.drop_count.saturating_add(source.drop_count);
    target.throttled |= source.throttled;
    target.truncated |= source.truncated;
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
    if target.clock_confidence == "unknown" && source.clock_confidence != "unknown" {
        target.clock_confidence = source.clock_confidence.clone();
    }
}

fn target_id_from_label(label: &str) -> Option<String> {
    let (target_id, _) = label.split_once('.')?;
    if target_id.is_empty() {
        None
    } else {
        Some(target_id.to_string())
    }
}

fn ref_kind(ref_uri: &str) -> &str {
    if ref_uri.ends_with(".json") || ref_uri.ends_with(".yaml") || ref_uri.ends_with(".yml") {
        "summary"
    } else if ref_uri.ends_with(".jsonl") {
        "raw"
    } else {
        "text"
    }
}

fn content_type(ref_uri: &str) -> &str {
    if ref_uri.ends_with(".json") {
        "application/json"
    } else if ref_uri.ends_with(".jsonl") {
        "application/jsonl"
    } else if ref_uri.ends_with(".yaml") || ref_uri.ends_with(".yml") {
        "application/yaml"
    } else {
        "text/plain"
    }
}

fn validate_relative_artifact_path(path: &str) -> AdcResult<()> {
    let relative = Path::new(path);
    if relative.as_os_str().is_empty() {
        return Err(AdcError::Artifact("artifact ref path is empty".to_string()));
    }
    for component in relative.components() {
        match component {
            Component::Normal(_) => {}
            _ => {
                return Err(AdcError::Artifact(format!(
                    "artifact ref path is not a safe relative path: {path}"
                )))
            }
        }
    }
    Ok(())
}

fn validate_segment(value: &str, label: &str) -> AdcResult<()> {
    if value.is_empty()
        || value.contains('/')
        || value.contains('\\')
        || value == "."
        || value == ".."
    {
        return Err(AdcError::Artifact(format!(
            "{label} must be a single relative path segment"
        )));
    }
    Ok(())
}
