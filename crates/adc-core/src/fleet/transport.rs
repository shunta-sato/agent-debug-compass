use std::{
    fs,
    io::{BufRead, BufReader, ErrorKind, Read, Write},
    net::TcpStream,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        mpsc::{self, RecvTimeoutError},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use rustls::{
    pki_types::{CertificateDer, PrivateKeyDer, ServerName},
    ClientConfig, ClientConnection, RootCertStore, StreamOwned,
};

use super::{
    bounded_text, preflight_model, validate_ssh_host, validate_ssh_user, FleetPreflightCheck,
    FleetPreflightTarget, FleetTargetConfig, FleetTargetRequest, FleetTargetRunResult,
};
use crate::{
    capture_for_target, create_snapshot_for_target, AdcError, AdcResult, CaptureOptions,
    CaptureTargetContext, DataQuality, ServiceInvestigationPack, ServiceInvestigationRequest,
    SnapshotTargetContext,
};

pub(super) fn snapshot_local_target(
    artifact_root: &Path,
    target: &FleetTargetConfig,
    request: &FleetTargetRequest,
) -> AdcResult<FleetTargetRunResult> {
    let bundle = create_snapshot_for_target(
        artifact_root,
        &request.run_id,
        SnapshotTargetContext {
            target_id: target.id.clone(),
            fleet_run_id: Some(request.fleet_run_id.clone()),
        },
    )?;
    Ok(FleetTargetRunResult {
        status: "captured".to_string(),
        run_id: Some(request.run_id.clone()),
        evidence_text: None,
        evidence_ref: Some(format!(
            "artifact://{}",
            bundle
                .evidence_index_path
                .strip_prefix(artifact_root)
                .unwrap_or(&bundle.evidence_index_path)
                .display()
        )),
        profile_id: Some(request.profile_id.clone()),
        capability_ref: Some(format!(
            "artifact://runs/{}/raw/capability.json",
            request.run_id
        )),
        artifact_ref: Some(format!("artifact://runs/{}/manifest.json", request.run_id)),
        data_quality: DataQuality {
            clock_confidence: "medium".to_string(),
            ..Default::default()
        },
    })
}

pub(super) fn capture_local_target(
    artifact_root: &Path,
    target: &FleetTargetConfig,
    request: &FleetTargetRequest,
) -> AdcResult<FleetTargetRunResult> {
    let bundle = capture_for_target(
        artifact_root,
        CaptureOptions {
            run_id: request.run_id.clone(),
            profile_id: request.profile_id.clone(),
            duration: request.duration,
            interval: request.interval,
            collectors: vec![
                "cpu".to_string(),
                "memory".to_string(),
                "network".to_string(),
            ],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
        CaptureTargetContext {
            target_id: target.id.clone(),
            fleet_run_id: Some(request.fleet_run_id.clone()),
        },
    )?;
    Ok(FleetTargetRunResult {
        status: "captured".to_string(),
        run_id: Some(request.run_id.clone()),
        evidence_text: None,
        evidence_ref: Some(format!(
            "artifact://{}",
            bundle
                .evidence_index_path
                .strip_prefix(artifact_root)
                .unwrap_or(&bundle.evidence_index_path)
                .display()
        )),
        profile_id: Some(request.profile_id.clone()),
        capability_ref: None,
        artifact_ref: Some(format!("artifact://runs/{}/manifest.json", request.run_id)),
        data_quality: DataQuality {
            clock_confidence: "medium".to_string(),
            ..Default::default()
        },
    })
}

pub(super) fn snapshot_target_mcp_over_ssh(
    target: &FleetTargetConfig,
    request: &FleetTargetRequest,
) -> AdcResult<FleetTargetRunResult> {
    let Some(host) = target.host.as_deref() else {
        return Ok(FleetTargetRunResult::failed(
            "unreachable",
            "target MCP-over-SSH endpoint is missing host",
        ));
    };
    if let Err(failure) = validate_target_mcp_ssh_endpoint(target) {
        return Ok(FleetTargetRunResult::failed(
            failure.status,
            failure.message,
        ));
    }

    let snapshot = match call_target_mcp_tool(
        target,
        "obs.snapshot",
        serde_json::json!({
            "run_id": request.run_id,
            "target_id": target.id,
        }),
        Duration::from_secs(20),
    ) {
        Ok(call) => call,
        Err(failure) => {
            return Ok(FleetTargetRunResult::failed(
                failure.status,
                failure.message,
            ))
        }
    };
    if snapshot.structured_content.is_none() {
        return Ok(FleetTargetRunResult::failed(
            "mcp_protocol_error",
            "obs.snapshot did not return structuredContent",
        ));
    }

    let evidence = match call_target_mcp_tool(
        target,
        "obs.get_evidence_index",
        serde_json::json!({
            "run_id": request.run_id,
        }),
        Duration::from_secs(15),
    ) {
        Ok(call) => call,
        Err(failure) => {
            return Ok(FleetTargetRunResult::failed(
                failure.status,
                failure.message,
            ))
        }
    };
    let Some(evidence_text) = evidence.text_content else {
        return Ok(FleetTargetRunResult::failed(
            "mcp_protocol_error",
            "obs.get_evidence_index did not return text content",
        ));
    };

    let mut data_quality = DataQuality {
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };
    data_quality.notes.push(format!("ssh_host={host}"));
    data_quality
        .notes
        .push("transport=mcp_stdio_over_ssh".to_string());
    Ok(FleetTargetRunResult {
        status: "captured".to_string(),
        run_id: Some(request.run_id.clone()),
        evidence_text: Some(evidence_text),
        evidence_ref: None,
        profile_id: Some(request.profile_id.clone()),
        capability_ref: None,
        artifact_ref: Some(format!("mcp+ssh://{host}/runs/{}", request.run_id)),
        data_quality,
    })
}

pub(super) fn capture_target_mcp_over_ssh(
    target: &FleetTargetConfig,
    request: &FleetTargetRequest,
) -> AdcResult<FleetTargetRunResult> {
    let Some(host) = target.host.as_deref() else {
        return Ok(FleetTargetRunResult::failed(
            "unreachable",
            "target MCP-over-SSH endpoint is missing host",
        ));
    };
    if let Err(failure) = validate_target_mcp_ssh_endpoint(target) {
        return Ok(FleetTargetRunResult::failed(
            failure.status,
            failure.message,
        ));
    }

    let observe = match call_target_mcp_tool(
        target,
        "obs.observe",
        serde_json::json!({
            "run_id": request.run_id,
            "target_id": target.id,
            "profile_id": request.profile_id,
            "duration_ms": request.duration.as_millis() as u64,
            "interval_ms": request.interval.as_millis() as u64,
        }),
        target_mcp_capture_timeout(request),
    ) {
        Ok(call) => call,
        Err(failure) => {
            return Ok(FleetTargetRunResult::failed(
                failure.status,
                failure.message,
            ))
        }
    };
    if observe.structured_content.is_none() {
        return Ok(FleetTargetRunResult::failed(
            "mcp_protocol_error",
            "obs.observe did not return structuredContent",
        ));
    }

    let evidence = match call_target_mcp_tool(
        target,
        "obs.get_evidence_index",
        serde_json::json!({
            "run_id": request.run_id,
        }),
        Duration::from_secs(15),
    ) {
        Ok(call) => call,
        Err(failure) => {
            return Ok(FleetTargetRunResult::failed(
                failure.status,
                failure.message,
            ))
        }
    };
    let Some(evidence_text) = evidence.text_content else {
        return Ok(FleetTargetRunResult::failed(
            "mcp_protocol_error",
            "obs.get_evidence_index did not return text content",
        ));
    };

    let mut data_quality = DataQuality {
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };
    data_quality.notes.push(format!("ssh_host={host}"));
    data_quality
        .notes
        .push("transport=mcp_stdio_over_ssh".to_string());
    Ok(FleetTargetRunResult {
        status: "captured".to_string(),
        run_id: Some(request.run_id.clone()),
        evidence_text: Some(evidence_text),
        evidence_ref: None,
        profile_id: Some(request.profile_id.clone()),
        capability_ref: None,
        artifact_ref: Some(format!("mcp+ssh://{host}/runs/{}", request.run_id)),
        data_quality,
    })
}

pub(super) fn snapshot_target_managed_mcp(
    target: &FleetTargetConfig,
    request: &FleetTargetRequest,
) -> AdcResult<FleetTargetRunResult> {
    let endpoint = match validate_managed_mcp_endpoint(target) {
        Ok(endpoint) => endpoint,
        Err(failure) => {
            return Ok(FleetTargetRunResult::failed(
                failure.status,
                failure.message,
            ))
        }
    };
    let snapshot = match call_target_managed_mcp_tool(
        target,
        "obs.snapshot",
        serde_json::json!({
            "run_id": request.run_id,
            "target_id": target.id,
        }),
        Duration::from_secs(20),
    ) {
        Ok(call) => call,
        Err(failure) => {
            return Ok(FleetTargetRunResult::failed(
                failure.status,
                failure.message,
            ))
        }
    };
    if snapshot.structured_content.is_none() {
        return Ok(FleetTargetRunResult::failed(
            "mcp_protocol_error",
            "obs.snapshot did not return structuredContent",
        ));
    }
    let evidence = match call_target_managed_mcp_tool(
        target,
        "obs.get_evidence_index",
        serde_json::json!({
            "run_id": request.run_id,
        }),
        Duration::from_secs(15),
    ) {
        Ok(call) => call,
        Err(failure) => {
            return Ok(FleetTargetRunResult::failed(
                failure.status,
                failure.message,
            ))
        }
    };
    let Some(evidence_text) = evidence.text_content else {
        return Ok(FleetTargetRunResult::failed(
            "mcp_protocol_error",
            "obs.get_evidence_index did not return text content",
        ));
    };
    Ok(managed_mcp_captured_result(
        request,
        evidence_text,
        endpoint,
    ))
}

pub(super) fn capture_target_managed_mcp(
    target: &FleetTargetConfig,
    request: &FleetTargetRequest,
) -> AdcResult<FleetTargetRunResult> {
    let endpoint = match validate_managed_mcp_endpoint(target) {
        Ok(endpoint) => endpoint,
        Err(failure) => {
            return Ok(FleetTargetRunResult::failed(
                failure.status,
                failure.message,
            ))
        }
    };
    let observe = match call_target_managed_mcp_tool(
        target,
        "obs.observe",
        serde_json::json!({
            "run_id": request.run_id,
            "target_id": target.id,
            "profile_id": request.profile_id,
            "duration_ms": request.duration.as_millis() as u64,
            "interval_ms": request.interval.as_millis() as u64,
        }),
        target_mcp_capture_timeout(request),
    ) {
        Ok(call) => call,
        Err(failure) => {
            return Ok(FleetTargetRunResult::failed(
                failure.status,
                failure.message,
            ))
        }
    };
    if observe.structured_content.is_none() {
        return Ok(FleetTargetRunResult::failed(
            "mcp_protocol_error",
            "obs.observe did not return structuredContent",
        ));
    }
    let evidence = match call_target_managed_mcp_tool(
        target,
        "obs.get_evidence_index",
        serde_json::json!({
            "run_id": request.run_id,
        }),
        Duration::from_secs(15),
    ) {
        Ok(call) => call,
        Err(failure) => {
            return Ok(FleetTargetRunResult::failed(
                failure.status,
                failure.message,
            ))
        }
    };
    let Some(evidence_text) = evidence.text_content else {
        return Ok(FleetTargetRunResult::failed(
            "mcp_protocol_error",
            "obs.get_evidence_index did not return text content",
        ));
    };
    Ok(managed_mcp_captured_result(
        request,
        evidence_text,
        endpoint,
    ))
}

fn managed_mcp_captured_result(
    request: &FleetTargetRequest,
    evidence_text: String,
    endpoint: String,
) -> FleetTargetRunResult {
    let mut data_quality = DataQuality {
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };
    data_quality
        .notes
        .push(format!("managed_mcp_endpoint={endpoint}"));
    data_quality.notes.push("transport=managed_mcp".to_string());
    FleetTargetRunResult {
        status: "captured".to_string(),
        run_id: Some(request.run_id.clone()),
        evidence_text: Some(evidence_text),
        evidence_ref: None,
        profile_id: Some(request.profile_id.clone()),
        capability_ref: None,
        artifact_ref: Some(format!("managed+mcp://{endpoint}/runs/{}", request.run_id)),
        data_quality,
    }
}

pub(super) fn investigate_service_for_fleet_target(
    artifact_root: &Path,
    target: &FleetTargetConfig,
    service_name: &str,
    max_journal_lines: usize,
) -> Result<ServiceInvestigationPack, TargetMcpFailure> {
    match target.transport.as_str() {
        "local" => crate::investigate_service(
            artifact_root,
            ServiceInvestigationRequest {
                service_name: service_name.to_string(),
                max_journal_lines,
            },
        )
        .map_err(|err| TargetMcpFailure {
            status: "collector_failed".to_string(),
            message: err.to_string(),
        }),
        "mcp_stdio_over_ssh" => {
            let call = call_target_mcp_tool(
                target,
                "obs.investigate_service",
                serde_json::json!({
                    "service_name": service_name,
                    "max_journal_lines": max_journal_lines,
                }),
                Duration::from_secs(12),
            )?;
            decode_service_investigation_call(call)
        }
        "managed_mcp" => {
            let call = call_target_managed_mcp_tool(
                target,
                "obs.investigate_service",
                serde_json::json!({
                    "service_name": service_name,
                    "max_journal_lines": max_journal_lines,
                }),
                Duration::from_secs(12),
            )?;
            decode_service_investigation_call(call)
        }
        transport => Err(TargetMcpFailure {
            status: "unsupported".to_string(),
            message: format!("transport {transport} is not supported for service investigation"),
        }),
    }
}

fn decode_service_investigation_call(
    call: TargetMcpToolCall,
) -> Result<ServiceInvestigationPack, TargetMcpFailure> {
    let Some(value) = call.structured_content else {
        return Err(TargetMcpFailure {
            status: "mcp_protocol_error".to_string(),
            message: "obs.investigate_service did not return structuredContent".to_string(),
        });
    };
    serde_json::from_value(value).map_err(|err| TargetMcpFailure {
        status: "mcp_protocol_error".to_string(),
        message: format!("obs.investigate_service structuredContent decode failed: {err}"),
    })
}

pub(super) fn preflight_target_mcp_over_ssh(
    target: &FleetTargetConfig,
) -> AdcResult<FleetPreflightTarget> {
    let Some(host) = target.host.as_deref() else {
        return Ok(preflight_model::failed_target(
            target,
            "unreachable",
            "target MCP-over-SSH endpoint is missing host",
            vec![FleetPreflightCheck::failed(
                "ssh_host_configured",
                "target MCP-over-SSH endpoint is missing host",
            )],
        ));
    };
    if let Err(failure) = validate_target_mcp_ssh_endpoint(target) {
        return Ok(preflight_model::failed_target(
            target,
            failure.status,
            failure.message.clone(),
            vec![FleetPreflightCheck::failed(
                "target_mcp_endpoint",
                failure.message,
            )],
        ));
    }

    let mut checks = vec![
        FleetPreflightCheck::ok("transport_supported"),
        FleetPreflightCheck::ok("ssh_host_configured"),
        FleetPreflightCheck::ok("target_mcp_server_configured"),
    ];

    for (name, tool, arguments, timeout) in [
        (
            "remote_obs_status",
            "obs.status",
            serde_json::json!({}),
            Duration::from_secs(8),
        ),
        (
            "remote_obs_doctor",
            "obs.doctor",
            serde_json::json!({}),
            Duration::from_secs(10),
        ),
        (
            "remote_obs_preflight",
            "obs.preflight",
            serde_json::json!({"target_id": target.id}),
            Duration::from_secs(10),
        ),
    ] {
        let output = match call_target_mcp_tool(target, tool, arguments, timeout) {
            Ok(output) => output,
            Err(failure) => {
                checks.push(FleetPreflightCheck::failed(name, failure.message.clone()));
                return Ok(preflight_model::failed_target(
                    target,
                    failure.status,
                    failure.message,
                    checks,
                ));
            }
        };
        if tool == "obs.preflight" {
            let remote_status = output
                .structured_content
                .as_ref()
                .and_then(|value| value.get("status"))
                .and_then(|status| status.as_str())
                .map(str::to_string);
            if remote_status.as_deref() != Some("ready") {
                let detail = format!(
                    "remote obs.preflight status={}",
                    remote_status.unwrap_or_else(|| "unreadable".to_string())
                );
                checks.push(FleetPreflightCheck::failed(name, detail.clone()));
                return Ok(preflight_model::failed_target(
                    target,
                    "artifact_unwritable",
                    detail,
                    checks,
                ));
            }
        }
        checks.push(FleetPreflightCheck::ok(name));
    }

    let mut result = preflight_model::ready_target(target, checks);
    result.data_quality.notes.push(format!("ssh_host={host}"));
    result
        .data_quality
        .notes
        .push("transport=mcp_stdio_over_ssh".to_string());
    Ok(result)
}

pub(super) fn preflight_target_managed_mcp(
    target: &FleetTargetConfig,
) -> AdcResult<FleetPreflightTarget> {
    let endpoint = match validate_managed_mcp_endpoint(target) {
        Ok(endpoint) => endpoint,
        Err(failure) => {
            return Ok(preflight_model::failed_target(
                target,
                failure.status,
                failure.message.clone(),
                vec![FleetPreflightCheck::failed(
                    "managed_mcp_endpoint",
                    failure.message,
                )],
            ))
        }
    };

    let mut checks = vec![
        FleetPreflightCheck::ok("transport_supported"),
        FleetPreflightCheck::ok("managed_mcp_host_configured"),
        FleetPreflightCheck::ok("managed_mcp_port_configured"),
        FleetPreflightCheck::ok("managed_mcp_token_configured"),
    ];
    for (name, tool, arguments, timeout) in [
        (
            "remote_obs_status",
            "obs.status",
            serde_json::json!({}),
            Duration::from_secs(5),
        ),
        (
            "remote_obs_doctor",
            "obs.doctor",
            serde_json::json!({}),
            Duration::from_secs(8),
        ),
        (
            "remote_obs_preflight",
            "obs.preflight",
            serde_json::json!({"target_id": target.id}),
            Duration::from_secs(8),
        ),
    ] {
        let output = match call_target_managed_mcp_tool(target, tool, arguments, timeout) {
            Ok(output) => output,
            Err(failure) => {
                checks.push(FleetPreflightCheck::failed(name, failure.message.clone()));
                return Ok(preflight_model::failed_target(
                    target,
                    failure.status,
                    failure.message,
                    checks,
                ));
            }
        };
        if tool == "obs.preflight" {
            let remote_status = output
                .structured_content
                .as_ref()
                .and_then(|value| value.get("status"))
                .and_then(|status| status.as_str())
                .map(str::to_string);
            if remote_status.as_deref() != Some("ready") {
                let detail = format!(
                    "remote obs.preflight status={}",
                    remote_status.unwrap_or_else(|| "unreadable".to_string())
                );
                checks.push(FleetPreflightCheck::failed(name, detail.clone()));
                return Ok(preflight_model::failed_target(
                    target,
                    "artifact_unwritable",
                    detail,
                    checks,
                ));
            }
        }
        checks.push(FleetPreflightCheck::ok(name));
    }
    let mut result = preflight_model::ready_target(target, checks);
    result
        .data_quality
        .notes
        .push(format!("managed_mcp_endpoint={endpoint}"));
    result
        .data_quality
        .notes
        .push("transport=managed_mcp".to_string());
    Ok(result)
}

fn build_ssh_destination_args(target: &FleetTargetConfig) -> AdcResult<Vec<String>> {
    let host = target
        .host
        .as_deref()
        .ok_or_else(|| AdcError::ProfileValidation("ssh target host is required".to_string()))?;
    let destination = match target.user.as_deref() {
        Some(user) => format!("{user}@{host}"),
        None => host.to_string(),
    };
    let mut args = vec![
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "ConnectTimeout=5".to_string(),
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
    ];
    if let Some(port) = target.port {
        args.push("-p".to_string());
        args.push(port.to_string());
    }
    args.push(destination);
    Ok(args)
}

fn build_target_mcp_ssh_args(target: &FleetTargetConfig) -> AdcResult<Vec<String>> {
    let mut args = build_ssh_destination_args(target)?;
    let server = target.mcp_server_path.as_deref().unwrap_or("adc-mcp");
    validate_remote_mcp_server_path(server)?;
    args.extend([server.to_string(), "--target-mode".to_string()]);
    Ok(args)
}

fn validate_target_mcp_ssh_endpoint(target: &FleetTargetConfig) -> Result<(), TargetMcpFailure> {
    let host = target.host.as_deref().ok_or_else(|| TargetMcpFailure {
        status: "unreachable".to_string(),
        message: "target MCP-over-SSH endpoint is missing host".to_string(),
    })?;
    validate_ssh_host(host).map_err(|err| TargetMcpFailure {
        status: "invalid_inventory".to_string(),
        message: err.to_string(),
    })?;
    if let Some(user) = target.user.as_deref() {
        validate_ssh_user(user).map_err(|err| TargetMcpFailure {
            status: "invalid_inventory".to_string(),
            message: err.to_string(),
        })?;
    }
    if let Some(server) = target.mcp_server_path.as_deref() {
        validate_remote_mcp_server_path(server).map_err(|err| TargetMcpFailure {
            status: "invalid_inventory".to_string(),
            message: err.to_string(),
        })?;
    }
    Ok(())
}

fn validate_remote_mcp_server_path(value: &str) -> AdcResult<()> {
    if value.trim().is_empty()
        || value.starts_with('-')
        || value
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, '"' | '\'' | '`' | '$' | ';' | '|'))
    {
        return Err(AdcError::ProfileValidation(
            "mcp_server_path must be a plain executable path".to_string(),
        ));
    }
    Ok(())
}

fn validate_managed_mcp_endpoint(target: &FleetTargetConfig) -> Result<String, TargetMcpFailure> {
    let host = target.host.as_deref().ok_or_else(|| TargetMcpFailure {
        status: "invalid_inventory".to_string(),
        message: "managed_mcp target requires host".to_string(),
    })?;
    validate_ssh_host(host).map_err(|err| TargetMcpFailure {
        status: "invalid_inventory".to_string(),
        message: err.to_string(),
    })?;
    let port = target.port.ok_or_else(|| TargetMcpFailure {
        status: "invalid_inventory".to_string(),
        message: "managed_mcp target requires port".to_string(),
    })?;
    let token_file = target
        .auth_token_file
        .as_deref()
        .ok_or_else(|| TargetMcpFailure {
            status: "invalid_inventory".to_string(),
            message: "managed_mcp target requires auth_token_file".to_string(),
        })?;
    validate_remote_mcp_server_path(token_file).map_err(|err| TargetMcpFailure {
        status: "invalid_inventory".to_string(),
        message: err.to_string(),
    })?;
    validate_managed_mcp_tls_config(target)?;
    Ok(format!("{host}:{port}"))
}

fn validate_managed_mcp_tls_config(target: &FleetTargetConfig) -> Result<(), TargetMcpFailure> {
    let tls_enabled = target.tls_ca_file.is_some()
        || target.tls_client_cert_file.is_some()
        || target.tls_client_key_file.is_some()
        || target.tls_server_name.is_some();
    if !tls_enabled {
        return Ok(());
    }
    for (field, value) in [
        ("tls_ca_file", target.tls_ca_file.as_deref()),
        (
            "tls_client_cert_file",
            target.tls_client_cert_file.as_deref(),
        ),
        ("tls_client_key_file", target.tls_client_key_file.as_deref()),
    ] {
        let Some(path) = value else {
            return Err(TargetMcpFailure {
                status: "invalid_inventory".to_string(),
                message: format!("managed_mcp mTLS target requires {field}"),
            });
        };
        validate_remote_mcp_server_path(path).map_err(|err| TargetMcpFailure {
            status: "invalid_inventory".to_string(),
            message: err.to_string(),
        })?;
    }
    if let Some(server_name) = target.tls_server_name.as_deref() {
        validate_ssh_host(server_name).map_err(|err| TargetMcpFailure {
            status: "invalid_inventory".to_string(),
            message: err.to_string(),
        })?;
    }
    Ok(())
}

fn call_target_managed_mcp_tool(
    target: &FleetTargetConfig,
    tool_name: &str,
    arguments: serde_json::Value,
    timeout: Duration,
) -> Result<TargetMcpToolCall, TargetMcpFailure> {
    let endpoint = validate_managed_mcp_endpoint(target)?;
    let token_file = target
        .auth_token_file
        .as_deref()
        .expect("validated auth_token_file");
    let token = fs::read_to_string(token_file).map_err(|err| TargetMcpFailure {
        status: "permission_denied".to_string(),
        message: format!("managed_mcp auth token file is not readable: {err}"),
    })?;
    let token = token.trim();
    if token.is_empty() {
        return Err(TargetMcpFailure {
            status: "permission_denied".to_string(),
            message: "managed_mcp auth token file is empty".to_string(),
        });
    }
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": arguments
        }
    });
    let response = if target.tls_ca_file.is_some() {
        managed_mcp_https_jsonrpc(target, &endpoint, token, &request, timeout)?
    } else {
        managed_mcp_http_jsonrpc(&endpoint, token, &request, timeout)?
    };
    parse_target_mcp_tool_response(tool_name, &response)
}

fn managed_mcp_https_jsonrpc(
    target: &FleetTargetConfig,
    endpoint: &str,
    token: &str,
    body: &serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, TargetMcpFailure> {
    let host = target.host.as_deref().ok_or_else(|| TargetMcpFailure {
        status: "invalid_inventory".to_string(),
        message: "managed_mcp target requires host".to_string(),
    })?;
    let server_name = target.tls_server_name.as_deref().unwrap_or(host);
    let config = managed_mcp_tls_client_config(target)?;
    let server_name =
        ServerName::try_from(server_name.to_string()).map_err(|err| TargetMcpFailure {
            status: "invalid_inventory".to_string(),
            message: format!("managed_mcp tls_server_name is invalid: {err}"),
        })?;
    let tcp = connect_managed_mcp_tcp(endpoint, timeout)?;
    let connection =
        ClientConnection::new(Arc::new(config), server_name).map_err(|err| TargetMcpFailure {
            status: "mcp_tls_failed".to_string(),
            message: format!("managed_mcp TLS client setup failed: {err}"),
        })?;
    let mut stream = StreamOwned::new(connection, tcp);
    write_managed_mcp_jsonrpc_request(&mut stream, endpoint, token, body)?;
    let response = read_managed_mcp_response(&mut stream, "mcp_tls_failed")?;
    parse_managed_mcp_http_response(&response)
}

fn managed_mcp_tls_client_config(
    target: &FleetTargetConfig,
) -> Result<ClientConfig, TargetMcpFailure> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut root_store = RootCertStore::empty();
    let ca_file = target
        .tls_ca_file
        .as_deref()
        .ok_or_else(|| TargetMcpFailure {
            status: "invalid_inventory".to_string(),
            message: "managed_mcp mTLS target requires tls_ca_file".to_string(),
        })?;
    let ca_certs = load_tls_certs(ca_file, "tls_ca_file")?;
    for cert in ca_certs {
        root_store.add(cert).map_err(|err| TargetMcpFailure {
            status: "permission_denied".to_string(),
            message: format!("managed_mcp TLS CA certificate was rejected: {err}"),
        })?;
    }
    let cert_file = target
        .tls_client_cert_file
        .as_deref()
        .ok_or_else(|| TargetMcpFailure {
            status: "invalid_inventory".to_string(),
            message: "managed_mcp mTLS target requires tls_client_cert_file".to_string(),
        })?;
    let key_file = target
        .tls_client_key_file
        .as_deref()
        .ok_or_else(|| TargetMcpFailure {
            status: "invalid_inventory".to_string(),
            message: "managed_mcp mTLS target requires tls_client_key_file".to_string(),
        })?;
    let client_certs = load_tls_certs(cert_file, "tls_client_cert_file")?;
    let client_key = load_tls_private_key(key_file, "tls_client_key_file")?;
    ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_client_auth_cert(client_certs, client_key)
        .map_err(|err| TargetMcpFailure {
            status: "permission_denied".to_string(),
            message: format!("managed_mcp TLS client certificate rejected: {err}"),
        })
}

fn load_tls_certs(
    path: &str,
    label: &str,
) -> Result<Vec<CertificateDer<'static>>, TargetMcpFailure> {
    let file = fs::File::open(path).map_err(|err| TargetMcpFailure {
        status: "permission_denied".to_string(),
        message: format!("managed_mcp {label} is not readable: {err}"),
    })?;
    let mut reader = BufReader::new(file);
    let certs = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| TargetMcpFailure {
            status: "permission_denied".to_string(),
            message: format!("managed_mcp {label} PEM parse failed: {err}"),
        })?;
    if certs.is_empty() {
        return Err(TargetMcpFailure {
            status: "permission_denied".to_string(),
            message: format!("managed_mcp {label} did not contain certificates"),
        });
    }
    Ok(certs)
}

fn load_tls_private_key(
    path: &str,
    label: &str,
) -> Result<PrivateKeyDer<'static>, TargetMcpFailure> {
    let file = fs::File::open(path).map_err(|err| TargetMcpFailure {
        status: "permission_denied".to_string(),
        message: format!("managed_mcp {label} is not readable: {err}"),
    })?;
    let mut reader = BufReader::new(file);
    rustls_pemfile::private_key(&mut reader)
        .map_err(|err| TargetMcpFailure {
            status: "permission_denied".to_string(),
            message: format!("managed_mcp {label} PEM parse failed: {err}"),
        })?
        .ok_or_else(|| TargetMcpFailure {
            status: "permission_denied".to_string(),
            message: format!("managed_mcp {label} did not contain a private key"),
        })
}

fn managed_mcp_http_jsonrpc(
    endpoint: &str,
    token: &str,
    body: &serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, TargetMcpFailure> {
    let mut stream = connect_managed_mcp_tcp(endpoint, timeout)?;
    write_managed_mcp_jsonrpc_request(&mut stream, endpoint, token, body)?;
    let response = read_managed_mcp_response(&mut BufReader::new(stream), "mcp_transport_failed")?;
    parse_managed_mcp_http_response(&response)
}

fn read_managed_mcp_response(
    reader: &mut impl Read,
    status: &str,
) -> Result<String, TargetMcpFailure> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => bytes.extend_from_slice(&buffer[..n]),
            Err(err) if err.kind() == ErrorKind::UnexpectedEof && !bytes.is_empty() => break,
            Err(err) => {
                return Err(TargetMcpFailure {
                    status: status.to_string(),
                    message: format!("managed_mcp response read failed: {err}"),
                });
            }
        }
    }
    String::from_utf8(bytes).map_err(|err| TargetMcpFailure {
        status: "mcp_protocol_error".to_string(),
        message: format!("managed_mcp response was not UTF-8: {err}"),
    })
}

fn connect_managed_mcp_tcp(
    endpoint: &str,
    timeout: Duration,
) -> Result<TcpStream, TargetMcpFailure> {
    let stream = TcpStream::connect(endpoint).map_err(|err| TargetMcpFailure {
        status: "unreachable".to_string(),
        message: format!("managed_mcp connect failed: {err}"),
    })?;
    stream
        .set_read_timeout(Some(timeout))
        .and_then(|_| stream.set_write_timeout(Some(timeout)))
        .map_err(|err| TargetMcpFailure {
            status: "mcp_transport_failed".to_string(),
            message: format!("managed_mcp timeout setup failed: {err}"),
        })?;
    Ok(stream)
}

fn write_managed_mcp_jsonrpc_request(
    stream: &mut impl Write,
    endpoint: &str,
    token: &str,
    body: &serde_json::Value,
) -> Result<(), TargetMcpFailure> {
    let body = body.to_string();
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: {endpoint}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|err| TargetMcpFailure {
            status: "mcp_transport_failed".to_string(),
            message: format!("managed_mcp request write failed: {err}"),
        })
}

fn parse_managed_mcp_http_response(response: &str) -> Result<serde_json::Value, TargetMcpFailure> {
    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| TargetMcpFailure {
            status: "mcp_protocol_error".to_string(),
            message: "managed_mcp response did not contain HTTP header boundary".to_string(),
        })?;
    let status_line = head.lines().next().unwrap_or_default();
    if status_line.contains(" 401 ") || status_line.contains(" 403 ") {
        return Err(TargetMcpFailure {
            status: "permission_denied".to_string(),
            message: "managed_mcp authentication failed".to_string(),
        });
    }
    if !status_line.contains(" 200 ") {
        return Err(TargetMcpFailure {
            status: "mcp_transport_failed".to_string(),
            message: format!("managed_mcp HTTP status failed: {status_line}"),
        });
    }
    serde_json::from_str(body).map_err(|err| TargetMcpFailure {
        status: "mcp_protocol_error".to_string(),
        message: format!("managed_mcp response JSON parse failed: {err}"),
    })
}

#[derive(Debug)]
struct TargetMcpToolCall {
    structured_content: Option<serde_json::Value>,
    text_content: Option<String>,
}

#[derive(Debug)]
pub(super) struct TargetMcpFailure {
    pub(super) status: String,
    pub(super) message: String,
}

#[derive(Debug)]
struct ProcessOutput {
    status_success: bool,
    exit_code: Option<i32>,
    stderr: String,
    timed_out: bool,
}

fn call_target_mcp_tool(
    target: &FleetTargetConfig,
    tool_name: &str,
    arguments: serde_json::Value,
    timeout: Duration,
) -> Result<TargetMcpToolCall, TargetMcpFailure> {
    let args = build_target_mcp_ssh_args(target).map_err(|err| TargetMcpFailure {
        status: "invalid_inventory".to_string(),
        message: err.to_string(),
    })?;
    let mut child = Command::new("ssh")
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| TargetMcpFailure {
            status: "mcp_transport_failed".to_string(),
            message: format!("failed to start ssh: {err}"),
        })?;
    let mut stdin = child.stdin.take().ok_or_else(|| TargetMcpFailure {
        status: "mcp_transport_failed".to_string(),
        message: "ssh stdin was not available".to_string(),
    })?;
    let stdout = child.stdout.take().ok_or_else(|| TargetMcpFailure {
        status: "mcp_transport_failed".to_string(),
        message: "ssh stdout was not available".to_string(),
    })?;
    let stderr = child.stderr.take().ok_or_else(|| TargetMcpFailure {
        status: "mcp_transport_failed".to_string(),
        message: "ssh stderr was not available".to_string(),
    })?;
    let (stdout_rx, stdout_handle) = spawn_target_mcp_stdout_reader(stdout);
    let stderr_handle = thread::spawn(move || {
        let mut stderr_text = String::new();
        let mut reader = BufReader::new(stderr);
        let _ = reader.read_to_string(&mut stderr_text);
        stderr_text
    });
    let mut stdout_text = String::new();
    let deadline = Instant::now() + timeout;

    if let Err(failure) = write_target_mcp_message(
        &mut stdin,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "adc-targetd-fleet", "version": crate::VERSION}
            }
        }),
    ) {
        drop(stdin);
        let output =
            collect_target_mcp_process(child, stdout_handle, stderr_handle, Duration::ZERO);
        return Err(target_mcp_failure_from_output(tool_name, failure, &output));
    }
    let initialize_response =
        match read_target_mcp_response(1, &stdout_rx, &mut stdout_text, deadline) {
            Ok(response) => response,
            Err(failure) => {
                drop(stdin);
                let output =
                    collect_target_mcp_process(child, stdout_handle, stderr_handle, Duration::ZERO);
                return Err(target_mcp_failure_from_output(tool_name, failure, &output));
            }
        };
    if let Some(error) = initialize_response.get("error") {
        drop(stdin);
        let _ =
            collect_target_mcp_process(child, stdout_handle, stderr_handle, Duration::from_secs(1));
        return Err(TargetMcpFailure {
            status: "mcp_protocol_error".to_string(),
            message: format!(
                "MCP initialize failed: {}",
                bounded_text(&error.to_string())
            ),
        });
    }

    if let Err(failure) = write_target_mcp_message(
        &mut stdin,
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
    ) {
        drop(stdin);
        let output =
            collect_target_mcp_process(child, stdout_handle, stderr_handle, Duration::ZERO);
        return Err(target_mcp_failure_from_output(tool_name, failure, &output));
    }
    thread::sleep(Duration::from_millis(10));
    if let Err(failure) = write_target_mcp_message(
        &mut stdin,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments
            }
        }),
    ) {
        drop(stdin);
        let output =
            collect_target_mcp_process(child, stdout_handle, stderr_handle, Duration::ZERO);
        return Err(target_mcp_failure_from_output(tool_name, failure, &output));
    }
    let tool_response = match read_target_mcp_response(2, &stdout_rx, &mut stdout_text, deadline) {
        Ok(response) => response,
        Err(failure) => {
            drop(stdin);
            let output =
                collect_target_mcp_process(child, stdout_handle, stderr_handle, Duration::ZERO);
            return Err(target_mcp_failure_from_output(tool_name, failure, &output));
        }
    };
    drop(stdin);
    let _ = collect_target_mcp_process(child, stdout_handle, stderr_handle, Duration::from_secs(2));
    parse_target_mcp_tool_response(tool_name, &tool_response)
}

fn parse_target_mcp_tool_response(
    tool_name: &str,
    response: &serde_json::Value,
) -> Result<TargetMcpToolCall, TargetMcpFailure> {
    if let Some(error) = response.get("error") {
        return Err(TargetMcpFailure {
            status: "mcp_tool_failed".to_string(),
            message: format!(
                "{tool_name} MCP error: {}",
                bounded_text(&error.to_string())
            ),
        });
    }
    let result = response.get("result").ok_or_else(|| TargetMcpFailure {
        status: "mcp_protocol_error".to_string(),
        message: format!("{tool_name} response did not contain result"),
    })?;
    let structured_content = result.get("structuredContent").cloned();
    let text_content = result
        .get("content")
        .and_then(|content| content.as_array())
        .and_then(|content| {
            content
                .iter()
                .find_map(|item| item.get("text").and_then(|text| text.as_str()))
        })
        .map(str::to_string);
    Ok(TargetMcpToolCall {
        structured_content,
        text_content,
    })
}

fn write_target_mcp_message(
    stdin: &mut impl Write,
    message: serde_json::Value,
) -> Result<(), TargetMcpFailure> {
    stdin
        .write_all(message.to_string().as_bytes())
        .and_then(|_| stdin.write_all(b"\n"))
        .and_then(|_| stdin.flush())
        .map_err(|err| TargetMcpFailure {
            status: "mcp_transport_failed".to_string(),
            message: format!("failed to write MCP request: {err}"),
        })
}

fn spawn_target_mcp_stdout_reader(
    stdout: impl Read + Send + 'static,
) -> (
    mpsc::Receiver<Result<String, String>>,
    thread::JoinHandle<()>,
) {
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if tx.send(Ok(line)).is_err() {
                        break;
                    }
                }
                Err(err) => {
                    let _ = tx.send(Err(err.to_string()));
                    break;
                }
            }
        }
    });
    (rx, handle)
}

fn read_target_mcp_response(
    response_id: i64,
    stdout_rx: &mpsc::Receiver<Result<String, String>>,
    stdout_text: &mut String,
    deadline: Instant,
) -> Result<serde_json::Value, TargetMcpFailure> {
    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err(TargetMcpFailure {
                status: "unreachable".to_string(),
                message: format!("MCP response id={response_id} timed out"),
            });
        }
        let wait = std::cmp::min(
            deadline.saturating_duration_since(now),
            Duration::from_millis(50),
        );
        match stdout_rx.recv_timeout(wait) {
            Ok(Ok(line)) => {
                stdout_text.push_str(&line);
                stdout_text.push('\n');
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                    if value.get("id").and_then(|id| id.as_i64()) == Some(response_id) {
                        return Ok(value);
                    }
                }
            }
            Ok(Err(err)) => {
                return Err(TargetMcpFailure {
                    status: "mcp_transport_failed".to_string(),
                    message: format!("failed to read MCP stdout: {err}"),
                });
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err(TargetMcpFailure {
                    status: "mcp_protocol_error".to_string(),
                    message: format!("MCP stdio ended before response id={response_id}"),
                });
            }
        }
    }
}

fn collect_target_mcp_process(
    mut child: Child,
    stdout_handle: thread::JoinHandle<()>,
    stderr_handle: thread::JoinHandle<String>,
    wait_timeout: Duration,
) -> ProcessOutput {
    let started = Instant::now();
    let mut stderr_note = String::new();
    let mut timed_out = false;
    let (status_success, exit_code) = loop {
        match child.try_wait() {
            Ok(Some(status)) => break (status.success(), status.code()),
            Ok(None) if started.elapsed() >= wait_timeout => {
                timed_out = true;
                let _ = child.kill();
                match child.wait() {
                    Ok(status) => break (false, status.code()),
                    Err(err) => {
                        stderr_note = format!("failed to wait for ssh after timeout: {err}");
                        break (false, None);
                    }
                }
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(err) => {
                stderr_note = format!("failed to poll ssh: {err}");
                let _ = child.kill();
                match child.wait() {
                    Ok(status) => break (false, status.code()),
                    Err(wait_err) => {
                        if !stderr_note.is_empty() {
                            stderr_note.push_str("; ");
                        }
                        stderr_note.push_str(&format!("failed to wait for ssh: {wait_err}"));
                        break (false, None);
                    }
                }
            }
        }
    };
    let _ = stdout_handle.join();
    let mut stderr = stderr_handle
        .join()
        .unwrap_or_else(|_| "stderr reader panicked".to_string());
    if !stderr_note.is_empty() {
        if !stderr.is_empty() {
            stderr.push_str("; ");
        }
        stderr.push_str(&stderr_note);
    }
    ProcessOutput {
        status_success: status_success && !timed_out,
        exit_code,
        stderr: bounded_text(&stderr),
        timed_out,
    }
}

fn target_mcp_failure_from_output(
    tool_name: &str,
    fallback: TargetMcpFailure,
    output: &ProcessOutput,
) -> TargetMcpFailure {
    if !output.status_success || output.timed_out {
        let (status, message) = classify_target_mcp_ssh_failure(tool_name, output);
        TargetMcpFailure { status, message }
    } else {
        fallback
    }
}

fn target_mcp_capture_timeout(request: &FleetTargetRequest) -> Duration {
    let expected_samples = request
        .duration
        .as_millis()
        .checked_div(request.interval.as_millis().max(1))
        .unwrap_or(0)
        .saturating_add(1);
    let sample_slack = Duration::from_millis((expected_samples as u64).saturating_mul(50));
    let timeout = request
        .duration
        .saturating_add(Duration::from_secs(10))
        .saturating_add(sample_slack);
    std::cmp::max(timeout, Duration::from_secs(30))
}

fn classify_target_mcp_ssh_failure(stage: &str, output: &ProcessOutput) -> (String, String) {
    let stderr = output.stderr.to_ascii_lowercase();
    let status = if stderr.contains("permission denied")
        || stderr.contains("publickey")
        || stderr.contains("authentication")
    {
        "permission_denied"
    } else if stderr.contains("command not found")
        || stderr.contains("adc-mcp: not found")
        || output.exit_code == Some(127)
    {
        "missing_binary"
    } else if output.timed_out
        || stderr.contains("could not resolve")
        || stderr.contains("connection timed out")
        || stderr.contains("connection refused")
        || stderr.contains("no route")
    {
        "unreachable"
    } else if stage == "obs.get_evidence_index" {
        "artifact_unavailable"
    } else {
        "mcp_transport_failed"
    };
    let code = output
        .exit_code
        .map_or_else(|| "signal".to_string(), |code| code.to_string());
    (
        status.to_string(),
        format!(
            "{stage} command failed with exit={code}: {}",
            bounded_text(&output.stderr)
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target_mcp_request(duration: Duration, interval: Duration) -> FleetTargetRequest {
        FleetTargetRequest {
            fleet_run_id: "F-TEST".to_string(),
            run_id: "F-TEST-target".to_string(),
            profile_id: "mcp_observe".to_string(),
            duration,
            interval,
        }
    }

    #[test]
    fn target_mcp_capture_timeout_exceeds_requested_capture_window() {
        assert_eq!(
            target_mcp_capture_timeout(&target_mcp_request(
                Duration::from_secs(5),
                Duration::from_millis(500)
            )),
            Duration::from_secs(30)
        );
        assert!(
            target_mcp_capture_timeout(&target_mcp_request(
                Duration::from_secs(30),
                Duration::from_millis(500)
            )) > Duration::from_secs(30)
        );
        assert!(
            target_mcp_capture_timeout(&target_mcp_request(
                Duration::from_secs(120),
                Duration::from_secs(1)
            )) > Duration::from_secs(120)
        );
    }

    #[test]
    fn target_mcp_failure_classification_distinguishes_auth_and_missing_binary() {
        let auth = ProcessOutput {
            status_success: false,
            exit_code: Some(255),
            stderr: "Permission denied (publickey).".to_string(),
            timed_out: false,
        };
        assert_eq!(
            classify_target_mcp_ssh_failure("remote_obs_status", &auth).0,
            "permission_denied"
        );

        let missing_binary = ProcessOutput {
            status_success: false,
            exit_code: Some(127),
            stderr: "adc-mcp: not found".to_string(),
            timed_out: false,
        };
        assert_eq!(
            classify_target_mcp_ssh_failure("remote_obs_status", &missing_binary).0,
            "missing_binary"
        );

        let evidence_failure = ProcessOutput {
            status_success: false,
            exit_code: Some(2),
            stderr: "evidence index not found".to_string(),
            timed_out: false,
        };
        assert_eq!(
            classify_target_mcp_ssh_failure("obs.get_evidence_index", &evidence_failure).0,
            "artifact_unavailable"
        );
    }

    #[test]
    fn target_mcp_response_reader_waits_for_requested_json_rpc_id() {
        let (tx, rx) = mpsc::channel();
        tx.send(Ok(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {"protocolVersion": "2025-03-26"}
        })
        .to_string()))
            .expect("send initialize response");
        tx.send(Ok(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "structuredContent": {"status": "ready"},
                "content": [{"type": "text", "text": "ok"}]
            }
        })
        .to_string()))
            .expect("send tool response");

        let mut stdout = String::new();
        let deadline = Instant::now() + Duration::from_secs(1);
        let initialize =
            read_target_mcp_response(1, &rx, &mut stdout, deadline).expect("initialize response");
        let tool = read_target_mcp_response(2, &rx, &mut stdout, deadline).expect("tool response");
        let parsed = parse_target_mcp_tool_response("obs.preflight", &tool).expect("parsed tool");

        assert_eq!(
            initialize["result"]["protocolVersion"].as_str(),
            Some("2025-03-26")
        );
        assert_eq!(
            parsed
                .structured_content
                .as_ref()
                .and_then(|content| content["status"].as_str()),
            Some("ready")
        );
        assert_eq!(parsed.text_content.as_deref(), Some("ok"));
        assert!(stdout.contains("\"id\":1"));
        assert!(stdout.contains("\"id\":2"));
    }

    #[test]
    fn target_mcp_uses_configured_server_path_without_shell_metacharacters() {
        let target = FleetTargetConfig {
            id: "mcp-a".to_string(),
            transport: "mcp_stdio_over_ssh".to_string(),
            host: Some("example-target".to_string()),
            user: None,
            port: None,
            profile: None,
            mcp_server_path: Some("/home/pi/.local/bin/adc-mcp".to_string()),
            auth_token_file: None,
            tls_ca_file: None,
            tls_client_cert_file: None,
            tls_client_key_file: None,
            tls_server_name: None,
        };

        let args = build_target_mcp_ssh_args(&target).expect("ssh args");

        assert!(args.contains(&"/home/pi/.local/bin/adc-mcp".to_string()));
        assert!(args.contains(&"--target-mode".to_string()));
        assert!(!args.contains(&"adc".to_string()));

        let unsafe_target = FleetTargetConfig {
            mcp_server_path: Some("adc-mcp; rm -rf /".to_string()),
            ..target
        };
        assert!(build_target_mcp_ssh_args(&unsafe_target).is_err());
    }
}
