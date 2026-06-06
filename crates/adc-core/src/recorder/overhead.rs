use super::{
    model::{
        RecorderBufferStatus, RecorderOverhead, RecorderOverheadAccounting, RecorderOverheadScope,
    },
    quality::data_quality_for_drop_count,
};
use crate::DataQuality;

pub fn recorder_overhead_for_service_run(
    target_id: impl Into<String>,
    buffer_status: &RecorderBufferStatus,
    accounting: RecorderOverheadAccounting,
) -> RecorderOverhead {
    let target_id = target_id.into();
    let dropped = buffer_status.data_quality.drop_count;
    RecorderOverhead {
        schema_version: "obs.recorder_overhead.v1".to_string(),
        target_id,
        overhead_scope: accounting.overhead_scope,
        since_mono_ns: accounting.since_mono_ns,
        through_mono_ns: accounting.through_mono_ns,
        cpu_percent: None,
        memory_bytes: None,
        disk_write_bytes: accounting.disk_write_bytes,
        artifact_bytes: accounting.artifact_bytes,
        status_write_bytes: accounting.status_write_bytes,
        frozen_artifact_bytes: accounting.frozen_artifact_bytes,
        samples_jsonl_bytes: accounting.samples_jsonl_bytes,
        incident_count: accounting.incident_count,
        estimated_memory_ring_bytes: estimated_recorder_memory_bytes(buffer_status),
        wakeup_rate_hz: None,
        self_samples_dropped: dropped,
        data_quality: recorder_overhead_data_quality(dropped),
    }
}

pub(super) fn default_recorder_overhead(
    target_id: &str,
    buffer_status: &RecorderBufferStatus,
    self_samples_dropped: u64,
) -> RecorderOverhead {
    RecorderOverhead {
        schema_version: "obs.recorder_overhead.v1".to_string(),
        target_id: target_id.to_string(),
        overhead_scope: RecorderOverheadScope::CurrentStatusSnapshot,
        since_mono_ns: buffer_status.current_retained_range_mono_ns.start,
        through_mono_ns: buffer_status.current_retained_range_mono_ns.end,
        cpu_percent: None,
        memory_bytes: None,
        disk_write_bytes: 0,
        artifact_bytes: 0,
        status_write_bytes: 0,
        frozen_artifact_bytes: 0,
        samples_jsonl_bytes: 0,
        incident_count: 0,
        estimated_memory_ring_bytes: estimated_recorder_memory_bytes(buffer_status),
        wakeup_rate_hz: None,
        self_samples_dropped,
        data_quality: recorder_overhead_data_quality(self_samples_dropped),
    }
}

fn estimated_recorder_memory_bytes(buffer_status: &RecorderBufferStatus) -> u64 {
    const ESTIMATED_SAMPLE_BYTES: u64 = 256;
    let retained_samples = buffer_status
        .signals
        .iter()
        .map(|signal| signal.recorded_samples)
        .sum::<u64>();
    retained_samples.saturating_mul(ESTIMATED_SAMPLE_BYTES)
}

fn recorder_overhead_data_quality(drop_count: u64) -> DataQuality {
    let mut data_quality = data_quality_for_drop_count(drop_count);
    data_quality
        .missing
        .push("recorder CPU and memory overhead are not measured in this MVP".to_string());
    data_quality.notes.push(
        "disk_write_bytes and status_write_bytes are cumulative for the service-run scope"
            .to_string(),
    );
    data_quality.notes.push(
        "artifact_bytes is a write-path retained-size estimate and may overcount overwritten marker/status artifacts in this MVP"
            .to_string(),
    );
    data_quality
        .notes
        .push("status_write_bytes excludes the current status artifact write".to_string());
    data_quality
        .notes
        .push("estimated_memory_ring_bytes uses a fixed per-sample estimate".to_string());
    data_quality
}
