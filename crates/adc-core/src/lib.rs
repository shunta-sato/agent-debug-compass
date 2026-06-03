pub mod agent_context;
pub mod artifact;
pub mod capture;
pub mod collectors;
pub mod compare;
pub mod contracts;
pub mod daemon;
pub mod discovery;
pub mod error;
pub mod event;
pub mod evidence;
pub mod fleet;
pub mod fleet_semantics;
pub mod investigation_facts;
pub mod investigation_state;
pub mod kernel;
pub mod managed_fleet;
pub mod overhead;
pub mod probe_packs;
pub mod profile;
pub mod route_compiler;
pub mod route_conditions;
pub mod route_packs;
pub mod service_investigation;
pub mod snapshot;
pub mod status;
pub mod symptom_context;
pub mod symptoms;
pub mod timeline;
pub mod trigger;

pub use agent_context::{
    build_fleet_agent_context, build_run_agent_context, build_session_evidence_index,
    cleanup_investigation_sessions, continue_investigation, get_investigation_session_state,
    latest_fleet_run_id, latest_run_id, render_agent_context_journald_jsonl,
    render_agent_context_markdown, render_agent_context_openmetrics,
    render_agent_context_otlp_json, render_agent_context_perfetto_json,
    render_investigation_route_markdown, resolve_agent_ref, resolve_global_agent_ref,
    stage_agent_context_inputs, start_investigation, AgentContext, AgentContextBudget,
    AgentContextDebt, AgentContextFact, AgentContextInputPaths, AgentContextOverhead,
    AgentContextRef, AgentContextRequest, AgentPlaybook, AgentPlaybookStep, AgentRefResolution,
    AgentTargetDossier, FleetAgentContext, FleetAgentContextRequest, FleetCrossTargetSummary,
    FleetFailureGroup, FleetTargetContextSummary, FleetTargetSourceSummary,
    InvestigationContinuationBudget, InvestigationContinuationFact, InvestigationContinuationPack,
    InvestigationContinuationRequest, InvestigationRoute, InvestigationSessionCleanupCandidate,
    InvestigationSessionCleanupReport, InvestigationSessionCleanupRequest,
    InvestigationSessionRequest, InvestigationStartPack, InvestigationStartRequest,
    InvestigationStep, OpenedRefSummary, RouteBranchCondition, RouteBudget, SessionEvidenceIndex,
    SessionRunEvidence,
};
pub use artifact::{ArtifactEntry, ArtifactManifest};
pub use capture::{
    capture_for, capture_for_target, CaptureBundle, CaptureOptions, CaptureTargetContext,
};
pub use collectors::{parse_meminfo, parse_net_dev, parse_proc_stat};
pub use compare::{compare_runs, CompareRunsResult, MetricDelta};
pub use contracts::{
    build_capability_report, classify_artifact_trust, content_class_for_raw_ref,
    content_class_for_ref, default_rootless_safety_policy, investigation_contracts_for,
    probe_result_for_unavailable_capability, AgentInstructionPolicy, ArtifactTrust,
    CapabilityEntry, CapabilityReport, CapabilityStatus, ClaimBoundary, ConfidenceLevel,
    ContentClass, EvidenceGraph, EvidenceGraphEdge, EvidenceGraphNode, EvidenceStrength,
    EvidenceSupport, Hypothesis, HypothesisSet, HypothesisStatus, InvestigationContracts,
    ProbeExecutor, ProbeHypothesisUpdate, ProbePlan, ProbePlanCandidate, ProbeProducedFact,
    ProbeProducedRef, ProbeResult, ProbeResultKind, ProbeResultStatus, ProbeSafetyStatus,
    PromptInjectionScanResult, PromptInjectionSeverity, SafetyDecision, SafetyPolicy,
    SafetyPolicyRule, ScanStatus, SecretScanResult, TrustLevel,
};
pub use daemon::{
    arm_profile, disarm_profile, initialize_state, read_state, record_run, run_service_for,
    state_path, DaemonState, ServiceRunSummary,
};
pub use discovery::{
    discover_same_network_targets_from_neighbors, DiscoveredTarget, TargetDiscoveryResult,
};
pub use error::{AdcError, AdcResult};
pub use event::{ClockSource, DataQuality, EventEnvelope, TimeRangeNs};
pub use evidence::{
    aggregate_event_data_quality, build_evidence_index, default_target_id, read_evidence_index,
    read_evidence_index_text, read_raw_slice, signal_series_for, validate_cause_neutral,
    write_evidence_index, CounterEvidence, EvidenceBuildInput, EvidenceIndex, EvidenceWindowRef,
    InformationDebt, NextProbeOption, ObservedFact, RawSlice, SalienceSignal, SignalSeries,
};
pub use fleet::{
    capture_fleet, capture_fleet_with_runner, investigate_fleet_service, preflight_fleet,
    preflight_fleet_with_runner, read_fleet_evidence_text, snapshot_fleet,
    snapshot_fleet_with_runner, FleetCaptureOptions, FleetCaptureResult, FleetEvidence,
    FleetPreflightCheck, FleetPreflightResult, FleetPreflightTarget,
    FleetServiceInvestigationOptions, FleetServiceInvestigationResult,
    FleetServiceInvestigationTarget, FleetSnapshotOptions, FleetTargetConfig, FleetTargetEvidence,
    FleetTargetRequest, FleetTargetRunResult, FleetTargetRunner,
};
pub use fleet_semantics::{
    build_fleet_semantic_diff, FleetSemanticDiff, FleetSemanticDiffGroup, SemanticFieldDiff,
};
pub use investigation_facts::{extract_evidence_facts_from_ref, EvidenceFact};
pub use investigation_state::{
    BranchEvaluation, CompletedInvestigationRef, InvestigationSessionBudget,
    InvestigationSessionState, NextInvestigationAction, SessionRetentionPolicy,
};
pub use kernel::{
    detect_default_kernel_capabilities, detect_kernel_capabilities, parse_privileged_operation,
    KernelCapabilityMap, KernelCapabilityPaths, PrivilegedOperation,
};
pub use managed_fleet::{
    create_managed_fleet_invite, enroll_managed_fleet_kit, initialize_managed_fleet_registry,
    managed_fleet_registry_path, materialize_managed_fleet_inventory, read_managed_fleet_registry,
    upsert_managed_fleet_target, verify_and_consume_managed_fleet_invite,
    ManagedFleetEnrollmentKit, ManagedFleetInventoryMaterialization, ManagedFleetInvite,
    ManagedFleetInviteOptions, ManagedFleetRegistry, ManagedFleetTarget,
};
pub use overhead::{
    build_overhead_report, evaluate_overhead, OverheadBudget, OverheadDecision, OverheadReport,
    OverheadSample,
};
pub use probe_packs::{
    default_safe_probe_packs, safe_probe_packs_for_missing_facts, SafeProbePack,
};
pub use profile::{default_profile_dir, load_profile, parse_profile, Profile};
pub use route_compiler::{
    compile_route_for_symptom, CompiledInvestigationRoute, CompiledRoutePack, RejectedRoutePack,
    RouteCompileInput,
};
pub use route_conditions::{
    evaluate_route_condition, ConditionStatus, RouteConditionEvaluation, RouteConditionExpr,
    RouteConditionInput,
};
pub use route_packs::{
    default_route_pack_registry, default_route_packs, RoutePack, RoutePackBudgetHint,
    RoutePackRegistry,
};
pub use service_investigation::{
    collect_service_state_for_context, investigate_service, ServiceInvestigationPack,
    ServiceInvestigationRequest, ServiceJournalLead, ServiceJournalSummary, ServicePortSummary,
    ServiceProcessSummary, ServiceStateSummary,
};
pub use snapshot::{
    create_snapshot, create_snapshot_for_target, SnapshotBundle, SnapshotTargetContext,
};
pub use status::{status_for, StatusResponse};
pub use symptom_context::{
    investigate_bug, SymptomContextBudget, SymptomContextPack, SymptomInvestigationRequest,
    SymptomTargetSummary,
};
pub use symptoms::{normalize_symptom, NormalizedSymptom, SymptomKind};
pub use timeline::{read_timeline_bounded, search_events, SearchEventsQuery, SearchEventsResult};
pub use trigger::{evaluate_trigger, TriggerEvaluation, TriggerInput};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildInfo {
    pub package: &'static str,
    pub version: &'static str,
}

impl BuildInfo {
    pub const fn new(package: &'static str, version: &'static str) -> Self {
        Self { package, version }
    }
}
