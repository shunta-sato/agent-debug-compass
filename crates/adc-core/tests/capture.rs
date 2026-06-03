use std::{fs, path::Path, time::Duration};

use adc_core::{capture_for, CaptureOptions};

#[test]
fn bounded_capture_writes_multi_sample_timeline_window_evidence_and_raw_refs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-CAPTURE-CORE";

    let bundle = capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(140),
            interval: Duration::from_millis(40),
            collectors: vec![
                "cpu".to_string(),
                "memory".to_string(),
                "network".to_string(),
            ],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");

    assert_eq!(bundle.run_id, run_id);
    assert!(
        bundle.sample_count >= 2,
        "expected multiple samples, got {}",
        bundle.sample_count
    );

    let run_dir = temp.path().join("runs").join(run_id);
    assert_v2_top_level_layout(&run_dir);
    assert!(run_dir.join("manifest.json").is_file());
    assert!(run_dir.join("evidence_index.yaml").is_file());
    assert!(run_dir.join("windows/W001.yaml").is_file());
    assert!(run_dir.join("timeline.jsonl").is_file());
    assert!(run_dir.join("raw/samples.jsonl").is_file());
    assert!(run_dir.join("raw/cpu.jsonl").is_file());
    assert!(run_dir.join("raw/memory.jsonl").is_file());
    assert!(run_dir.join("raw/network.jsonl").is_file());
    assert!(run_dir.join("overhead_report.json").is_file());

    let timeline = fs::read_to_string(run_dir.join("timeline.jsonl")).expect("timeline");
    let events = timeline
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("event json"))
        .collect::<Vec<_>>();
    assert!(
        events.len() >= bundle.sample_count * 3,
        "expected cpu/memory/network event per sample"
    );
    assert!(events.iter().any(|event| event["source"] == "cpu"));
    assert!(events.iter().any(|event| event["source"] == "memory"));
    assert!(events.iter().any(|event| event["source"] == "network"));
    assert!(events.iter().all(|event| event["event_type"] == "sample"));

    let first = events
        .first()
        .and_then(|event| event["time_mono_ns"].as_u64())
        .expect("first event time");
    let last = events
        .last()
        .and_then(|event| event["time_mono_ns"].as_u64())
        .expect("last event time");
    assert!(last > first, "capture timeline should span time");

    let raw_samples = fs::read_to_string(run_dir.join("raw/samples.jsonl")).expect("samples");
    assert_eq!(raw_samples.lines().count(), bundle.sample_count);

    let evidence = fs::read_to_string(run_dir.join("evidence_index.yaml")).expect("evidence");
    assert!(evidence.contains("capture_mode: capture"));
    assert!(evidence.contains("raw/samples.jsonl"));
    assert!(evidence.contains("raw/cpu.jsonl"));

    let window = fs::read_to_string(run_dir.join("windows/W001.yaml")).expect("window");
    assert!(window.contains("trigger_reason: manual_capture"));
    assert!(window.contains("event_count:"));

    let overhead: serde_json::Value =
        serde_json::from_slice(&fs::read(run_dir.join("overhead_report.json")).expect("overhead"))
            .expect("overhead json");
    assert!(
        overhead["sample"]["duration_ms"]
            .as_u64()
            .expect("duration")
            > 0,
        "capture duration should be reported"
    );
    assert_eq!(
        overhead["decision"]["throttled"], false,
        "scheduler jitter inside one interval should not mark capture throttled"
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
