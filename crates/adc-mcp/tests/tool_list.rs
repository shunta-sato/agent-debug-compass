use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use adc_core::{capture_fleet, capture_for, CaptureOptions, FleetCaptureOptions};

const EXPECTED_TOOLS: &[&str] = &[
    "obs.status",
    "obs.doctor",
    "obs.preflight",
    "obs.snapshot",
    "obs.observe",
    "obs.get_agent_context",
    "obs.investigate_bug",
    "obs.start_investigation",
    "obs.continue_investigation",
    "obs.get_investigation_session",
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

const EXPECTED_RESOURCES: &[&str] = &["obs://runs", "obs://capabilities"];

const EXPECTED_RESOURCE_TEMPLATES: &[&str] = &[
    "obs://runs/{run_id}/evidence",
    "obs://runs/{run_id}/timeline",
    "obs://runs/{run_id}/windows/{window_id}",
    "obs://fleet/{fleet_run_id}/evidence",
    "obs://compare/{before_run_id}/{after_run_id}",
];

const EXPECTED_TARGET_RESOURCE_TEMPLATES: &[&str] = &[
    "obs://runs/{run_id}/evidence",
    "obs://runs/{run_id}/timeline",
    "obs://runs/{run_id}/windows/{window_id}",
    "obs://compare/{before_run_id}/{after_run_id}",
];

#[test]
fn tool_list_is_bounded_and_contains_mvp_tools() {
    let output = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .arg("--tool-list-json")
        .output()
        .expect("run tool list");

    assert!(
        output.status.success(),
        "tool list failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("tool list json");
    let tools = value["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool.as_str().expect("tool name"))
        .collect::<Vec<_>>();
    assert_eq!(tools, EXPECTED_TOOLS);
    assert!(!value["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .any(|tool| tool.as_str().is_some_and(|name| name.contains("shell"))));
}

#[test]
fn target_mode_tool_list_excludes_controller_fleet_tools() {
    let output = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .args(["--target-mode", "--tool-list-json"])
        .output()
        .expect("run target-mode tool list");

    assert!(
        output.status.success(),
        "target-mode tool list failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("tool list json");
    let tools = value["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool.as_str().expect("tool name"))
        .collect::<Vec<_>>();
    assert!(tools.contains(&"obs.observe"));
    assert!(tools.contains(&"obs.investigate_service"));
    assert!(tools.contains(&"obs.get_evidence_index"));
    assert!(!tools.iter().any(|tool| tool.starts_with("obs.fleet_")));
    assert!(!tools.contains(&"obs.discover_targets"));
    assert!(!tools.contains(&"obs.get_fleet_evidence"));
}

#[test]
fn target_mode_resources_exclude_fleet_templates_and_fleet_context() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .arg("--target-mode")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn target-mode mcp server");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut stdin = child.stdin.take().expect("stdin");
    initialize(&mut stdin, &mut stdout);

    write_request(
        &mut stdin,
        2,
        "resources/templates/list",
        serde_json::json!({}),
    );
    let templates = read_response(&mut stdout);
    let template_uris = templates["result"]["resourceTemplates"]
        .as_array()
        .expect("resource templates")
        .iter()
        .map(|template| template["uriTemplate"].as_str().expect("uri template"))
        .collect::<Vec<_>>();
    assert_eq!(template_uris, EXPECTED_TARGET_RESOURCE_TEMPLATES);

    write_request(
        &mut stdin,
        3,
        "tools/call",
        serde_json::json!({
            "name": "obs.get_agent_context",
            "arguments": {
                "fleet_run_id": "F-LOCAL"
            }
        }),
    );
    let context_response = read_response(&mut stdout);
    assert!(context_response["error"]["message"]
        .as_str()
        .expect("error message")
        .contains("fleet_run_id is not available in target mode"));

    drop(stdin);
    let output = child.wait_with_output().expect("wait target-mode server");
    assert!(
        output.status.success(),
        "target-mode stdio server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn managed_listener_requires_token_and_serves_target_tools_only() {
    let temp = tempfile::tempdir().expect("tempdir");
    let token_path = temp.path().join("managed.token");
    fs::write(&token_path, "test-managed-token\n").expect("token");
    let addr = reserve_local_addr();

    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .args([
            "--target-mode",
            "--managed-listen",
            &addr,
            "--managed-token-file",
            token_path.to_str().expect("token path"),
        ])
        .env("ADC_HOME", temp.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn managed listener");

    wait_for_tcp(&addr);

    let unauthorized =
        managed_mcp_request(&addr, None, managed_tool_call("obs.status", json_obj()));
    assert!(unauthorized.starts_with("HTTP/1.1 401"));

    let status = managed_mcp_request(
        &addr,
        Some("test-managed-token"),
        managed_tool_call("obs.status", json_obj()),
    );
    assert!(status.starts_with("HTTP/1.1 200"));
    assert!(status.contains("\"service\":\"adc-mcp\""));

    let fleet = managed_mcp_request(
        &addr,
        Some("test-managed-token"),
        managed_tool_call(
            "obs.fleet_preflight",
            serde_json::json!({"inventory_path": "/tmp/nope"}),
        ),
    );
    assert!(fleet.starts_with("HTTP/1.1 200"));
    assert!(fleet.contains("not available in target mode"));

    child.kill().expect("kill managed listener");
    let output = child.wait_with_output().expect("wait managed listener");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("managed_mcp.listen"));
    assert!(!stderr.contains("test-managed-token"));
}

#[test]
fn managed_listener_requires_target_mode() {
    let temp = tempfile::tempdir().expect("tempdir");
    let token_path = temp.path().join("managed.token");
    fs::write(&token_path, "test-managed-token\n").expect("token");
    let addr = reserve_local_addr();

    let output = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .args([
            "--managed-listen",
            &addr,
            "--managed-token-file",
            token_path.to_str().expect("token path"),
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run managed listener without target-mode");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--target-mode"));
}

#[test]
fn managed_listener_reloads_rotated_token_file() {
    let temp = tempfile::tempdir().expect("tempdir");
    let token_path = temp.path().join("managed.token");
    fs::write(&token_path, "old-managed-token\n").expect("token");
    let addr = reserve_local_addr();

    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .args([
            "--target-mode",
            "--managed-listen",
            &addr,
            "--managed-token-file",
            token_path.to_str().expect("token path"),
        ])
        .env("ADC_HOME", temp.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn managed listener");

    wait_for_tcp(&addr);

    let old_ok = managed_mcp_request(
        &addr,
        Some("old-managed-token"),
        managed_tool_call("obs.status", json_obj()),
    );
    assert!(old_ok.starts_with("HTTP/1.1 200"));

    fs::write(&token_path, "new-managed-token\n").expect("rotate token");

    let old_denied = managed_mcp_request(
        &addr,
        Some("old-managed-token"),
        managed_tool_call("obs.status", json_obj()),
    );
    assert!(old_denied.starts_with("HTTP/1.1 401"));

    let new_ok = managed_mcp_request(
        &addr,
        Some("new-managed-token"),
        managed_tool_call("obs.status", json_obj()),
    );
    assert!(new_ok.starts_with("HTTP/1.1 200"));

    child.kill().expect("kill managed listener");
    let output = child.wait_with_output().expect("wait managed listener");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("old-managed-token"));
    assert!(!stderr.contains("new-managed-token"));
}

#[test]
fn managed_listener_keeps_serving_while_one_connection_is_slow() {
    let temp = tempfile::tempdir().expect("tempdir");
    let token_path = temp.path().join("managed.token");
    fs::write(&token_path, "test-managed-token\n").expect("token");
    let addr = reserve_local_addr();

    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .args([
            "--target-mode",
            "--managed-listen",
            &addr,
            "--managed-token-file",
            token_path.to_str().expect("token path"),
        ])
        .env("ADC_HOME", temp.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn managed listener");

    wait_for_tcp(&addr);

    let mut slow_stream = TcpStream::connect(&addr).expect("connect slow client");
    slow_stream
        .write_all(b"POST /mcp HTTP/1.1\r\nHost: slow-client\r\n")
        .expect("write partial request");

    let status = managed_mcp_request(
        &addr,
        Some("test-managed-token"),
        managed_tool_call("obs.status", json_obj()),
    );
    assert!(status.starts_with("HTTP/1.1 200"));

    drop(slow_stream);
    child.kill().expect("kill managed listener");
    child.wait().expect("wait managed listener");
}

#[test]
fn managed_listener_fails_closed_when_token_file_is_unavailable() {
    let temp = tempfile::tempdir().expect("tempdir");
    let token_path = temp.path().join("managed.token");
    fs::write(&token_path, "test-managed-token\n").expect("token");
    let addr = reserve_local_addr();

    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .args([
            "--target-mode",
            "--managed-listen",
            &addr,
            "--managed-token-file",
            token_path.to_str().expect("token path"),
        ])
        .env("ADC_HOME", temp.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn managed listener");

    wait_for_tcp(&addr);
    fs::remove_file(&token_path).expect("remove token");

    let response = managed_mcp_request(
        &addr,
        Some("test-managed-token"),
        managed_tool_call("obs.status", json_obj()),
    );
    assert!(response.starts_with("HTTP/1.1 503"));
    assert!(!response.contains("test-managed-token"));

    child.kill().expect("kill managed listener");
    let output = child.wait_with_output().expect("wait managed listener");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("auth_unavailable"));
    assert!(!stderr.contains("test-managed-token"));
}

#[test]
fn stdio_tools_list_returns_bounded_read_only_tools() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp server");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut stdin = child.stdin.take().expect("stdin");
    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "0.0.0"}
            }
        })
    )
    .expect("write initialize");
    stdin.flush().expect("flush initialize");

    let mut initialize_response = String::new();
    stdout
        .read_line(&mut initialize_response)
        .expect("read initialize response");
    assert!(initialize_response.contains("\"id\":1"));

    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        })
    )
    .expect("write initialized notification");
    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        })
    )
    .expect("write tool list");
    stdin.flush().expect("flush tool list");

    let mut list_response_line = String::new();
    stdout
        .read_line(&mut list_response_line)
        .expect("read tool list response");
    drop(stdin);

    let output = child.wait_with_output().expect("wait mcp server");
    assert!(
        output.status.success(),
        "stdio server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let list_response: serde_json::Value =
        serde_json::from_str(&list_response_line).expect("tools/list json");
    let tools = list_response["result"]["tools"].as_array().expect("tools");
    let tool_names = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name"))
        .collect::<Vec<_>>();
    assert_eq!(tool_names, EXPECTED_TOOLS);
    assert!(tools.iter().all(|tool| {
        let is_write = matches!(
            tool["name"].as_str(),
            Some(
                "obs.snapshot"
                    | "obs.observe"
                    | "obs.investigate_bug"
                    | "obs.start_investigation"
                    | "obs.continue_investigation"
                    | "obs.fleet_observe"
                    | "obs.fleet_capture"
                    | "obs.fleet_snapshot",
            )
        );
        let readonly_ok = if is_write {
            tool["annotations"]["readOnlyHint"] == false
        } else {
            tool["annotations"]["readOnlyHint"] == true
        };
        readonly_ok
            && tool["annotations"]["destructiveHint"] == false
            && tool["inputSchema"].is_object()
    }));
}

#[test]
fn stdio_list_route_packs_returns_typed_registry() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .env("ADC_HOME", temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp server");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut stdin = child.stdin.take().expect("stdin");
    initialize(&mut stdin, &mut stdout);

    write_request(
        &mut stdin,
        2,
        "tools/call",
        serde_json::json!({
            "name": "obs.list_route_packs",
            "arguments": {}
        }),
    );
    let response = read_response(&mut stdout);
    let registry = &response["result"]["structuredContent"];
    assert_eq!(registry["schema_version"], "obs.route_pack_registry.v1");
    assert!(registry["packs"]
        .as_array()
        .expect("packs")
        .iter()
        .any(|pack| pack["domain"] == "network_degradation"));

    drop(stdin);
    let output = child.wait_with_output().expect("wait mcp server");
    assert!(
        output.status.success(),
        "stdio server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn stdio_investigate_service_returns_structured_pack() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .env("ADC_HOME", temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp server");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut stdin = child.stdin.take().expect("stdin");
    initialize(&mut stdin, &mut stdout);

    write_request(
        &mut stdin,
        2,
        "tools/call",
        serde_json::json!({
            "name": "obs.investigate_service",
            "arguments": {
                "service_name": "definitely-not-a-real-adc-service",
                "max_journal_lines": 5
            }
        }),
    );
    let response = read_response(&mut stdout);
    assert_eq!(
        response["result"]["structuredContent"]["schema_version"],
        "obs.service_investigation.v1"
    );
    assert_eq!(
        response["result"]["structuredContent"]["service_name"],
        "definitely-not-a-real-adc-service"
    );
    assert_eq!(
        response["result"]["structuredContent"]["root_required"],
        false
    );

    drop(stdin);
    let output = child.wait_with_output().expect("wait mcp server");
    assert!(
        output.status.success(),
        "stdio server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn stdio_resources_and_prompts_are_listed() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp server");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut stdin = child.stdin.take().expect("stdin");
    initialize(&mut stdin, &mut stdout);

    write_request(&mut stdin, 2, "resources/list", serde_json::json!({}));
    let resources = read_response(&mut stdout);
    let resource_uris = resources["result"]["resources"]
        .as_array()
        .expect("resources")
        .iter()
        .map(|resource| resource["uri"].as_str().expect("resource uri"))
        .collect::<Vec<_>>();
    assert_eq!(resource_uris, EXPECTED_RESOURCES);

    write_request(
        &mut stdin,
        3,
        "resources/templates/list",
        serde_json::json!({}),
    );
    let templates = read_response(&mut stdout);
    let template_uris = templates["result"]["resourceTemplates"]
        .as_array()
        .expect("resource templates")
        .iter()
        .map(|template| template["uriTemplate"].as_str().expect("uri template"))
        .collect::<Vec<_>>();
    assert_eq!(template_uris, EXPECTED_RESOURCE_TEMPLATES);

    write_request(&mut stdin, 4, "prompts/list", serde_json::json!({}));
    let prompts = read_response(&mut stdout);
    assert!(prompts["result"]["prompts"]
        .as_array()
        .expect("prompts")
        .iter()
        .any(|prompt| prompt["name"] == "inspect-evidence-index"));

    drop(stdin);
    let output = child.wait_with_output().expect("wait mcp server");
    assert!(
        output.status.success(),
        "stdio server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn stdio_preflight_returns_structured_target_readiness() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .env("ADC_HOME", temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp server");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut stdin = child.stdin.take().expect("stdin");
    initialize(&mut stdin, &mut stdout);

    write_request(
        &mut stdin,
        2,
        "tools/call",
        serde_json::json!({
            "name": "obs.preflight",
            "arguments": {
                "target_id": "target-a"
            }
        }),
    );
    let response = read_response(&mut stdout);
    drop(stdin);

    let output = child.wait_with_output().expect("wait mcp server");
    assert!(
        output.status.success(),
        "stdio server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let preflight = &response["result"]["structuredContent"];
    assert_eq!(preflight["schema_version"], "obs.target_preflight.v1");
    assert_eq!(preflight["target_id"], "target-a");
    assert_eq!(preflight["status"], "ready");
    assert_eq!(preflight["root_required"], false);
    assert!(preflight["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .any(|check| check["name"] == "artifact_root_writable" && check["status"] == "ok"));
}

#[test]
fn stdio_snapshot_creates_target_scoped_evidence() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .env("ADC_HOME", temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp server");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut stdin = child.stdin.take().expect("stdin");
    initialize(&mut stdin, &mut stdout);

    write_request(
        &mut stdin,
        2,
        "tools/call",
        serde_json::json!({
            "name": "obs.snapshot",
            "arguments": {
                "run_id": "R-MCP-SNAPSHOT",
                "target_id": "target-a"
            }
        }),
    );
    let response = read_response(&mut stdout);
    drop(stdin);

    let output = child.wait_with_output().expect("wait mcp server");
    assert!(
        output.status.success(),
        "stdio server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let snapshot = &response["result"]["structuredContent"];
    assert_eq!(snapshot["run_id"], "R-MCP-SNAPSHOT");
    assert_eq!(snapshot["target_id"], "target-a");
    assert!(snapshot["evidence_index"]
        .as_str()
        .expect("evidence path")
        .contains("evidence_index.yaml"));
}

#[test]
fn stdio_get_agent_context_returns_structured_context_for_existing_run() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-MCP-AGENT-CONTEXT";
    capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
            collectors: vec![
                "cpu".to_string(),
                "memory".to_string(),
                "network".to_string(),
            ],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");

    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .env("ADC_HOME", temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp server");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut stdin = child.stdin.take().expect("stdin");
    initialize(&mut stdin, &mut stdout);

    write_request(
        &mut stdin,
        2,
        "tools/call",
        serde_json::json!({
            "name": "obs.get_agent_context",
            "arguments": {
                "run_id": run_id
            }
        }),
    );
    let response = read_response(&mut stdout);
    drop(stdin);

    let output = child.wait_with_output().expect("wait mcp server");
    assert!(
        output.status.success(),
        "stdio server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let context = &response["result"]["structuredContent"];
    assert_eq!(context["schema_version"], "obs.agent_context.v1");
    assert_eq!(context["run_id"], run_id);
    assert!(context["derived_facts"]
        .as_array()
        .expect("facts")
        .iter()
        .any(|fact| fact["kind"] == "cpu_busy_percent"));
}

#[test]
fn stdio_start_investigation_returns_route_and_agent_context() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-MCP-INVESTIGATION-START";
    capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
            collectors: vec!["cpu".to_string(), "memory".to_string()],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");
    let fake_bin = temp.path().join("fake-bin");
    fs::create_dir_all(&fake_bin).expect("fake bin");
    write_executable(
        &fake_bin.join("systemctl"),
        "#!/usr/bin/env bash\ncat <<'OUT'\nId=ssh.service\nLoadState=loaded\nActiveState=active\nSubState=running\nMainPID=999999\nFragmentPath=/usr/lib/systemd/system/ssh.service\nOUT\n",
    );
    write_executable(
        &fake_bin.join("journalctl"),
        "#!/usr/bin/env bash\ncat <<'OUT'\n2026-05-27T00:03:00+09:00 host sshd[3]: timeout while flushing session\nOUT\n",
    );
    let old_path = std::env::var("PATH").unwrap_or_default();

    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .env("ADC_HOME", temp.path())
        .env("PATH", format!("{}:{old_path}", fake_bin.display()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp server");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut stdin = child.stdin.take().expect("stdin");
    initialize(&mut stdin, &mut stdout);

    write_request(
        &mut stdin,
        2,
        "tools/call",
        serde_json::json!({
            "name": "obs.start_investigation",
            "arguments": {
                "run_id": run_id,
                "service_name": "ssh",
                "max_journal_lines": 1
            }
        }),
    );
    let response = read_response(&mut stdout);
    let pack = &response["result"]["structuredContent"];
    assert_eq!(pack["schema_version"], "obs.investigation_start.v1");
    assert_eq!(pack["scope"], "run");
    assert_eq!(pack["agent_context"]["run_id"], run_id);
    assert_eq!(
        pack["investigation_route"]["schema_version"],
        "obs.investigation_route.v1"
    );
    assert_eq!(pack["investigation_route"]["service_name"], "ssh");
    assert!(
        pack["investigation_route"]["raw_refs"]["service.journal_leads"]
            .as_str()
            .expect("journal ref")
            .contains("artifact://service_investigations/ssh/journal_leads.json")
    );
    assert!(pack["investigation_route"]["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .all(|step| step["cause_neutral"] == true));
    drop(stdin);
    let output = child.wait_with_output().expect("wait mcp server");
    assert!(
        output.status.success(),
        "stdio server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn stdio_investigate_bug_returns_symptom_context() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-MCP-SYMPTOM-CONTEXT";
    capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
            collectors: vec!["cpu".to_string(), "memory".to_string()],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");

    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .env("ADC_HOME", temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp server");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut stdin = child.stdin.take().expect("stdin");
    initialize(&mut stdin, &mut stdout);

    write_request(
        &mut stdin,
        2,
        "tools/call",
        serde_json::json!({
            "name": "obs.investigate_bug",
            "arguments": {
                "run_id": run_id,
                "symptom": "memory pressure"
            }
        }),
    );
    let response = read_response(&mut stdout);
    let context = &response["result"]["structuredContent"];
    assert_eq!(context["schema_version"], "obs.symptom_context.v1");
    assert_eq!(context["symptom"]["normalized"], "memory_growth");
    assert!(context["compiled_route"]["selected_packs"]
        .as_array()
        .expect("selected packs")
        .iter()
        .any(|pack| pack["domain"] == "memory_growth"));
    assert!(context["facts"]
        .as_array()
        .expect("facts")
        .iter()
        .any(|fact| fact["fact_id"] == "resource.memory_available_bytes"));

    drop(stdin);
    let output = child.wait_with_output().expect("wait mcp server");
    assert!(
        output.status.success(),
        "stdio server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn stdio_continue_investigation_returns_bounded_session_pack() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-MCP-INVESTIGATION-CONTINUE";
    capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
            collectors: vec!["cpu".to_string(), "memory".to_string()],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");
    let fake_bin = temp.path().join("fake-bin");
    fs::create_dir_all(&fake_bin).expect("fake bin");
    write_executable(
        &fake_bin.join("systemctl"),
        "#!/usr/bin/env bash\ncat <<'OUT'\nId=ssh.service\nLoadState=loaded\nActiveState=active\nSubState=running\nMainPID=999999\nFragmentPath=/usr/lib/systemd/system/ssh.service\nOUT\n",
    );
    write_executable(
        &fake_bin.join("journalctl"),
        "#!/usr/bin/env bash\ncat <<'OUT'\n2026-05-27T00:03:00+09:00 host sshd[3]: timeout while flushing session\nOUT\n",
    );
    let old_path = std::env::var("PATH").unwrap_or_default();

    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .env("ADC_HOME", temp.path())
        .env("PATH", format!("{}:{old_path}", fake_bin.display()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp server");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut stdin = child.stdin.take().expect("stdin");
    initialize(&mut stdin, &mut stdout);

    write_request(
        &mut stdin,
        2,
        "tools/call",
        serde_json::json!({
            "name": "obs.start_investigation",
            "arguments": {
                "run_id": run_id,
                "service_name": "ssh",
                "max_journal_lines": 1
            }
        }),
    );
    let _start_response = read_response(&mut stdout);

    write_request(
        &mut stdin,
        3,
        "tools/call",
        serde_json::json!({
            "name": "obs.continue_investigation",
            "arguments": {
                "run_id": run_id,
                "service_name": "ssh",
                "current_step_id": "IR001"
            }
        }),
    );
    let response = read_response(&mut stdout);
    let pack = &response["result"]["structuredContent"];
    assert_eq!(pack["schema_version"], "obs.investigation_continue.v1");
    assert_eq!(pack["scope"], "run");
    assert_eq!(pack["run_id"], run_id);
    assert_eq!(pack["current_step_id"], "IR001");
    assert!(pack["opened_refs"]
        .as_array()
        .expect("opened refs")
        .iter()
        .any(|entry| entry["label"] == "service_state"));
    assert!(pack["opened_refs"]
        .as_array()
        .expect("opened refs")
        .iter()
        .all(|entry| entry["text"].is_null()));
    assert!(pack["investigation_route"]["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .all(|step| step["cause_neutral"] == true));
    assert!(temp
        .path()
        .join(format!(
            "runs/{run_id}/investigation_sessions/{}.json",
            pack["session_id"].as_str().expect("session id")
        ))
        .is_file());
    write_request(
        &mut stdin,
        4,
        "tools/call",
        serde_json::json!({
            "name": "obs.get_investigation_session",
            "arguments": {
                "run_id": run_id,
                "session_id": pack["session_id"].as_str().expect("session id")
            }
        }),
    );
    let session_response = read_response(&mut stdout);
    let state = &session_response["result"]["structuredContent"];
    assert_eq!(
        state["schema_version"],
        "obs.investigation_session_state.v1"
    );
    assert!(state["completed_steps"]
        .as_array()
        .expect("completed steps")
        .iter()
        .any(|step| step == "IR001"));
    assert!(state["branch_evaluations"]
        .as_array()
        .expect("branch evaluations")
        .iter()
        .any(|branch| branch["status"] == "matched"));
    drop(stdin);

    let output = child.wait_with_output().expect("wait mcp server");
    assert!(
        output.status.success(),
        "stdio server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn stdio_get_ref_resolves_window_refs_for_existing_run() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-MCP-GET-REF";
    capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
            collectors: vec!["cpu".to_string()],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");

    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .env("ADC_HOME", temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp server");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut stdin = child.stdin.take().expect("stdin");
    initialize(&mut stdin, &mut stdout);

    write_request(
        &mut stdin,
        2,
        "tools/call",
        serde_json::json!({
            "name": "obs.get_ref",
            "arguments": {
                "run_id": run_id,
                "ref": "artifact://windows/W001.yaml",
                "limit": 20
            }
        }),
    );
    let response = read_response(&mut stdout);
    drop(stdin);

    let output = child.wait_with_output().expect("wait mcp server");
    assert!(
        output.status.success(),
        "stdio server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let resolved = &response["result"]["structuredContent"];
    assert_eq!(resolved["ref_kind"], "window");
    assert!(resolved["text"]
        .as_str()
        .expect("resolved text")
        .contains("window_id: W001"));
}

#[test]
fn stdio_get_ref_resolves_service_investigation_refs_without_run_id() {
    let temp = tempfile::tempdir().expect("tempdir");
    let service_dir = temp.path().join("service_investigations/ssh");
    std::fs::create_dir_all(&service_dir).expect("service dir");
    std::fs::write(
        service_dir.join("service_state.json"),
        r#"{"active_state":"active","sub_state":"running"}"#,
    )
    .expect("service state");

    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .env("ADC_HOME", temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp server");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut stdin = child.stdin.take().expect("stdin");
    initialize(&mut stdin, &mut stdout);

    write_request(
        &mut stdin,
        2,
        "tools/call",
        serde_json::json!({
            "name": "obs.get_ref",
            "arguments": {
                "ref": "artifact://service_investigations/ssh/service_state.json",
                "limit": 20
            }
        }),
    );
    let response = read_response(&mut stdout);
    drop(stdin);

    let output = child.wait_with_output().expect("wait mcp server");
    assert!(
        output.status.success(),
        "stdio server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let resolved = &response["result"]["structuredContent"];
    assert_eq!(resolved["ref_kind"], "service_investigation");
    assert!(resolved["text"]
        .as_str()
        .expect("resolved text")
        .contains("active_state"));
}

#[test]
fn stdio_fleet_investigate_service_returns_partial_target_packs() {
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
    .expect("inventory");
    let fake_bin = temp.path().join("fake-bin");
    fs::create_dir_all(&fake_bin).expect("fake bin");
    write_executable(
        &fake_bin.join("systemctl"),
        r#"#!/usr/bin/env sh
printf '%s\n' \
  'Id=ssh.service' \
  'LoadState=loaded' \
  'ActiveState=active' \
  'SubState=running' \
  'MainPID=999999' \
  'FragmentPath=/usr/lib/systemd/system/ssh.service'
"#,
    );
    write_executable(
        &fake_bin.join("journalctl"),
        r#"#!/usr/bin/env sh
printf '%s\n' '2026-05-27T00:03:00+09:00 host sshd[11]: timeout waiting for auth'
"#,
    );
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .env("ADC_HOME", temp.path())
        .env("PATH", path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp server");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut stdin = child.stdin.take().expect("stdin");
    initialize(&mut stdin, &mut stdout);

    write_request(
        &mut stdin,
        2,
        "tools/call",
        serde_json::json!({
            "name": "obs.fleet_investigate_service",
            "arguments": {
                "inventory_path": inventory_path,
                "fleet_run_id": "F-MCP-SERVICE",
                "service_name": "ssh",
                "max_journal_lines": 2
            }
        }),
    );
    let response = read_response(&mut stdout);
    drop(stdin);

    let output = child.wait_with_output().expect("wait mcp server");
    assert!(
        output.status.success(),
        "stdio server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result = &response["result"]["structuredContent"];
    assert_eq!(
        result["schema_version"],
        "obs.fleet_service_investigation.v1"
    );
    assert_eq!(result["captured_count"], 1);
    assert_eq!(result["failed_count"], 1);
    assert_eq!(
        result["targets"][0]["service_pack"]["service_state"]["active_state"],
        "active"
    );
    assert!(result["data_quality"]["missing"]
        .as_array()
        .expect("missing")
        .iter()
        .any(|entry| entry
            .as_str()
            .expect("missing string")
            .contains("unsupported-service")));
}

#[test]
fn stdio_observe_accepts_optional_agent_context_inputs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let log_path = temp.path().join("app.log");
    std::fs::write(&log_path, "info booted\nerror timeout request_id=abc\n").expect("log");
    let domain_path = temp.path().join("domain_events.jsonl");
    std::fs::write(
        &domain_path,
        r#"{"event_type":"sensor_frame_gap","frame_id":"42","gap_ms":120}"#,
    )
    .expect("domain events");

    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .env("ADC_HOME", temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp server");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut stdin = child.stdin.take().expect("stdin");
    initialize(&mut stdin, &mut stdout);

    write_request(
        &mut stdin,
        2,
        "tools/call",
        serde_json::json!({
            "name": "obs.observe",
            "arguments": {
                "run_id": "R-MCP-OBSERVE-INPUTS",
                "duration_ms": 120,
                "interval_ms": 40,
                "log_file": log_path,
                "domain_events_file": domain_path,
                "service_name": "sensor-gateway"
            }
        }),
    );
    let response = read_response(&mut stdout);
    drop(stdin);

    let output = child.wait_with_output().expect("wait mcp server");
    assert!(
        output.status.success(),
        "stdio server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let context = &response["result"]["structuredContent"]["agent_context"];
    let kinds = context["derived_facts"]
        .as_array()
        .expect("facts")
        .iter()
        .map(|fact| fact["kind"].as_str().expect("kind"))
        .collect::<Vec<_>>();
    assert!(kinds.contains(&"log_error_slice"));
    assert!(kinds.contains(&"domain_event_count"));
    assert!(kinds.contains(&"service_state"));
}

#[test]
fn stdio_fleet_preflight_returns_target_matrix() {
    let temp = tempfile::tempdir().expect("tempdir");
    let inventory_path = temp.path().join("targets.yaml");
    std::fs::write(
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

    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .env("ADC_HOME", temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp server");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut stdin = child.stdin.take().expect("stdin");
    initialize(&mut stdin, &mut stdout);

    write_request(
        &mut stdin,
        2,
        "tools/call",
        serde_json::json!({
            "name": "obs.fleet_preflight",
            "arguments": {
                "inventory_path": inventory_path
            }
        }),
    );
    let response = read_response(&mut stdout);
    drop(stdin);

    let output = child.wait_with_output().expect("wait mcp server");
    assert!(
        output.status.success(),
        "stdio server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let preflight = &response["result"]["structuredContent"];
    assert_eq!(preflight["schema_version"], "obs.fleet_preflight.v1");
    assert_eq!(preflight["status"], "degraded");
    assert_eq!(preflight["ready_count"], 1);
    assert_eq!(preflight["failed_count"], 1);
    assert_eq!(preflight["targets"][0]["target_id"], "local-a");
    assert_eq!(preflight["targets"][1]["status"], "unsupported");
}

#[test]
fn stdio_get_agent_context_returns_fleet_context_for_fleet_run() {
    let temp = tempfile::tempdir().expect("tempdir");
    let inventory_path = temp.path().join("targets.yaml");
    std::fs::write(
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
    capture_fleet(
        temp.path(),
        &inventory_path,
        FleetCaptureOptions {
            fleet_run_id: "F-MCP-AGENT-CONTEXT".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
        },
    )
    .expect("fleet capture");

    let mut child = Command::new(env!("CARGO_BIN_EXE_adc-mcp"))
        .env("ADC_HOME", temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcp server");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut stdin = child.stdin.take().expect("stdin");
    initialize(&mut stdin, &mut stdout);

    write_request(
        &mut stdin,
        2,
        "tools/call",
        serde_json::json!({
            "name": "obs.get_agent_context",
            "arguments": {
                "fleet_run_id": "F-MCP-AGENT-CONTEXT"
            }
        }),
    );
    let response = read_response(&mut stdout);
    drop(stdin);

    let output = child.wait_with_output().expect("wait mcp server");
    assert!(
        output.status.success(),
        "stdio server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let context = &response["result"]["structuredContent"];
    assert_eq!(context["schema_version"], "obs.agent_context.fleet.v1");
    assert_eq!(context["fleet_run_id"], "F-MCP-AGENT-CONTEXT");
    assert_eq!(context["captured_count"], 1);
    assert_eq!(context["failed_count"], 1);
}

fn initialize(stdin: &mut impl Write, stdout: &mut impl BufRead) {
    write_request(
        stdin,
        1,
        "initialize",
        serde_json::json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "0.0.0"}
        }),
    );
    let initialize_response = read_response(stdout);
    assert_eq!(initialize_response["id"], 1);
    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        })
    )
    .expect("write initialized notification");
}

fn write_request(stdin: &mut impl Write, id: u64, method: &str, params: serde_json::Value) {
    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        })
    )
    .expect("write request");
    stdin.flush().expect("flush request");
}

fn read_response(stdout: &mut impl BufRead) -> serde_json::Value {
    let mut response = String::new();
    stdout.read_line(&mut response).expect("read response");
    serde_json::from_str(&response).expect("response json")
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).expect("write executable");
    let mut permissions = fs::metadata(path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("chmod");
}

fn reserve_local_addr() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");
    drop(listener);
    addr.to_string()
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

fn managed_tool_call(tool_name: &str, arguments: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": arguments
        }
    })
}

fn json_obj() -> serde_json::Value {
    serde_json::json!({})
}

fn managed_mcp_request(addr: &str, token: Option<&str>, body: serde_json::Value) -> String {
    let body = body.to_string();
    let mut request = format!(
        "POST /mcp HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        body.len()
    );
    if let Some(token) = token {
        request.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(&body);

    let mut stream = connect_managed_listener(addr);
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    stream
        .write_all(request.as_bytes())
        .expect("write managed request");
    let mut response = String::new();
    BufReader::new(stream)
        .read_to_string(&mut response)
        .expect("read managed response");
    response
}

fn connect_managed_listener(addr: &str) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match TcpStream::connect(addr) {
            Ok(stream) => return stream,
            Err(err) if Instant::now() < deadline => {
                let _ = err;
                thread::sleep(Duration::from_millis(20));
            }
            Err(err) => panic!("connect managed listener at {addr}: {err}"),
        }
    }
}
