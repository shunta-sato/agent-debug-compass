use std::{
    collections::{BTreeMap, BTreeSet},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    CompiledInvestigationRoute, DataQuality, KernelCapabilityMap, NormalizedSymptom, SafeProbePack,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityReport {
    pub schema_version: String,
    pub target_id: String,
    pub generated_at_unix_ms: u64,
    pub capabilities: Vec<CapabilityEntry>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityEntry {
    pub capability_id: String,
    pub status: String,
    pub required_privilege: String,
    pub safe_default: bool,
    pub reason: String,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactTrust {
    pub schema_version: String,
    pub raw_ref: String,
    pub content_class: String,
    pub trust_level: String,
    pub agent_instruction_policy: String,
    pub secret_scan: SecretScanResult,
    pub prompt_injection_scan: PromptInjectionScanResult,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretScanResult {
    pub status: String,
    pub redaction_applied: bool,
    pub suspected_secret_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptInjectionScanResult {
    pub status: String,
    pub markers: Vec<String>,
    pub severity: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvestigationContracts {
    pub hypothesis_set: HypothesisSet,
    pub evidence_graph: EvidenceGraph,
    pub probe_plan: ProbePlan,
    pub safety_policy: SafetyPolicy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HypothesisSet {
    pub schema_version: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fleet_run_id: Option<String>,
    pub hypotheses: Vec<Hypothesis>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hypothesis {
    pub hypothesis_id: String,
    pub statement: String,
    pub status: String,
    pub confidence: String,
    pub supports: Vec<EvidenceSupport>,
    pub contradicts: Vec<EvidenceSupport>,
    pub missing_evidence: Vec<String>,
    pub next_discriminating_probes: Vec<String>,
    pub claim_boundary: String,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceSupport {
    pub fact_id: String,
    pub raw_ref: String,
    pub strength: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceGraph {
    pub schema_version: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fleet_run_id: Option<String>,
    pub nodes: Vec<EvidenceGraphNode>,
    pub edges: Vec<EvidenceGraphEdge>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceGraphNode {
    pub node_id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hypothesis_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceGraphEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strength: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProbePlan {
    pub schema_version: String,
    pub probe_plan_id: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fleet_run_id: Option<String>,
    pub goal: String,
    pub candidate_probes: Vec<ProbePlanCandidate>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbePlanCandidate {
    pub probe_id: String,
    pub title: String,
    pub required_capabilities: Vec<String>,
    pub required_privilege: String,
    pub safety_status: String,
    pub expected_cost: String,
    pub timeout_ms: u64,
    pub expected_evidence: Vec<String>,
    pub discriminates: Vec<String>,
    pub failure_contract: String,
    pub cause_neutral: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProbeResult {
    pub schema_version: String,
    pub probe_id: String,
    pub probe_plan_id: String,
    pub status: String,
    pub produced_refs: Vec<ProbeProducedRef>,
    pub produced_facts: Vec<ProbeProducedFact>,
    pub hypothesis_updates: Vec<ProbeHypothesisUpdate>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeProducedRef {
    pub label: String,
    pub raw_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeProducedFact {
    pub fact_id: String,
    pub statement: String,
    pub raw_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeHypothesisUpdate {
    pub hypothesis_id: String,
    pub update: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SafetyPolicy {
    pub schema_version: String,
    pub policy_id: String,
    pub default_decision: String,
    pub rules: Vec<SafetyPolicyRule>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SafetyPolicyRule {
    pub operation: String,
    pub decision: String,
    pub constraints: BTreeMap<String, Value>,
}

pub fn build_capability_report(target_id: &str, map: &KernelCapabilityMap) -> CapabilityReport {
    let mut capabilities =
        vec![
        capability(
            "linux.proc.cpu",
            "supported",
            "none",
            true,
            "available through bounded /proc/stat observation",
            &map.data_quality,
        ),
        capability(
            "linux.proc.memory",
            "supported",
            "none",
            true,
            "available through bounded /proc/meminfo observation",
            &map.data_quality,
        ),
        capability(
            "linux.proc.network",
            "supported",
            "none",
            true,
            "available through bounded /proc/net/dev observation",
            &map.data_quality,
        ),
        capability(
            "kernel.ftrace",
            kernel_write_status(map.ftrace_available, map.root_access, map.tracefs_path.is_some()),
            "root_or_tracefs_group",
            false,
            if map.ftrace_available {
                "tracefs ftrace files are visible"
            } else {
                "ftrace files are not visible in tracefs"
            },
            &map.data_quality,
        ),
        capability(
            "kernel.kprobe",
            kernel_write_status(map.kprobe_available, map.root_access, map.tracefs_path.is_some()),
            "root_or_tracefs_group",
            false,
            if map.kprobe_available {
                "tracefs kprobe control is visible"
            } else {
                "kprobe control is not visible in tracefs"
            },
            &map.data_quality,
        ),
        capability(
            "kernel.perf",
            perf_status(map),
            "root_or_perf_event_policy",
            false,
            match map.perf_event_paranoid {
                Some(value) => {
                    if value <= 2 {
                        "perf policy allows bounded unprivileged read paths"
                    } else {
                        "perf policy is restrictive for unprivileged use"
                    }
                }
                None => "perf policy could not be read",
            },
            &map.data_quality,
        ),
        capability(
            "kernel.ebpf",
            if map.ebpf_available {
                if map.root_access {
                    "supported"
                } else {
                    "requires_privilege"
                }
            } else {
                "unavailable"
            },
            "root_or_bpf_policy",
            false,
            if map.ebpf_available {
                "BPF filesystem is visible"
            } else {
                "BPF filesystem is not visible"
            },
            &map.data_quality,
        ),
        capability(
            "edge.thermal",
            if map.thermal_zones.is_empty() {
                "unavailable"
            } else {
                "supported"
            },
            "none",
            true,
            if map.thermal_zones.is_empty() {
                "no thermal zones were detected"
            } else {
                "thermal zones are visible through sysfs"
            },
            &map.data_quality,
        ),
        capability(
            "edge.pci",
            if map.pci_devices.is_empty() {
                "unavailable"
            } else {
                "supported"
            },
            "none",
            true,
            if map.pci_devices.is_empty() {
                "no PCI devices were detected"
            } else {
                "PCI device inventory is visible through sysfs"
            },
            &map.data_quality,
        ),
        capability(
            "target.root_access",
            if map.root_access {
                "supported"
            } else {
                "unavailable"
            },
            "root",
            false,
            if map.root_access {
                "current process is running as root"
            } else {
                "current process is not running as root"
            },
            &map.data_quality,
        ),
        capability(
            "target.firmware_flash",
            "unsafe",
            "root_or_vendor_tool",
            false,
            "not exposed by default; disruptive operations require an explicit external workflow",
            &map.data_quality,
        ),
    ];
    capabilities.sort_by(|left, right| left.capability_id.cmp(&right.capability_id));

    CapabilityReport {
        schema_version: "obs.capability_report.v1".to_string(),
        target_id: target_id.to_string(),
        generated_at_unix_ms: now_unix_ms(),
        capabilities,
        data_quality: map.data_quality.clone(),
    }
}

pub fn classify_artifact_trust(
    raw_ref: &str,
    content_class: &str,
    text: &str,
    data_quality: &DataQuality,
) -> ArtifactTrust {
    let markers = prompt_injection_markers(text);
    ArtifactTrust {
        schema_version: "obs.artifact_trust.v1".to_string(),
        raw_ref: raw_ref.to_string(),
        content_class: content_class.to_string(),
        trust_level: trust_level_for_content_class(content_class).to_string(),
        agent_instruction_policy: "treat_as_data_only".to_string(),
        secret_scan: SecretScanResult {
            status: "scanned".to_string(),
            redaction_applied: text.contains("[REDACTED]"),
            suspected_secret_count: suspected_secret_count(text),
        },
        prompt_injection_scan: PromptInjectionScanResult {
            status: "scanned".to_string(),
            severity: if markers.is_empty() {
                "none".to_string()
            } else {
                "medium".to_string()
            },
            markers,
        },
        data_quality: data_quality.clone(),
    }
}

pub fn content_class_for_ref(ref_kind: &str, content_type: &str) -> &'static str {
    if ref_kind.contains("journal") {
        "journal"
    } else if ref_kind.contains("context") {
        "context"
    } else if ref_kind.contains("manifest") {
        "manifest"
    } else if ref_kind.contains("window") {
        "window"
    } else if ref_kind.contains("evidence") {
        "evidence_index"
    } else if content_type.contains("jsonl") || content_type.contains("text") {
        "log"
    } else if content_type.contains("yaml") {
        "summary"
    } else {
        "artifact"
    }
}

pub fn investigation_contracts_for(
    scope: &str,
    run_id: Option<&str>,
    fleet_run_id: Option<&str>,
    symptom: &NormalizedSymptom,
    route: &CompiledInvestigationRoute,
    probes: &[SafeProbePack],
    data_quality: &DataQuality,
) -> InvestigationContracts {
    let hypothesis_set = hypothesis_set_for(
        scope,
        run_id,
        fleet_run_id,
        symptom,
        route,
        probes,
        data_quality,
    );
    let evidence_graph = evidence_graph_for(scope, run_id, fleet_run_id, route, &hypothesis_set);
    let probe_plan = probe_plan_for(
        scope,
        run_id,
        fleet_run_id,
        symptom,
        probes,
        &hypothesis_set,
    );
    let safety_policy = default_rootless_safety_policy(data_quality);
    InvestigationContracts {
        hypothesis_set,
        evidence_graph,
        probe_plan,
        safety_policy,
    }
}

pub fn default_rootless_safety_policy(data_quality: &DataQuality) -> SafetyPolicy {
    SafetyPolicy {
        schema_version: "obs.safety_policy.v1".to_string(),
        policy_id: "default-rootless-lab-policy".to_string(),
        default_decision: "deny".to_string(),
        rules: vec![
            rule(
                "read_bounded_artifact",
                "allow",
                [("max_lines".to_string(), json!(1000))]
                    .into_iter()
                    .collect(),
            ),
            rule("observe_rootless", "allow", BTreeMap::new()),
            rule(
                "managed_mcp_plain_http",
                "allow_only_on_trusted_lan",
                BTreeMap::new(),
            ),
            rule(
                "restart_service",
                "requires_human_approval",
                BTreeMap::new(),
            ),
            rule("firmware_flash", "deny", BTreeMap::new()),
            rule("arbitrary_shell", "deny", BTreeMap::new()),
        ],
        data_quality: data_quality.clone(),
    }
}

pub fn probe_result_for_unavailable_capability(
    probe_plan_id: &str,
    probe_id: &str,
    hypothesis_ids: &[String],
    missing_fact: &str,
    data_quality: &DataQuality,
) -> ProbeResult {
    ProbeResult {
        schema_version: "obs.probe_result.v1".to_string(),
        probe_id: probe_id.to_string(),
        probe_plan_id: probe_plan_id.to_string(),
        status: "failed_missing_capability".to_string(),
        produced_refs: Vec::new(),
        produced_facts: vec![ProbeProducedFact {
            fact_id: missing_fact.to_string(),
            statement: "The requested evidence was unavailable in the current capability set."
                .to_string(),
            raw_ref: "artifact://probe_result/unavailable.json".to_string(),
        }],
        hypothesis_updates: hypothesis_ids
            .iter()
            .map(|hypothesis_id| ProbeHypothesisUpdate {
                hypothesis_id: hypothesis_id.clone(),
                update: "needs_evidence".to_string(),
                reason:
                    "The probe did not produce the requested fact because a capability was missing."
                        .to_string(),
            })
            .collect(),
        data_quality: data_quality.clone(),
    }
}

fn hypothesis_set_for(
    scope: &str,
    run_id: Option<&str>,
    fleet_run_id: Option<&str>,
    symptom: &NormalizedSymptom,
    route: &CompiledInvestigationRoute,
    probes: &[SafeProbePack],
    data_quality: &DataQuality,
) -> HypothesisSet {
    let mut hypotheses = route
        .selected_packs
        .iter()
        .enumerate()
        .map(|(index, pack)| {
            let supports = route
                .available_fact_ids
                .iter()
                .filter(|fact_id| pack.required_facts.contains(*fact_id))
                .map(|fact_id| EvidenceSupport {
                    fact_id: fact_id.clone(),
                    raw_ref: suggested_support_ref(pack.suggested_refs.first()),
                    strength: "weak".to_string(),
                })
                .collect::<Vec<_>>();
            let next_probes = probes
                .iter()
                .filter(|probe| overlaps(&probe.emitted_fact_ids, &pack.missing_fact_ids))
                .map(probe_id_for_pack)
                .collect::<Vec<_>>();
            let status = if pack.missing_fact_ids.is_empty() {
                "open"
            } else {
                "needs_evidence"
            };
            Hypothesis {
                hypothesis_id: format!("H{:03}", index + 1),
                statement: hypothesis_statement(&pack.domain, &symptom.normalized),
                status: status.to_string(),
                confidence: if supports.is_empty() { "low" } else { "medium" }.to_string(),
                supports,
                contradicts: Vec::new(),
                missing_evidence: pack.missing_fact_ids.clone(),
                next_discriminating_probes: next_probes,
                claim_boundary: "hypothesis_only".to_string(),
                data_quality: data_quality.clone(),
            }
        })
        .collect::<Vec<_>>();

    if hypotheses.is_empty() {
        hypotheses.push(Hypothesis {
            hypothesis_id: "H001".to_string(),
            statement: "No falsifiable hypothesis is available from the current evidence."
                .to_string(),
            status: "closed_insufficient_evidence".to_string(),
            confidence: "low".to_string(),
            supports: Vec::new(),
            contradicts: Vec::new(),
            missing_evidence: route.missing_fact_ids.clone(),
            next_discriminating_probes: probes.iter().map(probe_id_for_pack).collect(),
            claim_boundary: "hypothesis_only".to_string(),
            data_quality: data_quality.clone(),
        });
    }

    HypothesisSet {
        schema_version: "obs.hypothesis_set.v1".to_string(),
        scope: scope.to_string(),
        run_id: run_id.map(str::to_string),
        fleet_run_id: fleet_run_id.map(str::to_string),
        hypotheses,
        data_quality: data_quality.clone(),
    }
}

fn evidence_graph_for(
    scope: &str,
    run_id: Option<&str>,
    fleet_run_id: Option<&str>,
    route: &CompiledInvestigationRoute,
    hypothesis_set: &HypothesisSet,
) -> EvidenceGraph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut seen_nodes = BTreeSet::new();

    for target_id in &route.target_ids {
        push_node(
            &mut nodes,
            &mut seen_nodes,
            EvidenceGraphNode {
                node_id: format!("target:{target_id}"),
                kind: "target".to_string(),
                label: Some(target_id.clone()),
                raw_ref: None,
                hypothesis_id: None,
                target_id: Some(target_id.clone()),
            },
        );
    }

    for hypothesis in &hypothesis_set.hypotheses {
        let hypothesis_node = format!("hypothesis:{}", hypothesis.hypothesis_id);
        push_node(
            &mut nodes,
            &mut seen_nodes,
            EvidenceGraphNode {
                node_id: hypothesis_node.clone(),
                kind: "hypothesis".to_string(),
                label: Some(hypothesis.statement.clone()),
                raw_ref: None,
                hypothesis_id: Some(hypothesis.hypothesis_id.clone()),
                target_id: None,
            },
        );
        for support in &hypothesis.supports {
            let ref_node = format!("ref:{}", sanitize_node_id(&support.raw_ref));
            push_node(
                &mut nodes,
                &mut seen_nodes,
                EvidenceGraphNode {
                    node_id: ref_node.clone(),
                    kind: "evidence_ref".to_string(),
                    label: Some(support.fact_id.clone()),
                    raw_ref: Some(support.raw_ref.clone()),
                    hypothesis_id: None,
                    target_id: None,
                },
            );
            edges.push(EvidenceGraphEdge {
                from: ref_node.clone(),
                to: hypothesis_node.clone(),
                kind: "supports".to_string(),
                strength: Some(support.strength.clone()),
            });
            for target_id in &route.target_ids {
                edges.push(EvidenceGraphEdge {
                    from: ref_node.clone(),
                    to: format!("target:{target_id}"),
                    kind: "observed_on".to_string(),
                    strength: None,
                });
            }
        }
    }

    EvidenceGraph {
        schema_version: "obs.evidence_graph.v1".to_string(),
        scope: scope.to_string(),
        run_id: run_id.map(str::to_string),
        fleet_run_id: fleet_run_id.map(str::to_string),
        nodes,
        edges,
        data_quality: hypothesis_set.data_quality.clone(),
    }
}

fn probe_plan_for(
    scope: &str,
    run_id: Option<&str>,
    fleet_run_id: Option<&str>,
    symptom: &NormalizedSymptom,
    probes: &[SafeProbePack],
    hypothesis_set: &HypothesisSet,
) -> ProbePlan {
    let hypothesis_ids = hypothesis_set
        .hypotheses
        .iter()
        .map(|hypothesis| hypothesis.hypothesis_id.clone())
        .collect::<Vec<_>>();
    ProbePlan {
        schema_version: "obs.probe_plan.v1".to_string(),
        probe_plan_id: format!("PP-{}", symptom.normalized),
        scope: scope.to_string(),
        run_id: run_id.map(str::to_string),
        fleet_run_id: fleet_run_id.map(str::to_string),
        goal: format!(
            "Reduce uncertainty for {} without making a cause claim.",
            symptom.normalized
        ),
        candidate_probes: probes
            .iter()
            .map(|probe| ProbePlanCandidate {
                probe_id: probe_id_for_pack(probe),
                title: probe.title.clone(),
                required_capabilities: probe.capability_requirements.clone(),
                required_privilege: probe.required_privilege.clone(),
                safety_status: if probe.required_privilege == "none" {
                    "allowed"
                } else {
                    "requires_approval"
                }
                .to_string(),
                expected_cost: probe.expected_cost.clone(),
                timeout_ms: probe.timeout_ms,
                expected_evidence: probe.emitted_fact_ids.clone(),
                discriminates: hypothesis_ids.clone(),
                failure_contract: probe.failure_contract.clone(),
                cause_neutral: probe.cause_neutral,
            })
            .collect(),
        data_quality: hypothesis_set.data_quality.clone(),
    }
}

fn capability(
    capability_id: &str,
    status: &str,
    required_privilege: &str,
    safe_default: bool,
    reason: &str,
    data_quality: &DataQuality,
) -> CapabilityEntry {
    CapabilityEntry {
        capability_id: capability_id.to_string(),
        status: status.to_string(),
        required_privilege: required_privilege.to_string(),
        safe_default,
        reason: reason.to_string(),
        data_quality: data_quality.clone(),
    }
}

fn kernel_write_status(available: bool, root_access: bool, tracefs_visible: bool) -> &'static str {
    if available && root_access {
        "supported"
    } else if available {
        "requires_privilege"
    } else if tracefs_visible {
        "degraded"
    } else {
        "unavailable"
    }
}

fn perf_status(map: &KernelCapabilityMap) -> &'static str {
    if map.perf_available {
        "supported"
    } else if map.perf_event_paranoid.is_some() {
        "requires_privilege"
    } else {
        "unknown"
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn trust_level_for_content_class(content_class: &str) -> &'static str {
    match content_class {
        "manifest" | "evidence_index" | "window" | "context" | "summary" => "target_observation",
        "binary" => "binary_or_opaque",
        _ => "untrusted_target_text",
    }
}

fn suspected_secret_count(text: &str) -> usize {
    let lower = text.to_ascii_lowercase();
    ["password", "passwd", "token", "secret", "api_key", "apikey"]
        .iter()
        .filter(|needle| lower.contains(**needle))
        .count()
}

fn prompt_injection_markers(text: &str) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    let mut markers = Vec::new();
    if [
        "ignore previous instructions",
        "ignore all previous instructions",
        "system prompt",
        "developer message",
        "follow these instructions",
        "you are chatgpt",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        markers.push("instruction_like_text_detected".to_string());
    }
    markers
}

fn hypothesis_statement(domain: &str, symptom: &str) -> String {
    format!(
        "{} may be relevant to the observed {} symptom, but it remains falsifiable until supporting and contradicting evidence are evaluated.",
        domain.replace('_', " "),
        symptom
    )
}

fn suggested_support_ref(first_ref: Option<&String>) -> String {
    first_ref
        .map(|value| {
            if value.starts_with("artifact://") {
                value.clone()
            } else {
                format!("artifact://{value}")
            }
        })
        .unwrap_or_else(|| "artifact://evidence_index.yaml".to_string())
}

fn probe_id_for_pack(probe: &SafeProbePack) -> String {
    format!("probe.{}", probe.probe_pack_id.replace('-', "_"))
}

fn overlaps(left: &[String], right: &[String]) -> bool {
    left.iter().any(|value| right.contains(value))
}

fn rule(operation: &str, decision: &str, constraints: BTreeMap<String, Value>) -> SafetyPolicyRule {
    SafetyPolicyRule {
        operation: operation.to_string(),
        decision: decision.to_string(),
        constraints,
    }
}

fn push_node(
    nodes: &mut Vec<EvidenceGraphNode>,
    seen_nodes: &mut BTreeSet<String>,
    node: EvidenceGraphNode,
) {
    if seen_nodes.insert(node.node_id.clone()) {
        nodes.push(node);
    }
}

fn sanitize_node_id(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}
