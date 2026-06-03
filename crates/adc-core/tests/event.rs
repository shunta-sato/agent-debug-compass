use adc_core::event::{ClockSource, DataQuality, EventEnvelope, TimeRangeNs};
use serde_json::json;

#[test]
fn event_envelope_round_trips_required_fields_and_data_quality() {
    let event = EventEnvelope {
        run_id: "R001".to_string(),
        source: "cpu".to_string(),
        event_type: "metric_sample".to_string(),
        time_mono_ns: 123_456_789,
        time_range_ns: TimeRangeNs {
            start: 123_456_000,
            end: 123_456_999,
        },
        clock_source: ClockSource::Monotonic,
        collector_id: "cpu/procfs".to_string(),
        profile_id: "pi5_basic".to_string(),
        payload: json!({
            "total_percent": 91.5,
            "core_count": 4
        }),
        data_quality: DataQuality {
            dropped: false,
            drop_count: 0,
            throttled: false,
            missing: vec![],
            truncated: false,
            clock_confidence: "high".to_string(),
            notes: vec!["sampled from /proc/stat".to_string()],
        },
    };

    let encoded = serde_json::to_string(&event).expect("serialize event");
    let decoded: EventEnvelope = serde_json::from_str(&encoded).expect("deserialize event");

    assert_eq!(decoded.run_id, "R001");
    assert_eq!(decoded.source, "cpu");
    assert_eq!(decoded.event_type, "metric_sample");
    assert_eq!(decoded.time_mono_ns, 123_456_789);
    assert_eq!(decoded.time_range_ns.start, 123_456_000);
    assert_eq!(decoded.clock_source, ClockSource::Monotonic);
    assert_eq!(decoded.payload["total_percent"], json!(91.5));
    assert_eq!(decoded.data_quality.clock_confidence, "high");
    assert_eq!(decoded.data_quality.notes[0], "sampled from /proc/stat");
}

#[test]
fn data_quality_default_is_explicitly_not_dropped_or_throttled() {
    let quality = DataQuality::default();

    assert!(!quality.dropped);
    assert_eq!(quality.drop_count, 0);
    assert!(!quality.throttled);
    assert!(!quality.truncated);
    assert_eq!(quality.clock_confidence, "unknown");
}
