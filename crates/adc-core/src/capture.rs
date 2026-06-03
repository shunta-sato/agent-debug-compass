use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use serde_json::json;

use crate::{
    build_evidence_index, build_overhead_report,
    collectors::{CpuSample, MemorySample, NetworkDeviceSample},
    default_target_id, parse_meminfo, parse_net_dev, parse_proc_stat, write_evidence_index,
    AdcError, AdcResult, ArtifactManifest, ClockSource, DataQuality, EventEnvelope,
    EvidenceBuildInput, OverheadBudget, OverheadSample, TimeRangeNs,
};

#[derive(Debug, Clone)]
pub struct CaptureOptions {
    pub run_id: String,
    pub profile_id: String,
    pub duration: Duration,
    pub interval: Duration,
    pub collectors: Vec<String>,
    pub max_artifact_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureTargetContext {
    pub target_id: String,
    pub fleet_run_id: Option<String>,
}

impl CaptureTargetContext {
    pub fn local() -> Self {
        Self {
            target_id: default_target_id(),
            fleet_run_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureBundle {
    pub run_id: String,
    pub run_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub evidence_index_path: PathBuf,
    pub timeline_path: PathBuf,
    pub sample_count: usize,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
struct CaptureSample {
    sample_index: usize,
    time_mono_ns: u64,
    cpu: Option<CpuSample>,
    memory: Option<MemorySample>,
    network: Option<NetworkDeviceSample>,
    data_quality: DataQuality,
}

#[derive(Debug, Serialize)]
struct RawSourceSample<'a, T: Serialize> {
    sample_index: usize,
    time_mono_ns: u64,
    sample: &'a T,
}

#[derive(Debug, Serialize)]
struct CaptureWindow {
    window_id: String,
    run_id: String,
    trigger_reason: String,
    start_mono_ns: u64,
    end_mono_ns: u64,
    sources: Vec<String>,
    event_count: usize,
    data_quality: DataQuality,
}

pub fn capture_for(
    artifact_root: impl AsRef<Path>,
    options: CaptureOptions,
) -> AdcResult<CaptureBundle> {
    capture_for_target(artifact_root, options, CaptureTargetContext::local())
}

pub fn capture_for_target(
    artifact_root: impl AsRef<Path>,
    options: CaptureOptions,
    target: CaptureTargetContext,
) -> AdcResult<CaptureBundle> {
    validate_run_id(&options.run_id)?;
    validate_run_id(&target.target_id)?;
    validate_capture_options(&options)?;

    let artifact_root = artifact_root.as_ref();
    let run_dir = artifact_root.join("runs").join(&options.run_id);
    let raw_dir = run_dir.join("raw");
    fs::create_dir_all(&raw_dir).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to create capture directory {}: {err}",
            raw_dir.display()
        ))
    })?;

    let started = Instant::now();
    let mut samples = Vec::new();
    loop {
        let sample = collect_capture_sample(&options, samples.len());
        samples.push(sample);

        let elapsed = started.elapsed();
        if elapsed >= options.duration {
            break;
        }
        thread::sleep(
            options
                .interval
                .min(options.duration.saturating_sub(elapsed)),
        );
    }
    let elapsed_ms = duration_ms(started.elapsed());

    let data_quality = aggregate_data_quality(&samples);
    write_jsonl(&raw_dir.join("samples.jsonl"), samples.iter())?;
    write_source_jsonl(
        &raw_dir.join("cpu.jsonl"),
        samples.iter().filter_map(|sample| {
            sample.cpu.as_ref().map(|cpu| RawSourceSample {
                sample_index: sample.sample_index,
                time_mono_ns: sample.time_mono_ns,
                sample: cpu,
            })
        }),
    )?;
    write_source_jsonl(
        &raw_dir.join("memory.jsonl"),
        samples.iter().filter_map(|sample| {
            sample.memory.as_ref().map(|memory| RawSourceSample {
                sample_index: sample.sample_index,
                time_mono_ns: sample.time_mono_ns,
                sample: memory,
            })
        }),
    )?;
    write_source_jsonl(
        &raw_dir.join("network.jsonl"),
        samples.iter().filter_map(|sample| {
            sample.network.as_ref().map(|network| RawSourceSample {
                sample_index: sample.sample_index,
                time_mono_ns: sample.time_mono_ns,
                sample: network,
            })
        }),
    )?;

    let events = build_capture_events(&options, &samples);
    let timeline_path = run_dir.join("timeline.jsonl");
    write_jsonl(&timeline_path, events.iter())?;

    let window = build_capture_window(&options, &samples, &events, data_quality.clone());
    write_yaml(&run_dir.join("windows/W001.yaml"), &window)?;

    let raw_refs = capture_raw_refs();
    let evidence = build_evidence_index(EvidenceBuildInput {
        run_id: options.run_id.clone(),
        target_id: target.target_id.clone(),
        fleet_run_id: target.fleet_run_id.clone(),
        capture_mode: "capture".to_string(),
        window_id: "W001".to_string(),
        start_mono_ns: samples
            .first()
            .map(|sample| sample.time_mono_ns)
            .unwrap_or(0),
        end_mono_ns: samples
            .last()
            .map(|sample| sample.time_mono_ns)
            .unwrap_or(0),
        events: events.clone(),
        raw_refs,
        data_quality: data_quality.clone(),
    });
    let evidence_index_path = run_dir.join("evidence_index.yaml");
    write_evidence_index(&evidence_index_path, &evidence)?;

    let budget = OverheadBudget {
        max_artifact_bytes: options.max_artifact_bytes,
        max_duration_ms: duration_ms(options.duration.saturating_add(options.interval)),
        ..OverheadBudget::default()
    };
    let overhead_report = build_overhead_report(
        budget,
        OverheadSample {
            artifact_bytes: directory_size_bytes(&run_dir)?,
            event_count: events.len() as u64,
            duration_ms: elapsed_ms,
        },
    );
    write_json(&run_dir.join("overhead_report.json"), &overhead_report)?;

    let mut manifest = ArtifactManifest::new_for_target(
        &options.run_id,
        &options.profile_id,
        &target.target_id,
        target.fleet_run_id.clone(),
    );
    manifest.add_file(&run_dir, "raw/samples.jsonl", "capture_samples")?;
    manifest.add_file(&run_dir, "raw/cpu.jsonl", "cpu_samples")?;
    manifest.add_file(&run_dir, "raw/memory.jsonl", "memory_samples")?;
    manifest.add_file(&run_dir, "raw/network.jsonl", "network_samples")?;
    manifest.add_file(&run_dir, "timeline.jsonl", "timeline")?;
    manifest.add_file(&run_dir, "windows/W001.yaml", "window")?;
    manifest.add_file(&run_dir, "evidence_index.yaml", "evidence_index")?;
    manifest.add_file(&run_dir, "overhead_report.json", "overhead")?;
    let manifest_path = run_dir.join("manifest.json");
    manifest.write_json(&manifest_path)?;

    Ok(CaptureBundle {
        run_id: options.run_id,
        run_dir,
        manifest_path,
        evidence_index_path,
        timeline_path,
        sample_count: samples.len(),
        duration_ms: elapsed_ms,
    })
}

fn validate_capture_options(options: &CaptureOptions) -> AdcResult<()> {
    if options.profile_id.trim().is_empty() {
        return Err(AdcError::ProfileValidation(
            "capture profile_id must not be empty".to_string(),
        ));
    }
    if options.duration.is_zero() {
        return Err(AdcError::ProfileValidation(
            "capture duration must be greater than zero".to_string(),
        ));
    }
    if options.interval.is_zero() {
        return Err(AdcError::ProfileValidation(
            "capture interval must be greater than zero".to_string(),
        ));
    }
    if options.collectors.is_empty() {
        return Err(AdcError::ProfileValidation(
            "capture collectors must not be empty".to_string(),
        ));
    }
    Ok(())
}

fn collect_capture_sample(options: &CaptureOptions, sample_index: usize) -> CaptureSample {
    let collector_set = options
        .collectors
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut data_quality = DataQuality {
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };

    for collector in &collector_set {
        if !matches!(*collector, "cpu" | "memory" | "network") {
            data_quality.missing.push(format!(
                "collector {collector} is not implemented for bounded capture"
            ));
        }
    }

    let cpu = if collector_set.contains("cpu") {
        collect_proc_sample("/proc/stat", parse_proc_stat, "cpu", &mut data_quality)
    } else {
        None
    };
    let memory = if collector_set.contains("memory") {
        collect_proc_sample("/proc/meminfo", parse_meminfo, "memory", &mut data_quality)
    } else {
        None
    };
    let network = if collector_set.contains("network") {
        collect_proc_sample("/proc/net/dev", parse_net_dev, "network", &mut data_quality)
    } else {
        None
    };

    CaptureSample {
        sample_index,
        time_mono_ns: monotonic_now_ns(),
        cpu,
        memory,
        network,
        data_quality,
    }
}

fn collect_proc_sample<T>(
    path: &str,
    parser: impl FnOnce(&str) -> AdcResult<T>,
    label: &str,
    data_quality: &mut DataQuality,
) -> Option<T> {
    match fs::read_to_string(path) {
        Ok(contents) => match parser(&contents) {
            Ok(sample) => Some(sample),
            Err(err) => {
                data_quality.missing.push(format!("{label}: {err}"));
                None
            }
        },
        Err(err) => {
            data_quality.missing.push(format!("{label}: {err}"));
            None
        }
    }
}

fn build_capture_events(options: &CaptureOptions, samples: &[CaptureSample]) -> Vec<EventEnvelope> {
    let mut events = Vec::new();
    for sample in samples {
        if let Some(cpu) = &sample.cpu {
            events.push(capture_event(
                options,
                sample,
                "cpu",
                "cpu/capture",
                json!({
                    "raw_ref": "artifact://raw/cpu.jsonl",
                    "sample_index": sample.sample_index,
                    "sample": cpu,
                }),
            ));
        }
        if let Some(memory) = &sample.memory {
            events.push(capture_event(
                options,
                sample,
                "memory",
                "memory/capture",
                json!({
                    "raw_ref": "artifact://raw/memory.jsonl",
                    "sample_index": sample.sample_index,
                    "sample": memory,
                }),
            ));
        }
        if let Some(network) = &sample.network {
            events.push(capture_event(
                options,
                sample,
                "network",
                "network/capture",
                json!({
                    "raw_ref": "artifact://raw/network.jsonl",
                    "sample_index": sample.sample_index,
                    "sample": network,
                }),
            ));
        }
    }
    events
}

fn capture_event(
    options: &CaptureOptions,
    sample: &CaptureSample,
    source: &str,
    collector_id: &str,
    payload: serde_json::Value,
) -> EventEnvelope {
    EventEnvelope {
        run_id: options.run_id.clone(),
        source: source.to_string(),
        event_type: "sample".to_string(),
        time_mono_ns: sample.time_mono_ns,
        time_range_ns: TimeRangeNs {
            start: sample.time_mono_ns,
            end: sample.time_mono_ns,
        },
        clock_source: ClockSource::Monotonic,
        collector_id: collector_id.to_string(),
        profile_id: options.profile_id.clone(),
        payload,
        data_quality: sample.data_quality.clone(),
    }
}

fn build_capture_window(
    options: &CaptureOptions,
    samples: &[CaptureSample],
    events: &[EventEnvelope],
    data_quality: DataQuality,
) -> CaptureWindow {
    let start = samples
        .first()
        .map(|sample| sample.time_mono_ns)
        .unwrap_or(0);
    let end = samples
        .last()
        .map(|sample| sample.time_mono_ns)
        .unwrap_or(start);
    let sources = events
        .iter()
        .map(|event| event.source.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    CaptureWindow {
        window_id: "W001".to_string(),
        run_id: options.run_id.clone(),
        trigger_reason: "manual_capture".to_string(),
        start_mono_ns: start,
        end_mono_ns: end,
        sources,
        event_count: events.len(),
        data_quality,
    }
}

fn capture_raw_refs() -> BTreeMap<String, String> {
    let mut raw_refs = BTreeMap::new();
    raw_refs.insert(
        "manifest".to_string(),
        "artifact://manifest.json".to_string(),
    );
    raw_refs.insert(
        "timeline".to_string(),
        "artifact://timeline.jsonl".to_string(),
    );
    raw_refs.insert(
        "window".to_string(),
        "artifact://windows/W001.yaml".to_string(),
    );
    raw_refs.insert(
        "samples".to_string(),
        "artifact://raw/samples.jsonl".to_string(),
    );
    raw_refs.insert("cpu".to_string(), "artifact://raw/cpu.jsonl".to_string());
    raw_refs.insert(
        "memory".to_string(),
        "artifact://raw/memory.jsonl".to_string(),
    );
    raw_refs.insert(
        "network".to_string(),
        "artifact://raw/network.jsonl".to_string(),
    );
    raw_refs.insert(
        "evidence_index".to_string(),
        "artifact://evidence_index.yaml".to_string(),
    );
    raw_refs.insert(
        "overhead".to_string(),
        "artifact://overhead_report.json".to_string(),
    );
    raw_refs
}

fn aggregate_data_quality(samples: &[CaptureSample]) -> DataQuality {
    let mut data_quality = DataQuality {
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };
    for sample in samples {
        data_quality.dropped |= sample.data_quality.dropped;
        data_quality.throttled |= sample.data_quality.throttled;
        data_quality.truncated |= sample.data_quality.truncated;
        data_quality.drop_count = data_quality
            .drop_count
            .saturating_add(sample.data_quality.drop_count);
        extend_unique(&mut data_quality.missing, &sample.data_quality.missing);
        extend_unique(&mut data_quality.notes, &sample.data_quality.notes);
    }
    data_quality
}

fn extend_unique(target: &mut Vec<String>, source: &[String]) {
    for value in source {
        if !target.contains(value) {
            target.push(value.clone());
        }
    }
}

fn write_json(path: &Path, value: &impl Serialize) -> AdcResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AdcError::Artifact(format!("failed to create {}: {err}", parent.display()))
        })?;
    }
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|err| AdcError::Artifact(format!("json serialization failed: {err}")))?;
    fs::write(path, bytes)
        .map_err(|err| AdcError::Artifact(format!("failed to write {}: {err}", path.display())))
}

fn write_yaml(path: &Path, value: &impl Serialize) -> AdcResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AdcError::Artifact(format!("failed to create {}: {err}", parent.display()))
        })?;
    }
    let bytes = yaml_serde::to_string(value)
        .map_err(|err| AdcError::Artifact(format!("yaml serialization failed: {err}")))?;
    fs::write(path, bytes)
        .map_err(|err| AdcError::Artifact(format!("failed to write {}: {err}", path.display())))
}

fn write_jsonl<'a, T, I>(path: &Path, values: I) -> AdcResult<()>
where
    T: Serialize + 'a,
    I: IntoIterator<Item = &'a T>,
{
    let mut lines = String::new();
    for value in values {
        let line = serde_json::to_string(value)
            .map_err(|err| AdcError::Artifact(format!("jsonl serialization failed: {err}")))?;
        lines.push_str(&line);
        lines.push('\n');
    }
    fs::write(path, lines).map_err(|err| {
        AdcError::Artifact(format!("failed to write jsonl {}: {err}", path.display()))
    })
}

fn write_source_jsonl<T, I>(path: &Path, values: I) -> AdcResult<()>
where
    T: Serialize,
    I: IntoIterator<Item = T>,
{
    let mut lines = String::new();
    for value in values {
        let line = serde_json::to_string(&value)
            .map_err(|err| AdcError::Artifact(format!("jsonl serialization failed: {err}")))?;
        lines.push_str(&line);
        lines.push('\n');
    }
    fs::write(path, lines).map_err(|err| {
        AdcError::Artifact(format!("failed to write jsonl {}: {err}", path.display()))
    })
}

fn directory_size_bytes(path: &Path) -> AdcResult<u64> {
    let mut total = 0;
    for entry in fs::read_dir(path)
        .map_err(|err| AdcError::Artifact(format!("failed to read {}: {err}", path.display())))?
    {
        let entry = entry
            .map_err(|err| AdcError::Artifact(format!("failed to read directory entry: {err}")))?;
        let metadata = entry.metadata().map_err(|err| {
            AdcError::Artifact(format!("failed to stat {}: {err}", entry.path().display()))
        })?;
        if metadata.is_dir() {
            total += directory_size_bytes(&entry.path())?;
        } else if metadata.is_file() {
            total += metadata.len();
        }
    }
    Ok(total)
}

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn monotonic_now_ns() -> u64 {
    fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|contents| contents.split_whitespace().next().map(str::to_string))
        .and_then(|seconds| seconds.parse::<f64>().ok())
        .map(|seconds| (seconds * 1_000_000_000.0) as u64)
        .unwrap_or(0)
}

fn validate_run_id(run_id: &str) -> AdcResult<()> {
    if run_id.trim().is_empty() {
        return Err(AdcError::Artifact("run_id must not be empty".to_string()));
    }
    let path = Path::new(run_id);
    let mut components = path.components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(AdcError::Artifact(
            "run_id must be a single relative path segment".to_string(),
        )),
    }
}
