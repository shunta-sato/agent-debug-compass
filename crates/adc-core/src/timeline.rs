use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{AdcError, AdcResult, DataQuality, EventEnvelope};

const MAX_SEARCH_LIMIT: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchEventsQuery {
    pub source: Option<String>,
    pub event_type: Option<String>,
    pub contains: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchEventsResult {
    pub run_id: String,
    pub matched_count: usize,
    pub returned_count: usize,
    pub truncated: bool,
    pub events: Vec<EventEnvelope>,
    pub data_quality: DataQuality,
}

pub fn search_events(
    artifact_root: impl AsRef<Path>,
    run_id: &str,
    query: &SearchEventsQuery,
) -> AdcResult<SearchEventsResult> {
    validate_segment(run_id, "run_id")?;
    let timeline_path = timeline_path_for(artifact_root, run_id);
    let contents = fs::read_to_string(&timeline_path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read timeline {}: {err}",
            timeline_path.display()
        ))
    })?;
    let limit = query.limit.clamp(1, MAX_SEARCH_LIMIT);
    let mut events = Vec::new();
    let mut matched_count = 0;
    let mut data_quality = DataQuality {
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };

    for (index, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let event = serde_json::from_str::<EventEnvelope>(line).map_err(|err| {
            AdcError::Artifact(format!(
                "timeline parse failed at line {}: {err}",
                index + 1
            ))
        })?;
        if !matches_query(&event, query) {
            continue;
        }
        matched_count += 1;
        if events.len() < limit {
            events.push(event);
        }
    }

    let truncated = matched_count > events.len();
    data_quality.truncated = truncated;
    if truncated {
        data_quality.notes.push(format!(
            "search returned {} of {} matching events",
            events.len(),
            matched_count
        ));
    }

    Ok(SearchEventsResult {
        run_id: run_id.to_string(),
        matched_count,
        returned_count: events.len(),
        truncated,
        events,
        data_quality,
    })
}

pub fn read_timeline_bounded(
    artifact_root: impl AsRef<Path>,
    run_id: &str,
    limit: usize,
) -> AdcResult<String> {
    let result = search_events(
        artifact_root,
        run_id,
        &SearchEventsQuery {
            source: None,
            event_type: None,
            contains: None,
            limit,
        },
    )?;
    serde_json::to_string_pretty(&result)
        .map_err(|err| AdcError::Artifact(format!("timeline serialization failed: {err}")))
}

fn matches_query(event: &EventEnvelope, query: &SearchEventsQuery) -> bool {
    if query
        .source
        .as_ref()
        .is_some_and(|source| event.source != *source)
    {
        return false;
    }
    if query
        .event_type
        .as_ref()
        .is_some_and(|event_type| event.event_type != *event_type)
    {
        return false;
    }
    if let Some(needle) = &query.contains {
        let haystack = serde_json::to_string(event).unwrap_or_default();
        if !haystack.contains(needle) {
            return false;
        }
    }
    true
}

fn timeline_path_for(artifact_root: impl AsRef<Path>, run_id: &str) -> PathBuf {
    artifact_root
        .as_ref()
        .join("runs")
        .join(run_id)
        .join("timeline.jsonl")
}

fn validate_segment(value: &str, label: &str) -> AdcResult<()> {
    if value.trim().is_empty() || value.contains('/') || value.contains('\\') {
        return Err(AdcError::Artifact(format!(
            "{label} must be a single relative path segment"
        )));
    }
    Ok(())
}
