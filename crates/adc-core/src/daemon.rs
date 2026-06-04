use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    aggregate_event_data_quality, build_evidence_index, build_overhead_report,
    collectors::{CpuSample, MemorySample, NetworkDeviceSample},
    default_recorder_budget, default_target_id, drain_pending_recorder_markers, evaluate_trigger,
    freeze_recorder_marker, freeze_recorder_trigger, parse_meminfo, parse_net_dev, parse_proc_stat,
    profile::{load_profile, RuleType, TriggerRule},
    recorder_marker_result_for_frozen, recorder_marker_result_for_refused,
    recorder_ring_capacity_for_budget, recorder_status_for, snapshot, write_evidence_index,
    write_recorder_marker_result, write_recorder_status_artifact, AdcError, AdcResult,
    ArtifactManifest, ClockSource, DataQuality, EventEnvelope, EvidenceBuildInput, OverheadBudget,
    OverheadSample, Profile, RecorderRing, RecorderSample, RecorderSignalSample, TimeRangeNs,
    TriggerEvaluation, TriggerInput,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonState {
    pub service: String,
    pub version: String,
    pub status: String,
    pub started_at_unix_ms: u128,
    pub artifact_root: PathBuf,
    pub active_profile: Option<String>,
    pub recovered_runs: Vec<String>,
    pub last_run_id: Option<String>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceRunSummary {
    pub state: DaemonState,
    pub captured_runs: Vec<String>,
    pub frozen_incidents: Vec<String>,
    pub iterations: u64,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, Serialize)]
struct LiveSample {
    time_mono_ns: u64,
    cpu: Option<CpuSample>,
    memory: Option<MemorySample>,
    network: Option<NetworkDeviceSample>,
    kmsg: Option<KmsgSample>,
    data_quality: DataQuality,
}

#[derive(Debug, Clone, Serialize)]
struct KmsgSample {
    severity: String,
    message: String,
    source: String,
}

#[derive(Debug, Clone)]
struct MatchedLiveTrigger {
    evaluation: TriggerEvaluation,
    input: TriggerInput,
    source: String,
}

#[derive(Debug, Serialize)]
struct DaemonWindow {
    window_id: String,
    run_id: String,
    trigger_reason: String,
    start_mono_ns: u64,
    end_mono_ns: u64,
    sources: Vec<String>,
    event_count: usize,
    data_quality: DataQuality,
}

pub fn state_path(artifact_root: impl AsRef<Path>) -> PathBuf {
    artifact_root.as_ref().join("daemon/state.json")
}

pub fn initialize_state(artifact_root: impl AsRef<Path>) -> AdcResult<DaemonState> {
    let artifact_root = artifact_root.as_ref();
    let mut state = read_state(artifact_root).unwrap_or_else(|_| default_state(artifact_root));
    state.service = "adc-targetd".to_string();
    state.version = crate::VERSION.to_string();
    state.status = if state.active_profile.is_some() {
        "armed".to_string()
    } else {
        "ready".to_string()
    };
    state.artifact_root = artifact_root.to_path_buf();
    state.recovered_runs = snapshot::list_runs(artifact_root)?;
    write_state(artifact_root, &state)?;
    Ok(state)
}

pub fn read_state(artifact_root: impl AsRef<Path>) -> AdcResult<DaemonState> {
    let path = state_path(artifact_root);
    let bytes = fs::read(&path).map_err(|err| {
        AdcError::Artifact(format!("failed to read state {}: {err}", path.display()))
    })?;
    serde_json::from_slice(&bytes)
        .map_err(|err| AdcError::Artifact(format!("state parse failed: {err}")))
}

pub fn write_state(artifact_root: impl AsRef<Path>, state: &DaemonState) -> AdcResult<()> {
    let path = state_path(artifact_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AdcError::Artifact(format!(
                "failed to create state directory {}: {err}",
                parent.display()
            ))
        })?;
    }
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|err| AdcError::Artifact(format!("state serialization failed: {err}")))?;
    fs::write(&path, bytes).map_err(|err| {
        AdcError::Artifact(format!("failed to write state {}: {err}", path.display()))
    })
}

pub fn arm_profile(artifact_root: impl AsRef<Path>, profile_id: &str) -> AdcResult<DaemonState> {
    validate_profile_id(profile_id)?;
    let artifact_root = artifact_root.as_ref();
    let mut state = initialize_state(artifact_root)?;
    state.active_profile = Some(profile_id.to_string());
    state.status = "armed".to_string();
    write_state(artifact_root, &state)?;
    Ok(state)
}

pub fn disarm_profile(artifact_root: impl AsRef<Path>) -> AdcResult<DaemonState> {
    let artifact_root = artifact_root.as_ref();
    let mut state = initialize_state(artifact_root)?;
    state.active_profile = None;
    state.status = "ready".to_string();
    write_state(artifact_root, &state)?;
    Ok(state)
}

pub fn record_run(artifact_root: impl AsRef<Path>, run_id: &str) -> AdcResult<DaemonState> {
    let artifact_root = artifact_root.as_ref();
    let mut state = initialize_state(artifact_root)?;
    state.last_run_id = Some(run_id.to_string());
    write_state(artifact_root, &state)?;
    Ok(state)
}

pub fn run_service_for(
    artifact_root: impl AsRef<Path>,
    profile_dir: impl AsRef<Path>,
    duration: Duration,
) -> AdcResult<ServiceRunSummary> {
    if duration.is_zero() {
        return Err(AdcError::ProfileValidation(
            "service duration must be greater than zero".to_string(),
        ));
    }

    let artifact_root = artifact_root.as_ref();
    let profile_dir = profile_dir.as_ref();
    let deadline = Instant::now() + duration;
    let mut captured_runs = Vec::new();
    let mut frozen_incidents = Vec::new();
    let mut iterations = 0_u64;
    let mut previous_sample = None;
    let recorder_budget = default_recorder_budget();
    let mut recorder_profile_id: Option<String> = None;
    let mut recorder_ring = RecorderRing::new(
        default_target_id(),
        recorder_ring_capacity_for_budget(&recorder_budget),
        recorder_budget.max_retention_ms,
    );
    let mut summary_quality = DataQuality {
        clock_confidence: crate::ClockConfidence::Medium,
        ..Default::default()
    };

    while Instant::now() < deadline || iterations == 0 {
        iterations += 1;
        let state = initialize_state(artifact_root)?;
        let Some(profile_id) = state.active_profile.clone() else {
            if !summary_quality
                .notes
                .iter()
                .any(|note| note == "daemon service loop ran without an active profile")
            {
                summary_quality
                    .notes
                    .push("daemon service loop ran without an active profile".to_string());
            }
            sleep_until_next(deadline, Duration::from_millis(50));
            continue;
        };

        let profile = load_profile(profile_dir, &profile_id)?;
        if recorder_profile_id.as_deref() != Some(profile_id.as_str()) {
            recorder_ring = RecorderRing::with_expected_signals(
                default_target_id(),
                recorder_ring_capacity_for_budget(&recorder_budget),
                recorder_budget.max_retention_ms,
                recorder_expected_signal_ids(&profile),
            );
            recorder_profile_id = Some(profile_id.clone());
        }
        let sample = collect_live_sample(&profile);
        push_recorder_samples(&mut recorder_ring, &sample);
        write_live_recorder_status(
            artifact_root,
            Some(&profile_id),
            Some("recording"),
            "recording",
            &recorder_ring,
            &recorder_budget,
        )?;
        for marker in drain_pending_recorder_markers(artifact_root)? {
            let incident_id = format!("INC-{}", marker.marker_id);
            if recorder_freeze_budget_exhausted(
                frozen_incidents.len(),
                &recorder_budget,
                &mut summary_quality,
            ) {
                let result = recorder_marker_result_for_refused(
                    marker,
                    incident_id,
                    "max_frozen_incidents_exceeded",
                );
                write_recorder_marker_result(artifact_root, &result)?;
                continue;
            }
            let window_id = format!("win-{}", marker.marker_id);
            freeze_recorder_marker(
                artifact_root,
                &incident_id,
                &window_id,
                &marker,
                &recorder_ring,
                &recorder_budget,
            )?;
            let result = recorder_marker_result_for_frozen(marker, incident_id.clone());
            write_recorder_marker_result(artifact_root, &result)?;
            frozen_incidents.push(incident_id);
        }
        if let Some(matched) = evaluate_live_triggers(&profile, previous_sample.as_ref(), &sample)?
        {
            let run_id = next_daemon_run_id();
            create_trigger_bundle(artifact_root, &run_id, &profile, &sample, &matched)?;
            let incident_id = format!("INC-TRIGGER-{}", sample.time_mono_ns);
            if !recorder_freeze_budget_exhausted(
                frozen_incidents.len(),
                &recorder_budget,
                &mut summary_quality,
            ) {
                freeze_recorder_trigger(
                    artifact_root,
                    &incident_id,
                    "win-trigger-001",
                    &matched.evaluation.trigger_name,
                    sample.time_mono_ns,
                    &recorder_ring,
                    &recorder_budget,
                )?;
                frozen_incidents.push(incident_id);
            }
            record_run(artifact_root, &run_id)?;
            captured_runs.push(run_id);
            break;
        }
        previous_sample = Some(sample);
        sleep_until_next(
            deadline,
            Duration::from_millis(profile.sampling.interval_ms),
        );
    }

    let state = initialize_state(artifact_root)?;
    write_live_recorder_status(
        artifact_root,
        state.active_profile.as_deref(),
        Some("recording"),
        if state.active_profile.is_some() {
            "recording"
        } else {
            "disabled"
        },
        &recorder_ring,
        &recorder_budget,
    )?;
    Ok(ServiceRunSummary {
        state,
        captured_runs,
        frozen_incidents,
        iterations,
        data_quality: summary_quality,
    })
}

fn write_live_recorder_status(
    artifact_root: &Path,
    active_profile: Option<&str>,
    previous_state: Option<&str>,
    recorder_state: &str,
    ring: &RecorderRing,
    budget: &crate::RecorderBudget,
) -> AdcResult<()> {
    let status = recorder_status_for(
        default_target_id(),
        active_profile,
        previous_state,
        recorder_state,
        ring.status(),
        budget.clone(),
    );
    write_recorder_status_artifact(artifact_root, &status)?;
    Ok(())
}

fn default_state(artifact_root: &Path) -> DaemonState {
    DaemonState {
        service: "adc-targetd".to_string(),
        version: crate::VERSION.to_string(),
        status: "ready".to_string(),
        started_at_unix_ms: unix_epoch_ms(),
        artifact_root: artifact_root.to_path_buf(),
        active_profile: None,
        recovered_runs: Vec::new(),
        last_run_id: None,
        data_quality: DataQuality {
            clock_confidence: crate::ClockConfidence::Medium,
            ..Default::default()
        },
    }
}

fn collect_live_sample(profile: &Profile) -> LiveSample {
    let collectors = profile
        .always_on
        .collectors
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut data_quality = DataQuality {
        clock_confidence: crate::ClockConfidence::Medium,
        ..Default::default()
    };

    let cpu = if collectors.contains("cpu") {
        collect_proc_sample("/proc/stat", parse_proc_stat, "cpu", &mut data_quality)
    } else {
        None
    };
    let memory = if collectors.contains("memory") {
        collect_proc_sample("/proc/meminfo", parse_meminfo, "memory", &mut data_quality)
    } else {
        None
    };
    let network = if collectors.contains("network") {
        collect_proc_sample("/proc/net/dev", parse_net_dev, "network", &mut data_quality)
    } else {
        None
    };
    let kmsg = if collectors.contains("kmsg") {
        collect_kmsg_fixture(&mut data_quality)
    } else {
        None
    };

    LiveSample {
        time_mono_ns: monotonic_now_ns(),
        cpu,
        memory,
        network,
        kmsg,
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

fn collect_kmsg_fixture(data_quality: &mut DataQuality) -> Option<KmsgSample> {
    let Some(path) = env::var_os("ADC_KMSG_FIXTURE").map(PathBuf::from) else {
        data_quality
            .missing
            .push("kmsg fixture not configured; live /dev/kmsg tail is not enabled".to_string());
        return None;
    };
    match fs::read_to_string(&path) {
        Ok(contents) => {
            let Some(message) = contents
                .lines()
                .rev()
                .find(|line| !line.trim().is_empty())
                .map(str::trim)
            else {
                data_quality
                    .missing
                    .push(format!("kmsg fixture {} is empty", path.display()));
                return None;
            };
            data_quality
                .notes
                .push(format!("mock kmsg fixture used: {}", path.display()));
            let severity = if message.to_ascii_lowercase().contains("warn") {
                "warning"
            } else {
                "info"
            };
            Some(KmsgSample {
                severity: severity.to_string(),
                message: message.to_string(),
                source: format!("fixture://{}", path.display()),
            })
        }
        Err(err) => {
            data_quality
                .missing
                .push(format!("kmsg fixture {}: {err}", path.display()));
            None
        }
    }
}

fn evaluate_live_triggers(
    profile: &Profile,
    previous: Option<&LiveSample>,
    current: &LiveSample,
) -> AdcResult<Option<MatchedLiveTrigger>> {
    for rule in &profile.triggers {
        let Some((input, source)) = trigger_input(rule, previous, current) else {
            continue;
        };
        let evaluation = evaluate_trigger(rule, &input)?;
        if evaluation.matched {
            return Ok(Some(MatchedLiveTrigger {
                evaluation,
                input,
                source,
            }));
        }
    }
    Ok(None)
}

fn trigger_input(
    rule: &TriggerRule,
    previous: Option<&LiveSample>,
    current: &LiveSample,
) -> Option<(TriggerInput, String)> {
    if rule.rule_type == RuleType::KmsgPattern {
        let kmsg = current.kmsg.as_ref()?;
        return Some((
            TriggerInput {
                signal: rule
                    .signal
                    .clone()
                    .unwrap_or_else(|| "kmsg.message".to_string()),
                value: None,
                duration_sec: None,
                text: Some(kmsg.message.clone()),
                severity: Some(kmsg.severity.clone()),
            },
            "kmsg".to_string(),
        ));
    }

    let signal = rule.signal.as_deref()?;
    let value = match signal {
        "cpu.total_percent" => {
            let previous = previous?.cpu.as_ref()?;
            let current = current.cpu.as_ref()?;
            CpuSample::usage_percent_between(previous, current)?
        }
        "memory.available_percent" => {
            let memory = current.memory.as_ref()?;
            (memory.mem_available_kb as f64 / memory.mem_total_kb as f64) * 100.0
        }
        "memory.available_delta_kb" => {
            let previous = previous?.memory.as_ref()?;
            let current = current.memory.as_ref()?;
            previous.mem_available_kb as f64 - current.mem_available_kb as f64
        }
        "network.total_delta_bytes" => {
            let previous = previous?.network.as_ref()?;
            let current = current.network.as_ref()?;
            network_total_bytes(current) as f64 - network_total_bytes(previous) as f64
        }
        _ => return None,
    };

    Some((
        TriggerInput {
            signal: signal.to_string(),
            value: Some(value),
            duration_sec: Some(rule.duration_sec.unwrap_or(0)),
            text: None,
            severity: None,
        },
        signal.split('.').next().unwrap_or("unknown").to_string(),
    ))
}

fn network_total_bytes(sample: &NetworkDeviceSample) -> u64 {
    sample
        .interfaces
        .iter()
        .map(|interface| interface.rx_bytes.saturating_add(interface.tx_bytes))
        .sum()
}

fn push_recorder_samples(ring: &mut RecorderRing, sample: &LiveSample) {
    let mut signals = Vec::new();
    if let Some(cpu) = &sample.cpu {
        signals.push(RecorderSignalSample {
            signal_id: "cpu.summary".to_string(),
            value: cpu.total_jiffies as f64,
        });
    }
    if let Some(memory) = &sample.memory {
        let available_percent = if memory.mem_total_kb == 0 {
            0.0
        } else {
            (memory.mem_available_kb as f64 / memory.mem_total_kb as f64) * 100.0
        };
        signals.push(RecorderSignalSample {
            signal_id: "memory.summary".to_string(),
            value: available_percent,
        });
    }
    if let Some(network) = &sample.network {
        signals.push(RecorderSignalSample {
            signal_id: "network.counters".to_string(),
            value: network_total_bytes(network) as f64,
        });
    }
    if sample.kmsg.is_some() {
        signals.push(RecorderSignalSample {
            signal_id: "kmsg.cursor".to_string(),
            value: 1.0,
        });
    }
    if signals.is_empty() {
        return;
    }
    ring.push(RecorderSample {
        time_mono_ns: sample.time_mono_ns,
        signals,
    });
}

fn recorder_expected_signal_ids(profile: &Profile) -> Vec<String> {
    let mut signal_ids = Vec::new();
    for collector in &profile.always_on.collectors {
        match collector.as_str() {
            "cpu" => signal_ids.push("cpu.summary".to_string()),
            "memory" => signal_ids.push("memory.summary".to_string()),
            "network" => signal_ids.push("network.counters".to_string()),
            "kmsg" => signal_ids.push("kmsg.cursor".to_string()),
            "thermal" => signal_ids.push("thermal.zone".to_string()),
            "cpufreq" => signal_ids.push("cpufreq.summary".to_string()),
            "process" => signal_ids.push("process.topN".to_string()),
            _ => {}
        }
    }
    signal_ids.sort();
    signal_ids.dedup();
    signal_ids
}

fn recorder_freeze_budget_exhausted(
    frozen_count: usize,
    budget: &crate::RecorderBudget,
    data_quality: &mut DataQuality,
) -> bool {
    if frozen_count < budget.max_frozen_incidents as usize {
        return false;
    }
    data_quality.throttled = true;
    let note = format!(
        "recorder max_frozen_incidents budget reached: {}",
        budget.max_frozen_incidents
    );
    if !data_quality.notes.iter().any(|existing| existing == &note) {
        data_quality.notes.push(note);
    }
    true
}

fn create_trigger_bundle(
    artifact_root: &Path,
    run_id: &str,
    profile: &Profile,
    sample: &LiveSample,
    matched: &MatchedLiveTrigger,
) -> AdcResult<()> {
    let run_dir = artifact_root.join("runs").join(run_id);
    let raw_dir = run_dir.join("raw");
    fs::create_dir_all(&raw_dir).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to create daemon run directory {}: {err}",
            raw_dir.display()
        ))
    })?;

    write_json(&raw_dir.join("live_sample.json"), sample)?;
    let events = build_live_events(run_id, profile, sample, matched);
    write_jsonl(&run_dir.join("timeline.jsonl"), &events)?;
    let window = build_live_window(run_id, sample, matched, &events);
    write_yaml(&run_dir.join("windows/W001.yaml"), &window)?;
    let raw_refs = live_raw_refs();
    let evidence = build_evidence_index(EvidenceBuildInput {
        run_id: run_id.to_string(),
        target_id: default_target_id(),
        fleet_run_id: None,
        capture_mode: "triggered_capture".to_string(),
        window_id: "W001".to_string(),
        start_mono_ns: sample.time_mono_ns,
        end_mono_ns: sample.time_mono_ns,
        events: events.clone(),
        raw_refs,
        data_quality: aggregate_event_data_quality(&events),
    });
    write_evidence_index(&run_dir.join("evidence_index.yaml"), &evidence)?;
    let budget = OverheadBudget {
        max_artifact_bytes: profile
            .budgets
            .max_artifact_mb_per_run
            .saturating_mul(1024 * 1024),
        ..OverheadBudget::default()
    };
    let overhead_report = build_overhead_report(
        budget,
        OverheadSample {
            artifact_bytes: directory_size_bytes(&run_dir)?,
            event_count: events.len() as u64,
            duration_ms: 0,
        },
    );
    write_json(&run_dir.join("overhead_report.json"), &overhead_report)?;

    let mut manifest = ArtifactManifest::new(run_id, &profile.id);
    manifest.add_file(&run_dir, "raw/live_sample.json", "daemon_live_sample")?;
    manifest.add_file(&run_dir, "timeline.jsonl", "timeline")?;
    manifest.add_file(&run_dir, "windows/W001.yaml", "window")?;
    manifest.add_file(&run_dir, "evidence_index.yaml", "evidence_index")?;
    manifest.add_file(&run_dir, "overhead_report.json", "overhead")?;
    manifest.write_json(run_dir.join("manifest.json"))?;
    Ok(())
}

fn build_live_events(
    run_id: &str,
    profile: &Profile,
    sample: &LiveSample,
    matched: &MatchedLiveTrigger,
) -> Vec<EventEnvelope> {
    let mut events = Vec::new();
    if let Some(cpu) = &sample.cpu {
        events.push(live_event(
            run_id,
            "cpu",
            "sample",
            sample.time_mono_ns,
            profile,
            json!({
                "raw_ref": "artifact://raw/live_sample.json",
                "sample": cpu,
            }),
            sample.data_quality.clone(),
        ));
    }
    if let Some(memory) = &sample.memory {
        events.push(live_event(
            run_id,
            "memory",
            "sample",
            sample.time_mono_ns,
            profile,
            json!({
                "raw_ref": "artifact://raw/live_sample.json",
                "sample": memory,
            }),
            sample.data_quality.clone(),
        ));
    }
    if let Some(network) = &sample.network {
        events.push(live_event(
            run_id,
            "network",
            "sample",
            sample.time_mono_ns,
            profile,
            json!({
                "raw_ref": "artifact://raw/live_sample.json",
                "sample": network,
            }),
            sample.data_quality.clone(),
        ));
    }
    if let Some(kmsg) = &sample.kmsg {
        events.push(live_event(
            run_id,
            "kmsg",
            "sample",
            sample.time_mono_ns,
            profile,
            json!({
                "raw_ref": "artifact://raw/live_sample.json",
                "severity": &kmsg.severity,
                "message": &kmsg.message,
                "source": &kmsg.source,
            }),
            sample.data_quality.clone(),
        ));
    }
    events.push(live_event(
        run_id,
        &matched.source,
        "trigger",
        sample.time_mono_ns,
        profile,
        json!({
            "trigger_name": &matched.evaluation.trigger_name,
            "reason": &matched.evaluation.reason,
            "signal": &matched.input.signal,
            "value": matched.input.value,
            "text": &matched.input.text,
        }),
        matched.evaluation.data_quality.clone(),
    ));
    events
}

fn live_event(
    run_id: &str,
    source: &str,
    event_type: &str,
    time_mono_ns: u64,
    profile: &Profile,
    payload: serde_json::Value,
    data_quality: DataQuality,
) -> EventEnvelope {
    EventEnvelope {
        run_id: run_id.to_string(),
        source: source.to_string(),
        event_type: event_type.to_string(),
        time_mono_ns,
        time_range_ns: TimeRangeNs {
            start: time_mono_ns,
            end: time_mono_ns,
        },
        clock_source: ClockSource::Monotonic,
        collector_id: format!("{source}/daemon"),
        profile_id: profile.id.clone(),
        payload,
        data_quality,
    }
}

fn build_live_window(
    run_id: &str,
    sample: &LiveSample,
    matched: &MatchedLiveTrigger,
    events: &[EventEnvelope],
) -> DaemonWindow {
    DaemonWindow {
        window_id: "W001".to_string(),
        run_id: run_id.to_string(),
        trigger_reason: matched.evaluation.trigger_name.clone(),
        start_mono_ns: sample.time_mono_ns,
        end_mono_ns: sample.time_mono_ns,
        sources: events.iter().map(|event| event.source.clone()).collect(),
        event_count: events.len(),
        data_quality: sample.data_quality.clone(),
    }
}

fn live_raw_refs() -> BTreeMap<String, String> {
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
        "live_sample".to_string(),
        "artifact://raw/live_sample.json".to_string(),
    );
    raw_refs.insert(
        "window".to_string(),
        "artifact://windows/W001.yaml".to_string(),
    );
    raw_refs.insert(
        "overhead".to_string(),
        "artifact://overhead_report.json".to_string(),
    );
    raw_refs.insert(
        "evidence_index".to_string(),
        "artifact://evidence_index.yaml".to_string(),
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

fn next_daemon_run_id() -> String {
    format!("R-DAEMON-{}-{}", unix_epoch_ms(), monotonic_now_ns())
}

fn monotonic_now_ns() -> u64 {
    fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|contents| contents.split_whitespace().next().map(str::to_string))
        .and_then(|seconds| seconds.parse::<f64>().ok())
        .map(|seconds| (seconds * 1_000_000_000.0) as u64)
        .unwrap_or(0)
}

fn sleep_until_next(deadline: Instant, interval: Duration) {
    let now = Instant::now();
    if now >= deadline {
        return;
    }
    let remaining = deadline.saturating_duration_since(now);
    thread::sleep(interval.min(remaining));
}

fn validate_profile_id(profile_id: &str) -> AdcResult<()> {
    if profile_id.trim().is_empty() {
        return Err(AdcError::ProfileValidation(
            "profile id must not be empty".to_string(),
        ));
    }
    if profile_id.contains('/') || profile_id.contains('\\') {
        return Err(AdcError::ProfileValidation(
            "profile id must be a single segment".to_string(),
        ));
    }
    Ok(())
}

fn unix_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}
