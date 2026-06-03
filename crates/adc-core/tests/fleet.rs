use std::{fs, path::Path, time::Duration};

use adc_core::{
    capture_fleet, capture_fleet_with_runner, read_evidence_index, AdcResult, ArtifactManifest,
    FleetCaptureOptions, FleetTargetConfig, FleetTargetRequest, FleetTargetRunResult,
    FleetTargetRunner,
};

#[test]
fn fleet_capture_uses_explicit_inventory_and_records_partial_failures() {
    let temp = tempfile::tempdir().expect("tempdir");
    let inventory_path = temp.path().join("targets.yaml");
    fs::write(
        &inventory_path,
        r#"
targets:
  - id: pi5-local
    transport: local
  - id: pi5-remote
    transport: serial
"#,
    )
    .expect("write inventory");

    let result = capture_fleet(
        temp.path(),
        &inventory_path,
        FleetCaptureOptions {
            fleet_run_id: "F-EVIDENCE".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
        },
    )
    .expect("fleet capture");

    assert_eq!(result.fleet_run_id, "F-EVIDENCE");
    assert_eq!(result.target_count, 2);
    assert_eq!(result.captured_count, 1);
    assert_eq!(result.failed_count, 1);
    assert!(result.evidence_path.is_file());
    assert!(result
        .targets
        .iter()
        .any(|target| target.target_id == "pi5-local" && target.status == "captured"));
    assert!(result
        .targets
        .iter()
        .any(|target| target.target_id == "pi5-remote"
            && target.transport == "serial"
            && target.status == "unsupported"));
    assert!(result
        .data_quality
        .missing
        .iter()
        .any(|missing| missing.contains("transport serial is not supported")));

    let evidence_yaml = fs::read_to_string(result.evidence_path).expect("fleet evidence");
    assert!(evidence_yaml.contains("schema_version: obs.fleet.v2"));
    assert!(evidence_yaml.contains("target_matrix:"));
    assert!(evidence_yaml.contains("cross_target_salience:"));
    assert!(!evidence_yaml.contains("likely_cause"));
    assert!(!evidence_yaml.contains("root_cause"));
}

#[test]
fn fleet_capture_keeps_local_target_identity_in_run_evidence_and_manifest() {
    let temp = tempfile::tempdir().expect("tempdir");
    let inventory_path = temp.path().join("targets.yaml");
    fs::write(
        &inventory_path,
        r#"
targets:
  - id: pi5-a
    transport: local
    profile: pi5_basic
"#,
    )
    .expect("write inventory");

    let result = capture_fleet(
        temp.path(),
        &inventory_path,
        FleetCaptureOptions {
            fleet_run_id: "F-IDENTITY".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
        },
    )
    .expect("fleet capture");

    let target = result
        .targets
        .iter()
        .find(|target| target.target_id == "pi5-a")
        .expect("target matrix row");
    let run_id = target.run_id.as_deref().expect("target run id");
    let evidence = read_evidence_index(temp.path(), run_id).expect("target evidence");
    assert_eq!(evidence.target_id, "pi5-a");
    assert_eq!(evidence.fleet_run_id.as_deref(), Some("F-IDENTITY"));

    let manifest_path = temp.path().join("runs").join(run_id).join("manifest.json");
    let manifest = ArtifactManifest::read_json(&manifest_path).expect("manifest");
    assert_eq!(manifest.target_id, "pi5-a");
    assert_eq!(manifest.fleet_run_id.as_deref(), Some("F-IDENTITY"));
    assert_eq!(manifest.profile_id, "pi5_basic");
}

#[test]
fn fleet_capture_uses_target_mcp_runner_and_records_permission_denied_as_partial_quality() {
    let temp = tempfile::tempdir().expect("tempdir");
    let inventory_path = temp.path().join("targets.yaml");
    fs::write(
        &inventory_path,
        r#"
targets:
  - id: pi5-mcp-ok
    transport: mcp_stdio_over_ssh
    host: pi5-mcp-ok.local
    profile: pi5_basic
  - id: pi5-mcp-denied
    transport: mcp_stdio_over_ssh
    host: pi5-mcp-denied.local
"#,
    )
    .expect("write inventory");

    let result = capture_fleet_with_runner(
        temp.path(),
        &inventory_path,
        FleetCaptureOptions {
            fleet_run_id: "F-MCP".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
        },
        &FakeTargetMcpRunner,
    )
    .expect("fleet capture");

    assert_eq!(result.target_count, 2);
    assert_eq!(result.captured_count, 1);
    assert_eq!(result.failed_count, 1);
    assert!(result
        .targets
        .iter()
        .any(|target| target.target_id == "pi5-mcp-ok"
            && target.transport == "mcp_stdio_over_ssh"
            && target.status == "captured"
            && target.profile_id.as_deref() == Some("pi5_basic")));
    assert!(result
        .targets
        .iter()
        .any(|target| target.target_id == "pi5-mcp-denied"
            && target.status == "permission_denied"
            && target
                .data_quality
                .missing
                .iter()
                .any(|missing| missing.contains("permission_denied"))));

    let ingested = temp
        .path()
        .join("fleet_runs/F-MCP/targets/pi5-mcp-ok/evidence_index.yaml");
    assert!(ingested.is_file());
    let ingested_yaml = fs::read_to_string(ingested).expect("ingested evidence");
    assert!(ingested_yaml.contains("target_id: pi5-mcp-ok"));
    assert!(result
        .data_quality
        .missing
        .iter()
        .any(|missing| missing.contains("pi5-mcp-denied")));
}

struct FakeTargetMcpRunner;

impl FleetTargetRunner for FakeTargetMcpRunner {
    fn capture(
        &self,
        _artifact_root: &Path,
        target: &FleetTargetConfig,
        request: &FleetTargetRequest,
    ) -> AdcResult<FleetTargetRunResult> {
        assert_eq!(request.fleet_run_id, "F-MCP");
        assert!(request.run_id.starts_with("F-MCP-"));
        match target.id.as_str() {
            "pi5-mcp-ok" => Ok(FleetTargetRunResult::captured_evidence_text(
                request.run_id.clone(),
                request.profile_id.clone(),
                "schema_version: obs.v2\nrun_id: F-MCP-pi5-mcp-ok\ntarget_id: pi5-mcp-ok\n",
            )),
            "pi5-mcp-denied" => Ok(FleetTargetRunResult::failed(
                "permission_denied",
                "permission_denied: target MCP-over-SSH authentication failed",
            )),
            other => panic!("unexpected target {other}"),
        }
    }
}
