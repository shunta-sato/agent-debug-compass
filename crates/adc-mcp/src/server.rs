use std::{
    borrow::Cow,
    env, fs,
    future::{self, Future},
    path::PathBuf,
    process,
    time::Duration,
};

use rmcp::{
    model::{
        CallToolRequestParam, CallToolResult, Content, GetPromptRequestParam, GetPromptResult,
        Implementation, JsonObject, ListPromptsResult, ListResourceTemplatesResult,
        ListResourcesResult, ListToolsResult, PaginatedRequestParam, PromptsCapability,
        ReadResourceRequestParam, ReadResourceResult, ResourcesCapability, ServerCapabilities,
        ServerInfo, ToolsCapability,
    },
    service::RequestContext,
    transport::stdio,
    ErrorData, RoleServer, ServerHandler, ServiceExt,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

mod managed;
mod prompts;
mod resources;
mod tools;

use managed::{managed_mcp_tls_server_config_from_args, run_managed_mcp_listener};
use prompts::{get_prompt_sync, prompts};
use resources::{read_resource_sync, resource_templates, resources};
use tools::tool_definitions;

const CONTROLLER_TOOLS: [&str; 29] = [
    "obs.status",
    "obs.doctor",
    "obs.preflight",
    "obs.snapshot",
    "obs.observe",
    "obs.get_capabilities",
    "obs.get_agent_context",
    "obs.investigate_bug",
    "obs.start_investigation",
    "obs.continue_investigation",
    "obs.get_investigation_session",
    "obs.record_probe_result",
    "obs.list_route_packs",
    "obs.get_evidence_index",
    "obs.get_window",
    "obs.get_signal_series",
    "obs.get_raw_slice",
    "obs.get_ref",
    "obs.suggest_next_probe",
    "obs.search_evidence",
    "obs.compare_runs",
    "obs.investigate_service",
    "obs.discover_targets",
    "obs.fleet_preflight",
    "obs.fleet_observe",
    "obs.fleet_snapshot",
    "obs.fleet_capture",
    "obs.fleet_investigate_service",
    "obs.get_fleet_evidence",
];

const TARGET_TOOLS: [&str; 22] = [
    "obs.status",
    "obs.doctor",
    "obs.preflight",
    "obs.snapshot",
    "obs.observe",
    "obs.get_capabilities",
    "obs.get_agent_context",
    "obs.investigate_bug",
    "obs.start_investigation",
    "obs.continue_investigation",
    "obs.get_investigation_session",
    "obs.record_probe_result",
    "obs.list_route_packs",
    "obs.get_evidence_index",
    "obs.get_window",
    "obs.get_signal_series",
    "obs.get_raw_slice",
    "obs.get_ref",
    "obs.suggest_next_probe",
    "obs.search_evidence",
    "obs.compare_runs",
    "obs.investigate_service",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServerMode {
    Controller,
    Target,
}

impl ServerMode {
    fn tool_names(self) -> &'static [&'static str] {
        match self {
            Self::Controller => &CONTROLLER_TOOLS,
            Self::Target => &TARGET_TOOLS,
        }
    }

    fn allows_tool(self, name: &str) -> bool {
        self.tool_names().contains(&name)
    }

    fn allows_resource_uri(self, uri: &str) -> bool {
        match self {
            Self::Controller => true,
            Self::Target => !uri.starts_with("obs://fleet/"),
        }
    }
}

#[derive(Clone)]
struct AdcMcpServer {
    artifact_root: PathBuf,
    mode: ServerMode,
}

#[derive(Debug, Deserialize)]
struct RunIdParams {
    run_id: String,
}

#[derive(Debug, Deserialize)]
struct TargetPreflightParams {
    target_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SnapshotParams {
    run_id: String,
    target_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AgentContextParams {
    run_id: Option<String>,
    fleet_run_id: Option<String>,
    service_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StartInvestigationParams {
    run_id: Option<String>,
    fleet_run_id: Option<String>,
    service_name: Option<String>,
    inventory_path: Option<String>,
    max_journal_lines: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct InvestigateBugParams {
    run_id: Option<String>,
    fleet_run_id: Option<String>,
    service_name: Option<String>,
    inventory_path: Option<String>,
    symptom: String,
    max_journal_lines: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ContinueInvestigationParams {
    run_id: Option<String>,
    fleet_run_id: Option<String>,
    service_name: Option<String>,
    route_id: Option<String>,
    session_id: Option<String>,
    current_step_id: String,
    open_ref_labels: Option<Vec<String>>,
    open_raw_refs: Option<Vec<String>>,
    max_ref_lines: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ProbeResultParams {
    probe_plan_id: String,
    probe_id: String,
    missing_fact: String,
    hypothesis_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct InvestigationSessionParams {
    run_id: Option<String>,
    fleet_run_id: Option<String>,
    session_id: String,
}

#[derive(Debug, Deserialize)]
struct ObserveParams {
    run_id: String,
    duration_ms: u64,
    interval_ms: Option<u64>,
    target_id: Option<String>,
    profile_id: Option<String>,
    log_file: Option<String>,
    domain_events_file: Option<String>,
    config_file: Option<String>,
    service_name: Option<String>,
    otlp_file: Option<String>,
    journald_jsonl_file: Option<String>,
    perfetto_file: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WindowParams {
    run_id: String,
    window_id: String,
}

#[derive(Debug, Deserialize)]
struct SearchEventsParams {
    run_id: String,
    source: Option<String>,
    event_type: Option<String>,
    contains: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SignalSeriesParams {
    run_id: String,
    source: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct RawSliceParams {
    run_id: String,
    raw_ref: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct AgentRefParams {
    run_id: Option<String>,
    #[serde(rename = "ref")]
    ref_uri: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ServiceInvestigationParams {
    service_name: String,
    max_journal_lines: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct CompareRunsParams {
    before_run_id: String,
    after_run_id: String,
}

#[derive(Debug, Deserialize)]
struct DiscoverTargetsParams {
    network_cidr: String,
}

#[derive(Debug, Deserialize)]
struct FleetSnapshotParams {
    inventory_path: String,
    fleet_run_id: String,
}

#[derive(Debug, Deserialize)]
struct FleetPreflightParams {
    inventory_path: String,
}

#[derive(Debug, Deserialize)]
struct FleetCaptureParams {
    inventory_path: String,
    fleet_run_id: String,
    duration_ms: u64,
    interval_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct FleetServiceInvestigationParams {
    inventory_path: String,
    fleet_run_id: String,
    service_name: String,
    max_journal_lines: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct FleetRunParams {
    fleet_run_id: String,
}

#[derive(Debug, Serialize)]
struct ToolList {
    tools: Vec<&'static str>,
}

impl AdcMcpServer {
    fn call_tool_sync(&self, request: CallToolRequestParam) -> Result<CallToolResult, ErrorData> {
        if self.mode == ServerMode::Target && !self.mode.allows_tool(request.name.as_ref()) {
            return Err(ErrorData::invalid_params(
                format!("tool {} is not available in target mode", request.name),
                None,
            ));
        }
        match request.name.as_ref() {
            "obs.status" => {
                let status =
                    serde_json::to_value(adc_core::status_for("adc-mcp", adc_core::VERSION))
                        .map_err(to_internal_error)?;
                Ok(CallToolResult::structured(status))
            }
            "obs.doctor" => Ok(CallToolResult::structured(json!({
                "service": "adc-mcp",
                "version": adc_core::VERSION,
                "status": "ready",
                "root_required": false,
                "checks": [
                    {
                        "name": "artifact_root",
                        "status": if self.artifact_root.exists() { "ok" } else { "missing" },
                        "path": self.artifact_root
                    }
                ]
            }))),
            "obs.preflight" => {
                let params: TargetPreflightParams = decode_arguments(request.arguments)?;
                let target_id = params.target_id.unwrap_or_else(|| "local".to_string());
                validate_mcp_target_id(&target_id)?;
                Ok(CallToolResult::structured(target_preflight_value(
                    &self.artifact_root,
                    &target_id,
                )))
            }
            "obs.snapshot" => {
                let params: SnapshotParams = decode_arguments(request.arguments)?;
                let target_id = params.target_id.unwrap_or_else(|| "local".to_string());
                validate_mcp_target_id(&target_id)?;
                let bundle = adc_core::create_snapshot_for_target(
                    &self.artifact_root,
                    &params.run_id,
                    adc_core::SnapshotTargetContext {
                        target_id: target_id.clone(),
                        fleet_run_id: None,
                    },
                )
                .map_err(to_mcp_error)?;
                adc_core::record_run(&self.artifact_root, &params.run_id).map_err(to_mcp_error)?;
                Ok(CallToolResult::structured(json!({
                    "run_id": bundle.run_id,
                    "target_id": target_id,
                    "run_dir": bundle.run_dir,
                    "manifest": bundle.manifest_path,
                    "evidence_index": bundle.evidence_index_path,
                    "timeline": bundle.timeline_path,
                })))
            }
            "obs.observe" => {
                let params: ObserveParams = decode_arguments(request.arguments)?;
                let target_id = params
                    .target_id
                    .clone()
                    .unwrap_or_else(|| "local".to_string());
                let bundle = adc_core::capture_for_target(
                    &self.artifact_root,
                    adc_core::CaptureOptions {
                        run_id: params.run_id.clone(),
                        profile_id: params
                            .profile_id
                            .clone()
                            .unwrap_or_else(|| "mcp_observe".to_string()),
                        duration: Duration::from_millis(params.duration_ms),
                        interval: Duration::from_millis(params.interval_ms.unwrap_or(1_000)),
                        collectors: vec![
                            "cpu".to_string(),
                            "memory".to_string(),
                            "network".to_string(),
                        ],
                        max_artifact_bytes: 512 * 1024 * 1024,
                    },
                    adc_core::CaptureTargetContext {
                        target_id,
                        fleet_run_id: None,
                    },
                )
                .map_err(to_mcp_error)?;
                stage_mcp_agent_context_inputs(&params, &bundle.run_dir)?;
                adc_core::record_run(&self.artifact_root, &params.run_id).map_err(to_mcp_error)?;
                let context = adc_core::build_run_agent_context(
                    &self.artifact_root,
                    adc_core::AgentContextRequest {
                        run_id: params.run_id,
                        service_name: params.service_name,
                        max_markdown_bytes: 40 * 1024,
                    },
                )
                .map_err(to_mcp_error)?;
                serde_json::to_value(json!({
                    "run_id": bundle.run_id,
                    "run_dir": bundle.run_dir,
                    "agent_context": context,
                }))
                .map(CallToolResult::structured)
                .map_err(to_internal_error)
            }
            "obs.get_agent_context" => {
                let params: AgentContextParams = decode_arguments(request.arguments)?;
                if self.mode == ServerMode::Target && params.fleet_run_id.is_some() {
                    return Err(ErrorData::invalid_params(
                        "fleet_run_id is not available in target mode".to_string(),
                        None,
                    ));
                }
                if let Some(fleet_run_id) = params.fleet_run_id {
                    let fleet_run_id = if fleet_run_id == "latest" {
                        adc_core::latest_fleet_run_id(&self.artifact_root)
                            .map_err(to_mcp_error)?
                            .ok_or_else(|| {
                                ErrorData::invalid_params(
                                    "no fleet runs are available for fleet_run_id=latest"
                                        .to_string(),
                                    None,
                                )
                            })?
                    } else {
                        fleet_run_id
                    };
                    let context = adc_core::build_fleet_agent_context(
                        &self.artifact_root,
                        adc_core::FleetAgentContextRequest {
                            fleet_run_id,
                            max_markdown_bytes: 40 * 1024,
                        },
                    )
                    .map_err(to_mcp_error)?;
                    return serde_json::to_value(context)
                        .map(CallToolResult::structured)
                        .map_err(to_internal_error);
                }
                let Some(run_id) = params.run_id else {
                    return Err(ErrorData::invalid_params(
                        "obs.get_agent_context requires run_id or fleet_run_id".to_string(),
                        None,
                    ));
                };
                let run_id = if run_id == "latest" {
                    adc_core::latest_run_id(&self.artifact_root)
                        .map_err(to_mcp_error)?
                        .ok_or_else(|| {
                            ErrorData::invalid_params(
                                "no runs are available for run_id=latest".to_string(),
                                None,
                            )
                        })?
                } else {
                    run_id
                };
                let context = adc_core::build_run_agent_context(
                    &self.artifact_root,
                    adc_core::AgentContextRequest {
                        run_id,
                        service_name: params.service_name,
                        max_markdown_bytes: 40 * 1024,
                    },
                )
                .map_err(to_mcp_error)?;
                serde_json::to_value(context)
                    .map(CallToolResult::structured)
                    .map_err(to_internal_error)
            }
            "obs.get_capabilities" => {
                let map = adc_core::detect_default_kernel_capabilities().map_err(to_mcp_error)?;
                let report = adc_core::build_capability_report("local", &map);
                serde_json::to_value(report)
                    .map(CallToolResult::structured)
                    .map_err(to_internal_error)
            }
            "obs.investigate_bug" => {
                let params: InvestigateBugParams = decode_arguments(request.arguments)?;
                if self.mode == ServerMode::Target && params.fleet_run_id.is_some() {
                    return Err(ErrorData::invalid_params(
                        "fleet_run_id is not available in target mode".to_string(),
                        None,
                    ));
                }
                let context = adc_core::investigate_bug(
                    &self.artifact_root,
                    adc_core::SymptomInvestigationRequest {
                        run_id: params.run_id,
                        fleet_run_id: params.fleet_run_id,
                        service_name: params.service_name,
                        inventory_path: params.inventory_path.map(PathBuf::from),
                        symptom: params.symptom,
                        max_journal_lines: params.max_journal_lines,
                        max_markdown_bytes: 40 * 1024,
                        max_context_bytes: 64 * 1024,
                    },
                )
                .map_err(to_mcp_error)?;
                serde_json::to_value(context)
                    .map(CallToolResult::structured)
                    .map_err(to_internal_error)
            }
            "obs.start_investigation" => {
                let params: StartInvestigationParams = decode_arguments(request.arguments)?;
                if self.mode == ServerMode::Target && params.fleet_run_id.is_some() {
                    return Err(ErrorData::invalid_params(
                        "fleet_run_id is not available in target mode".to_string(),
                        None,
                    ));
                }
                let pack = adc_core::start_investigation(
                    &self.artifact_root,
                    adc_core::InvestigationStartRequest {
                        run_id: params.run_id,
                        fleet_run_id: params.fleet_run_id,
                        service_name: params.service_name,
                        inventory_path: params.inventory_path.map(PathBuf::from),
                        max_journal_lines: params.max_journal_lines,
                        max_markdown_bytes: 40 * 1024,
                    },
                )
                .map_err(to_mcp_error)?;
                serde_json::to_value(pack)
                    .map(CallToolResult::structured)
                    .map_err(to_internal_error)
            }
            "obs.continue_investigation" => {
                let params: ContinueInvestigationParams = decode_arguments(request.arguments)?;
                if self.mode == ServerMode::Target && params.fleet_run_id.is_some() {
                    return Err(ErrorData::invalid_params(
                        "fleet_run_id is not available in target mode".to_string(),
                        None,
                    ));
                }
                let pack = adc_core::continue_investigation(
                    &self.artifact_root,
                    adc_core::InvestigationContinuationRequest {
                        run_id: params.run_id,
                        fleet_run_id: params.fleet_run_id,
                        service_name: params.service_name,
                        route_id: params.route_id,
                        session_id: params.session_id,
                        current_step_id: params.current_step_id,
                        open_ref_labels: params.open_ref_labels.unwrap_or_default(),
                        open_raw_refs: params.open_raw_refs.unwrap_or_default(),
                        max_context_bytes: 10 * 1024,
                        max_ref_lines: params.max_ref_lines.unwrap_or(80),
                    },
                )
                .map_err(to_mcp_error)?;
                serde_json::to_value(pack)
                    .map(CallToolResult::structured)
                    .map_err(to_internal_error)
            }
            "obs.get_investigation_session" => {
                let params: InvestigationSessionParams = decode_arguments(request.arguments)?;
                if self.mode == ServerMode::Target && params.fleet_run_id.is_some() {
                    return Err(ErrorData::invalid_params(
                        "fleet_run_id is not available in target mode".to_string(),
                        None,
                    ));
                }
                let state = adc_core::get_investigation_session_state(
                    &self.artifact_root,
                    adc_core::InvestigationSessionRequest {
                        run_id: params.run_id,
                        fleet_run_id: params.fleet_run_id,
                        session_id: params.session_id,
                    },
                )
                .map_err(to_mcp_error)?;
                serde_json::to_value(state)
                    .map(CallToolResult::structured)
                    .map_err(to_internal_error)
            }
            "obs.record_probe_result" => {
                let params: ProbeResultParams = decode_arguments(request.arguments)?;
                let data_quality = adc_core::DataQuality {
                    missing: vec![format!(
                        "{} unavailable in recorded probe result",
                        params.missing_fact
                    )],
                    clock_confidence: "medium".to_string(),
                    ..Default::default()
                };
                let result = adc_core::probe_result_for_unavailable_capability(
                    &params.probe_plan_id,
                    &params.probe_id,
                    &params.hypothesis_ids.unwrap_or_default(),
                    &params.missing_fact,
                    &data_quality,
                );
                serde_json::to_value(result)
                    .map(CallToolResult::structured)
                    .map_err(to_internal_error)
            }
            "obs.list_route_packs" => serde_json::to_value(adc_core::default_route_pack_registry())
                .map(CallToolResult::structured)
                .map_err(to_internal_error),
            "obs.get_evidence_index" => {
                let params: RunIdParams = decode_arguments(request.arguments)?;
                let evidence =
                    adc_core::read_evidence_index_text(&self.artifact_root, &params.run_id)
                        .map_err(to_mcp_error)?;
                Ok(CallToolResult::success(vec![Content::text(evidence)]))
            }
            "obs.get_window" => {
                let params: WindowParams = decode_arguments(request.arguments)?;
                let window = adc_core::snapshot::read_window(
                    &self.artifact_root,
                    &params.run_id,
                    &params.window_id,
                )
                .map_err(to_mcp_error)?;
                Ok(CallToolResult::success(vec![Content::text(window)]))
            }
            "obs.get_signal_series" => {
                let params: SignalSeriesParams = decode_arguments(request.arguments)?;
                let series = adc_core::signal_series_for(
                    &self.artifact_root,
                    &params.run_id,
                    &params.source,
                    params.limit.unwrap_or(20),
                )
                .map_err(to_mcp_error)?;
                serde_json::to_value(series)
                    .map(CallToolResult::structured)
                    .map_err(to_internal_error)
            }
            "obs.get_raw_slice" => {
                let params: RawSliceParams = decode_arguments(request.arguments)?;
                let slice = adc_core::read_raw_slice(
                    &self.artifact_root,
                    &params.run_id,
                    &params.raw_ref,
                    params.limit.unwrap_or(20),
                )
                .map_err(to_mcp_error)?;
                serde_json::to_value(slice)
                    .map(CallToolResult::structured)
                    .map_err(to_internal_error)
            }
            "obs.get_ref" => {
                let params: AgentRefParams = decode_arguments(request.arguments)?;
                let resolved = if let Some(run_id) = params.run_id {
                    adc_core::resolve_agent_ref(
                        &self.artifact_root,
                        &run_id,
                        &params.ref_uri,
                        params.limit.unwrap_or(20),
                    )
                } else {
                    adc_core::resolve_global_agent_ref(
                        &self.artifact_root,
                        &params.ref_uri,
                        params.limit.unwrap_or(20),
                    )
                }
                .map_err(to_mcp_error)?;
                serde_json::to_value(resolved)
                    .map(CallToolResult::structured)
                    .map_err(to_internal_error)
            }
            "obs.suggest_next_probe" => {
                let params: RunIdParams = decode_arguments(request.arguments)?;
                let evidence = adc_core::read_evidence_index(&self.artifact_root, &params.run_id)
                    .map_err(to_mcp_error)?;
                Ok(CallToolResult::structured(json!({
                    "run_id": evidence.run_id,
                    "target_id": evidence.target_id,
                    "next_probe_options": evidence.next_probe_options,
                    "information_debt": evidence.information_debt,
                    "data_quality": evidence.data_quality,
                })))
            }
            "obs.search_evidence" => {
                let params: SearchEventsParams = decode_arguments(request.arguments)?;
                let result = adc_core::search_events(
                    &self.artifact_root,
                    &params.run_id,
                    &adc_core::SearchEventsQuery {
                        source: params.source,
                        event_type: params.event_type,
                        contains: params.contains,
                        limit: params.limit.unwrap_or(20),
                    },
                )
                .map_err(to_mcp_error)?;
                serde_json::to_value(result)
                    .map(CallToolResult::structured)
                    .map_err(to_internal_error)
            }
            "obs.compare_runs" => {
                let params: CompareRunsParams = decode_arguments(request.arguments)?;
                let result = adc_core::compare_runs(
                    &self.artifact_root,
                    &params.before_run_id,
                    &params.after_run_id,
                )
                .map_err(to_mcp_error)?;
                serde_json::to_value(result)
                    .map(CallToolResult::structured)
                    .map_err(to_internal_error)
            }
            "obs.investigate_service" => {
                let params: ServiceInvestigationParams = decode_arguments(request.arguments)?;
                let pack = adc_core::investigate_service(
                    &self.artifact_root,
                    adc_core::ServiceInvestigationRequest {
                        service_name: params.service_name,
                        max_journal_lines: params.max_journal_lines.unwrap_or(80),
                    },
                )
                .map_err(to_mcp_error)?;
                serde_json::to_value(pack)
                    .map(CallToolResult::structured)
                    .map_err(to_internal_error)
            }
            "obs.discover_targets" => {
                let params: DiscoverTargetsParams = decode_arguments(request.arguments)?;
                let neighbors = fs::read_to_string("/proc/net/arp").map_err(|err| {
                    ErrorData::internal_error(
                        format!("failed to read /proc/net/arp for discovery: {err}"),
                        None,
                    )
                })?;
                let result = adc_core::discover_same_network_targets_from_neighbors(
                    &params.network_cidr,
                    &neighbors,
                )
                .map_err(to_mcp_error)?;
                serde_json::to_value(result)
                    .map(CallToolResult::structured)
                    .map_err(to_internal_error)
            }
            "obs.fleet_preflight" => {
                let params: FleetPreflightParams = decode_arguments(request.arguments)?;
                let result =
                    adc_core::preflight_fleet(&params.inventory_path).map_err(to_mcp_error)?;
                serde_json::to_value(result)
                    .map(CallToolResult::structured)
                    .map_err(to_internal_error)
            }
            "obs.fleet_observe" => {
                let params: FleetCaptureParams = decode_arguments(request.arguments)?;
                let result = adc_core::capture_fleet(
                    &self.artifact_root,
                    &params.inventory_path,
                    adc_core::FleetCaptureOptions {
                        fleet_run_id: params.fleet_run_id,
                        duration: Duration::from_millis(params.duration_ms),
                        interval: Duration::from_millis(params.interval_ms.unwrap_or(1_000)),
                    },
                )
                .map_err(to_mcp_error)?;
                serde_json::to_value(result)
                    .map(CallToolResult::structured)
                    .map_err(to_internal_error)
            }
            "obs.fleet_snapshot" => {
                let params: FleetSnapshotParams = decode_arguments(request.arguments)?;
                let result = adc_core::snapshot_fleet(
                    &self.artifact_root,
                    &params.inventory_path,
                    adc_core::FleetSnapshotOptions {
                        fleet_run_id: params.fleet_run_id,
                    },
                )
                .map_err(to_mcp_error)?;
                serde_json::to_value(result)
                    .map(CallToolResult::structured)
                    .map_err(to_internal_error)
            }
            "obs.fleet_capture" => {
                let params: FleetCaptureParams = decode_arguments(request.arguments)?;
                let result = adc_core::capture_fleet(
                    &self.artifact_root,
                    &params.inventory_path,
                    adc_core::FleetCaptureOptions {
                        fleet_run_id: params.fleet_run_id,
                        duration: Duration::from_millis(params.duration_ms),
                        interval: Duration::from_millis(params.interval_ms.unwrap_or(1_000)),
                    },
                )
                .map_err(to_mcp_error)?;
                serde_json::to_value(result)
                    .map(CallToolResult::structured)
                    .map_err(to_internal_error)
            }
            "obs.fleet_investigate_service" => {
                let params: FleetServiceInvestigationParams = decode_arguments(request.arguments)?;
                let result = adc_core::investigate_fleet_service(
                    &self.artifact_root,
                    &params.inventory_path,
                    adc_core::FleetServiceInvestigationOptions {
                        fleet_run_id: params.fleet_run_id,
                        service_name: params.service_name,
                        max_journal_lines: params.max_journal_lines.unwrap_or(80),
                    },
                )
                .map_err(to_mcp_error)?;
                serde_json::to_value(result)
                    .map(CallToolResult::structured)
                    .map_err(to_internal_error)
            }
            "obs.get_fleet_evidence" => {
                let params: FleetRunParams = decode_arguments(request.arguments)?;
                let evidence =
                    adc_core::read_fleet_evidence_text(&self.artifact_root, &params.fleet_run_id)
                        .map_err(to_mcp_error)?;
                Ok(CallToolResult::success(vec![Content::text(evidence)]))
            }
            other => Err(ErrorData::invalid_params(
                format!("unknown adc-targetd tool: {other}"),
                None,
            )),
        }
    }
}

impl ServerHandler for AdcMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: Default::default(),
            capabilities: ServerCapabilities {
                prompts: Some(PromptsCapability {
                    list_changed: Some(false),
                }),
                resources: Some(ResourcesCapability {
                    subscribe: Some(false),
                    list_changed: Some(false),
                }),
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
                ..Default::default()
            },
            server_info: Implementation {
                name: "adc-mcp".to_string(),
                title: None,
                version: adc_core::VERSION.to_string(),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Use obs.get_evidence_index first, then bounded window/series/raw-slice tools. Evidence is not a root-cause conclusion."
                    .to_string(),
            ),
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        future::ready(Ok(ListToolsResult::with_all_items(tool_definitions(
            self.mode,
        ))))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResult, ErrorData>> + Send + '_ {
        future::ready(self.call_tool_sync(request))
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourcesResult, ErrorData>> + Send + '_ {
        future::ready(Ok(ListResourcesResult::with_all_items(resources())))
    }

    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourceTemplatesResult, ErrorData>> + Send + '_ {
        future::ready(Ok(ListResourceTemplatesResult::with_all_items(
            resource_templates(self.mode),
        )))
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ReadResourceResult, ErrorData>> + Send + '_ {
        future::ready(if self.mode.allows_resource_uri(&request.uri) {
            read_resource_sync(&self.artifact_root, &request.uri)
        } else {
            Err(ErrorData::invalid_params(
                format!("resource {} is not available in target mode", request.uri),
                None,
            ))
        })
    }

    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListPromptsResult, ErrorData>> + Send + '_ {
        future::ready(Ok(ListPromptsResult::with_all_items(prompts())))
    }

    fn get_prompt(
        &self,
        request: GetPromptRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<GetPromptResult, ErrorData>> + Send + '_ {
        future::ready(get_prompt_sync(&request.name))
    }
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args().collect::<Vec<_>>();
    let mode = if args.iter().any(|arg| arg == "--target-mode") {
        ServerMode::Target
    } else {
        ServerMode::Controller
    };
    if args.iter().any(|arg| arg == "--tool-list-json") {
        serde_json::to_writer_pretty(std::io::stdout(), &tool_list(mode))?;
        println!();
        return Ok(());
    }
    if let Some(listen_addr) = arg_value(&args, "--managed-listen") {
        if mode != ServerMode::Target {
            return Err("--managed-listen requires --target-mode".into());
        }
        let token_file = arg_value(&args, "--managed-token-file")
            .ok_or("--managed-token-file is required with --managed-listen")?;
        let tls = managed_mcp_tls_server_config_from_args(&args)?;
        run_managed_mcp_listener(&listen_addr, &token_file, mode, tls)?;
        return Ok(());
    }

    let service = AdcMcpServer {
        artifact_root: adc_core::snapshot::default_artifact_root(),
        mode,
    }
    .serve(stdio())
    .await?;
    service.waiting().await?;
    Ok(())
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].clone())
}

fn stage_mcp_agent_context_inputs(
    params: &ObserveParams,
    run_dir: impl AsRef<std::path::Path>,
) -> Result<(), ErrorData> {
    let inputs = adc_core::AgentContextInputPaths {
        log_file: params.log_file.as_ref().map(PathBuf::from),
        domain_events_file: params.domain_events_file.as_ref().map(PathBuf::from),
        config_file: params.config_file.as_ref().map(PathBuf::from),
        service_name: params.service_name.clone(),
        otlp_file: params.otlp_file.as_ref().map(PathBuf::from),
        journald_jsonl_file: params.journald_jsonl_file.as_ref().map(PathBuf::from),
        perfetto_file: params.perfetto_file.as_ref().map(PathBuf::from),
    };
    adc_core::stage_agent_context_inputs(run_dir, &inputs).map_err(to_mcp_error)
}

fn target_preflight_value(artifact_root: &std::path::Path, target_id: &str) -> serde_json::Value {
    let mut checks = Vec::new();
    let mut missing = Vec::new();
    match fs::create_dir_all(artifact_root) {
        Ok(()) => {
            let write_test_path =
                artifact_root.join(format!(".preflight-write-{}.tmp", process::id()));
            match fs::write(&write_test_path, b"ok") {
                Ok(()) => {
                    let _ = fs::remove_file(&write_test_path);
                    checks.push(json!({
                        "name": "artifact_root_writable",
                        "status": "ok",
                        "path": artifact_root,
                    }));
                }
                Err(err) => {
                    missing.push(format!("artifact_root_writable: {err}"));
                    checks.push(json!({
                        "name": "artifact_root_writable",
                        "status": "error",
                        "path": artifact_root,
                        "error": err.to_string(),
                    }));
                }
            }
        }
        Err(err) => {
            missing.push(format!("artifact_root_create: {err}"));
            checks.push(json!({
                "name": "artifact_root_writable",
                "status": "error",
                "path": artifact_root,
                "error": err.to_string(),
            }));
        }
    }
    checks.push(json!({
        "name": "adc_mcp_server",
        "status": "ok",
        "version": adc_core::VERSION,
    }));
    let status = if missing.is_empty() {
        "ready"
    } else {
        "degraded"
    };
    json!({
        "schema_version": "obs.target_preflight.v1",
        "target_id": target_id,
        "status": status,
        "root_required": false,
        "checks": checks,
        "data_quality": {
            "dropped": false,
            "drop_count": 0,
            "throttled": false,
            "missing": missing,
            "truncated": false,
            "clock_confidence": "medium",
            "notes": ["target_mcp_endpoint=true"]
        }
    })
}

fn validate_mcp_target_id(target_id: &str) -> Result<(), ErrorData> {
    let invalid = target_id.trim().is_empty()
        || target_id.contains('/')
        || target_id.contains("..")
        || target_id
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, '"' | '\'' | '`' | '$' | ';' | '|'));
    if invalid {
        return Err(ErrorData::invalid_params(
            "target_id must be a single safe path segment".to_string(),
            None,
        ));
    }
    Ok(())
}

fn decode_arguments<T: for<'de> Deserialize<'de>>(
    arguments: Option<JsonObject>,
) -> Result<T, ErrorData> {
    serde_json::from_value(serde_json::Value::Object(arguments.unwrap_or_default()))
        .map_err(|err| ErrorData::invalid_params(format!("invalid tool arguments: {err}"), None))
}

fn tool_list(mode: ServerMode) -> ToolList {
    ToolList {
        tools: mode.tool_names().to_vec(),
    }
}

fn to_mcp_error(err: adc_core::AdcError) -> ErrorData {
    match err {
        adc_core::AdcError::Artifact(message)
            if message.contains("run_id")
                || message.contains("window_id")
                || message.contains("before_run_id")
                || message.contains("after_run_id") =>
        {
            ErrorData::invalid_params(message, None)
        }
        adc_core::AdcError::Artifact(message)
            if message.contains("not found") || message.contains("No such file") =>
        {
            ErrorData::resource_not_found(message, None)
        }
        other => ErrorData::internal_error(other.to_string(), None),
    }
}

fn to_internal_error(err: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(Cow::Owned(err.to_string()), None)
}
