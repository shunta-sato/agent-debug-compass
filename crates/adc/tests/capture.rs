use std::{fs, path::Path, process::Command};

#[test]
fn capture_command_creates_bounded_run_and_v2_evidence_tools_can_analyze_it() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-CLI-CAPTURE";

    let capture = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "capture",
            "--run-id",
            run_id,
            "--duration-ms",
            "140",
            "--interval-ms",
            "40",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run capture");

    assert!(
        capture.status.success(),
        "capture failed: {}",
        String::from_utf8_lossy(&capture.stderr)
    );
    let capture_json: serde_json::Value =
        serde_json::from_slice(&capture.stdout).expect("capture response json");
    assert_eq!(capture_json["run_id"], run_id);
    assert!(capture_json["sample_count"].as_u64().expect("sample_count") >= 2);

    let run_dir = temp.path().join("runs").join(run_id);
    assert_v2_top_level_layout(&run_dir);
    assert!(run_dir.join("manifest.json").is_file());
    assert!(run_dir.join("evidence_index.yaml").is_file());
    assert!(run_dir.join("raw/samples.jsonl").is_file());
    assert!(run_dir.join("raw/cpu.jsonl").is_file());
    assert!(run_dir.join("timeline.jsonl").is_file());
    assert!(run_dir.join("windows/W001.yaml").is_file());

    let evidence = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["evidence", "get", "--run-id", run_id])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("get evidence");
    assert!(
        evidence.status.success(),
        "evidence get failed: {}",
        String::from_utf8_lossy(&evidence.stderr)
    );
    let evidence_stdout = String::from_utf8(evidence.stdout).expect("evidence utf8");
    assert!(evidence_stdout.contains("capture_mode: capture"));
    assert!(evidence_stdout.contains("raw/samples.jsonl"));

    let search = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "evidence", "series", "--run-id", run_id, "--source", "cpu", "--limit", "5",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("search events");
    assert!(
        search.status.success(),
        "evidence series failed: {}",
        String::from_utf8_lossy(&search.stderr)
    );
    let search_json: serde_json::Value =
        serde_json::from_slice(&search.stdout).expect("search response json");
    assert_eq!(search_json["source"], "cpu");
    assert!(
        search_json["returned_count"]
            .as_u64()
            .expect("returned_count")
            >= 2
    );

    let raw_samples = fs::read_to_string(run_dir.join("raw/samples.jsonl")).expect("samples");
    assert!(
        raw_samples.lines().count() >= 2,
        "capture should persist multiple raw samples"
    );
}

fn assert_v2_top_level_layout(run_dir: &Path) {
    let mut entries = fs::read_dir(run_dir)
        .expect("run dir")
        .map(|entry| {
            entry
                .expect("run dir entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    entries.sort();
    assert_eq!(
        entries,
        [
            "evidence_index.yaml",
            "manifest.json",
            "overhead_report.json",
            "raw",
            "timeline.jsonl",
            "windows",
        ]
    );
}

#[test]
fn capture_command_rejects_zero_or_ambiguous_duration() {
    let temp = tempfile::tempdir().expect("tempdir");

    let zero_duration = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "capture",
            "--run-id",
            "R-ZERO-CAPTURE",
            "--duration-ms",
            "0",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run zero duration capture");
    assert!(!zero_duration.status.success());
    assert!(String::from_utf8_lossy(&zero_duration.stderr).contains("greater than zero"));

    let ambiguous_duration = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "capture",
            "--run-id",
            "R-AMBIGUOUS-CAPTURE",
            "--duration-ms",
            "100",
            "--duration-sec",
            "1",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run ambiguous duration capture");
    assert!(!ambiguous_duration.status.success());
    assert!(String::from_utf8_lossy(&ambiguous_duration.stderr).contains("use only one"));
}

#[test]
fn target_capture_records_non_local_target_identity_for_remote_transport() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-REMOTE-TARGET-CAPTURE";

    let capture = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "target",
            "capture",
            "--target",
            "pi5-remote-a",
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
    let evidence = fs::read_to_string(
        temp.path()
            .join("runs")
            .join(run_id)
            .join("evidence_index.yaml"),
    )
    .expect("evidence");
    assert!(evidence.contains("target_id: pi5-remote-a"));
}
