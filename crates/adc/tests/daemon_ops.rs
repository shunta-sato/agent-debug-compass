use std::{fs, process::Command};

#[test]
fn arm_disarm_capture_and_evidence_get_use_persistent_state() {
    let temp = tempfile::tempdir().expect("tempdir");

    let arm = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["arm", "--profile", "pi5_basic"])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run arm");
    assert!(
        arm.status.success(),
        "arm failed: {}",
        String::from_utf8_lossy(&arm.stderr)
    );
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(temp.path().join("daemon/state.json")).expect("state"))
            .expect("state json");
    assert_eq!(state["active_profile"], "pi5_basic");
    assert_eq!(state["status"], "armed");

    let capture = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "capture",
            "--run-id",
            "R-CAPTURE-001",
            "--duration-ms",
            "10",
            "--interval-ms",
            "10",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run capture");
    assert!(
        capture.status.success(),
        "capture failed: {}",
        String::from_utf8_lossy(&capture.stderr)
    );
    let capture_response: serde_json::Value =
        serde_json::from_slice(&capture.stdout).expect("capture json");
    assert_eq!(capture_response["run_id"], "R-CAPTURE-001");
    assert_eq!(capture_response["target_id"], "local");

    let evidence = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["evidence", "get", "--run-id", "R-CAPTURE-001"])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run evidence get");
    assert!(
        evidence.status.success(),
        "evidence get failed: {}",
        String::from_utf8_lossy(&evidence.stderr)
    );
    let evidence_stdout = String::from_utf8(evidence.stdout).expect("evidence utf8");
    assert!(evidence_stdout.contains("run_id: R-CAPTURE-001"));
    assert!(evidence_stdout.contains("raw_refs:"));

    let disarm = Command::new(env!("CARGO_BIN_EXE_adc"))
        .arg("disarm")
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run disarm");
    assert!(
        disarm.status.success(),
        "disarm failed: {}",
        String::from_utf8_lossy(&disarm.stderr)
    );
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(temp.path().join("daemon/state.json")).expect("state"))
            .expect("state json");
    assert!(state["active_profile"].is_null());
    assert_eq!(state["status"], "ready");
    assert_eq!(state["last_run_id"], "R-CAPTURE-001");
}
