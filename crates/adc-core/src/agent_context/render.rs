use serde_json::{json, Value};

use super::{AgentContext, InvestigationRoute};
use crate::{AdcError, AdcResult};

pub fn render_agent_context_markdown(context: &AgentContext) -> AdcResult<String> {
    let mut out = String::new();
    out.push_str("# Agent Context\n\n");
    if let Some(run_id) = &context.run_id {
        out.push_str(&format!("- run_id: `{run_id}`\n"));
    }
    if let Some(target_id) = &context.target_id {
        out.push_str(&format!("- target_id: `{target_id}`\n"));
    }
    if let Some(profile_id) = &context.profile_id {
        out.push_str(&format!("- profile_id: `{profile_id}`\n"));
    }
    out.push_str(&format!(
        "- primary_window: `{}` events={}\n",
        context.primary_window.window_id, context.primary_window.event_count
    ));
    if let Some(overhead) = &context.overhead {
        out.push_str(&format!(
            "- overhead: artifact_bytes={} event_count={} duration_ms={} throttled={} dropped={}\n",
            overhead.artifact_bytes,
            overhead.event_count,
            overhead.duration_ms,
            overhead.throttled,
            overhead.dropped
        ));
    }

    out.push_str("\n## Target Dossier\n\n");
    out.push_str(&format!(
        "- target_id: `{}`\n",
        context.target_dossier.target_id
    ));
    if let Some(profile_id) = &context.target_dossier.profile_id {
        out.push_str(&format!("- profile_id: `{profile_id}`\n"));
    }
    if let Some(run_id) = &context.target_dossier.run_id {
        out.push_str(&format!("- run_id: `{run_id}`\n"));
    }
    if let Some(fleet_run_id) = &context.target_dossier.fleet_run_id {
        out.push_str(&format!("- fleet_run_id: `{fleet_run_id}`\n"));
    }
    out.push_str(&format!(
        "- primary_window: `{}` events={}\n",
        context.target_dossier.primary_window_id, context.target_dossier.primary_window_event_count
    ));
    out.push_str(&format!(
        "- raw_artifacts_are_ref_only={}\n",
        context.target_dossier.raw_artifacts_are_ref_only
    ));
    out.push_str(&format!(
        "- root_required={}\n",
        context.target_dossier.root_required
    ));
    if !context.target_dossier.redacted_artifacts.is_empty() {
        out.push_str(&format!(
            "- redacted_artifacts={}\n",
            context.target_dossier.redacted_artifacts.join(",")
        ));
    }
    for (key, value) in context.target_dossier.capability_summary.iter().take(8) {
        out.push_str(&format!("- capability.{key}={value}\n"));
    }

    out.push_str("\n## Derived Facts\n\n");
    if context.derived_facts.is_empty() {
        out.push_str("- No derived facts were available; inspect evidence refs.\n");
    } else {
        for fact in &context.derived_facts {
            out.push_str(&format!(
                "- `{}` {} (ref `{}`)\n",
                fact.kind, fact.statement, fact.raw_ref
            ));
        }
    }

    out.push_str("\n## Recommended Refs\n\n");
    for reference in &context.recommended_refs {
        out.push_str(&format!(
            "- {}: `{}` - {}\n",
            reference.label, reference.raw_ref, reference.reason
        ));
    }

    out.push_str("\n## Agent Playbook\n\n");
    if context.playbook.steps.is_empty() {
        out.push_str(
            "- No playbook steps were generated; inspect data_quality and evidence refs.\n",
        );
    } else {
        for step in &context.playbook.steps {
            let refs = step
                .refs
                .iter()
                .map(|reference| format!("{}=`{}`", reference.label, reference.raw_ref))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!(
                "- `{}` {} cost={} privilege={} refs=[{}] stop={}\n",
                step.step_id,
                step.title,
                step.estimated_cost,
                step.required_privilege,
                refs,
                step.stop_condition
            ));
        }
    }

    out.push_str(&render_investigation_route_markdown(
        &context.investigation_route,
    ));

    out.push_str("\n## Data Quality\n\n");
    out.push_str(&format!(
        "- dropped={} drop_count={} throttled={} truncated={} clock_confidence={}\n",
        context.data_quality.dropped,
        context.data_quality.drop_count,
        context.data_quality.throttled,
        context.data_quality.truncated,
        context.data_quality.clock_confidence
    ));
    for missing in &context.data_quality.missing {
        out.push_str(&format!("- missing: {missing}\n"));
    }
    for debt in &context.information_debt {
        out.push_str(&format!(
            "- debt `{}`: {}. next: {}\n",
            debt.kind, debt.description, debt.remediation_hint
        ));
    }

    out.push_str("\n## Next Probes\n\n");
    for probe in &context.next_probe_options {
        out.push_str(&format!(
            "- `{}` cost={} privilege={} - {}\n",
            probe.probe_id, probe.estimated_cost, probe.required_privilege, probe.reason
        ));
    }
    Ok(out)
}

pub fn render_investigation_route_markdown(route: &InvestigationRoute) -> String {
    let mut out = String::new();
    out.push_str("\n## Investigation Route\n\n");
    out.push_str(&format!(
        "- route_id: `{}` scope={} steps={} raw_refs={}\n",
        route.route_id, route.scope, route.budget.returned_step_count, route.budget.raw_ref_count
    ));
    if let Some(service_name) = &route.service_name {
        out.push_str(&format!("- service_name: `{service_name}`\n"));
    }
    if route.budget.max_context_bytes <= 2_000 {
        out.push_str(&format!(
            "- compact_route: steps={} raw_refs={}\n",
            route.budget.returned_step_count, route.budget.raw_ref_count
        ));
        return out;
    }
    for step in route.steps.iter().take(4) {
        let refs = step
            .refs
            .iter()
            .take(2)
            .map(|reference| reference.label.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let next = step
            .branch_conditions
            .first()
            .map(|branch| branch.next_step_id.as_str())
            .unwrap_or("IR-END");
        out.push_str(&format!(
            "- `{}` {} refs={} next={}\n",
            step.step_id, step.title, refs, next
        ));
    }
    if route.steps.len() > 4 {
        out.push_str(&format!(
            "- omitted_route_steps: {}\n",
            route.steps.len().saturating_sub(4)
        ));
    }
    out
}

pub fn render_agent_context_openmetrics(context: &AgentContext) -> AdcResult<String> {
    let run_id = context.run_id.as_deref().unwrap_or("");
    let target_id = context.target_id.as_deref().unwrap_or("");
    let profile_id = context.profile_id.as_deref().unwrap_or("");
    let mut out = String::new();
    out.push_str("# HELP adc_agent_context_info Agent context metadata.\n");
    out.push_str("# TYPE adc_agent_context_info gauge\n");
    out.push_str(&format!(
        "adc_agent_context_info{{context_id=\"{}\",run_id=\"{}\",target_id=\"{}\",profile_id=\"{}\"}} 1\n",
        escape_label(&context.context_id),
        escape_label(run_id),
        escape_label(target_id),
        escape_label(profile_id)
    ));
    out.push_str("# HELP adc_agent_context_derived_facts_total Number of derived facts in the context pack.\n");
    out.push_str("# TYPE adc_agent_context_derived_facts_total gauge\n");
    out.push_str(&format!(
        "adc_agent_context_derived_facts_total{{run_id=\"{}\"}} {}\n",
        escape_label(run_id),
        context.derived_facts.len()
    ));
    out.push_str(
        "# HELP adc_agent_context_data_quality_missing_total Missing data_quality item count.\n",
    );
    out.push_str("# TYPE adc_agent_context_data_quality_missing_total gauge\n");
    out.push_str(&format!(
        "adc_agent_context_data_quality_missing_total{{run_id=\"{}\"}} {}\n",
        escape_label(run_id),
        context.data_quality.missing.len()
    ));
    if let Some(overhead) = &context.overhead {
        out.push_str(
            "# HELP adc_agent_context_artifact_bytes Artifact bytes for the source run.\n",
        );
        out.push_str("# TYPE adc_agent_context_artifact_bytes gauge\n");
        out.push_str(&format!(
            "adc_agent_context_artifact_bytes{{run_id=\"{}\"}} {}\n",
            escape_label(run_id),
            overhead.artifact_bytes
        ));
        out.push_str("# HELP adc_agent_context_events_total Event count for the source run.\n");
        out.push_str("# TYPE adc_agent_context_events_total gauge\n");
        out.push_str(&format!(
            "adc_agent_context_events_total{{run_id=\"{}\"}} {}\n",
            escape_label(run_id),
            overhead.event_count
        ));
    }
    Ok(out)
}

pub fn render_agent_context_otlp_json(context: &AgentContext) -> AdcResult<String> {
    let run_id = context.run_id.as_deref().unwrap_or("");
    let target_id = context.target_id.as_deref().unwrap_or("");
    let profile_id = context.profile_id.as_deref().unwrap_or("");
    let metrics = vec![
        otlp_gauge_metric(
            "obs.agent_context.derived_facts",
            context.derived_facts.len() as i64,
            "1",
            "Number of derived facts in the Agent context pack.",
        ),
        otlp_gauge_metric(
            "obs.agent_context.data_quality.missing",
            context.data_quality.missing.len() as i64,
            "1",
            "Number of missing data_quality items.",
        ),
        otlp_gauge_metric(
            "obs.agent_context.raw_refs",
            context.raw_refs.len() as i64,
            "1",
            "Number of raw artifact references in the context pack.",
        ),
    ];
    let document = json!({
        "resourceMetrics": [{
            "resource": {
                "attributes": [
                    otlp_string_attr("service.name", "adc-targetd"),
                    otlp_string_attr("obs.context_id", &context.context_id),
                    otlp_string_attr("obs.run_id", run_id),
                    otlp_string_attr("obs.target_id", target_id),
                    otlp_string_attr("obs.profile_id", profile_id),
                ]
            },
            "scopeMetrics": [{
                "scope": {
                    "name": "adc-targetd.agent_context",
                    "version": crate::VERSION,
                },
                "metrics": metrics,
            }]
        }]
    });
    serde_json::to_string_pretty(&document)
        .map_err(|err| AdcError::Artifact(format!("otlp json serialization failed: {err}")))
}

pub fn render_agent_context_journald_jsonl(context: &AgentContext) -> AdcResult<String> {
    let run_id = context.run_id.as_deref().unwrap_or("");
    let target_id = context.target_id.as_deref().unwrap_or("");
    let entries = [
        json!({
            "MESSAGE": "Agent context ready",
            "PRIORITY": "6",
            "SYSLOG_IDENTIFIER": "adc-targetd",
            "ADC_CONTEXT_ID": context.context_id,
            "ADC_RUN_ID": run_id,
            "ADC_TARGET_ID": target_id,
            "ADC_DERIVED_FACTS": context.derived_facts.len().to_string(),
            "ADC_RAW_REFS": context.raw_refs.len().to_string(),
        }),
        json!({
            "MESSAGE": "Agent context data_quality summary",
            "PRIORITY": if context.data_quality.missing.is_empty() { "6" } else { "4" },
            "SYSLOG_IDENTIFIER": "adc-targetd",
            "ADC_CONTEXT_ID": context.context_id,
            "ADC_RUN_ID": run_id,
            "ADC_TARGET_ID": target_id,
            "ADC_MISSING_COUNT": context.data_quality.missing.len().to_string(),
            "ADC_TRUNCATED": context.data_quality.truncated.to_string(),
            "ADC_THROTTLED": context.data_quality.throttled.to_string(),
        }),
    ];
    let mut out = String::new();
    for entry in entries {
        let line = serde_json::to_string(&entry).map_err(|err| {
            AdcError::Artifact(format!("journald jsonl serialization failed: {err}"))
        })?;
        out.push_str(&line);
        out.push('\n');
    }
    Ok(out)
}

pub fn render_agent_context_perfetto_json(context: &AgentContext) -> AdcResult<String> {
    let run_id = context.run_id.as_deref().unwrap_or("");
    let target_id = context.target_id.as_deref().unwrap_or("");
    let start_us = context.primary_window.start_mono_ns / 1_000;
    let duration_us = context
        .primary_window
        .end_mono_ns
        .saturating_sub(context.primary_window.start_mono_ns)
        / 1_000;
    let mut events = vec![
        json!({
            "name": "process_name",
            "ph": "M",
            "pid": 1,
            "tid": 1,
            "args": { "name": "adc-targetd" },
        }),
        json!({
            "name": "obs.agent_context",
            "cat": "obs",
            "ph": "X",
            "ts": start_us,
            "dur": duration_us,
            "pid": 1,
            "tid": 1,
            "args": {
                "context_id": context.context_id,
                "run_id": run_id,
                "target_id": target_id,
                "derived_facts": context.derived_facts.len(),
                "raw_refs": context.raw_refs.len(),
                "data_quality_missing": context.data_quality.missing.len(),
            }
        }),
    ];
    for (index, fact) in context.derived_facts.iter().take(20).enumerate() {
        events.push(json!({
            "name": format!("obs.fact.{}", fact.kind),
            "cat": "obs.fact",
            "ph": "i",
            "s": "t",
            "ts": start_us.saturating_add(index as u64),
            "pid": 1,
            "tid": 1,
            "args": {
                "fact_id": fact.fact_id,
                "source": fact.source,
                "kind": fact.kind,
                "raw_ref": fact.raw_ref,
            }
        }));
    }
    let document = json!({
        "displayTimeUnit": "ns",
        "traceEvents": events,
    });
    serde_json::to_string_pretty(&document)
        .map_err(|err| AdcError::Artifact(format!("perfetto json serialization failed: {err}")))
}

fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn otlp_string_attr(key: &str, value: &str) -> Value {
    json!({
        "key": key,
        "value": {
            "stringValue": value,
        }
    })
}

fn otlp_gauge_metric(name: &str, value: i64, unit: &str, description: &str) -> Value {
    json!({
        "name": name,
        "description": description,
        "unit": unit,
        "gauge": {
            "dataPoints": [{
                "asInt": value.to_string(),
            }]
        }
    })
}
