use std::fs;

use adc_core::{
    search_events, ClockSource, DataQuality, EventEnvelope, SearchEventsQuery, TimeRangeNs,
};

#[test]
fn search_events_filters_source_and_limits_results() {
    let temp = tempfile::tempdir().expect("tempdir");
    let run_id = "R-TIMELINE-001";
    let run_dir = temp.path().join("runs").join(run_id);
    fs::create_dir_all(&run_dir).expect("run dir");

    let events = [
        event(run_id, "cpu", 1),
        event(run_id, "memory", 2),
        event(run_id, "cpu", 3),
    ];
    let timeline = events
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()
        .expect("serialize events")
        .join("\n");
    fs::write(run_dir.join("timeline.jsonl"), format!("{timeline}\n")).expect("timeline");

    let result = search_events(
        temp.path(),
        run_id,
        &SearchEventsQuery {
            source: Some("cpu".to_string()),
            event_type: None,
            contains: None,
            limit: 1,
        },
    )
    .expect("search");

    assert_eq!(result.events.len(), 1);
    assert_eq!(result.events[0].source, "cpu");
    assert_eq!(result.matched_count, 2);
    assert!(result.truncated);
    assert!(result.data_quality.truncated);
}

fn event(run_id: &str, source: &str, time_mono_ns: u64) -> EventEnvelope {
    EventEnvelope {
        run_id: run_id.to_string(),
        source: source.to_string(),
        event_type: "snapshot".to_string(),
        time_mono_ns,
        time_range_ns: TimeRangeNs {
            start: time_mono_ns,
            end: time_mono_ns,
        },
        clock_source: ClockSource::Monotonic,
        collector_id: format!("{source}/test"),
        profile_id: "test".to_string(),
        payload: serde_json::json!({"value": source}),
        data_quality: DataQuality::default(),
    }
}
