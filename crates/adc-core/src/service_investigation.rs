use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::{AdcError, AdcResult, DataQuality, NextProbeOption};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceInvestigationRequest {
    pub service_name: String,
    pub max_journal_lines: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceInvestigationPack {
    pub schema_version: String,
    pub service_name: String,
    pub root_required: bool,
    pub service_state: ServiceStateSummary,
    pub process_summary: ServiceProcessSummary,
    pub port_summary: ServicePortSummary,
    pub journal_leads: Vec<ServiceJournalLead>,
    pub journal_summary: ServiceJournalSummary,
    pub raw_refs: BTreeMap<String, String>,
    pub data_quality: DataQuality,
    pub next_probe_options: Vec<NextProbeOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceStateSummary {
    pub service: String,
    pub availability: String,
    pub active_state: String,
    pub sub_state: String,
    pub load_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main_pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fragment_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceProcessSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmdline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rss_kb: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServicePortSummary {
    pub availability: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socket_inode_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_socket_table_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceJournalLead {
    pub line_index: usize,
    pub severity_hint: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceJournalSummary {
    pub requested_line_count: usize,
    pub returned_lead_count: usize,
    pub window_basis: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oldest_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub newest_timestamp: Option<String>,
    pub stale_lead_count: usize,
}

pub fn investigate_service(
    artifact_root: impl AsRef<Path>,
    request: ServiceInvestigationRequest,
) -> AdcResult<ServiceInvestigationPack> {
    validate_service_name(&request.service_name)?;
    let artifact_root = artifact_root.as_ref();
    let service_dir = artifact_root
        .join("service_investigations")
        .join(&request.service_name);
    fs::create_dir_all(&service_dir).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to create service investigation dir {}: {err}",
            service_dir.display()
        ))
    })?;

    let mut data_quality = DataQuality {
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };
    let service_state = collect_service_state(&request.service_name, &mut data_quality);
    let process_summary = collect_process_summary(service_state.main_pid, &mut data_quality);
    let port_summary = collect_port_summary(service_state.main_pid, &mut data_quality);
    let (journal_leads, journal_summary) = collect_journal_leads(
        &request.service_name,
        request.max_journal_lines,
        &mut data_quality,
    );

    let mut raw_refs = BTreeMap::new();
    write_artifact(&service_dir.join("service_state.json"), &service_state)?;
    raw_refs.insert(
        "service_state".to_string(),
        format!(
            "artifact://service_investigations/{}/service_state.json",
            request.service_name
        ),
    );
    write_artifact(&service_dir.join("process_summary.json"), &process_summary)?;
    raw_refs.insert(
        "process_summary".to_string(),
        format!(
            "artifact://service_investigations/{}/process_summary.json",
            request.service_name
        ),
    );
    write_artifact(&service_dir.join("port_summary.json"), &port_summary)?;
    raw_refs.insert(
        "port_summary".to_string(),
        format!(
            "artifact://service_investigations/{}/port_summary.json",
            request.service_name
        ),
    );
    write_artifact(&service_dir.join("journal_leads.json"), &journal_leads)?;
    raw_refs.insert(
        "journal_leads".to_string(),
        format!(
            "artifact://service_investigations/{}/journal_leads.json",
            request.service_name
        ),
    );
    raw_refs.insert(
        "service_investigation".to_string(),
        format!(
            "artifact://service_investigations/{}/service_investigation.json",
            request.service_name
        ),
    );

    let pack = ServiceInvestigationPack {
        schema_version: "obs.service_investigation.v1".to_string(),
        service_name: request.service_name,
        root_required: false,
        service_state,
        process_summary,
        port_summary,
        journal_leads,
        journal_summary,
        raw_refs,
        data_quality,
        next_probe_options: service_next_probe_options(),
    };
    write_artifact(&service_dir.join("service_investigation.json"), &pack)?;
    Ok(pack)
}

pub fn collect_service_state_for_context(
    service_name: &str,
) -> AdcResult<(ServiceStateSummary, DataQuality)> {
    validate_service_name(service_name)?;
    let mut data_quality = DataQuality {
        clock_confidence: "medium".to_string(),
        ..Default::default()
    };
    let service_state = collect_service_state(service_name, &mut data_quality);
    Ok((service_state, data_quality))
}

fn validate_service_name(service_name: &str) -> AdcResult<()> {
    if service_name.is_empty()
        || service_name.len() > 128
        || service_name
            .chars()
            .any(|ch| !(ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '@')))
    {
        return Err(AdcError::ProfileValidation(format!(
            "invalid service name {service_name:?}; expected a unit-like name without path separators"
        )));
    }
    Ok(())
}

fn collect_service_state(
    service_name: &str,
    data_quality: &mut DataQuality,
) -> ServiceStateSummary {
    let output = run_fixed_command(
        "systemctl",
        &[
            "show",
            service_name,
            "--no-pager",
            "--property=Id,LoadState,ActiveState,SubState,MainPID,FragmentPath",
        ],
        Duration::from_millis(1500),
    );
    let Ok(output) = output else {
        data_quality
            .missing
            .push(format!("service_state: {}", output.err().unwrap()));
        return ServiceStateSummary::unknown(service_name, "unavailable");
    };
    if !output.status_success {
        data_quality.missing.push(format!(
            "service_state: systemctl show failed: {}",
            first_nonempty(&output.stderr).unwrap_or_else(|| output.status.clone())
        ));
    }
    let values = parse_systemctl_show(&output.stdout);
    let active_state = values
        .get("ActiveState")
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());
    let sub_state = values
        .get("SubState")
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());
    let load_state = values
        .get("LoadState")
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());
    let availability = if output.status_success && active_state != "unknown" {
        "available"
    } else {
        "unavailable"
    };
    ServiceStateSummary {
        service: service_name.to_string(),
        availability: availability.to_string(),
        active_state,
        sub_state,
        load_state,
        unit_id: values.get("Id").cloned().filter(|value| !value.is_empty()),
        main_pid: values
            .get("MainPID")
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|pid| *pid > 0),
        fragment_path: values
            .get("FragmentPath")
            .cloned()
            .filter(|value| !value.is_empty()),
    }
}

impl ServiceStateSummary {
    fn unknown(service_name: &str, availability: &str) -> Self {
        Self {
            service: service_name.to_string(),
            availability: availability.to_string(),
            active_state: "unknown".to_string(),
            sub_state: "unknown".to_string(),
            load_state: "unknown".to_string(),
            unit_id: None,
            main_pid: None,
            fragment_path: None,
        }
    }
}

fn collect_process_summary(
    main_pid: Option<u32>,
    data_quality: &mut DataQuality,
) -> ServiceProcessSummary {
    let Some(pid) = main_pid else {
        data_quality
            .missing
            .push("process_summary: no main pid reported by systemctl".to_string());
        return ServiceProcessSummary {
            pid: None,
            comm: None,
            cmdline: None,
            rss_kb: None,
        };
    };
    let proc_dir = PathBuf::from("/proc").join(pid.to_string());
    let comm = fs::read_to_string(proc_dir.join("comm"))
        .ok()
        .map(|value| value.trim().to_string());
    let cmdline = fs::read(proc_dir.join("cmdline")).ok().and_then(|bytes| {
        let parts = bytes
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).to_string())
            .collect::<Vec<_>>();
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    });
    let rss_kb = fs::read_to_string(proc_dir.join("status"))
        .ok()
        .and_then(|status| parse_status_kb(&status, "VmRSS:"));
    if comm.is_none() && cmdline.is_none() && rss_kb.is_none() {
        data_quality
            .missing
            .push(format!("process_summary: /proc/{pid} is unavailable"));
    }
    ServiceProcessSummary {
        pid: Some(pid),
        comm,
        cmdline,
        rss_kb,
    }
}

fn collect_port_summary(
    main_pid: Option<u32>,
    data_quality: &mut DataQuality,
) -> ServicePortSummary {
    let Some(pid) = main_pid else {
        return ServicePortSummary {
            availability: "not_applicable".to_string(),
            socket_inode_count: None,
            matched_socket_table_count: None,
            unavailable_reason: Some("no main pid reported by systemctl".to_string()),
        };
    };
    let entries = match fs::read_dir(format!("/proc/{pid}/fd")) {
        Ok(entries) => entries,
        Err(err) => {
            let reason = format!("pid {pid} fd unavailable: {err}");
            data_quality.missing.push(format!("port_summary: {reason}"));
            return ServicePortSummary {
                availability: "unavailable".to_string(),
                socket_inode_count: None,
                matched_socket_table_count: None,
                unavailable_reason: Some(reason),
            };
        }
    };
    let socket_inode_count = entries
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read_link(entry.path()).ok())
        .filter(|target| target.to_string_lossy().starts_with("socket:["))
        .count();
    ServicePortSummary {
        availability: "available".to_string(),
        socket_inode_count: Some(socket_inode_count),
        matched_socket_table_count: Some(0),
        unavailable_reason: None,
    }
}

fn collect_journal_leads(
    service_name: &str,
    max_journal_lines: usize,
    data_quality: &mut DataQuality,
) -> (Vec<ServiceJournalLead>, ServiceJournalSummary) {
    let line_limit = max_journal_lines.clamp(1, 200).to_string();
    let requested_line_count = line_limit.parse::<usize>().unwrap_or(200);
    let output = run_fixed_command(
        "journalctl",
        &[
            "-u",
            service_name,
            "-n",
            &line_limit,
            "--no-pager",
            "-o",
            "short-iso",
        ],
        Duration::from_millis(1500),
    );
    let Ok(output) = output else {
        data_quality
            .missing
            .push(format!("journal: {}", output.err().unwrap()));
        return (
            Vec::new(),
            ServiceJournalSummary::empty(requested_line_count, "last_n_lines"),
        );
    };
    if !output.status_success {
        data_quality.missing.push(format!(
            "journal: journalctl failed: {}",
            first_nonempty(&output.stderr).unwrap_or_else(|| output.status.clone())
        ));
    }
    let journal_leads = output
        .stdout
        .lines()
        .enumerate()
        .filter_map(|(index, line)| journal_lead_from_line(index, line))
        .take(10)
        .collect::<Vec<_>>();
    let journal_summary =
        summarize_journal_leads(requested_line_count, "last_n_lines", &journal_leads);
    (journal_leads, journal_summary)
}

fn journal_lead_from_line(index: usize, line: &str) -> Option<ServiceJournalLead> {
    let lower = line.to_ascii_lowercase();
    let severity_hint = if lower.contains("error")
        || lower.contains("failed")
        || lower.contains("failure")
        || lower.contains("panic")
    {
        "error"
    } else if lower.contains("warn") || lower.contains("timeout") {
        "warning"
    } else {
        return None;
    };
    Some(ServiceJournalLead {
        line_index: index,
        severity_hint: severity_hint.to_string(),
        message: redact_line(line),
    })
}

impl ServiceJournalSummary {
    fn empty(requested_line_count: usize, window_basis: &str) -> Self {
        Self {
            requested_line_count,
            returned_lead_count: 0,
            window_basis: window_basis.to_string(),
            oldest_timestamp: None,
            newest_timestamp: None,
            stale_lead_count: 0,
        }
    }
}

fn summarize_journal_leads(
    requested_line_count: usize,
    window_basis: &str,
    leads: &[ServiceJournalLead],
) -> ServiceJournalSummary {
    let mut timestamps = leads
        .iter()
        .filter_map(|lead| lead.message.split_whitespace().next())
        .filter(|token| token.contains('T'))
        .map(str::to_string)
        .collect::<Vec<_>>();
    timestamps.sort();
    ServiceJournalSummary {
        requested_line_count,
        returned_lead_count: leads.len(),
        window_basis: window_basis.to_string(),
        oldest_timestamp: timestamps.first().cloned(),
        newest_timestamp: timestamps.last().cloned(),
        stale_lead_count: 0,
    }
}

struct FixedCommandOutput {
    status_success: bool,
    status: String,
    stdout: String,
    stderr: String,
}

fn run_fixed_command(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<FixedCommandOutput, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("{program} unavailable: {err}"))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                let output = child
                    .wait_with_output()
                    .map_err(|err| format!("{program} output failed: {err}"))?;
                return Ok(FixedCommandOutput {
                    status_success: output.status.success(),
                    status: output.status.to_string(),
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                });
            }
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{program} timed out after {} ms",
                    timeout.as_millis()
                ));
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(err) => return Err(format!("{program} wait failed: {err}")),
        }
    }
}

fn parse_systemctl_show(stdout: &str) -> BTreeMap<String, String> {
    stdout
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn parse_status_kb(status: &str, key: &str) -> Option<u64> {
    status.lines().find_map(|line| {
        let rest = line.strip_prefix(key)?.trim();
        rest.split_whitespace().next()?.parse::<u64>().ok()
    })
}

fn first_nonempty(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn redact_line(line: &str) -> String {
    line.split_whitespace()
        .map(|token| {
            let lower = token.to_ascii_lowercase();
            if lower.contains("token=")
                || lower.contains("password=")
                || lower.contains("secret=")
                || lower.contains("authorization:")
            {
                "<redacted>"
            } else {
                token
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn write_artifact(path: &Path, value: &impl Serialize) -> AdcResult<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|err| AdcError::Artifact(format!("service artifact serialize failed: {err}")))?;
    fs::write(path, bytes).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to write service artifact {}: {err}",
            path.display()
        ))
    })
}

fn service_next_probe_options() -> Vec<NextProbeOption> {
    vec![
        NextProbeOption {
            probe_id: "observe_service_window".to_string(),
            label: "Observe this service window".to_string(),
            reason: "Correlates service state with CPU, memory, network, and runtime snapshots"
                .to_string(),
            required_privilege: "none".to_string(),
            estimated_cost: "low".to_string(),
            expected_evidence: vec![
                "service_state".to_string(),
                "resource_series".to_string(),
                "journal_leads".to_string(),
            ],
            profile_hint: "manual_observe --service-name".to_string(),
        },
        NextProbeOption {
            probe_id: "open_service_refs".to_string(),
            label: "Open bounded service refs".to_string(),
            reason: "Reads the service pack artifacts without dumping unrelated logs".to_string(),
            required_privilege: "none".to_string(),
            estimated_cost: "low".to_string(),
            expected_evidence: vec![
                "service_state_ref".to_string(),
                "journal_leads_ref".to_string(),
            ],
            profile_hint: "investigate ref or obs.get_ref".to_string(),
        },
    ]
}
