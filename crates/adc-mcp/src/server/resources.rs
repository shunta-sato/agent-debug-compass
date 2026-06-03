use rmcp::{
    model::{
        AnnotateAble, RawResource, RawResourceTemplate, ReadResourceResult, Resource,
        ResourceContents, ResourceTemplate,
    },
    ErrorData,
};
use serde_json::json;

use super::{to_internal_error, to_mcp_error, ServerMode};

pub(super) fn resources() -> Vec<Resource> {
    vec![
        resource("obs://runs", "runs", "List known run ids."),
        resource(
            "obs://capabilities",
            "capabilities",
            "Current capability map or documented capability gaps.",
        ),
    ]
}

fn resource(uri: &str, name: &str, description: &str) -> Resource {
    let mut raw = RawResource::new(uri, name);
    raw.description = Some(description.to_string());
    raw.mime_type = Some("application/json".to_string());
    raw.no_annotation()
}

pub(super) fn resource_templates(mode: ServerMode) -> Vec<ResourceTemplate> {
    [
        (
            "obs://runs/{run_id}/evidence",
            "run-evidence-index",
            "Bounded v2 evidence index.",
            "text/yaml",
        ),
        (
            "obs://runs/{run_id}/timeline",
            "run-evidence-timeline",
            "Bounded timeline search result, not raw JSONL.",
            "application/json",
        ),
        (
            "obs://runs/{run_id}/windows/{window_id}",
            "run-evidence-window",
            "Bounded evidence window.",
            "text/yaml",
        ),
        (
            "obs://fleet/{fleet_run_id}/evidence",
            "fleet-evidence",
            "Bounded fleet evidence index.",
            "text/yaml",
        ),
        (
            "obs://compare/{before_run_id}/{after_run_id}",
            "compare-runs",
            "Bounded before/after metric delta result.",
            "application/json",
        ),
    ]
    .into_iter()
    .filter(|(uri_template, _, _, _)| mode.allows_resource_uri(uri_template))
    .map(|(uri_template, name, description, mime_type)| {
        RawResourceTemplate {
            uri_template: uri_template.to_string(),
            name: name.to_string(),
            title: None,
            description: Some(description.to_string()),
            mime_type: Some(mime_type.to_string()),
        }
        .no_annotation()
    })
    .collect()
}

pub(super) fn read_resource_sync(
    artifact_root: &std::path::Path,
    uri: &str,
) -> Result<ReadResourceResult, ErrorData> {
    let contents = if uri == "obs://runs" {
        let runs = adc_core::snapshot::list_runs(artifact_root).map_err(to_mcp_error)?;
        resource_json(uri, json!({ "runs": runs }))
    } else if uri == "obs://capabilities" {
        resource_json(
            uri,
            serde_json::to_value(
                adc_core::detect_default_kernel_capabilities().map_err(to_mcp_error)?,
            )
            .map_err(to_internal_error)?,
        )
    } else if let Some(fleet_run_id) = uri
        .strip_prefix("obs://fleet/")
        .and_then(|rest| rest.strip_suffix("/evidence"))
    {
        ResourceContents::text(
            adc_core::read_fleet_evidence_text(artifact_root, fleet_run_id)
                .map_err(to_mcp_error)?,
            uri,
        )
    } else if let Some(rest) = uri.strip_prefix("obs://compare/") {
        let (before_run_id, after_run_id) = rest.split_once('/').ok_or_else(|| {
            ErrorData::invalid_params(format!("invalid compare resource: {uri}"), None)
        })?;
        resource_json(
            uri,
            serde_json::to_value(
                adc_core::compare_runs(artifact_root, before_run_id, after_run_id)
                    .map_err(to_mcp_error)?,
            )
            .map_err(to_internal_error)?,
        )
    } else if let Some((run_id, suffix)) = uri
        .strip_prefix("obs://runs/")
        .and_then(|rest| rest.split_once('/'))
    {
        match suffix {
            "evidence" => ResourceContents::text(
                adc_core::read_evidence_index_text(artifact_root, run_id).map_err(to_mcp_error)?,
                uri,
            ),
            "timeline" => ResourceContents::text(
                adc_core::read_timeline_bounded(artifact_root, run_id, 50).map_err(to_mcp_error)?,
                uri,
            ),
            suffix if suffix.starts_with("windows/") => {
                let window_id = suffix.trim_start_matches("windows/");
                ResourceContents::text(
                    adc_core::snapshot::read_window(artifact_root, run_id, window_id)
                        .map_err(to_mcp_error)?,
                    uri,
                )
            }
            _ => {
                return Err(ErrorData::resource_not_found(
                    format!("unknown resource: {uri}"),
                    None,
                ));
            }
        }
    } else {
        return Err(ErrorData::resource_not_found(
            format!("unknown resource: {uri}"),
            None,
        ));
    };

    Ok(ReadResourceResult {
        contents: vec![contents],
    })
}

fn resource_json(uri: &str, value: serde_json::Value) -> ResourceContents {
    ResourceContents::TextResourceContents {
        uri: uri.to_string(),
        mime_type: Some("application/json".to_string()),
        text: value.to_string(),
        meta: None,
    }
}
