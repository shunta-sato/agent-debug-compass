use std::{fs, process::Command};

#[test]
fn baseline_writes_bounded_summary_and_events() {
    let temp = tempfile::tempdir().expect("tempdir");
    let events_path = temp.path().join("events.jsonl");

    let output = Command::new(env!("CARGO_BIN_EXE_adc-demo-sensor-gateway"))
        .args([
            "baseline",
            "--duration-ms",
            "1",
            "--events-jsonl",
            events_path.to_str().expect("events path"),
        ])
        .output()
        .expect("run baseline demo");

    assert!(
        output.status.success(),
        "baseline failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).expect("summary json");
    assert_eq!(summary["scenario"], "baseline");
    assert_eq!(summary["status"], "completed");
    assert_eq!(summary["warning_count"], 0);

    let events = fs::read_to_string(events_path).expect("events");
    assert!(events.contains("\"event_type\":\"startup\""));
    assert!(events.contains("\"scenario\":\"baseline\""));
}

#[test]
fn retry_storm_generates_warning_fixture_and_retry_events() {
    let temp = tempfile::tempdir().expect("tempdir");
    let events_path = temp.path().join("events.jsonl");
    let kmsg_path = temp.path().join("kmsg.log");

    let output = Command::new(env!("CARGO_BIN_EXE_adc-demo-sensor-gateway"))
        .args([
            "retry-storm",
            "--duration-ms",
            "1",
            "--packet-attempts",
            "3",
            "--events-jsonl",
            events_path.to_str().expect("events path"),
            "--kmsg-fixture",
            kmsg_path.to_str().expect("kmsg path"),
        ])
        .output()
        .expect("run retry demo");

    assert!(
        output.status.success(),
        "retry-storm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).expect("summary json");
    assert_eq!(summary["scenario"], "retry-storm");
    assert_eq!(summary["packet_attempts"], 3);
    assert!(summary["warning_count"].as_u64().expect("warning count") >= 1);

    let stderr = String::from_utf8(output.stderr).expect("stderr utf8");
    assert!(stderr.contains("warning: demo retry storm"));
    let kmsg = fs::read_to_string(kmsg_path).expect("kmsg");
    assert!(kmsg.contains("warning: demo retry storm"));
    let events = fs::read_to_string(events_path).expect("events");
    assert!(events.contains("\"event_type\":\"retry_attempt\""));
}

#[test]
fn memory_leak_reports_bounded_retained_bytes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let events_path = temp.path().join("events.jsonl");

    let output = Command::new(env!("CARGO_BIN_EXE_adc-demo-sensor-gateway"))
        .args([
            "memory-leak",
            "--duration-ms",
            "1",
            "--retained-kb",
            "64",
            "--events-jsonl",
            events_path.to_str().expect("events path"),
        ])
        .output()
        .expect("run memory demo");

    assert!(
        output.status.success(),
        "memory-leak failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).expect("summary json");
    assert_eq!(summary["scenario"], "memory-leak");
    assert_eq!(summary["retained_bytes"], 64 * 1024);

    let events = fs::read_to_string(events_path).expect("events");
    assert!(events.contains("\"event_type\":\"buffer_retained\""));
}

#[test]
fn invalid_zero_duration_fails_without_panic() {
    let output = Command::new(env!("CARGO_BIN_EXE_adc-demo-sensor-gateway"))
        .args(["baseline", "--duration-ms", "0"])
        .output()
        .expect("run invalid demo");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr utf8");
    assert!(stderr.contains("duration must be greater than zero"));
    assert!(!stderr.contains("panicked"));
}
