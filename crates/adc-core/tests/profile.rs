use adc_core::profile::{parse_profile, RuleType};

const PI5_PROFILE: &str = r#"
profile: pi5_basic
sampling:
  interval_ms: 1000
always_on:
  collectors:
    - cpu
    - memory
    - network
    - kmsg
    - thermal
    - rp1_pcie_snapshot
budgets:
  max_daemon_cpu_percent: 3
  max_memory_mb: 128
  max_artifact_mb_per_run: 512
triggers:
  - name: cpu_sustained_high
    type: threshold_duration
    signal: cpu.total_percent
    op: ">"
    value: 85
    duration_sec: 5
    capture_profile: perf_short
"#;

#[test]
fn parses_pi5_profile_with_collectors_budgets_and_triggers() {
    let profile = parse_profile(PI5_PROFILE).expect("profile parses");

    assert_eq!(profile.id, "pi5_basic");
    assert_eq!(profile.sampling.interval_ms, 1000);
    assert_eq!(
        profile.always_on.collectors,
        vec![
            "cpu".to_string(),
            "memory".to_string(),
            "network".to_string(),
            "kmsg".to_string(),
            "thermal".to_string(),
            "rp1_pcie_snapshot".to_string()
        ]
    );
    assert_eq!(profile.budgets.max_daemon_cpu_percent, 3);
    assert_eq!(profile.budgets.max_memory_mb, 128);
    assert_eq!(profile.budgets.max_artifact_mb_per_run, 512);
    assert_eq!(profile.triggers[0].name, "cpu_sustained_high");
    assert_eq!(profile.triggers[0].rule_type, RuleType::ThresholdDuration);
    assert_eq!(
        profile.triggers[0].capture_profile.as_deref(),
        Some("perf_short")
    );
}

#[test]
fn rejects_unknown_profile_fields() {
    let err = parse_profile(
        r#"
profile: invalid
sampling:
  interval_ms: 1000
always_on:
  collectors: [cpu]
budgets:
  max_daemon_cpu_percent: 3
  max_memory_mb: 128
  max_artifact_mb_per_run: 512
workload_executor:
  command: ./scripts/repro.sh
"#,
    )
    .expect_err("unknown fields must not be accepted");

    assert!(err.to_string().contains("workload_executor"));
}

#[test]
fn rejects_profile_with_no_always_on_collectors() {
    let err = parse_profile(
        r#"
profile: invalid
sampling:
  interval_ms: 1000
always_on:
  collectors: []
budgets:
  max_daemon_cpu_percent: 3
  max_memory_mb: 128
  max_artifact_mb_per_run: 512
"#,
    )
    .expect_err("empty collectors must be rejected");

    assert!(err.to_string().contains("at least one collector"));
}

#[test]
fn rejects_profile_with_zero_artifact_budget() {
    let err = parse_profile(
        r#"
profile: invalid
sampling:
  interval_ms: 1000
always_on:
  collectors: [cpu]
budgets:
  max_daemon_cpu_percent: 3
  max_memory_mb: 128
  max_artifact_mb_per_run: 0
"#,
    )
    .expect_err("zero artifact budget must be rejected");

    assert!(err.to_string().contains("max_artifact_mb_per_run"));
}
