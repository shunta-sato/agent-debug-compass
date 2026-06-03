use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{search_events, AdcError, AdcResult, DataQuality, EventEnvelope, SearchEventsQuery};

const SCHEMA_VERSION: &str = "obs.v2";
const DEFAULT_TARGET_ID: &str = "local";
const MAX_RAW_SLICE_LINES: usize = 1_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceIndex {
    pub schema_version: String,
    pub run_id: String,
    pub target_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fleet_run_id: Option<String>,
    pub capture_mode: String,
    pub clock_basis: String,
    pub primary_window: EvidenceWindowRef,
    pub observed_facts: Vec<ObservedFact>,
    pub salience_map: Vec<SalienceSignal>,
    pub counter_evidence: Vec<CounterEvidence>,
    pub information_debt: Vec<InformationDebt>,
    pub next_probe_options: Vec<NextProbeOption>,
    pub raw_refs: BTreeMap<String, String>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceWindowRef {
    pub window_id: String,
    pub start_mono_ns: u64,
    pub end_mono_ns: u64,
    pub event_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservedFact {
    pub fact_id: String,
    pub source: String,
    pub window_id: String,
    pub time_mono_ns: u64,
    pub statement: String,
    pub raw_ref: String,
    #[serde(default)]
    pub attributes: BTreeMap<String, Value>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SalienceSignal {
    pub signal_id: String,
    pub source: String,
    pub window_id: String,
    pub score: f64,
    pub calculation: String,
    pub raw_ref: String,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CounterEvidence {
    pub item_id: String,
    pub source: String,
    pub statement: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_ref: Option<String>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InformationDebt {
    pub debt_id: String,
    pub kind: String,
    pub description: String,
    pub impact: String,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextProbeOption {
    pub probe_id: String,
    pub label: String,
    pub reason: String,
    pub required_privilege: String,
    pub estimated_cost: String,
    pub expected_evidence: Vec<String>,
    pub profile_hint: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawSlice {
    pub run_id: String,
    pub raw_ref: String,
    pub returned_lines: usize,
    pub total_lines: usize,
    pub truncated: bool,
    pub lines: Vec<String>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignalSeries {
    pub run_id: String,
    pub source: String,
    pub returned_count: usize,
    pub events: Vec<EventEnvelope>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone)]
pub struct EvidenceBuildInput {
    pub run_id: String,
    pub target_id: String,
    pub fleet_run_id: Option<String>,
    pub capture_mode: String,
    pub window_id: String,
    pub start_mono_ns: u64,
    pub end_mono_ns: u64,
    pub events: Vec<EventEnvelope>,
    pub raw_refs: BTreeMap<String, String>,
    pub data_quality: DataQuality,
}

pub fn default_target_id() -> String {
    DEFAULT_TARGET_ID.to_string()
}

pub fn build_evidence_index(input: EvidenceBuildInput) -> EvidenceIndex {
    let observed_facts = build_observed_facts(&input);
    let salience_map = build_salience_map(&input);
    let counter_evidence = build_counter_evidence(&input);
    let information_debt = build_information_debt(&input.data_quality);
    let next_probe_options = build_next_probe_options(&input.data_quality);

    EvidenceIndex {
        schema_version: SCHEMA_VERSION.to_string(),
        run_id: input.run_id,
        target_id: input.target_id,
        fleet_run_id: input.fleet_run_id,
        capture_mode: input.capture_mode,
        clock_basis: "CLOCK_MONOTONIC".to_string(),
        primary_window: EvidenceWindowRef {
            window_id: input.window_id,
            start_mono_ns: input.start_mono_ns,
            end_mono_ns: input.end_mono_ns,
            event_count: input.events.len(),
        },
        observed_facts,
        salience_map,
        counter_evidence,
        information_debt,
        next_probe_options,
        raw_refs: input.raw_refs,
        data_quality: input.data_quality,
    }
}

pub fn write_evidence_index(path: &Path, evidence: &EvidenceIndex) -> AdcResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AdcError::Artifact(format!("failed to create {}: {err}", parent.display()))
        })?;
    }
    let bytes = yaml_serde::to_string(evidence)
        .map_err(|err| AdcError::Artifact(format!("evidence yaml serialization failed: {err}")))?;
    fs::write(path, bytes)
        .map_err(|err| AdcError::Artifact(format!("failed to write {}: {err}", path.display())))
}

pub fn read_evidence_index(
    artifact_root: impl AsRef<Path>,
    run_id: &str,
) -> AdcResult<EvidenceIndex> {
    validate_segment(run_id, "run_id")?;
    let path = artifact_root
        .as_ref()
        .join("runs")
        .join(run_id)
        .join("evidence_index.yaml");
    let contents = fs::read_to_string(&path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read evidence index {}: {err}",
            path.display()
        ))
    })?;
    yaml_serde::from_str(&contents)
        .map_err(|err| AdcError::Artifact(format!("evidence yaml parse failed: {err}")))
}

pub fn read_evidence_index_text(
    artifact_root: impl AsRef<Path>,
    run_id: &str,
) -> AdcResult<String> {
    validate_segment(run_id, "run_id")?;
    let path = artifact_root
        .as_ref()
        .join("runs")
        .join(run_id)
        .join("evidence_index.yaml");
    fs::read_to_string(&path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read evidence index {}: {err}",
            path.display()
        ))
    })
}

pub fn validate_cause_neutral(evidence: &EvidenceIndex) -> AdcResult<()> {
    let rendered = serde_json::to_string(evidence)
        .map_err(|err| AdcError::Artifact(format!("evidence serialization failed: {err}")))?;
    let lower = rendered.to_ascii_lowercase();
    for banned in [
        "root cause",
        "root_cause",
        "likely cause",
        "cause candidate",
        "root_cause_candidate",
        "suspect",
        "culprit",
        "caused by",
        "原因候補",
        "原因断定",
    ] {
        if lower.contains(banned) {
            return Err(AdcError::Artifact(format!(
                "evidence_index contains cause-inference wording: {banned}"
            )));
        }
    }
    Ok(())
}

pub fn aggregate_event_data_quality(events: &[EventEnvelope]) -> DataQuality {
    let mut quality = DataQuality {
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };
    for event in events {
        merge_data_quality(&mut quality, &event.data_quality);
    }
    quality
}

pub fn read_raw_slice(
    artifact_root: impl AsRef<Path>,
    run_id: &str,
    raw_ref: &str,
    limit: usize,
) -> AdcResult<RawSlice> {
    validate_segment(run_id, "run_id")?;
    let relative_path = raw_ref_to_relative_path(raw_ref)?;
    let path = artifact_root
        .as_ref()
        .join("runs")
        .join(run_id)
        .join(&relative_path);
    let contents = fs::read_to_string(&path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read raw slice {}: {err}",
            path.display()
        ))
    })?;
    let max_lines = limit.clamp(1, MAX_RAW_SLICE_LINES);
    let all_lines = contents.lines().map(str::to_string).collect::<Vec<_>>();
    let lines = all_lines
        .iter()
        .take(max_lines)
        .cloned()
        .collect::<Vec<_>>();
    let truncated = all_lines.len() > lines.len();
    let mut data_quality = DataQuality {
        truncated,
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };
    if truncated {
        data_quality.notes.push(format!(
            "raw slice returned {} of {} lines",
            lines.len(),
            all_lines.len()
        ));
    }
    add_text_quality_groups(&mut data_quality, &all_lines);

    Ok(RawSlice {
        run_id: run_id.to_string(),
        raw_ref: raw_ref.to_string(),
        returned_lines: lines.len(),
        total_lines: all_lines.len(),
        truncated,
        lines,
        data_quality,
    })
}

fn add_text_quality_groups(data_quality: &mut DataQuality, lines: &[String]) {
    let permission_denied = lines
        .iter()
        .filter(|line| line.to_ascii_lowercase().contains("permission denied"))
        .count();
    if permission_denied > 0 {
        let sample = lines
            .iter()
            .find(|line| line.to_ascii_lowercase().contains("permission denied"))
            .map(|line| line.trim().to_string())
            .unwrap_or_else(|| "permission denied".to_string());
        data_quality.notes.push(format!(
            "permission_denied repeated {permission_denied} line(s)"
        ));
        data_quality
            .missing
            .push(format!("permission_denied: {sample}"));
    }
}

pub fn signal_series_for(
    artifact_root: impl AsRef<Path>,
    run_id: &str,
    source: &str,
    limit: usize,
) -> AdcResult<SignalSeries> {
    validate_segment(source, "source")?;
    let result = search_events(
        artifact_root,
        run_id,
        &SearchEventsQuery {
            source: Some(source.to_string()),
            event_type: None,
            contains: None,
            limit,
        },
    )?;
    Ok(SignalSeries {
        run_id: result.run_id,
        source: source.to_string(),
        returned_count: result.returned_count,
        events: result.events,
        data_quality: result.data_quality,
    })
}

fn build_observed_facts(input: &EvidenceBuildInput) -> Vec<ObservedFact> {
    let grouped = group_events_by_source(&input.events);
    grouped
        .into_iter()
        .enumerate()
        .map(|(index, (source, events))| {
            let first = events[0];
            let raw_ref = raw_ref_for_source(&source, &first.payload, &input.raw_refs);
            let mut attributes = BTreeMap::new();
            attributes.insert("event_count".to_string(), Value::from(events.len() as u64));
            let trigger_event = events
                .iter()
                .find(|event| event.payload.get("trigger_name").is_some());
            let statement = if let Some(trigger_event) = trigger_event {
                let trigger_name = trigger_event
                    .payload
                    .get("trigger_name")
                    .and_then(Value::as_str)
                    .unwrap_or("unnamed_trigger");
                attributes.insert(
                    "trigger_name".to_string(),
                    Value::from(trigger_name.to_string()),
                );
                if let Some(reason) = trigger_event.payload.get("reason").and_then(Value::as_str) {
                    attributes.insert(
                        "trigger_reason".to_string(),
                        Value::from(reason.to_string()),
                    );
                }
                format!(
                    "trigger {trigger_name} was observed from source {source} in window {}",
                    input.window_id
                )
            } else {
                format!(
                    "source {source} produced {} observation event(s) in window {}",
                    events.len(),
                    input.window_id
                )
            };
            ObservedFact {
                fact_id: format!("F{:03}", index + 1),
                source: source.clone(),
                window_id: input.window_id.clone(),
                time_mono_ns: first.time_mono_ns,
                statement,
                raw_ref,
                attributes,
                data_quality: first.data_quality.clone(),
            }
        })
        .collect()
}

fn build_salience_map(input: &EvidenceBuildInput) -> Vec<SalienceSignal> {
    let grouped = group_events_by_source(&input.events);
    grouped
        .into_iter()
        .enumerate()
        .map(|(index, (source, events))| {
            let first = events[0];
            SalienceSignal {
                signal_id: format!("S{:03}", index + 1),
                source: source.clone(),
                window_id: input.window_id.clone(),
                score: events.len() as f64,
                calculation: format!("event_count(source == {source}) within {}", input.window_id),
                raw_ref: raw_ref_for_source(&source, &first.payload, &input.raw_refs),
                data_quality: first.data_quality.clone(),
            }
        })
        .collect()
}

fn build_counter_evidence(input: &EvidenceBuildInput) -> Vec<CounterEvidence> {
    let manifest_ref = input.raw_refs.get("manifest").cloned();
    let mut items = Vec::new();
    if !input.data_quality.dropped {
        items.push(CounterEvidence {
            item_id: "C001".to_string(),
            source: "data_quality".to_string(),
            statement: "No dropped samples were reported in the observed window".to_string(),
            raw_ref: manifest_ref.clone(),
            data_quality: input.data_quality.clone(),
        });
    }
    if !input.data_quality.throttled {
        items.push(CounterEvidence {
            item_id: "C002".to_string(),
            source: "data_quality".to_string(),
            statement: "No collector throttling was reported in the observed window".to_string(),
            raw_ref: manifest_ref,
            data_quality: input.data_quality.clone(),
        });
    }
    items
}

fn build_information_debt(data_quality: &DataQuality) -> Vec<InformationDebt> {
    let mut debts = Vec::new();
    for (index, missing) in data_quality.missing.iter().enumerate() {
        debts.push(InformationDebt {
            debt_id: format!("D{:03}", index + 1),
            kind: "missing".to_string(),
            description: missing.clone(),
            impact: "Agent should treat related observations as unavailable rather than normal"
                .to_string(),
            data_quality: data_quality.clone(),
        });
    }
    if data_quality.truncated {
        debts.push(InformationDebt {
            debt_id: format!("D{:03}", debts.len() + 1),
            kind: "truncated".to_string(),
            description: "One or more bounded outputs were truncated".to_string(),
            impact: "Agent may need an explicit raw slice request for deeper inspection"
                .to_string(),
            data_quality: data_quality.clone(),
        });
    }
    if data_quality.throttled {
        debts.push(InformationDebt {
            debt_id: format!("D{:03}", debts.len() + 1),
            kind: "throttled".to_string(),
            description: "Capture was degraded by an overhead budget".to_string(),
            impact: "Agent should prefer lower-cost probes or shorter capture windows".to_string(),
            data_quality: data_quality.clone(),
        });
    }
    if data_quality.dropped || data_quality.drop_count > 0 {
        debts.push(InformationDebt {
            debt_id: format!("D{:03}", debts.len() + 1),
            kind: "dropped".to_string(),
            description: format!(
                "{} event(s) or sample(s) were dropped",
                data_quality.drop_count
            ),
            impact: "Agent should avoid treating missing samples as stable behavior".to_string(),
            data_quality: data_quality.clone(),
        });
    }
    debts
}

fn build_next_probe_options(data_quality: &DataQuality) -> Vec<NextProbeOption> {
    let mut probes = vec![
        NextProbeOption {
            probe_id: "capture_process_snapshot".to_string(),
            label: "Process snapshot".to_string(),
            reason: "Adds process-level context for observed resource changes".to_string(),
            required_privilege: "none".to_string(),
            estimated_cost: "low".to_string(),
            expected_evidence: vec!["process list".to_string(), "per-process memory".to_string()],
            profile_hint: "process_snapshot".to_string(),
        },
        NextProbeOption {
            probe_id: "increase_capture_resolution".to_string(),
            label: "Higher-resolution bounded capture".to_string(),
            reason: "Collects denser samples around the same observation window".to_string(),
            required_privilege: "none".to_string(),
            estimated_cost: "medium".to_string(),
            expected_evidence: vec!["shorter interval series".to_string()],
            profile_hint: "capture_100ms".to_string(),
        },
    ];

    let missing_text = data_quality.missing.join("\n").to_ascii_lowercase();
    if missing_text.contains("perf") {
        probes.push(NextProbeOption {
            probe_id: "enable_privileged_perf_short".to_string(),
            label: "Short perf capture".to_string(),
            reason: "Adds hardware/software counter evidence when perf access is available"
                .to_string(),
            required_privilege: "optional-root-helper".to_string(),
            estimated_cost: "medium".to_string(),
            expected_evidence: vec!["perf stat counters".to_string()],
            profile_hint: "perf_short".to_string(),
        });
    }
    if missing_text.contains("kmsg") || missing_text.contains("dmesg") {
        probes.push(NextProbeOption {
            probe_id: "enable_kmsg_window".to_string(),
            label: "Kernel message window".to_string(),
            reason: "Adds bounded kernel message observations for the same time window".to_string(),
            required_privilege: "optional-root-helper".to_string(),
            estimated_cost: "low".to_string(),
            expected_evidence: vec!["kmsg warnings".to_string(), "driver messages".to_string()],
            profile_hint: "kmsg_window".to_string(),
        });
    }
    probes
}

fn group_events_by_source(events: &[EventEnvelope]) -> Vec<(String, Vec<&EventEnvelope>)> {
    let sources = events
        .iter()
        .map(|event| event.source.clone())
        .collect::<BTreeSet<_>>();
    sources
        .into_iter()
        .map(|source| {
            let items = events
                .iter()
                .filter(|event| event.source == source)
                .collect::<Vec<_>>();
            (source, items)
        })
        .filter(|(_, items)| !items.is_empty())
        .collect()
}

fn raw_ref_for_source(
    source: &str,
    payload: &Value,
    raw_refs: &BTreeMap<String, String>,
) -> String {
    payload
        .get("raw_ref")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| raw_refs.get(source).cloned())
        .unwrap_or_else(|| "artifact://timeline.jsonl".to_string())
}

fn merge_data_quality(target: &mut DataQuality, source: &DataQuality) {
    target.dropped |= source.dropped;
    target.drop_count = target.drop_count.saturating_add(source.drop_count);
    target.throttled |= source.throttled;
    target.truncated |= source.truncated;
    extend_unique(&mut target.missing, &source.missing);
    extend_unique(&mut target.notes, &source.notes);
}

fn extend_unique(target: &mut Vec<String>, source: &[String]) {
    for value in source {
        if !target.contains(value) {
            target.push(value.clone());
        }
    }
}

fn raw_ref_to_relative_path(raw_ref: &str) -> AdcResult<PathBuf> {
    let suffix = raw_ref
        .strip_prefix("artifact://")
        .ok_or_else(|| AdcError::Artifact("raw_ref must start with artifact://".to_string()))?;
    if !suffix.starts_with("raw/") {
        return Err(AdcError::Artifact(
            "raw_ref must point under artifact://raw/".to_string(),
        ));
    }
    let path = Path::new(suffix);
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => clean.push(segment),
            _ => {
                return Err(AdcError::Artifact(
                    "raw_ref must be a relative raw artifact path".to_string(),
                ))
            }
        }
    }
    Ok(clean)
}

fn validate_segment(value: &str, label: &str) -> AdcResult<()> {
    if value.trim().is_empty() || value.contains('/') || value.contains('\\') {
        return Err(AdcError::Artifact(format!(
            "{label} must be a single relative path segment"
        )));
    }
    Ok(())
}
