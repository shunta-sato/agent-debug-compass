use std::process::Command;

#[test]
fn status_command_emits_ready_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_adc"))
        .arg("status")
        .output()
        .expect("run adc status");

    assert!(
        output.status.success(),
        "status command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("status stdout is json");
    assert_eq!(value["service"], "adc");
    assert_eq!(value["status"], "ready");
    assert!(value["version"].as_str().is_some());
    assert!(value["message"].as_str().is_some());
}
