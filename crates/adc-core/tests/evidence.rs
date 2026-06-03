use std::{fs, time::Duration};

use adc_core::{
    capture_for, create_snapshot, read_evidence_index, read_raw_slice, signal_series_for,
    validate_cause_neutral, CaptureOptions,
};

#[test]
fn snapshot_writes_cause_neutral_evidence_index_with_verifiable_refs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-EVIDENCE-SNAPSHOT";

    create_snapshot(temp.path(), run_id).expect("snapshot");

    let run_dir = temp.path().join("runs").join(run_id);
    assert!(run_dir.join("evidence_index.yaml").is_file());
    let evidence = read_evidence_index(temp.path(), run_id).expect("evidence index");

    assert_eq!(evidence.schema_version, "obs.v2");
    assert_eq!(evidence.run_id, run_id);
    assert_eq!(evidence.target_id, "local");
    assert_eq!(evidence.capture_mode, "snapshot");
    assert_eq!(evidence.clock_basis, "CLOCK_MONOTONIC");
    assert!(!evidence.observed_facts.is_empty());
    assert!(evidence.raw_refs.contains_key("cpu"));
    assert!(evidence.raw_refs.contains_key("manifest"));
    validate_cause_neutral(&evidence).expect("cause-neutral evidence");

    let evidence_yaml = fs::read_to_string(run_dir.join("evidence_index.yaml")).expect("yaml");
    assert!(evidence_yaml.contains("observed_facts:"));
    assert!(evidence_yaml.contains("salience_map:"));
    assert!(evidence_yaml.contains("information_debt:"));
    assert!(evidence_yaml.contains("next_probe_options:"));
}

#[test]
fn capture_evidence_records_salience_information_debt_and_next_probes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-EVIDENCE-CAPTURE";

    capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
            collectors: vec![
                "cpu".to_string(),
                "memory".to_string(),
                "network".to_string(),
                "perf".to_string(),
            ],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");

    let evidence = read_evidence_index(temp.path(), run_id).expect("evidence index");
    assert_eq!(evidence.capture_mode, "capture");
    assert!(evidence
        .observed_facts
        .iter()
        .any(|fact| fact.source == "cpu" && fact.raw_ref == "artifact://raw/cpu.jsonl"));
    assert!(evidence
        .salience_map
        .iter()
        .any(|signal| signal.source == "cpu" && signal.calculation.contains("event_count")));
    assert!(evidence.information_debt.iter().any(|debt| {
        debt.kind == "missing"
            && debt
                .description
                .contains("collector perf is not implemented")
    }));
    assert!(evidence
        .next_probe_options
        .iter()
        .any(|probe| probe.probe_id == "enable_privileged_perf_short"));
    validate_cause_neutral(&evidence).expect("cause-neutral capture evidence");
}

#[test]
fn raw_slice_is_bounded_and_rejects_non_raw_refs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-EVIDENCE-RAW-SLICE";
    capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
            collectors: vec!["cpu".to_string()],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");

    let slice =
        read_raw_slice(temp.path(), run_id, "artifact://raw/samples.jsonl", 2).expect("raw slice");
    assert_eq!(slice.run_id, run_id);
    assert_eq!(slice.raw_ref, "artifact://raw/samples.jsonl");
    assert_eq!(slice.returned_lines, 2);
    assert!(slice.truncated);

    let err = read_raw_slice(temp.path(), run_id, "artifact://../manifest.json", 2)
        .expect_err("reject traversal");
    assert!(err.to_string().contains("raw_ref"));
}

#[test]
fn signal_series_filters_timeline_by_source() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-EVIDENCE-SERIES";
    capture_for(
        temp.path(),
        CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: "capture_test".to_string(),
            duration: Duration::from_millis(120),
            interval: Duration::from_millis(40),
            collectors: vec!["cpu".to_string(), "memory".to_string()],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
    )
    .expect("capture");

    let series = signal_series_for(temp.path(), run_id, "cpu", 10).expect("signal series");
    assert_eq!(series.run_id, run_id);
    assert_eq!(series.source, "cpu");
    assert!(series.returned_count >= 2);
    assert!(series.events.iter().all(|event| event.source == "cpu"));
}
