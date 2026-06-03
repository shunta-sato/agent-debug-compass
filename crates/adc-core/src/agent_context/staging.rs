use std::{fs, path::Path};

use super::AgentContextInputPaths;
use crate::{AdcError, AdcResult};

pub fn stage_agent_context_inputs(
    run_dir: impl AsRef<Path>,
    inputs: &AgentContextInputPaths,
) -> AdcResult<()> {
    let raw_dir = run_dir.as_ref().join("raw");
    if let Some(path) = &inputs.log_file {
        copy_bounded_lines(path, &raw_dir.join("app.log"), 200)?;
    }
    if let Some(path) = &inputs.domain_events_file {
        copy_bounded_lines(path, &raw_dir.join("domain_events.jsonl"), 500)?;
    }
    if let Some(path) = &inputs.otlp_file {
        copy_bounded_lines(path, &raw_dir.join("otlp_metrics.json"), 1_000)?;
    }
    if let Some(path) = &inputs.journald_jsonl_file {
        copy_bounded_lines(path, &raw_dir.join("journald.jsonl"), 1_000)?;
    }
    if let Some(path) = &inputs.perfetto_file {
        copy_bounded_lines(path, &raw_dir.join("perfetto_trace.json"), 1_000)?;
    }
    if let Some(path) = &inputs.config_file {
        write_redacted_config(path, &raw_dir.join("config_redacted.txt"))?;
    }
    if let Some(service_name) = &inputs.service_name {
        write_service_state(service_name, &raw_dir.join("service_state.json"))?;
    }
    Ok(())
}

fn copy_bounded_lines(input_path: &Path, output_path: &Path, max_lines: usize) -> AdcResult<()> {
    let contents = fs::read_to_string(input_path).map_err(|err| {
        AdcError::Artifact(format!("failed to read {}: {err}", input_path.display()))
    })?;
    let mut output = contents
        .lines()
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n");
    if !output.is_empty() {
        output.push('\n');
    }
    fs::write(output_path, output).map_err(|err| {
        AdcError::Artifact(format!("failed to write {}: {err}", output_path.display()))
    })
}

fn write_redacted_config(input_path: &Path, output_path: &Path) -> AdcResult<()> {
    let contents = fs::read_to_string(input_path).map_err(|err| {
        AdcError::Artifact(format!("failed to read {}: {err}", input_path.display()))
    })?;
    let mut redacted = String::new();
    for line in contents.lines() {
        redacted.push_str(&redact_config_line(line));
        redacted.push('\n');
    }
    fs::write(output_path, redacted).map_err(|err| {
        AdcError::Artifact(format!("failed to write {}: {err}", output_path.display()))
    })
}

fn redact_config_line(line: &str) -> String {
    let Some((key, _value)) = line.split_once('=') else {
        return line.to_string();
    };
    let lower_key = key.to_ascii_lowercase();
    if lower_key.contains("password")
        || lower_key.contains("passwd")
        || lower_key.contains("secret")
        || lower_key.contains("token")
        || lower_key.contains("api_key")
        || lower_key.contains("apikey")
    {
        format!("{key}=<redacted>")
    } else {
        line.to_string()
    }
}

fn write_service_state(service_name: &str, output_path: &Path) -> AdcResult<()> {
    let (mut response, data_quality) = crate::collect_service_state_for_context(service_name)
        .map_err(|err| {
            AdcError::Artifact(format!(
                "failed to collect service state for {service_name}: {err}"
            ))
        })?;
    if !data_quality.missing.is_empty() {
        response.availability = "unavailable".to_string();
    }
    let bytes = serde_json::to_vec_pretty(&response)
        .map_err(|err| AdcError::Artifact(format!("service state serialization failed: {err}")))?;
    fs::write(output_path, bytes).map_err(|err| {
        AdcError::Artifact(format!("failed to write {}: {err}", output_path.display()))
    })
}
