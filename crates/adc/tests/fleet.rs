use std::{
    fs,
    net::{TcpListener, TcpStream},
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[test]
fn fleet_capture_reads_explicit_inventory_and_returns_fleet_evidence_path() {
    let temp = tempfile::tempdir().expect("tempdir");
    let inventory_path = temp.path().join("targets.yaml");
    fs::write(
        &inventory_path,
        r#"
targets:
  - id: local-a
    transport: local
  - id: remote-b
    transport: serial
"#,
    )
    .expect("write inventory");

    let capture = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "fleet",
            "capture",
            "--inventory",
            inventory_path.to_str().expect("inventory path"),
            "--fleet-run-id",
            "F-CLI",
            "--duration-ms",
            "120",
            "--interval-ms",
            "40",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("fleet capture");
    assert!(
        capture.status.success(),
        "fleet capture failed: {}",
        String::from_utf8_lossy(&capture.stderr)
    );
    let response: serde_json::Value =
        serde_json::from_slice(&capture.stdout).expect("fleet response json");
    assert_eq!(response["fleet_run_id"], "F-CLI");
    assert_eq!(response["captured_count"], 1);
    assert_eq!(response["failed_count"], 1);
    assert!(response["evidence_path"]
        .as_str()
        .expect("evidence path")
        .contains("fleet_evidence.yaml"));

    let evidence = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["fleet", "evidence", "--fleet-run-id", "F-CLI"])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("fleet evidence");
    assert!(
        evidence.status.success(),
        "fleet evidence failed: {}",
        String::from_utf8_lossy(&evidence.stderr)
    );
    let stdout = String::from_utf8(evidence.stdout).expect("evidence utf8");
    assert!(stdout.contains("schema_version: obs.fleet.v2"));
    assert!(stdout.contains("target_matrix:"));
    assert!(!stdout.contains("root_cause"));
}

#[test]
fn fleet_snapshot_reads_explicit_inventory_and_returns_fleet_evidence_path() {
    let temp = tempfile::tempdir().expect("tempdir");
    let inventory_path = temp.path().join("targets.yaml");
    fs::write(
        &inventory_path,
        r#"
targets:
  - id: local-snapshot
    transport: local
  - id: unsupported-snapshot
    transport: serial
"#,
    )
    .expect("write inventory");

    let snapshot = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "fleet",
            "snapshot",
            "--inventory",
            inventory_path.to_str().expect("inventory path"),
            "--fleet-run-id",
            "F-SNAPSHOT",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("fleet snapshot");
    assert!(
        snapshot.status.success(),
        "fleet snapshot failed: {}",
        String::from_utf8_lossy(&snapshot.stderr)
    );
    let response: serde_json::Value =
        serde_json::from_slice(&snapshot.stdout).expect("fleet snapshot response json");
    assert_eq!(response["fleet_run_id"], "F-SNAPSHOT");
    assert_eq!(response["captured_count"], 1);
    assert_eq!(response["failed_count"], 1);

    let evidence = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["fleet", "evidence", "--fleet-run-id", "F-SNAPSHOT"])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("fleet evidence");
    assert!(
        evidence.status.success(),
        "fleet evidence failed: {}",
        String::from_utf8_lossy(&evidence.stderr)
    );
    let stdout = String::from_utf8(evidence.stdout).expect("evidence utf8");
    assert!(stdout.contains("schema_version: obs.fleet.v2"));
    assert!(stdout.contains("local-snapshot"));
    assert!(!stdout.contains("root_cause"));

    let run_evidence = fs::read_to_string(
        temp.path()
            .join("runs/F-SNAPSHOT-local-snapshot/evidence_index.yaml"),
    )
    .expect("target evidence");
    assert!(run_evidence.contains("capture_mode: snapshot"));
    assert!(run_evidence.contains("target_id: local-snapshot"));
}

#[test]
fn fleet_investigate_service_returns_partial_target_packs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let inventory_path = temp.path().join("targets.yaml");
    fs::write(
        &inventory_path,
        r#"
targets:
  - id: local-service
    transport: local
  - id: unsupported-service
    transport: serial
"#,
    )
    .expect("write inventory");
    let fake_bin = temp.path().join("fake-bin");
    fs::create_dir_all(&fake_bin).expect("fake bin");
    write_executable(
        &fake_bin.join("systemctl"),
        r#"#!/usr/bin/env sh
if [ "$1" = "show" ]; then
  printf '%s\n' \
    'Id=ssh.service' \
    'LoadState=loaded' \
    'ActiveState=active' \
    'SubState=running' \
    'MainPID=999999' \
    'FragmentPath=/usr/lib/systemd/system/ssh.service'
  exit 0
fi
exit 1
"#,
    );
    write_executable(
        &fake_bin.join("journalctl"),
        r#"#!/usr/bin/env sh
printf '%s\n' \
  '2026-05-27T00:02:00+09:00 host sshd[10]: failed password for invalid user' \
  '2026-05-27T00:03:00+09:00 host sshd[11]: timeout waiting for auth'
"#,
    );
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let snapshot = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "fleet",
            "snapshot",
            "--inventory",
            inventory_path.to_str().expect("inventory path"),
            "--fleet-run-id",
            "F-SERVICE",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("fleet snapshot");
    assert!(
        snapshot.status.success(),
        "fleet snapshot failed: {}",
        String::from_utf8_lossy(&snapshot.stderr)
    );

    let output = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "fleet",
            "investigate",
            "service",
            "ssh",
            "--inventory",
            inventory_path.to_str().expect("inventory path"),
            "--fleet-run-id",
            "F-SERVICE",
            "--journal-lines",
            "3",
        ])
        .env("ADC_HOME", temp.path())
        .env("PATH", &path)
        .output()
        .expect("fleet service investigation");
    assert!(
        output.status.success(),
        "fleet service investigation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("fleet service response json");
    assert_eq!(
        response["schema_version"],
        "obs.fleet_service_investigation.v1"
    );
    assert_eq!(response["fleet_run_id"], "F-SERVICE");
    assert_eq!(response["service_name"], "ssh");
    assert_eq!(response["target_count"], 2);
    assert_eq!(response["captured_count"], 1);
    assert_eq!(response["failed_count"], 1);
    assert_eq!(response["targets"][0]["target_id"], "local-service");
    assert_eq!(response["targets"][0]["status"], "captured");
    assert_eq!(
        response["targets"][0]["service_pack"]["service_state"]["active_state"],
        "active"
    );
    assert_eq!(
        response["targets"][0]["service_pack"]["port_summary"]["availability"],
        "unavailable"
    );
    assert_eq!(response["targets"][1]["target_id"], "unsupported-service");
    assert_eq!(response["targets"][1]["status"], "unsupported");
    assert!(response["data_quality"]["missing"]
        .as_array()
        .expect("missing")
        .iter()
        .any(|entry| entry
            .as_str()
            .expect("missing string")
            .contains("unsupported-service")));
    assert!(temp
        .path()
        .join("fleet_runs/F-SERVICE/service_investigation.json")
        .is_file());

    let start = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "investigate",
            "start",
            "--fleet-run-id",
            "F-SERVICE",
            "--service-name",
            "ssh",
            "--inventory",
            inventory_path.to_str().expect("inventory path"),
            "--journal-lines",
            "3",
        ])
        .env("ADC_HOME", temp.path())
        .env("PATH", &path)
        .output()
        .expect("fleet investigation start");
    assert!(
        start.status.success(),
        "fleet investigation start failed: {}",
        String::from_utf8_lossy(&start.stderr)
    );
    let start_json: serde_json::Value =
        serde_json::from_slice(&start.stdout).expect("fleet start json");
    assert_eq!(start_json["schema_version"], "obs.investigation_start.v1");
    assert_eq!(start_json["scope"], "fleet");
    assert_eq!(start_json["fleet_run_id"], "F-SERVICE");
    assert_eq!(start_json["investigation_route"]["service_name"], "ssh");
    assert!(start_json["investigation_route"]["raw_refs"]
        ["fleet_service.local-service.service_investigation"]
        .as_str()
        .expect("fleet service ref")
        .contains(
            "artifact://fleet_runs/F-SERVICE/targets/local-service/service_investigation.json"
        ));
    assert_eq!(
        start_json["investigation_route"]["raw_refs"]["fleet_semantic_diff"],
        "artifact://fleet_runs/F-SERVICE/fleet_semantic_diff.json"
    );
    assert!(start_json["investigation_route"]["steps"]
        .as_array()
        .expect("route steps")
        .iter()
        .any(|step| step["title"]
            .as_str()
            .expect("title")
            .contains("Compare service investigation packs")));
    assert!(start_json["investigation_route"]["data_quality"]["missing"]
        .as_array()
        .expect("missing")
        .iter()
        .any(|entry| entry
            .as_str()
            .expect("missing string")
            .contains("unsupported-service")));
    let semantic_diff_path = temp
        .path()
        .join("fleet_runs/F-SERVICE/fleet_semantic_diff.json");
    assert!(semantic_diff_path.is_file());
    let semantic_diff: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(semantic_diff_path).expect("semantic diff"))
            .expect("semantic diff json");
    assert_eq!(
        semantic_diff["schema_version"],
        "obs.fleet_semantic_diff.v1"
    );
    assert!(semantic_diff["field_diffs"]
        .as_array()
        .expect("field diffs")
        .iter()
        .any(|field| field["field"] == "service.availability"));
    assert!(semantic_diff["field_diffs"]
        .as_array()
        .expect("field diffs")
        .iter()
        .any(|field| field["field"] == "service.sub_state"));
    assert!(semantic_diff["field_diffs"]
        .as_array()
        .expect("field diffs")
        .iter()
        .any(|field| field["field"] == "journal.severity_buckets"
            && field["values_by_target"]["local-service"]["error"] == 1
            && field["values_by_target"]["local-service"]["warning"] == 1));
    assert!(semantic_diff["field_diffs"]
        .as_array()
        .expect("field diffs")
        .iter()
        .any(|field| field["field"] == "data_quality.class"
            && field["values_by_target"]["unsupported-service"] == "collector_failed"));
    assert!(semantic_diff["field_diffs"]
        .as_array()
        .expect("field diffs")
        .iter()
        .any(
            |field| field["quality_class_by_target"]["unsupported-service"] == "collector_failed"
        ));
    assert!(semantic_diff["field_diffs"]
        .as_array()
        .expect("field diffs")
        .iter()
        .any(|field| field["status"] == "partial"));
}

#[test]
fn fleet_discover_reads_neighbor_fixture_and_returns_safe_candidates() {
    let temp = tempfile::tempdir().expect("tempdir");
    let neighbors_path = temp.path().join("neighbors.txt");
    fs::write(
        &neighbors_path,
        "198.51.100.10 dev eth0 lladdr aa:bb:cc:dd:ee:ff REACHABLE\n",
    )
    .expect("write neighbors");

    let discover = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "fleet",
            "discover",
            "--cidr",
            "198.51.100.0/24",
            "--neighbors-file",
            neighbors_path.to_str().expect("neighbors path"),
            "--write-inventory",
            temp.path()
                .join("discovered-targets.yaml")
                .to_str()
                .expect("inventory output path"),
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("fleet discover");
    assert!(
        discover.status.success(),
        "fleet discover failed: {}",
        String::from_utf8_lossy(&discover.stderr)
    );
    let response: serde_json::Value =
        serde_json::from_slice(&discover.stdout).expect("discovery json");
    assert_eq!(response["schema_version"], "obs.discovery.v2");
    assert_eq!(response["candidate_count"], 1);
    assert_eq!(
        response["candidates"][0]["target_id"],
        "target-198-51-100-10"
    );
    assert_eq!(response["candidates"][0]["host"], "198.51.100.10");
    assert_eq!(response["candidates"][0]["transport"], "mcp_stdio_over_ssh");
    let rendered = String::from_utf8(discover.stdout).expect("stdout utf8");
    assert!(!rendered.contains("aa:bb:cc"));

    let inventory = fs::read_to_string(temp.path().join("discovered-targets.yaml"))
        .expect("discovered inventory");
    assert!(inventory.contains("targets:"));
    assert!(inventory.contains("id: target-198-51-100-10"));
    assert!(inventory.contains("transport: mcp_stdio_over_ssh"));
    assert!(inventory.contains("host: 198.51.100.10"));
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).expect("write executable");
    let mut permissions = fs::metadata(path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("chmod");
}

#[test]
fn fleet_registry_commands_round_trip_and_selector_snapshot() {
    let temp = tempfile::tempdir().expect("tempdir");

    let init = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["fleet", "init"])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("fleet init");
    assert!(
        init.status.success(),
        "fleet init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    let init_json: serde_json::Value =
        serde_json::from_slice(&init.stdout).expect("init response json");
    assert_eq!(init_json["schema_version"], "obs.managed_fleet_registry.v1");

    let enroll = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "fleet",
            "enroll",
            "--target-id",
            "local-managed",
            "--transport",
            "local",
            "--profile",
            "pi5_basic",
            "--tag",
            "lab",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("fleet enroll");
    assert!(
        enroll.status.success(),
        "fleet enroll failed: {}",
        String::from_utf8_lossy(&enroll.stderr)
    );

    let targets = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["fleet", "targets"])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("fleet targets");
    assert!(
        targets.status.success(),
        "fleet targets failed: {}",
        String::from_utf8_lossy(&targets.stderr)
    );
    let targets_json: serde_json::Value =
        serde_json::from_slice(&targets.stdout).expect("targets response json");
    assert_eq!(targets_json["target_count"], 1);
    assert_eq!(targets_json["targets"][0]["target_id"], "local-managed");

    let snapshot = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "fleet",
            "snapshot",
            "--selector",
            "all",
            "--fleet-run-id",
            "F-MANAGED-SNAPSHOT",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("fleet snapshot selector");
    assert!(
        snapshot.status.success(),
        "fleet snapshot selector failed: {}",
        String::from_utf8_lossy(&snapshot.stderr)
    );
    let response: serde_json::Value =
        serde_json::from_slice(&snapshot.stdout).expect("snapshot response json");
    assert_eq!(response["captured_count"], 1);
    assert_eq!(response["failed_count"], 0);
    assert!(temp
        .path()
        .join("runs/F-MANAGED-SNAPSHOT-local-managed/evidence_index.yaml")
        .is_file());
}

#[test]
fn fleet_invite_does_not_store_plain_join_code() {
    let temp = tempfile::tempdir().expect("tempdir");

    let invite = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "fleet",
            "invite",
            "--target-id-hint",
            "pi4-a",
            "--ttl-sec",
            "600",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("fleet invite");
    assert!(
        invite.status.success(),
        "fleet invite failed: {}",
        String::from_utf8_lossy(&invite.stderr)
    );
    let response: serde_json::Value =
        serde_json::from_slice(&invite.stdout).expect("invite response json");
    let join_code = response["join_code"].as_str().expect("join_code");
    assert!(join_code.contains('-'));

    let invite_id = response["invite_id"].as_str().expect("invite_id");
    let stored = fs::read_to_string(
        temp.path()
            .join("fleet/enrollment/invites")
            .join(format!("{invite_id}.json")),
    )
    .expect("stored invite");
    assert!(stored.contains("join_code_sha256"));
    assert!(!stored.contains(join_code));
}

#[test]
fn fleet_enroll_kit_registers_managed_mcp_target() {
    let temp = tempfile::tempdir().expect("tempdir");
    let kit_path = temp.path().join("enrollment-kit.json");
    fs::write(
        &kit_path,
        r#"
{
  "schema_version": "obs.managed_mcp_enrollment_kit.v1",
  "target": {
    "target_id": "kit-target",
    "transport": "managed_mcp",
    "host": "127.0.0.1",
    "port": 39245,
    "auth_token_file": "/tmp/kit/managed.token",
    "tls_ca_file": "/tmp/kit/target-ca.pem",
    "tls_client_cert_file": "/tmp/kit/controller.pem",
    "tls_client_key_file": "/tmp/kit/controller.key",
    "tls_server_name": "kit-target.local",
    "tags": ["kit"],
    "trust_state": "trusted",
    "enrollment_mode": "kit"
  }
}
"#,
    )
    .expect("kit");

    run_ok(
        Command::new(env!("CARGO_BIN_EXE_adc"))
            .args(["fleet", "init"])
            .env("ADC_HOME", temp.path()),
        "fleet init",
    );
    let enroll = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "fleet",
            "enroll-kit",
            "--kit",
            kit_path.to_str().expect("kit"),
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("fleet enroll-kit");
    assert!(
        enroll.status.success(),
        "fleet enroll-kit failed: {}",
        String::from_utf8_lossy(&enroll.stderr)
    );
    let response: serde_json::Value =
        serde_json::from_slice(&enroll.stdout).expect("enroll-kit response");
    assert_eq!(response["target_count"], 1);
    assert_eq!(response["targets"][0]["target_id"], "kit-target");
    assert_eq!(response["targets"][0]["enrollment_mode"], "kit");
    assert_eq!(
        response["targets"][0]["tls_server_name"],
        "kit-target.local"
    );
}

#[test]
fn fleet_selector_snapshot_uses_managed_mcp_without_ssh() {
    let temp = tempfile::tempdir().expect("tempdir");
    let token_path = temp.path().join("managed.token");
    fs::write(&token_path, "fleet-managed-token\n").expect("token");
    let addr = reserve_local_addr();
    let port = addr.rsplit_once(':').expect("host port").1.to_string();
    let mut server = spawn_managed_listener(temp.path(), &addr, &token_path);

    run_ok(
        Command::new(env!("CARGO_BIN_EXE_adc"))
            .args(["fleet", "init"])
            .env("ADC_HOME", temp.path()),
        "fleet init",
    );
    run_ok(
        Command::new(env!("CARGO_BIN_EXE_adc"))
            .args([
                "fleet",
                "enroll",
                "--target-id",
                "managed-local",
                "--transport",
                "managed_mcp",
                "--host",
                "127.0.0.1",
                "--port",
                &port,
                "--auth-token-file",
                token_path.to_str().expect("token path"),
                "--tag",
                "managed",
            ])
            .env("ADC_HOME", temp.path()),
        "fleet enroll managed",
    );

    let preflight = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["fleet", "preflight", "--selector", "tag=managed"])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("fleet preflight managed");
    assert!(
        preflight.status.success(),
        "fleet preflight managed failed: {}",
        String::from_utf8_lossy(&preflight.stderr)
    );
    let preflight_json: serde_json::Value =
        serde_json::from_slice(&preflight.stdout).expect("preflight json");
    assert_eq!(preflight_json["ready_count"], 1);
    assert_eq!(preflight_json["targets"][0]["transport"], "managed_mcp");

    let snapshot = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "fleet",
            "snapshot",
            "--selector",
            "tag=managed",
            "--fleet-run-id",
            "F-MANAGED-MCP",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("fleet snapshot managed");
    assert!(
        snapshot.status.success(),
        "fleet snapshot managed failed: {}",
        String::from_utf8_lossy(&snapshot.stderr)
    );
    let response: serde_json::Value =
        serde_json::from_slice(&snapshot.stdout).expect("snapshot response json");
    assert_eq!(response["captured_count"], 1);
    assert_eq!(response["targets"][0]["transport"], "managed_mcp");
    assert!(response["targets"][0]["artifact_ref"]
        .as_str()
        .expect("artifact_ref")
        .starts_with("managed+mcp://127.0.0.1:"));

    server.kill().expect("kill managed server");
    let _ = server.wait();
}

#[test]
fn fleet_managed_mcp_auth_failure_is_target_scoped_data_quality() {
    let temp = tempfile::tempdir().expect("tempdir");
    let server_token = temp.path().join("server.token");
    let wrong_token = temp.path().join("wrong.token");
    fs::write(&server_token, "server-token\n").expect("server token");
    fs::write(&wrong_token, "wrong-token\n").expect("wrong token");
    let addr = reserve_local_addr();
    let port = addr.rsplit_once(':').expect("host port").1.to_string();
    let mut server = spawn_managed_listener(temp.path(), &addr, &server_token);

    let inventory_path = temp.path().join("managed-bad.yaml");
    fs::write(
        &inventory_path,
        format!(
            r#"
targets:
  - id: managed-bad
    transport: managed_mcp
    host: 127.0.0.1
    port: {port}
    auth_token_file: {}
"#,
            wrong_token.display()
        ),
    )
    .expect("inventory");

    let snapshot = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "fleet",
            "snapshot",
            "--inventory",
            inventory_path.to_str().expect("inventory"),
            "--fleet-run-id",
            "F-MANAGED-MCP-AUTH-FAIL",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("fleet snapshot auth failure");
    assert!(
        snapshot.status.success(),
        "fleet snapshot auth failure command failed: {}",
        String::from_utf8_lossy(&snapshot.stderr)
    );
    let response: serde_json::Value =
        serde_json::from_slice(&snapshot.stdout).expect("snapshot response json");
    assert_eq!(response["captured_count"], 0);
    assert_eq!(response["failed_count"], 1);
    assert_eq!(response["targets"][0]["status"], "permission_denied");
    assert!(response["targets"][0]["data_quality"]["missing"][0]
        .as_str()
        .expect("missing")
        .contains("managed_mcp authentication failed"));

    server.kill().expect("kill managed server");
    let _ = server.wait();
}

#[test]
fn fleet_snapshot_uses_managed_mcp_mutual_tls_without_ssh() {
    let temp = tempfile::tempdir().expect("tempdir");
    let certs = write_managed_mcp_test_certs(temp.path());
    let token_path = temp.path().join("managed.token");
    fs::write(&token_path, "fleet-managed-token\n").expect("token");
    let addr = reserve_local_addr();
    let port = addr.rsplit_once(':').expect("host port").1.to_string();
    let mut server = spawn_managed_listener_with_args(
        temp.path(),
        &addr,
        &token_path,
        &[
            "--managed-tls-server-cert",
            certs.server_cert.to_str().expect("server cert"),
            "--managed-tls-server-key",
            certs.server_key.to_str().expect("server key"),
            "--managed-tls-client-ca",
            certs.ca_cert.to_str().expect("ca cert"),
        ],
    );

    let inventory_path = temp.path().join("managed-mtls.yaml");
    fs::write(
        &inventory_path,
        format!(
            r#"
targets:
  - id: managed-mtls
    transport: managed_mcp
    host: 127.0.0.1
    port: {port}
    auth_token_file: {}
    tls_ca_file: {}
    tls_client_cert_file: {}
    tls_client_key_file: {}
    tls_server_name: adc-managed.test
"#,
            token_path.display(),
            certs.ca_cert.display(),
            certs.client_cert.display(),
            certs.client_key.display()
        ),
    )
    .expect("inventory");

    let snapshot = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "fleet",
            "snapshot",
            "--inventory",
            inventory_path.to_str().expect("inventory"),
            "--fleet-run-id",
            "F-MANAGED-MTLS",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("fleet snapshot mtls");
    assert!(
        snapshot.status.success(),
        "fleet snapshot mtls failed: {}",
        String::from_utf8_lossy(&snapshot.stderr)
    );
    let response: serde_json::Value =
        serde_json::from_slice(&snapshot.stdout).expect("snapshot response json");
    assert_eq!(response["captured_count"], 1);
    assert_eq!(response["targets"][0]["transport"], "managed_mcp");
    assert!(response["targets"][0]["data_quality"]["notes"]
        .as_array()
        .expect("notes")
        .iter()
        .any(|note| note.as_str() == Some("transport=managed_mcp")));

    server.kill().expect("kill managed server");
    let _ = server.wait();
}

fn run_ok(command: &mut Command, label: &str) {
    let output = command.output().expect(label);
    assert!(
        output.status.success(),
        "{label} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn reserve_local_addr() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    drop(listener);
    addr.to_string()
}

fn spawn_managed_listener(
    root: &std::path::Path,
    addr: &str,
    token_path: &std::path::Path,
) -> Child {
    spawn_managed_listener_with_args(root, addr, token_path, &[])
}

fn spawn_managed_listener_with_args(
    root: &std::path::Path,
    addr: &str,
    token_path: &std::path::Path,
    extra_args: &[&str],
) -> Child {
    let server_bin = std::env::var("CARGO_BIN_EXE_adc-mcp").unwrap_or_else(|_| {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("workspace root")
            .join("target/debug/adc-mcp")
            .display()
            .to_string()
    });
    let child = Command::new(server_bin)
        .args([
            "--target-mode",
            "--managed-listen",
            addr,
            "--managed-token-file",
            token_path.to_str().expect("token path"),
        ])
        .args(extra_args)
        .env("ADC_HOME", root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn managed listener");
    wait_for_tcp(addr);
    child
}

struct ManagedMcpTestCerts {
    ca_cert: std::path::PathBuf,
    server_cert: std::path::PathBuf,
    server_key: std::path::PathBuf,
    client_cert: std::path::PathBuf,
    client_key: std::path::PathBuf,
}

fn write_managed_mcp_test_certs(root: &Path) -> ManagedMcpTestCerts {
    let cert_dir = root.join("certs");
    fs::create_dir_all(&cert_dir).expect("cert dir");
    let ca_cert = cert_dir.join("ca.pem");
    let ca_key = cert_dir.join("ca.key");
    let server_cert = cert_dir.join("server.pem");
    let server_key_path = cert_dir.join("server.key");
    let server_csr = cert_dir.join("server.csr");
    let server_ext = cert_dir.join("server.ext");
    let client_cert = cert_dir.join("client.pem");
    let client_key_path = cert_dir.join("client.key");
    let client_csr = cert_dir.join("client.csr");
    let client_ext = cert_dir.join("client.ext");

    run_openssl([
        "req",
        "-x509",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-days",
        "2",
        "-subj",
        "/CN=adc-managed-test-ca",
        "-addext",
        "basicConstraints=critical,CA:TRUE",
        "-addext",
        "keyUsage=critical,keyCertSign,cRLSign",
        "-keyout",
        ca_key.to_str().expect("ca key"),
        "-out",
        ca_cert.to_str().expect("ca cert"),
    ]);
    run_openssl([
        "req",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-subj",
        "/CN=adc-managed.test",
        "-keyout",
        server_key_path.to_str().expect("server key"),
        "-out",
        server_csr.to_str().expect("server csr"),
    ]);
    fs::write(
        &server_ext,
        "subjectAltName=DNS:adc-managed.test\nextendedKeyUsage=serverAuth\n",
    )
    .expect("server ext");
    run_openssl([
        "x509",
        "-req",
        "-in",
        server_csr.to_str().expect("server csr"),
        "-CA",
        ca_cert.to_str().expect("ca cert"),
        "-CAkey",
        ca_key.to_str().expect("ca key"),
        "-CAcreateserial",
        "-days",
        "2",
        "-extfile",
        server_ext.to_str().expect("server ext"),
        "-out",
        server_cert.to_str().expect("server cert"),
    ]);
    run_openssl([
        "req",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-subj",
        "/CN=adc-controller.test",
        "-keyout",
        client_key_path.to_str().expect("client key"),
        "-out",
        client_csr.to_str().expect("client csr"),
    ]);
    fs::write(&client_ext, "extendedKeyUsage=clientAuth\n").expect("client ext");
    run_openssl([
        "x509",
        "-req",
        "-in",
        client_csr.to_str().expect("client csr"),
        "-CA",
        ca_cert.to_str().expect("ca cert"),
        "-CAkey",
        ca_key.to_str().expect("ca key"),
        "-CAcreateserial",
        "-days",
        "2",
        "-extfile",
        client_ext.to_str().expect("client ext"),
        "-out",
        client_cert.to_str().expect("client cert"),
    ]);
    ManagedMcpTestCerts {
        ca_cert,
        server_cert,
        server_key: server_key_path,
        client_cert,
        client_key: client_key_path,
    }
}

fn run_openssl<const N: usize>(args: [&str; N]) {
    let output = Command::new("openssl")
        .args(args)
        .output()
        .expect("run openssl");
    assert!(
        output.status.success(),
        "openssl failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn wait_for_tcp(addr: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if TcpStream::connect(addr).is_ok() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "managed listener did not accept connections at {addr}"
        );
        thread::sleep(Duration::from_millis(25));
    }
}
