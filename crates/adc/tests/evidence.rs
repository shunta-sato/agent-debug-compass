use std::process::Command;

#[test]
fn evidence_commands_read_index_series_raw_slice_and_next_probe() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-CLI-EVIDENCE";

    let capture = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "target",
            "capture",
            "--target",
            "local",
            "--run-id",
            run_id,
            "--duration-ms",
            "120",
            "--interval-ms",
            "40",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("target capture");
    assert!(
        capture.status.success(),
        "target capture failed: {}",
        String::from_utf8_lossy(&capture.stderr)
    );
    let capture_json: serde_json::Value =
        serde_json::from_slice(&capture.stdout).expect("capture json");
    assert_eq!(capture_json["target_id"], "local");
    assert!(capture_json["evidence_index"]
        .as_str()
        .expect("evidence path")
        .contains("evidence_index.yaml"));

    let evidence = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["evidence", "get", "--run-id", run_id])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("evidence get");
    assert!(
        evidence.status.success(),
        "evidence get failed: {}",
        String::from_utf8_lossy(&evidence.stderr)
    );
    let evidence_stdout = String::from_utf8(evidence.stdout).expect("evidence utf8");
    assert!(evidence_stdout.contains("schema_version: obs.v2"));
    assert!(evidence_stdout.contains("observed_facts:"));

    let series = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "evidence", "series", "--run-id", run_id, "--source", "cpu", "--limit", "2",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("evidence series");
    assert!(
        series.status.success(),
        "evidence series failed: {}",
        String::from_utf8_lossy(&series.stderr)
    );
    let series_json: serde_json::Value =
        serde_json::from_slice(&series.stdout).expect("series json");
    assert_eq!(series_json["source"], "cpu");
    assert!(series_json["returned_count"].as_u64().expect("returned") >= 1);

    let raw_slice = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "evidence",
            "raw-slice",
            "--run-id",
            run_id,
            "--ref",
            "artifact://raw/samples.jsonl",
            "--limit",
            "1",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("raw slice");
    assert!(
        raw_slice.status.success(),
        "raw slice failed: {}",
        String::from_utf8_lossy(&raw_slice.stderr)
    );
    let raw_json: serde_json::Value =
        serde_json::from_slice(&raw_slice.stdout).expect("raw slice json");
    assert_eq!(raw_json["returned_lines"], 1);
    assert_eq!(raw_json["truncated"], true);

    let window_ref = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "evidence",
            "ref",
            "--run-id",
            run_id,
            "--ref",
            "artifact://windows/W001.yaml",
            "--limit",
            "20",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("window ref");
    assert!(
        window_ref.status.success(),
        "window ref failed: {}",
        String::from_utf8_lossy(&window_ref.stderr)
    );
    let window_ref_json: serde_json::Value =
        serde_json::from_slice(&window_ref.stdout).expect("window ref json");
    assert_eq!(window_ref_json["ref_kind"], "window");
    assert!(window_ref_json["text"]
        .as_str()
        .expect("window text")
        .contains("window_id: W001"));

    let next_probe = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["next-probe", "--run-id", run_id])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("next probe");
    assert!(
        next_probe.status.success(),
        "next probe failed: {}",
        String::from_utf8_lossy(&next_probe.stderr)
    );
    let next_json: serde_json::Value =
        serde_json::from_slice(&next_probe.stdout).expect("next probe json");
    assert!(next_json["next_probe_options"]
        .as_array()
        .expect("options")
        .iter()
        .any(|option| option["probe_id"] == "capture_process_snapshot"));
}
