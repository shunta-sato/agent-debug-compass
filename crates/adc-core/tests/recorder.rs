use adc_core::{
    default_recorder_budget, recorder_status_for, RecorderRing, RecorderSample,
    RecorderSignalSample,
};

#[test]
fn recorder_ring_drops_oldest_and_reports_loss_semantics() {
    let mut ring = RecorderRing::new("local", 2, 1_000);

    ring.push(sample(1_000, "cpu.summary", 10.0));
    ring.push(sample(2_000, "cpu.summary", 20.0));
    ring.push(sample(3_000, "cpu.summary", 30.0));

    let status = ring.status();
    assert_eq!(status.schema_version, "obs.recorder_buffer_status.v1");
    assert_eq!(status.storage_mode, "memory_ring");
    assert!(status.volatile);
    assert!(!status.survives_daemon_restart);
    assert_eq!(status.signals[0].recorded_samples, 2);
    assert_eq!(status.signals[0].dropped_samples, 1);
    assert!(status.data_quality.dropped);
}

#[test]
fn recorder_status_exposes_budget_overhead_and_volatility() {
    let mut ring = RecorderRing::new("local", 4, 60_000);
    ring.push(sample(1_000, "adc.self_overhead", 1.0));

    let status = recorder_status_for(
        "local",
        Some("camera_inference_degradation"),
        "armed",
        "recording",
        ring.status(),
        default_recorder_budget(),
    );

    assert_eq!(status.schema_version, "obs.recorder_status.v1");
    assert_eq!(status.recorder_state, adc_core::RecorderState::Recording);
    assert!(status.armed);
    assert!(status.storage.volatile);
    assert_eq!(status.budget.schema_version, "obs.recorder_budget.v1");
    assert_eq!(status.overhead.schema_version, "obs.recorder_overhead.v1");
}

fn sample(time_mono_ns: u64, signal_id: &str, value: f64) -> RecorderSample {
    RecorderSample {
        time_mono_ns,
        signals: vec![RecorderSignalSample {
            signal_id: signal_id.to_string(),
            value,
        }],
    }
}
