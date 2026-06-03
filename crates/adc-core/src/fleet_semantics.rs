use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{DataQuality, FleetServiceInvestigationResult, FleetServiceInvestigationTarget};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetSemanticDiff {
    pub schema_version: String,
    pub fleet_run_id: String,
    pub service_name: String,
    pub target_count: usize,
    pub diff_groups: Vec<FleetSemanticDiffGroup>,
    pub field_diffs: Vec<SemanticFieldDiff>,
    pub raw_refs: BTreeMap<String, String>,
    pub data_quality: DataQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetSemanticDiffGroup {
    pub group_id: String,
    pub status: String,
    pub targets: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticFieldDiff {
    pub field: String,
    pub status: String,
    pub values_by_target: BTreeMap<String, Value>,
    pub data_quality_by_target: BTreeMap<String, DataQuality>,
    pub quality_class_by_target: BTreeMap<String, String>,
    pub raw_refs_by_target: BTreeMap<String, String>,
}

pub fn build_fleet_semantic_diff(
    service_result: &FleetServiceInvestigationResult,
) -> FleetSemanticDiff {
    let mut data_quality = service_result.data_quality.clone();
    let mut groups = BTreeMap::<String, Vec<String>>::new();
    for target in &service_result.targets {
        let key = if target.status != "captured" {
            quality_class(target)
        } else if let Some(pack) = &target.service_pack {
            format!(
                "availability:{}:active:{}:sub:{}",
                pack.service_state.availability,
                pack.service_state.active_state,
                pack.service_state.sub_state
            )
        } else {
            "missing_service_pack".to_string()
        };
        groups
            .entry(key)
            .or_default()
            .push(target.target_id.clone());
        merge_data_quality(&mut data_quality, &target.data_quality);
        if let Some(pack) = &target.service_pack {
            merge_data_quality(&mut data_quality, &pack.data_quality);
        }
    }
    let diff_groups = groups
        .into_iter()
        .enumerate()
        .map(|(index, (status, targets))| FleetSemanticDiffGroup {
            group_id: format!("FSD{:03}", index + 1),
            summary: format!("{} target(s) share {status}", targets.len()),
            status,
            targets,
        })
        .collect::<Vec<_>>();
    let field_diffs = [
        "target.status",
        "service.availability",
        "service.active_state",
        "service.sub_state",
        "process.comm",
        "process.pid_present",
        "port.availability",
        "port.socket_inode_count",
        "journal.returned_lead_count",
        "journal.severity_buckets",
        "data_quality.missing_count",
        "data_quality.class",
    ]
    .iter()
    .map(|field| semantic_field_diff(service_result, field))
    .collect::<Vec<_>>();
    let mut raw_refs = service_result.raw_refs.clone();
    raw_refs.insert(
        "fleet_semantic_diff".to_string(),
        format!(
            "artifact://fleet_runs/{}/fleet_semantic_diff.json",
            service_result.fleet_run_id
        ),
    );
    FleetSemanticDiff {
        schema_version: "obs.fleet_semantic_diff.v1".to_string(),
        fleet_run_id: service_result.fleet_run_id.clone(),
        service_name: service_result.service_name.clone(),
        target_count: service_result.target_count,
        diff_groups,
        field_diffs,
        raw_refs,
        data_quality,
    }
}

fn semantic_field_diff(
    service_result: &FleetServiceInvestigationResult,
    field: &str,
) -> SemanticFieldDiff {
    let mut values_by_target = BTreeMap::new();
    let mut data_quality_by_target = BTreeMap::new();
    let mut quality_class_by_target = BTreeMap::new();
    let mut raw_refs_by_target = BTreeMap::new();
    for target in &service_result.targets {
        values_by_target.insert(
            target.target_id.clone(),
            semantic_field_value(target, field),
        );
        data_quality_by_target.insert(target.target_id.clone(), target.data_quality.clone());
        quality_class_by_target.insert(target.target_id.clone(), quality_class(target));
        if let Some(raw_ref) = service_result
            .raw_refs
            .get(&format!("{}.service_investigation", target.target_id))
        {
            raw_refs_by_target.insert(target.target_id.clone(), raw_ref.clone());
        }
    }
    let distinct_values = values_by_target
        .values()
        .map(|value| value.to_string())
        .collect::<BTreeSet<_>>();
    let quality_classes = quality_class_by_target
        .values()
        .cloned()
        .collect::<BTreeSet<_>>();
    let status = if quality_classes.iter().any(|class| class != "ok") {
        "partial"
    } else if distinct_values.len() <= 1 {
        "same"
    } else {
        "different"
    };
    SemanticFieldDiff {
        field: field.to_string(),
        status: status.to_string(),
        values_by_target,
        data_quality_by_target,
        quality_class_by_target,
        raw_refs_by_target,
    }
}

fn semantic_field_value(target: &FleetServiceInvestigationTarget, field: &str) -> Value {
    let Some(pack) = &target.service_pack else {
        return match field {
            "target.status" => json!(target.status),
            "data_quality.class" => json!(quality_class(target)),
            _ => json!({
                "status": target.status,
                "quality_class": quality_class(target),
                "missing": target.data_quality.missing,
            }),
        };
    };
    match field {
        "target.status" => json!(target.status),
        "service.availability" => json!(pack.service_state.availability),
        "service.active_state" => json!(pack.service_state.active_state),
        "service.sub_state" => json!(pack.service_state.sub_state),
        "process.comm" => json!(pack.process_summary.comm),
        "process.pid_present" => json!(pack.process_summary.pid.is_some()),
        "port.availability" => json!(pack.port_summary.availability),
        "port.socket_inode_count" => json!(pack.port_summary.socket_inode_count),
        "journal.returned_lead_count" => json!(pack.journal_summary.returned_lead_count),
        "journal.severity_buckets" => json!(journal_severity_buckets(target)),
        "data_quality.missing_count" => json!(pack.data_quality.missing.len()),
        "data_quality.class" => json!(quality_class(target)),
        _ => Value::Null,
    }
}

fn journal_severity_buckets(target: &FleetServiceInvestigationTarget) -> BTreeMap<String, usize> {
    let mut buckets = BTreeMap::new();
    let Some(pack) = &target.service_pack else {
        return buckets;
    };
    for lead in &pack.journal_leads {
        *buckets.entry(lead.severity_hint.clone()).or_default() += 1;
    }
    buckets
}

fn quality_class(target: &FleetServiceInvestigationTarget) -> String {
    let combined_missing = target
        .service_pack
        .as_ref()
        .map(|pack| {
            pack.data_quality
                .missing
                .iter()
                .chain(target.data_quality.missing.iter())
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| target.data_quality.missing.clone());
    let missing_text = combined_missing.join(" ").to_ascii_lowercase();
    let status = target.status.to_ascii_lowercase();
    if status.contains("unreachable") {
        "unreachable".to_string()
    } else if status.contains("permission") || missing_text.contains("permission") {
        "permission_denied".to_string()
    } else if status != "captured" {
        "collector_failed".to_string()
    } else if target.service_pack.is_none()
        || missing_text.contains("missing_service_pack")
        || !combined_missing.is_empty()
    {
        "missing".to_string()
    } else {
        "ok".to_string()
    }
}

fn merge_data_quality(target: &mut DataQuality, source: &DataQuality) {
    target.dropped |= source.dropped;
    target.throttled |= source.throttled;
    target.truncated |= source.truncated;
    target.drop_count += source.drop_count;
    for missing in &source.missing {
        if !target.missing.contains(missing) {
            target.missing.push(missing.clone());
        }
    }
    for note in &source.notes {
        if !target.notes.contains(note) {
            target.notes.push(note.clone());
        }
    }
    if target.clock_confidence.is_empty() {
        target.clock_confidence = source.clock_confidence.clone();
    }
}
