use std::{
    collections::BTreeMap,
    env, fs,
    path::{Component, Path, PathBuf},
};

use serde::Serialize;
use serde_json::json;

use crate::{
    aggregate_event_data_quality, build_evidence_index, build_overhead_report, default_target_id,
    parse_meminfo, parse_net_dev, parse_proc_stat, write_evidence_index, AdcError, AdcResult,
    ArtifactManifest, ClockSource, DataQuality, EventEnvelope, EvidenceBuildInput, OverheadBudget,
    OverheadSample, TimeRangeNs,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotBundle {
    pub run_id: String,
    pub run_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub evidence_index_path: PathBuf,
    pub timeline_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotTargetContext {
    pub target_id: String,
    pub fleet_run_id: Option<String>,
}

impl SnapshotTargetContext {
    pub fn local() -> Self {
        Self {
            target_id: default_target_id(),
            fleet_run_id: None,
        }
    }
}

#[derive(Debug, Serialize)]
struct RawSystemSnapshot {
    run_id: String,
    os_release: Option<String>,
    board_model: Option<String>,
    data_quality: DataQuality,
}

#[derive(Debug, Serialize)]
struct RawCollectorSnapshot<T: Serialize> {
    sample: Option<T>,
    data_quality: DataQuality,
}

#[derive(Debug, Serialize)]
struct CapabilitySnapshot {
    arch: String,
    kernel_release: Option<String>,
    board_model: Option<String>,
    thermal_zones: Vec<String>,
    pci_devices: Vec<String>,
    loaded_modules: Vec<String>,
    tracefs_available: bool,
    data_quality: DataQuality,
}

#[derive(Debug, Serialize)]
struct WindowArtifact {
    window_id: String,
    run_id: String,
    trigger_reason: String,
    start_mono_ns: u64,
    end_mono_ns: u64,
    sources: Vec<String>,
    event_count: usize,
    data_quality: DataQuality,
}

pub fn default_artifact_root() -> PathBuf {
    if let Some(path) = env::var_os("ADC_HOME") {
        return PathBuf::from(path);
    }
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home).join(".local/share/agent-debug-compass");
    }
    PathBuf::from(".adc-targetd")
}

pub fn create_snapshot(artifact_root: impl AsRef<Path>, run_id: &str) -> AdcResult<SnapshotBundle> {
    create_snapshot_for_target(artifact_root, run_id, SnapshotTargetContext::local())
}

pub fn create_snapshot_for_target(
    artifact_root: impl AsRef<Path>,
    run_id: &str,
    target: SnapshotTargetContext,
) -> AdcResult<SnapshotBundle> {
    validate_run_id(run_id)?;
    validate_run_id(&target.target_id)?;
    let run_dir = artifact_root.as_ref().join("runs").join(run_id);
    let raw_dir = run_dir.join("raw");
    fs::create_dir_all(&raw_dir).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to create run directory {}: {err}",
            raw_dir.display()
        ))
    })?;

    let raw_snapshot = collect_system_snapshot(run_id);
    write_json(&raw_dir.join("system.json"), &raw_snapshot)?;

    let time_mono_ns = monotonic_now_ns();
    let mut events = vec![snapshot_event(
        run_id,
        "system",
        "system/snapshot",
        time_mono_ns,
        json!({
            "raw_ref": "artifact://raw/system.json",
            "board_model_available": raw_snapshot.board_model.is_some(),
        }),
        raw_snapshot.data_quality.clone(),
    )];

    let cpu_snapshot = collect_proc_file("/proc/stat", parse_proc_stat);
    write_json(&raw_dir.join("cpu.json"), &cpu_snapshot)?;
    events.push(snapshot_event(
        run_id,
        "cpu",
        "cpu/procfs",
        time_mono_ns,
        json!({
            "raw_ref": "artifact://raw/cpu.json",
            "sample": cpu_snapshot.sample,
        }),
        cpu_snapshot.data_quality,
    ));

    let memory_snapshot = collect_proc_file("/proc/meminfo", parse_meminfo);
    write_json(&raw_dir.join("memory.json"), &memory_snapshot)?;
    events.push(snapshot_event(
        run_id,
        "memory",
        "memory/procfs",
        time_mono_ns,
        json!({
            "raw_ref": "artifact://raw/memory.json",
            "sample": memory_snapshot.sample,
        }),
        memory_snapshot.data_quality,
    ));

    let network_snapshot = collect_proc_file("/proc/net/dev", parse_net_dev);
    write_json(&raw_dir.join("network.json"), &network_snapshot)?;
    events.push(snapshot_event(
        run_id,
        "network",
        "network/procfs",
        time_mono_ns,
        json!({
            "raw_ref": "artifact://raw/network.json",
            "sample": network_snapshot.sample,
        }),
        network_snapshot.data_quality,
    ));

    let capability_snapshot = collect_capability_snapshot();
    write_json(&raw_dir.join("capability.json"), &capability_snapshot)?;
    events.push(snapshot_event(
        run_id,
        "capability",
        "capability/detect",
        time_mono_ns,
        json!({
            "raw_ref": "artifact://raw/capability.json",
            "arch": capability_snapshot.arch,
            "board_model": capability_snapshot.board_model,
            "thermal_zone_count": capability_snapshot.thermal_zones.len(),
            "pci_device_count": capability_snapshot.pci_devices.len(),
            "tracefs_available": capability_snapshot.tracefs_available,
        }),
        capability_snapshot.data_quality,
    ));

    let timeline_path = run_dir.join("timeline.jsonl");
    write_jsonl(&timeline_path, &events)?;

    let window = build_snapshot_window(run_id, time_mono_ns, &events);
    write_yaml(&run_dir.join("windows/W001.yaml"), &window)?;

    let raw_refs = snapshot_raw_refs();
    let event_quality = aggregate_event_data_quality(&events);
    let evidence = build_evidence_index(EvidenceBuildInput {
        run_id: run_id.to_string(),
        target_id: target.target_id.clone(),
        fleet_run_id: target.fleet_run_id.clone(),
        capture_mode: "snapshot".to_string(),
        window_id: "W001".to_string(),
        start_mono_ns: time_mono_ns,
        end_mono_ns: time_mono_ns,
        events: events.clone(),
        raw_refs,
        data_quality: event_quality,
    });
    let evidence_index_path = run_dir.join("evidence_index.yaml");
    write_evidence_index(&evidence_index_path, &evidence)?;

    let overhead_report = build_overhead_report(
        OverheadBudget::default(),
        OverheadSample {
            artifact_bytes: directory_size_bytes(&run_dir)?,
            event_count: events.len() as u64,
            duration_ms: 0,
        },
    );
    write_json(&run_dir.join("overhead_report.json"), &overhead_report)?;

    let mut manifest = ArtifactManifest::new_for_target(
        run_id,
        "snapshot",
        &target.target_id,
        target.fleet_run_id,
    );
    manifest.add_file(&run_dir, "raw/system.json", "system_snapshot")?;
    manifest.add_file(&run_dir, "raw/cpu.json", "cpu")?;
    manifest.add_file(&run_dir, "raw/memory.json", "memory")?;
    manifest.add_file(&run_dir, "raw/network.json", "network")?;
    manifest.add_file(&run_dir, "raw/capability.json", "capability")?;
    manifest.add_file(&run_dir, "timeline.jsonl", "timeline")?;
    manifest.add_file(&run_dir, "windows/W001.yaml", "window")?;
    manifest.add_file(&run_dir, "evidence_index.yaml", "evidence_index")?;
    manifest.add_file(&run_dir, "overhead_report.json", "overhead")?;
    let manifest_path = run_dir.join("manifest.json");
    manifest.write_json(&manifest_path)?;

    Ok(SnapshotBundle {
        run_id: run_id.to_string(),
        run_dir,
        manifest_path,
        evidence_index_path,
        timeline_path,
    })
}

fn build_snapshot_window(
    run_id: &str,
    time_mono_ns: u64,
    events: &[EventEnvelope],
) -> WindowArtifact {
    WindowArtifact {
        window_id: "W001".to_string(),
        run_id: run_id.to_string(),
        trigger_reason: "manual_snapshot".to_string(),
        start_mono_ns: time_mono_ns,
        end_mono_ns: time_mono_ns,
        sources: events.iter().map(|event| event.source.clone()).collect(),
        event_count: events.len(),
        data_quality: DataQuality {
            clock_confidence: crate::ClockConfidence::Medium,
            ..Default::default()
        },
    }
}

fn snapshot_event(
    run_id: &str,
    source: &str,
    collector_id: &str,
    time_mono_ns: u64,
    payload: serde_json::Value,
    data_quality: DataQuality,
) -> EventEnvelope {
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
        collector_id: collector_id.to_string(),
        profile_id: "snapshot".to_string(),
        payload,
        data_quality,
    }
}

pub fn read_window(
    artifact_root: impl AsRef<Path>,
    run_id: &str,
    window_id: &str,
) -> AdcResult<String> {
    validate_run_id(run_id)?;
    validate_run_id(window_id)?;
    let window_path = artifact_root
        .as_ref()
        .join("runs")
        .join(run_id)
        .join("windows")
        .join(format!("{window_id}.yaml"));
    fs::read_to_string(&window_path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read window {}: {err}",
            window_path.display()
        ))
    })
}

pub fn list_runs(artifact_root: impl AsRef<Path>) -> AdcResult<Vec<String>> {
    let runs_root = artifact_root.as_ref().join("runs");
    if !runs_root.exists() {
        return Ok(Vec::new());
    }
    let mut runs = Vec::new();
    for entry in fs::read_dir(&runs_root).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read runs {}: {err}",
            runs_root.display()
        ))
    })? {
        let entry = entry.map_err(|err| {
            AdcError::Artifact(format!("failed to read run directory entry: {err}"))
        })?;
        if entry.path().join("manifest.json").is_file() {
            if let Some(name) = entry.file_name().to_str() {
                runs.push(name.to_string());
            }
        }
    }
    runs.sort();
    Ok(runs)
}

pub fn manifest_path_for(artifact_root: impl AsRef<Path>, run_id: &str) -> AdcResult<PathBuf> {
    validate_run_id(run_id)?;
    let path = artifact_root
        .as_ref()
        .join("runs")
        .join(run_id)
        .join("manifest.json");
    if !path.is_file() {
        return Err(AdcError::Artifact(format!(
            "manifest not found for run_id {run_id}: {}",
            path.display()
        )));
    }
    Ok(path)
}

fn collect_system_snapshot(run_id: &str) -> RawSystemSnapshot {
    let mut data_quality = DataQuality {
        clock_confidence: crate::ClockConfidence::Medium,
        ..Default::default()
    };
    data_quality
        .notes
        .push("thin MVP snapshot; detailed collectors are added in later WBS".to_string());

    let os_release = read_optional_text("/etc/os-release", "os_release", &mut data_quality);
    let board_model = read_optional_text(
        "/proc/device-tree/model",
        "raspberry_pi_model",
        &mut data_quality,
    )
    .map(|value| value.trim_matches(char::from(0)).trim().to_string());

    RawSystemSnapshot {
        run_id: run_id.to_string(),
        os_release,
        board_model,
        data_quality,
    }
}

fn collect_capability_snapshot() -> CapabilitySnapshot {
    let mut data_quality = DataQuality {
        clock_confidence: crate::ClockConfidence::Medium,
        ..Default::default()
    };
    let board_model = read_optional_text(
        "/proc/device-tree/model",
        "raspberry_pi_model",
        &mut data_quality,
    )
    .map(|value| value.trim_matches(char::from(0)).trim().to_string());
    let kernel_release = read_optional_text(
        "/proc/sys/kernel/osrelease",
        "kernel_release",
        &mut data_quality,
    )
    .map(|value| value.trim().to_string());
    let thermal_zones = list_dir_names("/sys/class/thermal", "thermal_zones", &mut data_quality);
    let pci_devices = list_dir_names("/sys/bus/pci/devices", "pci_devices", &mut data_quality);
    let loaded_modules = read_optional_text("/proc/modules", "loaded_modules", &mut data_quality)
        .map(|contents| {
            contents
                .lines()
                .filter_map(|line| line.split_whitespace().next().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let tracefs_available = Path::new("/sys/kernel/tracing").is_dir()
        || Path::new("/sys/kernel/debug/tracing").is_dir();
    if !tracefs_available {
        data_quality
            .missing
            .push("tracefs: no tracing directory visible".to_string());
    }

    CapabilitySnapshot {
        arch: env::consts::ARCH.to_string(),
        kernel_release,
        board_model,
        thermal_zones,
        pci_devices,
        loaded_modules,
        tracefs_available,
        data_quality,
    }
}

fn read_optional_text(path: &str, label: &str, data_quality: &mut DataQuality) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(value) => Some(value),
        Err(err) => {
            data_quality.missing.push(format!("{label}: {err}"));
            None
        }
    }
}

fn list_dir_names(path: &str, label: &str, data_quality: &mut DataQuality) -> Vec<String> {
    match fs::read_dir(path) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect(),
        Err(err) => {
            data_quality.missing.push(format!("{label}: {err}"));
            Vec::new()
        }
    }
}

fn collect_proc_file<T: Serialize>(
    path: &str,
    parser: impl FnOnce(&str) -> AdcResult<T>,
) -> RawCollectorSnapshot<T> {
    let mut data_quality = DataQuality {
        clock_confidence: crate::ClockConfidence::Medium,
        ..Default::default()
    };
    match fs::read_to_string(path) {
        Ok(contents) => match parser(&contents) {
            Ok(sample) => RawCollectorSnapshot {
                sample: Some(sample),
                data_quality,
            },
            Err(err) => {
                data_quality.missing.push(format!("{path}: {err}"));
                RawCollectorSnapshot {
                    sample: None,
                    data_quality,
                }
            }
        },
        Err(err) => {
            data_quality.missing.push(format!("{path}: {err}"));
            RawCollectorSnapshot {
                sample: None,
                data_quality,
            }
        }
    }
}

fn snapshot_raw_refs() -> BTreeMap<String, String> {
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
        "system".to_string(),
        "artifact://raw/system.json".to_string(),
    );
    raw_refs.insert("cpu".to_string(), "artifact://raw/cpu.json".to_string());
    raw_refs.insert(
        "memory".to_string(),
        "artifact://raw/memory.json".to_string(),
    );
    raw_refs.insert(
        "network".to_string(),
        "artifact://raw/network.json".to_string(),
    );
    raw_refs.insert(
        "capability".to_string(),
        "artifact://raw/capability.json".to_string(),
    );
    raw_refs.insert(
        "window".to_string(),
        "artifact://windows/W001.yaml".to_string(),
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

fn write_jsonl(path: &Path, events: &[EventEnvelope]) -> AdcResult<()> {
    let mut lines = String::new();
    for event in events {
        let line = serde_json::to_string(event)
            .map_err(|err| AdcError::Artifact(format!("timeline serialization failed: {err}")))?;
        lines.push_str(&line);
        lines.push('\n');
    }
    fs::write(path, lines).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to write timeline {}: {err}",
            path.display()
        ))
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
