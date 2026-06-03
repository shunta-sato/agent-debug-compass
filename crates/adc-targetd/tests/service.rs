use std::{fs, path::Path, process::Command};

#[test]
fn service_once_writes_state_and_recovers_existing_runs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_dir = temp.path().join("runs/R-OLD");
    fs::create_dir_all(&run_dir).expect("run dir");
    fs::write(run_dir.join("manifest.json"), "{}").expect("manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_adc-targetd"))
        .arg("--service-once")
        .env("ADC_HOME", temp.path())
        .output()
        .expect("run service once");

    assert!(
        output.status.success(),
        "service-once failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let state_path = temp.path().join("daemon/state.json");
    assert!(state_path.is_file());
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(state_path).expect("state")).expect("state json");
    assert_eq!(state["service"], "adc-targetd");
    assert_eq!(state["status"], "ready");
    assert_eq!(state["recovered_runs"][0], "R-OLD");
}

#[test]
fn service_for_ms_captures_kmsg_fixture_trigger_for_armed_profile() {
    let temp = tempfile::tempdir().expect("tempdir");
    let profile_dir = temp.path().join("profiles");
    fs::create_dir_all(&profile_dir).expect("profile dir");
    fs::write(
        profile_dir.join("e2e_kmsg.yaml"),
        r#"
profile: e2e_kmsg
sampling:
  interval_ms: 10
always_on:
  collectors: [kmsg]
budgets:
  max_daemon_cpu_percent: 3
  max_memory_mb: 128
  max_artifact_mb_per_run: 16
triggers:
  - name: kmsg_warning_pattern
    type: kmsg_pattern
    severity_at_least: warning
    patterns: [warning, timeout]
"#,
    )
    .expect("profile");
    let kmsg_fixture = temp.path().join("kmsg.log");
    fs::write(&kmsg_fixture, "warning: synthetic timeout observed\n").expect("kmsg fixture");

    adc_core::arm_profile(temp.path(), "e2e_kmsg").expect("arm profile");

    let output = Command::new(env!("CARGO_BIN_EXE_adc-targetd"))
        .args(["--service-for-ms", "80"])
        .env("ADC_HOME", temp.path())
        .env("ADC_PROFILE_DIR", &profile_dir)
        .env("ADC_KMSG_FIXTURE", &kmsg_fixture)
        .output()
        .expect("run bounded service");

    assert!(
        output.status.success(),
        "service-for-ms failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("service summary json");
    let run_id = summary["captured_runs"][0]
        .as_str()
        .expect("captured run id");
    let run_dir = temp.path().join("runs").join(run_id);
    assert_v2_top_level_layout(&run_dir);
    let evidence = fs::read_to_string(run_dir.join("evidence_index.yaml")).expect("evidence");
    let timeline = fs::read_to_string(run_dir.join("timeline.jsonl")).expect("timeline");
    assert!(evidence.contains("kmsg_warning_pattern"));
    assert!(evidence.contains("raw_refs:"));
    assert!(timeline.contains(r#""source":"kmsg""#));
    assert!(timeline.contains("synthetic timeout"));
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
