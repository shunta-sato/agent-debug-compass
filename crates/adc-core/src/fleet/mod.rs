use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};

use crate::{AdcError, AdcResult, DataQuality, InformationDebt, ServiceInvestigationPack};

mod preflight_model;
mod transport;

use transport::{
    capture_local_target, capture_target_managed_mcp, capture_target_mcp_over_ssh,
    investigate_service_for_fleet_target, preflight_target_managed_mcp,
    preflight_target_mcp_over_ssh, snapshot_local_target, snapshot_target_managed_mcp,
    snapshot_target_mcp_over_ssh,
};

#[derive(Debug, Clone)]
pub struct FleetCaptureOptions {
    pub fleet_run_id: String,
    pub duration: Duration,
    pub interval: Duration,
}

#[derive(Debug, Clone)]
pub struct FleetSnapshotOptions {
    pub fleet_run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetServiceInvestigationOptions {
    pub fleet_run_id: String,
    pub service_name: String,
    pub max_journal_lines: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetPreflightResult {
    pub schema_version: String,
    pub inventory_path: PathBuf,
    pub status: String,
    pub root_required: bool,
    pub inventory_bytes: u64,
    pub target_count: usize,
    pub ready_count: usize,
    pub failed_count: usize,
    pub checks: Vec<FleetPreflightCheck>,
    pub targets: Vec<FleetPreflightTarget>,
    pub data_quality: DataQuality,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetPreflightTarget {
    pub target_id: String,
    pub transport: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    pub checks: Vec<FleetPreflightCheck>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetPreflightCheck {
    pub name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetTargetRequest {
    pub fleet_run_id: String,
    pub run_id: String,
    pub profile_id: String,
    pub duration: Duration,
    pub interval: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetTargetRunResult {
    pub status: String,
    pub run_id: Option<String>,
    pub evidence_text: Option<String>,
    pub evidence_ref: Option<String>,
    pub profile_id: Option<String>,
    pub capability_ref: Option<String>,
    pub artifact_ref: Option<String>,
    pub data_quality: DataQuality,
}

impl FleetTargetRunResult {
    pub fn captured_evidence_text(
        run_id: impl Into<String>,
        profile_id: impl Into<String>,
        evidence_text: impl Into<String>,
    ) -> Self {
        Self {
            status: "captured".to_string(),
            run_id: Some(run_id.into()),
            evidence_text: Some(evidence_text.into()),
            evidence_ref: None,
            profile_id: Some(profile_id.into()),
            capability_ref: None,
            artifact_ref: None,
            data_quality: DataQuality {
                clock_confidence: "medium".to_string(),
                ..Default::default()
            },
        }
    }

    pub fn failed(status: impl Into<String>, message: impl Into<String>) -> Self {
        let status = status.into();
        Self {
            status: status.clone(),
            run_id: None,
            evidence_text: None,
            evidence_ref: None,
            profile_id: None,
            capability_ref: None,
            artifact_ref: None,
            data_quality: DataQuality {
                missing: vec![format!("{}: {}", status, message.into())],
                clock_confidence: "medium".to_string(),
                ..Default::default()
            },
        }
    }
}

pub trait FleetTargetRunner {
    fn preflight(&self, target: &FleetTargetConfig) -> AdcResult<FleetPreflightTarget> {
        Ok(preflight_model::failed_target(
            target,
            "unsupported",
            "preflight is not supported by this runner",
            vec![FleetPreflightCheck::failed(
                "transport_supported",
                "preflight is not supported by this runner",
            )],
        ))
    }

    fn snapshot(
        &self,
        artifact_root: &Path,
        target: &FleetTargetConfig,
        request: &FleetTargetRequest,
    ) -> AdcResult<FleetTargetRunResult> {
        let _ = (artifact_root, target, request);
        Ok(FleetTargetRunResult::failed(
            "unsupported",
            "snapshot is not supported by this runner",
        ))
    }

    fn capture(
        &self,
        artifact_root: &Path,
        target: &FleetTargetConfig,
        request: &FleetTargetRequest,
    ) -> AdcResult<FleetTargetRunResult>;
}

#[derive(Debug, Clone, Default)]
pub struct DefaultFleetTargetRunner;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetCaptureResult {
    pub fleet_run_id: String,
    pub target_count: usize,
    pub captured_count: usize,
    pub failed_count: usize,
    pub evidence_path: PathBuf,
    pub targets: Vec<FleetTargetEvidence>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetEvidence {
    pub schema_version: String,
    pub fleet_run_id: String,
    pub target_count: usize,
    pub captured_count: usize,
    pub failed_count: usize,
    pub target_matrix: Vec<FleetTargetEvidence>,
    pub cross_target_salience: Vec<String>,
    pub information_debt: Vec<InformationDebt>,
    pub raw_refs: BTreeMap<String, String>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetServiceInvestigationResult {
    pub schema_version: String,
    pub fleet_run_id: String,
    pub service_name: String,
    pub target_count: usize,
    pub captured_count: usize,
    pub failed_count: usize,
    pub targets: Vec<FleetServiceInvestigationTarget>,
    pub raw_refs: BTreeMap<String, String>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetServiceInvestigationTarget {
    pub target_id: String,
    pub transport: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_pack: Option<ServiceInvestigationPack>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetTargetEvidence {
    pub target_id: String,
    pub transport: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_ref: Option<String>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Deserialize)]
struct FleetInventory {
    targets: Vec<FleetTargetConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetTargetConfig {
    pub id: String,
    pub transport: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_server_path: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_token_file: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_ca_file: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_client_cert_file: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_client_key_file: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_server_name: Option<String>,
}

pub fn capture_fleet(
    artifact_root: impl AsRef<Path>,
    inventory_path: impl AsRef<Path>,
    options: FleetCaptureOptions,
) -> AdcResult<FleetCaptureResult> {
    capture_fleet_with_runner(
        artifact_root,
        inventory_path,
        options,
        &DefaultFleetTargetRunner,
    )
}

pub fn preflight_fleet(inventory_path: impl AsRef<Path>) -> AdcResult<FleetPreflightResult> {
    preflight_fleet_with_runner(inventory_path, &DefaultFleetTargetRunner)
}

pub fn preflight_fleet_with_runner(
    inventory_path: impl AsRef<Path>,
    runner: &impl FleetTargetRunner,
) -> AdcResult<FleetPreflightResult> {
    let inventory_path = inventory_path.as_ref();
    let metadata = fs::metadata(inventory_path).map_err(|err| {
        AdcError::ProfileParse(format!(
            "fleet inventory is not readable: {}: {err}",
            inventory_path.display()
        ))
    })?;
    let inventory = read_inventory(inventory_path)?;
    if inventory.targets.is_empty() {
        return Err(AdcError::ProfileValidation(
            "fleet inventory must contain at least one target".to_string(),
        ));
    }

    let mut checks = vec![FleetPreflightCheck::ok("inventory_readable")];
    checks.push(FleetPreflightCheck {
        name: "inventory_targets".to_string(),
        status: "ok".to_string(),
        detail: Some(format!("{} target(s)", inventory.targets.len())),
    });

    let mut data_quality = DataQuality {
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };
    let mut targets = Vec::new();
    for target in inventory.targets {
        validate_segment(&target.id, "target id")?;
        validate_segment(&target.transport, "target transport")?;
        let target_result = runner.preflight(&target)?;
        if target_result.status != "ready" {
            data_quality.missing.push(format!(
                "target {} preflight: {}",
                target.id, target_result.status
            ));
        }
        merge_data_quality(&mut data_quality, &target_result.data_quality);
        targets.push(target_result);
    }

    let ready_count = targets
        .iter()
        .filter(|target| target.status == "ready")
        .count();
    let failed_count = targets.len().saturating_sub(ready_count);
    let status = match (ready_count, failed_count) {
        (_, 0) => "ready",
        (0, _) => "failed",
        _ => "degraded",
    }
    .to_string();
    let next_actions = preflight_model::next_actions(&status);

    Ok(FleetPreflightResult {
        schema_version: "obs.fleet_preflight.v1".to_string(),
        inventory_path: inventory_path.to_path_buf(),
        status,
        root_required: false,
        inventory_bytes: metadata.len(),
        target_count: targets.len(),
        ready_count,
        failed_count,
        checks,
        targets,
        data_quality,
        next_actions,
    })
}

pub fn capture_fleet_with_runner(
    artifact_root: impl AsRef<Path>,
    inventory_path: impl AsRef<Path>,
    options: FleetCaptureOptions,
    runner: &impl FleetTargetRunner,
) -> AdcResult<FleetCaptureResult> {
    validate_segment(&options.fleet_run_id, "fleet_run_id")?;
    if options.duration.is_zero() {
        return Err(AdcError::ProfileValidation(
            "fleet capture duration must be greater than zero".to_string(),
        ));
    }
    if options.interval.is_zero() {
        return Err(AdcError::ProfileValidation(
            "fleet capture interval must be greater than zero".to_string(),
        ));
    }
    let inventory = read_inventory(inventory_path)?;
    if inventory.targets.is_empty() {
        return Err(AdcError::ProfileValidation(
            "fleet inventory must contain at least one target".to_string(),
        ));
    }

    let artifact_root = artifact_root.as_ref();
    let mut matrix = Vec::new();
    let mut raw_refs = BTreeMap::new();
    let mut data_quality = DataQuality {
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };

    for target in inventory.targets {
        validate_segment(&target.id, "target id")?;
        validate_segment(&target.transport, "target transport")?;
        let profile_id = profile_id_for_target(&target);
        validate_segment(&profile_id, "profile id")?;
        let run_id = format!("{}-{}", options.fleet_run_id, target.id);
        let request = FleetTargetRequest {
            fleet_run_id: options.fleet_run_id.clone(),
            run_id: run_id.clone(),
            profile_id,
            duration: options.duration,
            interval: options.interval,
        };
        let mut outcome = runner.capture(artifact_root, &target, &request)?;
        if outcome.status != "captured" {
            outcome.data_quality.missing.push(format!(
                "target {}: {}",
                target.id,
                outcome.status.as_str()
            ));
        }
        if let Some(evidence_text) = outcome.evidence_text.take() {
            let evidence_path =
                fleet_target_evidence_path(artifact_root, &options.fleet_run_id, &target.id);
            write_text(&evidence_path, &evidence_text)?;
            outcome.evidence_ref = Some(format!(
                "artifact://{}",
                evidence_path
                    .strip_prefix(artifact_root)
                    .unwrap_or(&evidence_path)
                    .display()
            ));
        }
        if let Some(evidence_ref) = &outcome.evidence_ref {
            raw_refs.insert(
                format!("{}.evidence_index", target.id),
                evidence_ref.clone(),
            );
        }
        if let Some(artifact_ref) = &outcome.artifact_ref {
            raw_refs.insert(format!("{}.artifact", target.id), artifact_ref.clone());
        }
        merge_data_quality(&mut data_quality, &outcome.data_quality);
        matrix.push(FleetTargetEvidence {
            target_id: target.id,
            transport: target.transport,
            status: outcome.status,
            run_id: outcome.run_id,
            profile_id: outcome.profile_id,
            evidence_ref: outcome.evidence_ref,
            capability_ref: outcome.capability_ref,
            artifact_ref: outcome.artifact_ref,
            data_quality: outcome.data_quality,
        });
    }

    let captured_count = matrix
        .iter()
        .filter(|target| target.status == "captured")
        .count();
    let failed_count = matrix.len().saturating_sub(captured_count);
    let information_debt = data_quality
        .missing
        .iter()
        .enumerate()
        .map(|(index, missing)| InformationDebt {
            debt_id: format!("FD{:03}", index + 1),
            kind: "missing".to_string(),
            description: missing.clone(),
            impact: "Fleet evidence is partial; inspect captured targets and rerun unsupported transports separately".to_string(),
            data_quality: data_quality.clone(),
        })
        .collect::<Vec<_>>();
    let cross_target_salience = vec![format!(
        "{} of {} target(s) produced evidence",
        captured_count,
        matrix.len()
    )];
    let evidence = FleetEvidence {
        schema_version: "obs.fleet.v2".to_string(),
        fleet_run_id: options.fleet_run_id.clone(),
        target_count: matrix.len(),
        captured_count,
        failed_count,
        target_matrix: matrix.clone(),
        cross_target_salience,
        information_debt,
        raw_refs,
        data_quality: data_quality.clone(),
    };

    let fleet_dir = artifact_root.join("fleet_runs").join(&options.fleet_run_id);
    fs::create_dir_all(&fleet_dir).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to create fleet directory {}: {err}",
            fleet_dir.display()
        ))
    })?;
    let evidence_path = fleet_dir.join("fleet_evidence.yaml");
    write_yaml(&evidence_path, &evidence)?;

    Ok(FleetCaptureResult {
        fleet_run_id: options.fleet_run_id,
        target_count: evidence.target_count,
        captured_count,
        failed_count,
        evidence_path,
        targets: matrix,
        data_quality,
    })
}

pub fn snapshot_fleet(
    artifact_root: impl AsRef<Path>,
    inventory_path: impl AsRef<Path>,
    options: FleetSnapshotOptions,
) -> AdcResult<FleetCaptureResult> {
    snapshot_fleet_with_runner(
        artifact_root,
        inventory_path,
        options,
        &DefaultFleetTargetRunner,
    )
}

pub fn snapshot_fleet_with_runner(
    artifact_root: impl AsRef<Path>,
    inventory_path: impl AsRef<Path>,
    options: FleetSnapshotOptions,
    runner: &impl FleetTargetRunner,
) -> AdcResult<FleetCaptureResult> {
    validate_segment(&options.fleet_run_id, "fleet_run_id")?;
    let inventory = read_inventory(inventory_path)?;
    if inventory.targets.is_empty() {
        return Err(AdcError::ProfileValidation(
            "fleet inventory must contain at least one target".to_string(),
        ));
    }

    let artifact_root = artifact_root.as_ref();
    let mut matrix = Vec::new();
    let mut raw_refs = BTreeMap::new();
    let mut data_quality = DataQuality {
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };

    for target in inventory.targets {
        validate_segment(&target.id, "target id")?;
        validate_segment(&target.transport, "target transport")?;
        let profile_id = target
            .profile
            .clone()
            .unwrap_or_else(|| "snapshot".to_string());
        validate_segment(&profile_id, "profile id")?;
        let run_id = format!("{}-{}", options.fleet_run_id, target.id);
        let request = FleetTargetRequest {
            fleet_run_id: options.fleet_run_id.clone(),
            run_id,
            profile_id,
            duration: Duration::ZERO,
            interval: Duration::ZERO,
        };
        let mut outcome = runner.snapshot(artifact_root, &target, &request)?;
        if outcome.status != "captured" {
            outcome.data_quality.missing.push(format!(
                "target {}: {}",
                target.id,
                outcome.status.as_str()
            ));
        }
        if let Some(evidence_text) = outcome.evidence_text.take() {
            let evidence_path =
                fleet_target_evidence_path(artifact_root, &options.fleet_run_id, &target.id);
            write_text(&evidence_path, &evidence_text)?;
            outcome.evidence_ref = Some(format!(
                "artifact://{}",
                evidence_path
                    .strip_prefix(artifact_root)
                    .unwrap_or(&evidence_path)
                    .display()
            ));
        }
        if let Some(evidence_ref) = &outcome.evidence_ref {
            raw_refs.insert(
                format!("{}.evidence_index", target.id),
                evidence_ref.clone(),
            );
        }
        if let Some(artifact_ref) = &outcome.artifact_ref {
            raw_refs.insert(format!("{}.artifact", target.id), artifact_ref.clone());
        }
        merge_data_quality(&mut data_quality, &outcome.data_quality);
        matrix.push(FleetTargetEvidence {
            target_id: target.id,
            transport: target.transport,
            status: outcome.status,
            run_id: outcome.run_id,
            profile_id: outcome.profile_id,
            evidence_ref: outcome.evidence_ref,
            capability_ref: outcome.capability_ref,
            artifact_ref: outcome.artifact_ref,
            data_quality: outcome.data_quality,
        });
    }

    let captured_count = matrix
        .iter()
        .filter(|target| target.status == "captured")
        .count();
    let failed_count = matrix.len().saturating_sub(captured_count);
    let information_debt = data_quality
        .missing
        .iter()
        .enumerate()
        .map(|(index, missing)| InformationDebt {
            debt_id: format!("FD{:03}", index + 1),
            kind: "missing".to_string(),
            description: missing.clone(),
            impact: "Fleet snapshot evidence is partial; inspect captured targets and rerun failed transports separately".to_string(),
            data_quality: data_quality.clone(),
        })
        .collect::<Vec<_>>();
    let evidence = FleetEvidence {
        schema_version: "obs.fleet.v2".to_string(),
        fleet_run_id: options.fleet_run_id.clone(),
        target_count: matrix.len(),
        captured_count,
        failed_count,
        target_matrix: matrix.clone(),
        cross_target_salience: vec![format!(
            "{} of {} target(s) produced snapshot evidence",
            captured_count,
            matrix.len()
        )],
        information_debt,
        raw_refs,
        data_quality: data_quality.clone(),
    };

    let fleet_dir = artifact_root.join("fleet_runs").join(&options.fleet_run_id);
    fs::create_dir_all(&fleet_dir).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to create fleet directory {}: {err}",
            fleet_dir.display()
        ))
    })?;
    let evidence_path = fleet_dir.join("fleet_evidence.yaml");
    write_yaml(&evidence_path, &evidence)?;

    Ok(FleetCaptureResult {
        fleet_run_id: options.fleet_run_id,
        target_count: evidence.target_count,
        captured_count,
        failed_count,
        evidence_path,
        targets: matrix,
        data_quality,
    })
}

pub fn read_fleet_evidence_text(
    artifact_root: impl AsRef<Path>,
    fleet_run_id: &str,
) -> AdcResult<String> {
    validate_segment(fleet_run_id, "fleet_run_id")?;
    let path = artifact_root
        .as_ref()
        .join("fleet_runs")
        .join(fleet_run_id)
        .join("fleet_evidence.yaml");
    fs::read_to_string(&path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read fleet evidence {}: {err}",
            path.display()
        ))
    })
}

pub fn investigate_fleet_service(
    artifact_root: impl AsRef<Path>,
    inventory_path: impl AsRef<Path>,
    options: FleetServiceInvestigationOptions,
) -> AdcResult<FleetServiceInvestigationResult> {
    validate_segment(&options.fleet_run_id, "fleet_run_id")?;
    validate_segment(&options.service_name, "service name")?;
    let inventory = read_inventory(inventory_path)?;
    if inventory.targets.is_empty() {
        return Err(AdcError::ProfileValidation(
            "fleet inventory must contain at least one target".to_string(),
        ));
    }

    let artifact_root = artifact_root.as_ref();
    let mut targets = Vec::new();
    let mut raw_refs = BTreeMap::new();
    let mut data_quality = DataQuality {
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };

    for target in inventory.targets {
        validate_segment(&target.id, "target id")?;
        validate_segment(&target.transport, "target transport")?;
        let outcome = investigate_service_for_fleet_target(
            artifact_root,
            &target,
            &options.service_name,
            options.max_journal_lines,
        );
        let (status, service_pack, mut target_data_quality) = match outcome {
            Ok(pack) => {
                raw_refs.insert(
                    format!("{}.service_investigation", target.id),
                    format!(
                        "artifact://fleet_runs/{}/targets/{}/service_investigation.json",
                        options.fleet_run_id, target.id
                    ),
                );
                (
                    "captured".to_string(),
                    Some(pack),
                    DataQuality {
                        clock_confidence: "medium".to_string(),
                        ..Default::default()
                    },
                )
            }
            Err(failure) => {
                let mut target_data_quality = DataQuality {
                    clock_confidence: "medium".to_string(),
                    ..Default::default()
                };
                target_data_quality
                    .missing
                    .push(format!("target {}: {}", target.id, failure.message));
                (failure.status, None, target_data_quality)
            }
        };
        if status != "captured" {
            target_data_quality.missing.push(format!(
                "target {} service investigation: {status}",
                target.id
            ));
        }
        if let Some(pack) = &service_pack {
            merge_data_quality(&mut target_data_quality, &pack.data_quality);
            let target_path = artifact_root
                .join("fleet_runs")
                .join(&options.fleet_run_id)
                .join("targets")
                .join(&target.id)
                .join("service_investigation.json");
            write_json(&target_path, pack)?;
        }
        merge_data_quality(&mut data_quality, &target_data_quality);
        targets.push(FleetServiceInvestigationTarget {
            target_id: target.id,
            transport: target.transport,
            status,
            service_pack,
            data_quality: target_data_quality,
        });
    }

    let captured_count = targets
        .iter()
        .filter(|target| target.status == "captured")
        .count();
    let failed_count = targets.len().saturating_sub(captured_count);
    let result = FleetServiceInvestigationResult {
        schema_version: "obs.fleet_service_investigation.v1".to_string(),
        fleet_run_id: options.fleet_run_id.clone(),
        service_name: options.service_name,
        target_count: targets.len(),
        captured_count,
        failed_count,
        targets,
        raw_refs,
        data_quality,
    };
    let summary_path = artifact_root
        .join("fleet_runs")
        .join(&options.fleet_run_id)
        .join("service_investigation.json");
    write_json(&summary_path, &result)?;
    Ok(result)
}

impl FleetTargetRunner for DefaultFleetTargetRunner {
    fn preflight(&self, target: &FleetTargetConfig) -> AdcResult<FleetPreflightTarget> {
        match target.transport.as_str() {
            "local" => Ok(preflight_model::ready_target(
                target,
                vec![
                    FleetPreflightCheck::ok("transport_supported"),
                    FleetPreflightCheck::ok("local_capture_supported"),
                ],
            )),
            "mcp_stdio_over_ssh" => preflight_target_mcp_over_ssh(target),
            "managed_mcp" => preflight_target_managed_mcp(target),
            transport => Ok(preflight_model::failed_target(
                target,
                "unsupported",
                format!("transport {transport} is not supported"),
                vec![FleetPreflightCheck::failed(
                    "transport_supported",
                    format!("transport {transport} is not supported"),
                )],
            )),
        }
    }

    fn snapshot(
        &self,
        artifact_root: &Path,
        target: &FleetTargetConfig,
        request: &FleetTargetRequest,
    ) -> AdcResult<FleetTargetRunResult> {
        match target.transport.as_str() {
            "local" => snapshot_local_target(artifact_root, target, request),
            "mcp_stdio_over_ssh" => snapshot_target_mcp_over_ssh(target, request),
            "managed_mcp" => snapshot_target_managed_mcp(target, request),
            transport => Ok(FleetTargetRunResult::failed(
                "unsupported",
                format!("transport {transport} is not supported"),
            )),
        }
    }

    fn capture(
        &self,
        artifact_root: &Path,
        target: &FleetTargetConfig,
        request: &FleetTargetRequest,
    ) -> AdcResult<FleetTargetRunResult> {
        match target.transport.as_str() {
            "local" => capture_local_target(artifact_root, target, request),
            "mcp_stdio_over_ssh" => capture_target_mcp_over_ssh(target, request),
            "managed_mcp" => capture_target_managed_mcp(target, request),
            transport => Ok(FleetTargetRunResult::failed(
                "unsupported",
                format!("transport {transport} is not supported"),
            )),
        }
    }
}

fn read_inventory(path: impl AsRef<Path>) -> AdcResult<FleetInventory> {
    let path = path.as_ref();
    let contents = fs::read_to_string(path).map_err(|err| {
        AdcError::ProfileParse(format!(
            "failed to read inventory {}: {err}",
            path.display()
        ))
    })?;
    yaml_serde::from_str(&contents)
        .map_err(|err| AdcError::ProfileParse(format!("fleet inventory parse failed: {err}")))
}

fn write_yaml(path: &Path, value: &impl Serialize) -> AdcResult<()> {
    let bytes = yaml_serde::to_string(value)
        .map_err(|err| AdcError::Artifact(format!("fleet yaml serialization failed: {err}")))?;
    fs::write(path, bytes)
        .map_err(|err| AdcError::Artifact(format!("failed to write {}: {err}", path.display())))
}

fn write_json(path: &Path, value: &impl Serialize) -> AdcResult<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|err| AdcError::Artifact(format!("fleet json serialization failed: {err}")))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AdcError::Artifact(format!("failed to create {}: {err}", parent.display()))
        })?;
    }
    fs::write(path, bytes)
        .map_err(|err| AdcError::Artifact(format!("failed to write {}: {err}", path.display())))
}

fn write_text(path: &Path, contents: &str) -> AdcResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AdcError::Artifact(format!("failed to create {}: {err}", parent.display()))
        })?;
    }
    fs::write(path, contents)
        .map_err(|err| AdcError::Artifact(format!("failed to write {}: {err}", path.display())))
}

fn fleet_target_evidence_path(
    artifact_root: &Path,
    fleet_run_id: &str,
    target_id: &str,
) -> PathBuf {
    artifact_root
        .join("fleet_runs")
        .join(fleet_run_id)
        .join("targets")
        .join(target_id)
        .join("evidence_index.yaml")
}

fn profile_id_for_target(target: &FleetTargetConfig) -> String {
    target
        .profile
        .clone()
        .unwrap_or_else(|| match target.transport.as_str() {
            "local" => "fleet_local_capture".to_string(),
            "mcp_stdio_over_ssh" => "mcp_observe".to_string(),
            _ => "fleet_capture".to_string(),
        })
}

fn merge_data_quality(target: &mut DataQuality, source: &DataQuality) {
    target.dropped |= source.dropped;
    target.throttled |= source.throttled;
    target.truncated |= source.truncated;
    target.drop_count = target.drop_count.saturating_add(source.drop_count);
    extend_unique(&mut target.missing, &source.missing);
    extend_unique(&mut target.notes, &source.notes);
}

fn extend_unique(target: &mut Vec<String>, source: &[String]) {
    for item in source {
        if !target.contains(item) {
            target.push(item.clone());
        }
    }
}

fn validate_segment(value: &str, label: &str) -> AdcResult<()> {
    if value.trim().is_empty() {
        return Err(AdcError::Artifact(format!("{label} must not be empty")));
    }
    let path = Path::new(value);
    let mut components = path.components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(AdcError::Artifact(format!(
            "{label} must be a single relative path segment"
        ))),
    }
}

fn validate_ssh_host(value: &str) -> AdcResult<()> {
    if value.trim().is_empty()
        || value.starts_with('-')
        || value
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, '"' | '\'' | '`' | '$' | ';' | '|'))
    {
        return Err(AdcError::ProfileValidation(
            "ssh host must be a plain host name or address".to_string(),
        ));
    }
    Ok(())
}

fn validate_ssh_user(value: &str) -> AdcResult<()> {
    if value.trim().is_empty()
        || value.starts_with('-')
        || value
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, '@' | '"' | '\'' | '`' | '$' | ';' | '|'))
    {
        return Err(AdcError::ProfileValidation(
            "ssh user must be a plain user name".to_string(),
        ));
    }
    Ok(())
}

fn bounded_text(value: &str) -> String {
    const MAX_LEN: usize = 240;
    let trimmed = value.trim();
    if trimmed.len() <= MAX_LEN {
        trimmed.to_string()
    } else {
        format!("{}...", &trimmed[..MAX_LEN])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preflight_reports_per_target_readiness_and_data_quality() {
        let temp = tempfile::tempdir().expect("tempdir");
        let inventory_path = temp.path().join("targets.yaml");
        fs::write(
            &inventory_path,
            r#"
targets:
  - id: local-a
    transport: local
  - id: unsupported-b
    transport: serial
"#,
        )
        .expect("inventory");

        let result = preflight_fleet(&inventory_path).expect("preflight");

        assert_eq!(result.schema_version, "obs.fleet_preflight.v1");
        assert_eq!(result.status, "degraded");
        assert_eq!(result.target_count, 2);
        assert_eq!(result.ready_count, 1);
        assert_eq!(result.failed_count, 1);
        assert_eq!(result.targets[0].target_id, "local-a");
        assert_eq!(result.targets[0].status, "ready");
        assert_eq!(result.targets[1].target_id, "unsupported-b");
        assert_eq!(result.targets[1].status, "unsupported");
        assert!(result
            .data_quality
            .missing
            .iter()
            .any(|missing| missing.contains("unsupported-b")));
    }

    #[test]
    fn preflight_records_target_mcp_configuration_failures_as_target_data_quality() {
        let temp = tempfile::tempdir().expect("tempdir");
        let inventory_path = temp.path().join("targets.yaml");
        fs::write(
            &inventory_path,
            r#"
targets:
  - id: mcp-a
    transport: mcp_stdio_over_ssh
"#,
        )
        .expect("inventory");

        let result = preflight_fleet(&inventory_path).expect("preflight");

        assert_eq!(result.status, "failed");
        assert_eq!(result.ready_count, 0);
        assert_eq!(result.failed_count, 1);
        assert_eq!(result.targets[0].target_id, "mcp-a");
        assert_eq!(result.targets[0].status, "unreachable");
        assert!(result.targets[0]
            .data_quality
            .missing
            .iter()
            .any(|missing| missing.contains("target MCP-over-SSH endpoint is missing host")));
        assert!(result
            .data_quality
            .missing
            .iter()
            .any(|missing| missing.contains("mcp-a")));
    }
}
