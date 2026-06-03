use std::{fs, path::Path, process::Command};

#[test]
fn snapshot_command_creates_v2_evidence_bundle() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-SMOKE-001";

    let snapshot = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["snapshot", "--run-id", run_id])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run snapshot");

    assert!(
        snapshot.status.success(),
        "snapshot failed: {}",
        String::from_utf8_lossy(&snapshot.stderr)
    );

    let run_dir = temp.path().join("runs").join(run_id);
    assert_v2_top_level_layout(&run_dir);
    assert!(run_dir.join("manifest.json").is_file());
    assert!(run_dir.join("timeline.jsonl").is_file());
    assert!(run_dir.join("evidence_index.yaml").is_file());
    assert!(run_dir.join("overhead_report.json").is_file());
    assert!(run_dir.join("windows/W001.yaml").is_file());
    assert!(run_dir.join("raw/system.json").is_file());
    assert!(run_dir.join("raw/cpu.json").is_file());
    assert!(run_dir.join("raw/memory.json").is_file());
    assert!(run_dir.join("raw/network.json").is_file());
    assert!(run_dir.join("raw/capability.json").is_file());

    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(run_dir.join("manifest.json")).expect("manifest"))
            .expect("manifest json");
    assert_eq!(manifest["run_id"], run_id);
    assert!(manifest["artifacts"].as_array().expect("artifacts").len() >= 7);
    let timeline = fs::read_to_string(run_dir.join("timeline.jsonl")).expect("timeline");
    assert!(timeline.contains(r#""source":"cpu""#));
    assert!(timeline.contains(r#""source":"memory""#));
    assert!(timeline.contains(r#""source":"network""#));
    assert!(timeline.contains(r#""source":"capability""#));

    let evidence = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["evidence", "get", "--run-id", run_id])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run evidence get");

    assert!(
        evidence.status.success(),
        "evidence get failed: {}",
        String::from_utf8_lossy(&evidence.stderr)
    );
    let evidence_stdout = String::from_utf8(evidence.stdout).expect("evidence is utf8");
    assert!(evidence_stdout.contains("run_id: R-SMOKE-001"));
    assert!(evidence_stdout.contains("capture_mode: snapshot"));
    assert!(evidence_stdout.contains("raw_refs:"));
    assert!(evidence_stdout.contains("overhead"));
    assert!(evidence_stdout.contains("cpu:"));
    assert!(evidence_stdout.contains("memory:"));
    assert!(evidence_stdout.contains("network:"));
    assert!(evidence_stdout.contains("capability:"));

    let window = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "evidence",
            "window",
            "--run-id",
            run_id,
            "--window-id",
            "W001",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run evidence window");
    assert!(
        window.status.success(),
        "evidence window failed: {}",
        String::from_utf8_lossy(&window.stderr)
    );
    let window_stdout = String::from_utf8(window.stdout).expect("window is utf8");
    assert!(window_stdout.contains("window_id: W001"));
    assert!(window_stdout.contains("trigger_reason: manual_snapshot"));

    let list_runs = Command::new(env!("CARGO_BIN_EXE_adc"))
        .arg("list-runs")
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run list-runs");
    assert!(
        list_runs.status.success(),
        "list-runs failed: {}",
        String::from_utf8_lossy(&list_runs.stderr)
    );
    let runs: serde_json::Value =
        serde_json::from_slice(&list_runs.stdout).expect("list-runs json");
    assert_eq!(runs["runs"][0]["run_id"], run_id);

    let bundle = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["bundle", "--run-id", run_id])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run bundle");
    assert!(
        bundle.status.success(),
        "bundle failed: {}",
        String::from_utf8_lossy(&bundle.stderr)
    );
    let bundle_json: serde_json::Value =
        serde_json::from_slice(&bundle.stdout).expect("bundle json");
    assert_eq!(bundle_json["run_id"], run_id);
    assert!(bundle_json["manifest"]
        .as_str()
        .expect("manifest path")
        .ends_with("manifest.json"));

    let search = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "evidence", "series", "--run-id", run_id, "--source", "cpu", "--limit", "1",
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run evidence series");
    assert!(
        search.status.success(),
        "evidence series failed: {}",
        String::from_utf8_lossy(&search.stderr)
    );
    let search_json: serde_json::Value =
        serde_json::from_slice(&search.stdout).expect("search json");
    assert_eq!(search_json["source"], "cpu");
    assert!(
        search_json["returned_count"]
            .as_u64()
            .expect("returned_count")
            >= 1
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
fn snapshot_rejects_nested_run_id_path_segments() {
    let temp = tempfile::tempdir().expect("tempdir");

    let snapshot = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args(["snapshot", "--run-id", "nested/run"])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run snapshot");

    assert!(!snapshot.status.success());
    let stderr = String::from_utf8_lossy(&snapshot.stderr);
    assert!(stderr.contains("run_id must be a single relative path segment"));
}

#[test]
fn target_snapshot_records_non_local_target_identity_for_remote_transport() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-REMOTE-TARGET-SNAPSHOT";

    let snapshot = Command::new(env!("CARGO_BIN_EXE_adc"))
        .args([
            "target",
            "snapshot",
            "--target",
            "pi5-remote-a",
            "--run-id",
            run_id,
        ])
        .env("ADC_HOME", temp.path())
        .output()
        .expect("target snapshot");

    assert!(
        snapshot.status.success(),
        "target snapshot failed: {}",
        String::from_utf8_lossy(&snapshot.stderr)
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
