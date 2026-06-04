use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    snapshot, AdcError, AdcResult, ArtifactManifest, CounterEvidence, DataQuality, EvidenceIndex,
    EvidenceWindowRef, InformationDebt, NextProbeOption, ObservedFact, SalienceSignal,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompareRunsResult {
    pub before_run_id: String,
    pub after_run_id: String,
    pub before_profile_id: String,
    pub after_profile_id: String,
    pub profile_match: bool,
    pub metric_deltas: BTreeMap<String, MetricDelta>,
    pub raw_refs: BTreeMap<String, String>,
    pub data_quality: DataQuality,
    pub evidence_index: EvidenceIndex,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricDelta {
    pub before: Option<f64>,
    pub after: Option<f64>,
    pub delta: Option<f64>,
    pub unit: String,
}

pub fn compare_runs(
    artifact_root: impl AsRef<Path>,
    before_run_id: &str,
    after_run_id: &str,
) -> AdcResult<CompareRunsResult> {
    validate_segment(before_run_id, "before_run_id")?;
    validate_segment(after_run_id, "after_run_id")?;
    let artifact_root = artifact_root.as_ref();
    let before_manifest =
        ArtifactManifest::read_json(snapshot::manifest_path_for(artifact_root, before_run_id)?)?;
    let after_manifest =
        ArtifactManifest::read_json(snapshot::manifest_path_for(artifact_root, after_run_id)?)?;
    let before_run_dir = artifact_root.join("runs").join(before_run_id);
    let after_run_dir = artifact_root.join("runs").join(after_run_id);

    let mut metric_deltas = BTreeMap::new();
    add_metric_delta(
        &mut metric_deltas,
        "memory.mem_available_kb",
        "KiB",
        read_metric(
            &before_run_dir,
            "raw/memory.json",
            &["sample", "mem_available_kb"],
        )?,
        read_metric(
            &after_run_dir,
            "raw/memory.json",
            &["sample", "mem_available_kb"],
        )?,
    );
    add_metric_delta(
        &mut metric_deltas,
        "memory.mem_free_kb",
        "KiB",
        read_metric(
            &before_run_dir,
            "raw/memory.json",
            &["sample", "mem_free_kb"],
        )?,
        read_metric(
            &after_run_dir,
            "raw/memory.json",
            &["sample", "mem_free_kb"],
        )?,
    );
    add_metric_delta(
        &mut metric_deltas,
        "cpu.total_jiffies",
        "jiffies",
        read_metric(
            &before_run_dir,
            "raw/cpu.json",
            &["sample", "total_jiffies"],
        )?,
        read_metric(&after_run_dir, "raw/cpu.json", &["sample", "total_jiffies"])?,
    );

    let profile_match = before_manifest.profile_id == after_manifest.profile_id;
    let mut data_quality = DataQuality {
        clock_confidence: crate::ClockConfidence::Medium,
        ..Default::default()
    };
    if profile_match {
        data_quality.notes.push("profile ids match".to_string());
    } else {
        data_quality.notes.push(format!(
            "profile mismatch: {} vs {}",
            before_manifest.profile_id, after_manifest.profile_id
        ));
    }

    let mut raw_refs = BTreeMap::new();
    raw_refs.insert(
        "before_manifest".to_string(),
        format!("artifact://runs/{before_run_id}/manifest.json"),
    );
    raw_refs.insert(
        "after_manifest".to_string(),
        format!("artifact://runs/{after_run_id}/manifest.json"),
    );
    raw_refs.insert(
        "before_timeline".to_string(),
        format!("artifact://runs/{before_run_id}/timeline.jsonl"),
    );
    raw_refs.insert(
        "after_timeline".to_string(),
        format!("artifact://runs/{after_run_id}/timeline.jsonl"),
    );

    let evidence_index = build_compare_evidence(CompareEvidenceInput {
        before_run_id,
        after_run_id,
        before_profile_id: &before_manifest.profile_id,
        after_profile_id: &after_manifest.profile_id,
        profile_match,
        metric_deltas: &metric_deltas,
        raw_refs: &raw_refs,
        data_quality: data_quality.clone(),
    });

    Ok(CompareRunsResult {
        before_run_id: before_run_id.to_string(),
        after_run_id: after_run_id.to_string(),
        before_profile_id: before_manifest.profile_id,
        after_profile_id: after_manifest.profile_id,
        profile_match,
        metric_deltas,
        raw_refs,
        data_quality,
        evidence_index,
    })
}

struct CompareEvidenceInput<'a> {
    before_run_id: &'a str,
    after_run_id: &'a str,
    before_profile_id: &'a str,
    after_profile_id: &'a str,
    profile_match: bool,
    metric_deltas: &'a BTreeMap<String, MetricDelta>,
    raw_refs: &'a BTreeMap<String, String>,
    data_quality: DataQuality,
}

fn build_compare_evidence(input: CompareEvidenceInput<'_>) -> EvidenceIndex {
    let mut observed_facts = Vec::new();
    for (index, (metric, delta)) in input.metric_deltas.iter().enumerate() {
        let mut attributes = BTreeMap::new();
        attributes.insert(
            "before".to_string(),
            delta.before.map(Value::from).unwrap_or(Value::Null),
        );
        attributes.insert(
            "after".to_string(),
            delta.after.map(Value::from).unwrap_or(Value::Null),
        );
        attributes.insert(
            "delta".to_string(),
            delta.delta.map(Value::from).unwrap_or(Value::Null),
        );
        attributes.insert("unit".to_string(), Value::from(delta.unit.clone()));
        observed_facts.push(ObservedFact {
            fact_id: format!("CF{:03}", index + 1),
            source: "compare".to_string(),
            window_id: "compare".to_string(),
            time_mono_ns: 0,
            statement: format!(
                "metric {metric} changed from {:?} to {:?} {}",
                delta.before, delta.after, delta.unit
            ),
            raw_ref: input
                .raw_refs
                .get("before_manifest")
                .cloned()
                .unwrap_or_else(|| "artifact://manifest.json".to_string()),
            attributes,
            data_quality: input.data_quality.clone(),
        });
    }

    let salience_map = input
        .metric_deltas
        .iter()
        .enumerate()
        .map(|(index, (metric, delta))| SalienceSignal {
            signal_id: format!("CS{:03}", index + 1),
            source: "compare".to_string(),
            window_id: "compare".to_string(),
            score: delta.delta.map(f64::abs).unwrap_or(0.0),
            calculation: format!("abs(after - before) for metric {metric}"),
            raw_ref: input
                .raw_refs
                .get("after_manifest")
                .cloned()
                .unwrap_or_else(|| "artifact://manifest.json".to_string()),
            data_quality: input.data_quality.clone(),
        })
        .collect();

    let mut counter_evidence = Vec::new();
    if input.profile_match {
        counter_evidence.push(CounterEvidence {
            item_id: "CC001".to_string(),
            source: "compare".to_string(),
            statement: "Profile ids matched for the compared runs".to_string(),
            raw_ref: input.raw_refs.get("before_manifest").cloned(),
            data_quality: input.data_quality.clone(),
        });
    }

    let information_debt = if input.profile_match {
        Vec::new()
    } else {
        vec![InformationDebt {
            debt_id: "CD001".to_string(),
            kind: "profile_mismatch".to_string(),
            description: format!(
                "profile mismatch: {} vs {}",
                input.before_profile_id, input.after_profile_id
            ),
            impact: "Agent should compare deltas with profile mismatch in mind".to_string(),
            data_quality: input.data_quality.clone(),
        }]
    };

    EvidenceIndex {
        schema_version: "obs.v2".to_string(),
        run_id: format!("{}__{}", input.before_run_id, input.after_run_id),
        target_id: "local".to_string(),
        fleet_run_id: None,
        capture_mode: "compare".to_string(),
        clock_basis: "CLOCK_MONOTONIC".to_string(),
        primary_window: EvidenceWindowRef {
            window_id: "compare".to_string(),
            start_mono_ns: 0,
            end_mono_ns: 0,
            event_count: input.metric_deltas.len(),
        },
        observed_facts,
        salience_map,
        counter_evidence,
        information_debt,
        next_probe_options: vec![NextProbeOption {
            probe_id: "capture_before_after_window".to_string(),
            label: "Before/after bounded capture".to_string(),
            reason: "Collects aligned time-series evidence for both states".to_string(),
            required_privilege: "none".to_string(),
            estimated_cost: "medium".to_string(),
            expected_evidence: vec!["aligned metric series".to_string()],
            profile_hint: "compare_capture".to_string(),
        }],
        raw_refs: input.raw_refs.clone(),
        data_quality: input.data_quality,
    }
}

fn add_metric_delta(
    metric_deltas: &mut BTreeMap<String, MetricDelta>,
    name: &str,
    unit: &str,
    before: Option<f64>,
    after: Option<f64>,
) {
    let delta = before.zip(after).map(|(before, after)| after - before);
    metric_deltas.insert(
        name.to_string(),
        MetricDelta {
            before,
            after,
            delta,
            unit: unit.to_string(),
        },
    );
}

fn read_metric(run_dir: &Path, relative_path: &str, path: &[&str]) -> AdcResult<Option<f64>> {
    let artifact_path = run_dir.join(relative_path);
    let bytes = match fs::read(&artifact_path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(AdcError::Artifact(format!(
                "failed to read {}: {err}",
                artifact_path.display()
            )))
        }
    };
    let root: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|err| AdcError::Artifact(format!("metric json parse failed: {err}")))?;
    let mut value = &root;
    for key in path {
        value = match value.get(*key) {
            Some(value) => value,
            None => return Ok(None),
        };
    }
    Ok(value.as_f64())
}

fn validate_segment(value: &str, label: &str) -> AdcResult<()> {
    if value.trim().is_empty() {
        return Err(AdcError::Artifact(format!("{label} must not be empty")));
    }
    let path = Path::new(value);
    let mut components = path.components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(AdcError::Artifact(format!(
            "{label} must be a single relative path segment"
        ))),
    }
}
