use adc_core::{compare_runs, validate_cause_neutral};

#[test]
fn compare_runs_reports_bounded_metric_deltas_and_refs() {
    let temp = tempfile::tempdir().expect("tempdir");
    adc_core::create_snapshot(temp.path(), "R-BEFORE").expect("before snapshot");
    adc_core::create_snapshot(temp.path(), "R-AFTER").expect("after snapshot");

    let comparison = compare_runs(temp.path(), "R-BEFORE", "R-AFTER").expect("compare");

    assert_eq!(comparison.before_run_id, "R-BEFORE");
    assert_eq!(comparison.after_run_id, "R-AFTER");
    assert!(comparison
        .metric_deltas
        .contains_key("memory.mem_available_kb"));
    assert_eq!(
        comparison.raw_refs["before_manifest"],
        "artifact://runs/R-BEFORE/manifest.json"
    );
    assert_eq!(
        comparison.raw_refs["after_manifest"],
        "artifact://runs/R-AFTER/manifest.json"
    );
    assert_eq!(comparison.evidence_index.schema_version, "obs.v2");
    assert_eq!(comparison.evidence_index.capture_mode, "compare");
    assert!(comparison
        .evidence_index
        .observed_facts
        .iter()
        .any(|fact| fact.source == "compare"));
    validate_cause_neutral(&comparison.evidence_index).expect("cause-neutral compare evidence");
}
