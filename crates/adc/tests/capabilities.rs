use std::process::Command;

#[test]
fn capabilities_command_outputs_bounded_kernel_map() {
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
    assert!(value["arch"].as_str().is_some());
    assert!(value["data_quality"].is_object());
    assert!(value.get("ftrace_available").is_some());
    assert!(value.get("perf_available").is_some());
}
