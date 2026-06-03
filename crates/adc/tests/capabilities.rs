use std::process::Command;

#[test]
fn capabilities_command_outputs_safety_aware_contract() {
    let output = Command::new(env!("CARGO_BIN_EXE_adc"))
        .arg("capabilities")
        .output()
        .expect("run capabilities");

    assert!(
        output.status.success(),
        "capabilities failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("capability json");
    assert_eq!(value["schema_version"], "obs.capability_report.v1");
    assert_eq!(value["target_id"], "local");
    assert!(value["generated_at_unix_ms"].as_u64().is_some());
    let capabilities = value["capabilities"].as_array().expect("capabilities");
    assert!(capabilities
        .iter()
        .any(|capability| capability["capability_id"] == "linux.proc.cpu"
            && capability["status"] == "supported"));
    assert!(capabilities
        .iter()
        .any(|capability| capability["capability_id"] == "kernel.ftrace"));
    assert!(value["data_quality"].is_object());
}
