use std::process::Command;

#[test]
fn helper_lists_only_allowlisted_operations() {
    let output = Command::new(env!("CARGO_BIN_EXE_adc-priv-helper"))
        .arg("--list-ops")
        .output()
        .expect("list ops");

    assert!(
        output.status.success(),
        "list ops failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("ops json");
    let ops = value["operations"].as_array().expect("operations");
    assert!(ops.iter().any(|op| op == "capability-report"));
    assert!(!ops.iter().any(|op| op == "shell"));
}

#[test]
fn helper_rejects_unknown_operation() {
    let output = Command::new(env!("CARGO_BIN_EXE_adc-priv-helper"))
        .arg("shell")
        .output()
        .expect("run unknown op");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not allowlisted"));
}
