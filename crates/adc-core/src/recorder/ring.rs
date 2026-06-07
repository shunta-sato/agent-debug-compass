use std::collections::{BTreeMap, BTreeSet};

use super::{
    coverage::recorder_expected_signal_for_id,
    model::{
        RecorderBufferStatus, RecorderExpectedSignal, RecorderSample, RecorderSignalStatus,
        RecorderTimeRange,
    },
    quality::data_quality_for_drop_count,
};

pub struct RecorderRing {
    target_id: String,
    capacity: usize,
    retention_ms: u64,
    samples: Vec<RecorderSample>,
    dropped_by_signal: BTreeMap<String, u64>,
    expected_signals: BTreeMap<String, RecorderExpectedSignal>,
    throttled_sample_count: u64,
    throttle_notes: BTreeSet<String>,
}

impl RecorderRing {
    pub fn new(target_id: impl Into<String>, capacity: usize, retention_ms: u64) -> Self {
        Self::with_expected_signals(
            target_id,
            capacity,
            retention_ms,
            std::iter::empty::<String>(),
        )
    }

    pub fn with_expected_signals(
        target_id: impl Into<String>,
        capacity: usize,
        retention_ms: u64,
        expected_signal_ids: impl IntoIterator<Item = String>,
    ) -> Self {
        Self::with_expected_signal_model(
            target_id,
            capacity,
            retention_ms,
            expected_signal_ids
                .into_iter()
                .map(|signal_id| recorder_expected_signal_for_id(&signal_id, 1000)),
        )
    }

    pub fn with_expected_signal_model(
        target_id: impl Into<String>,
        capacity: usize,
        retention_ms: u64,
        expected_signals: impl IntoIterator<Item = RecorderExpectedSignal>,
    ) -> Self {
        Self {
            target_id: target_id.into(),
            capacity: capacity.max(1),
            retention_ms,
            samples: Vec::new(),
            dropped_by_signal: BTreeMap::new(),
            expected_signals: expected_signals
                .into_iter()
                .map(|signal| (signal.signal_id.clone(), signal))
                .collect(),
            throttled_sample_count: 0,
            throttle_notes: BTreeSet::new(),
        }
    }

    pub fn push(&mut self, sample: RecorderSample) {
        while self.samples.len() >= self.capacity {
            self.drop_oldest_sample();
        }
        self.samples.push(sample);
        self.evict_expired_samples();
    }

    pub fn samples(&self) -> &[RecorderSample] {
        &self.samples
    }

    pub fn expected_signals(&self) -> Vec<RecorderExpectedSignal> {
        self.expected_signals.values().cloned().collect()
    }

    pub fn expected_signal(&self, signal_id: &str) -> Option<&RecorderExpectedSignal> {
        self.expected_signals.get(signal_id)
    }

    pub fn record_throttled_sample(&mut self, note: impl Into<String>) {
        self.throttled_sample_count = self.throttled_sample_count.saturating_add(1);
        self.throttle_notes.insert(note.into());
    }

    pub fn status(&self) -> RecorderBufferStatus {
        let mut recorded_by_signal: BTreeMap<String, u64> = BTreeMap::new();
        for sample in &self.samples {
            for signal in &sample.signals {
                *recorded_by_signal
                    .entry(signal.signal_id.clone())
                    .or_default() += 1;
            }
        }
        let signal_ids = recorded_by_signal
            .keys()
            .chain(self.dropped_by_signal.keys())
            .chain(self.expected_signals.keys())
            .cloned()
            .collect::<BTreeSet<_>>();

        let mut signals = Vec::new();
        let mut total_dropped = 0_u64;
        let mut buffer_quality = data_quality_for_drop_count(0);
        for signal_id in signal_ids {
            let recorded = recorded_by_signal.get(&signal_id).copied().unwrap_or(0);
            let dropped = self.dropped_by_signal.get(&signal_id).copied().unwrap_or(0);
            total_dropped = total_dropped.saturating_add(dropped);
            let expected_but_absent =
                self.expected_signals.contains_key(&signal_id) && recorded == 0 && dropped == 0;
            let mut signal_quality = data_quality_for_drop_count(dropped);
            if let Some(expected_signal) = self.expected_signals.get(&signal_id) {
                signal_quality.throttled |= expected_signal.data_quality.throttled;
                signal_quality.truncated |= expected_signal.data_quality.truncated;
                signal_quality.dropped |= expected_signal.data_quality.dropped;
                signal_quality.drop_count = signal_quality
                    .drop_count
                    .saturating_add(expected_signal.data_quality.drop_count);
                signal_quality
                    .missing
                    .extend(expected_signal.data_quality.missing.clone());
                signal_quality
                    .notes
                    .extend(expected_signal.data_quality.notes.clone());
                buffer_quality.throttled |= expected_signal.data_quality.throttled;
                buffer_quality
                    .missing
                    .extend(expected_signal.data_quality.missing.clone());
                buffer_quality
                    .notes
                    .extend(expected_signal.data_quality.notes.clone());
            }
            if expected_but_absent {
                signal_quality.missing.push(format!(
                    "expected recorder signal {signal_id} has no retained samples"
                ));
                buffer_quality.missing.push(format!(
                    "expected recorder signal {signal_id} has no retained samples"
                ));
            }
            let configured_interval_ms = self
                .expected_signals
                .get(&signal_id)
                .map(|signal| signal.configured_interval_ms)
                .unwrap_or(1000);
            signals.push(RecorderSignalStatus {
                signal_id,
                configured_interval_ms,
                expected_samples: Some(recorded.saturating_add(dropped)),
                recorded_samples: recorded,
                dropped_samples: dropped,
                gap_ranges: Vec::new(),
                degraded: dropped > 0 || expected_but_absent,
                data_quality: signal_quality,
            });
        }
        let dropped_quality = data_quality_for_drop_count(total_dropped);
        if dropped_quality.dropped {
            buffer_quality.dropped = true;
            buffer_quality.drop_count = dropped_quality.drop_count;
            buffer_quality.notes.extend(dropped_quality.notes);
        }
        if self.throttled_sample_count > 0 {
            buffer_quality.throttled = true;
            buffer_quality
                .notes
                .extend(self.throttle_notes.iter().cloned());
        }

        RecorderBufferStatus {
            schema_version: "obs.recorder_buffer_status.v1".to_string(),
            target_id: self.target_id.clone(),
            storage_mode: "memory_ring".to_string(),
            volatile: true,
            survives_daemon_restart: false,
            survives_target_reboot: false,
            survives_power_loss: false,
            retention_ms: self.retention_ms,
            current_retained_range_mono_ns: RecorderTimeRange {
                start: self.samples.first().map(|sample| sample.time_mono_ns),
                end: self.samples.last().map(|sample| sample.time_mono_ns),
            },
            signals,
            data_quality: buffer_quality,
        }
    }

    fn drop_oldest_sample(&mut self) {
        if self.samples.is_empty() {
            return;
        }
        let removed = self.samples.remove(0);
        for signal in removed.signals {
            *self.dropped_by_signal.entry(signal.signal_id).or_default() += 1;
        }
    }

    fn evict_expired_samples(&mut self) {
        let Some(newest_time) = self.samples.last().map(|sample| sample.time_mono_ns) else {
            return;
        };
        let retention_ns = self.retention_ms.saturating_mul(1_000_000);
        let cutoff = newest_time.saturating_sub(retention_ns);
        while self
            .samples
            .first()
            .is_some_and(|sample| sample.time_mono_ns < cutoff)
        {
            self.drop_oldest_sample();
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecorderSampleRateGovernor {
    min_interval_ns: u64,
    last_sample_mono_ns: Option<u64>,
}

impl RecorderSampleRateGovernor {
    pub fn new(max_samples_per_second: u64) -> Self {
        let min_interval_ns = if max_samples_per_second == 0 {
            u64::MAX
        } else {
            1_000_000_000_u64 / max_samples_per_second
        };
        Self {
            min_interval_ns: min_interval_ns.max(1),
            last_sample_mono_ns: None,
        }
    }

    pub fn should_record(&mut self, now_mono_ns: u64) -> bool {
        let Some(last) = self.last_sample_mono_ns else {
            self.last_sample_mono_ns = Some(now_mono_ns);
            return true;
        };
        if now_mono_ns.saturating_sub(last) < self.min_interval_ns {
            return false;
        }
        self.last_sample_mono_ns = Some(now_mono_ns);
        true
    }
}

#[derive(Debug, Clone)]
pub struct RecorderStatusWriteGovernor {
    interval_ns: u64,
    last_write_mono_ns: Option<u64>,
}

impl RecorderStatusWriteGovernor {
    pub fn new(max_status_write_interval_ms: u64) -> Self {
        Self {
            interval_ns: max_status_write_interval_ms
                .max(1)
                .saturating_mul(1_000_000),
            last_write_mono_ns: None,
        }
    }

    pub fn should_write(&mut self, now_mono_ns: u64, force: bool) -> bool {
        if force {
            self.last_write_mono_ns = Some(now_mono_ns);
            return true;
        }
        let Some(last) = self.last_write_mono_ns else {
            self.last_write_mono_ns = Some(now_mono_ns);
            return true;
        };
        if now_mono_ns.saturating_sub(last) < self.interval_ns {
            return false;
        }
        self.last_write_mono_ns = Some(now_mono_ns);
        true
    }
}
