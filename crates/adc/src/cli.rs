use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{self, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod args;
mod help;

use args::{
    capture_duration, flag_values, has_flag, optional_flag, parse_millis, required_flag,
    validate_target_id,
};
use help::{print_help, print_help_for};

pub fn run(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let args = args.into_iter().collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_help_for(&args);
        return Ok(());
    }
    match args.as_slice() {
        [] => print_status(),
        [cmd] if cmd == "status" => print_status(),
        [cmd] if cmd == "doctor" => doctor(),
        [cmd] if cmd == "capabilities" => capabilities(),
        [cmd, rest @ ..] if cmd == "observe" => observe(rest),
        [cmd, rest @ ..] if cmd == "agent-context" => agent_context(rest),
        [cmd, rest @ ..] if cmd == "snapshot" => snapshot(rest),
        [cmd, rest @ ..] if cmd == "capture" => capture(rest),
        [cmd, rest @ ..] if cmd == "target" => target(rest),
        [cmd, rest @ ..] if cmd == "evidence" => evidence(rest),
        [cmd, rest @ ..] if cmd == "next-probe" => next_probe(rest),
        [cmd, rest @ ..] if cmd == "fleet" => fleet(rest),
        [cmd, rest @ ..] if cmd == "recorder" => recorder(rest),
        [cmd, rest @ ..] if cmd == "arm" => arm(rest),
        [cmd] if cmd == "disarm" => disarm(),
        [cmd, rest @ ..] if cmd == "compare" => compare(rest),
        [cmd] if cmd == "list-runs" => list_runs(),
        [cmd, rest @ ..] if cmd == "investigate" => investigate(rest),
        [cmd, rest @ ..] if cmd == "bundle" => bundle(rest),
        [flag] if flag == "-h" || flag == "--help" => {
            print_help();
            Ok(())
        }
        _ => Err("usage: adc <status|doctor|capabilities|observe|agent-context|snapshot|capture|target|evidence|next-probe|fleet|recorder|arm|disarm|compare|list-runs|investigate|bundle>".to_string()),
    }
}

fn print_status() -> Result<(), String> {
    let status = adc_core::status_for("adc", adc_core::VERSION);
    serde_json::to_writer_pretty(std::io::stdout(), &status)
        .map_err(|err| format!("failed to serialize status: {err}"))?;
    println!();
    Ok(())
}

fn doctor() -> Result<(), String> {
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let mut checks = Vec::new();
    match fs::create_dir_all(&artifact_root) {
        Ok(()) => checks.push(serde_json::json!({
            "name": "artifact_root",
            "status": "ok",
            "path": artifact_root,
        })),
        Err(err) => checks.push(serde_json::json!({
            "name": "artifact_root",
            "status": "error",
            "error": err.to_string(),
        })),
    }
    let capabilities = adc_core::detect_default_kernel_capabilities().ok();
    if let Some(capabilities) = &capabilities {
        checks.push(serde_json::json!({
            "name": "capabilities",
            "status": "ok",
            "root_access": capabilities.root_access,
        }));
    } else {
        checks.push(serde_json::json!({
            "name": "capabilities",
            "status": "degraded",
        }));
    }
    let response = serde_json::json!({
        "service": "adc",
        "version": adc_core::VERSION,
        "status": "ready",
        "root_required": false,
        "checks": checks,
    });
    serde_json::to_writer_pretty(std::io::stdout(), &response)
        .map_err(|err| format!("failed to serialize doctor response: {err}"))?;
    println!();
    Ok(())
}

fn recorder(args: &[String]) -> Result<(), String> {
    match args {
        [cmd] if cmd == "status" => recorder_status(),
        [cmd, rest @ ..] if cmd == "mark" => recorder_mark(rest),
        [cmd] if cmd == "incidents" => recorder_incidents(),
        [cmd, subcmd, rest @ ..] if cmd == "incident" && subcmd == "get" => {
            recorder_incident_get(rest)
        }
        _ => Err("usage: adc recorder status".to_string()),
    }
}

fn recorder_status() -> Result<(), String> {
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let active_profile = adc_core::read_state(&artifact_root)
        .ok()
        .and_then(|state| state.active_profile);
    let ring = adc_core::RecorderRing::new("local", 1, 60_000);
    let (previous, current) = if active_profile.is_some() {
        ("disabled", "armed")
    } else {
        ("error", "disabled")
    };
    let status = adc_core::recorder_status_for(
        "local",
        active_profile.as_deref(),
        previous,
        current,
        ring.status(),
        adc_core::default_recorder_budget(),
    );
    serde_json::to_writer_pretty(std::io::stdout(), &status)
        .map_err(|err| format!("failed to serialize recorder status: {err}"))?;
    println!();
    Ok(())
}

fn recorder_mark(args: &[String]) -> Result<(), String> {
    let symptom = required_flag(args, "--symptom")?;
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let received_at = monotonic_now_ns();
    let incident_id = optional_flag(args, "--incident-id")
        .map(str::to_string)
        .unwrap_or_else(|| format!("INC-{received_at}"));
    let marker_id = optional_flag(args, "--marker-id")
        .map(str::to_string)
        .unwrap_or_else(|| format!("marker-{received_at}"));
    let marker = adc_core::marker_at_received_time(&marker_id, "operator", symptom, received_at);
    let ring = adc_core::RecorderRing::new("local", 1, 60_000);
    let freeze = adc_core::freeze_recorder_marker(
        &artifact_root,
        &incident_id,
        "win-001",
        &marker,
        &ring,
        &adc_core::default_recorder_budget(),
    )
    .map_err(|err| err.to_string())?;
    let response = serde_json::json!({
        "marker": freeze.marker,
        "incident": freeze.incident,
        "frozen_window": freeze.frozen_window,
        "incident_dir": freeze.run_dir,
    });
    serde_json::to_writer_pretty(std::io::stdout(), &response)
        .map_err(|err| format!("failed to serialize recorder marker freeze: {err}"))?;
    println!();
    Ok(())
}

fn recorder_incidents() -> Result<(), String> {
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let root = artifact_root.join("recorder/incidents");
    let mut incidents = Vec::new();
    if let Ok(entries) = fs::read_dir(&root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.join("incident.json").is_file() {
                incidents.push(entry.file_name().to_string_lossy().to_string());
            }
        }
    }
    incidents.sort();
    let response = serde_json::json!({
        "schema_version": "obs.recorder_incident_list.v1",
        "incidents": incidents,
    });
    serde_json::to_writer_pretty(std::io::stdout(), &response)
        .map_err(|err| format!("failed to serialize recorder incidents: {err}"))?;
    println!();
    Ok(())
}

fn recorder_incident_get(args: &[String]) -> Result<(), String> {
    let incident_id = required_flag(args, "--incident-id")?;
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let incident_dir = artifact_root.join("recorder/incidents").join(incident_id);
    let marker: serde_json::Value = read_json_file(&incident_dir.join("marker.json"))?;
    let incident: serde_json::Value = read_json_file(&incident_dir.join("incident.json"))?;
    let frozen_window: serde_json::Value =
        read_json_file(&incident_dir.join("frozen_window.json"))?;
    let response = serde_json::json!({
        "marker": marker,
        "incident": incident,
        "frozen_window": frozen_window,
        "incident_dir": incident_dir,
    });
    serde_json::to_writer_pretty(std::io::stdout(), &response)
        .map_err(|err| format!("failed to serialize recorder incident: {err}"))?;
    println!();
    Ok(())
}

fn read_json_file(path: &Path) -> Result<serde_json::Value, String> {
    let bytes =
        fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|err| format!("failed to parse {}: {err}", path.display()))
}

fn monotonic_now_ns() -> u64 {
    fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|contents| contents.split_whitespace().next().map(str::to_string))
        .and_then(|seconds| seconds.parse::<f64>().ok())
        .map(|seconds| (seconds * 1_000_000_000.0) as u64)
        .unwrap_or(0)
}

fn observe(args: &[String]) -> Result<(), String> {
    let run_id = required_flag(args, "--run-id")?;
    let duration = capture_duration(args)?;
    let interval = optional_flag(args, "--interval-ms")
        .map(parse_millis)
        .transpose()?
        .unwrap_or_else(|| Duration::from_millis(1000));
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let profile_id = optional_flag(args, "--profile").unwrap_or("manual_observe");
    let bundle = adc_core::capture_for_target(
        &artifact_root,
        adc_core::CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: profile_id.to_string(),
            duration,
            interval,
            collectors: vec![
                "cpu".to_string(),
                "memory".to_string(),
                "network".to_string(),
            ],
            max_artifact_bytes: 512 * 1024 * 1024,
        },
        adc_core::CaptureTargetContext {
            target_id: optional_flag(args, "--target")
                .unwrap_or("local")
                .to_string(),
            fleet_run_id: None,
        },
    )
    .map_err(|err| err.to_string())?;
    adc_core::record_run(&artifact_root, run_id).map_err(|err| err.to_string())?;
    write_runtime_snapshots(&bundle.run_dir)?;
    stage_agent_context_inputs(args, &bundle.run_dir)?;
    let context = adc_core::build_run_agent_context(
        &artifact_root,
        adc_core::AgentContextRequest {
            run_id: run_id.to_string(),
            service_name: None,
            max_markdown_bytes: 40 * 1024,
        },
    )
    .map_err(|err| err.to_string())?;
    let markdown =
        adc_core::render_agent_context_markdown(&context).map_err(|err| err.to_string())?;
    let context_json = serde_json::to_vec_pretty(&context)
        .map_err(|err| format!("failed to serialize agent context: {err}"))?;
    let markdown_path = bundle.run_dir.join("agent_context.md");
    let json_path = bundle.run_dir.join("agent_context.json");
    fs::write(&markdown_path, markdown)
        .map_err(|err| format!("failed to write {}: {err}", markdown_path.display()))?;
    fs::write(&json_path, context_json)
        .map_err(|err| format!("failed to write {}: {err}", json_path.display()))?;
    let response = serde_json::json!({
        "run_id": bundle.run_id,
        "target_id": context.target_id,
        "run_dir": bundle.run_dir,
        "evidence_index": bundle.evidence_index_path,
        "agent_context": markdown_path,
        "agent_context_json": json_path,
        "sample_count": bundle.sample_count,
        "duration_ms": bundle.duration_ms,
    });
    serde_json::to_writer_pretty(std::io::stdout(), &response)
        .map_err(|err| format!("failed to serialize observe response: {err}"))?;
    println!();
    Ok(())
}

fn stage_agent_context_inputs(args: &[String], run_dir: &Path) -> Result<(), String> {
    let inputs = adc_core::AgentContextInputPaths {
        log_file: optional_flag(args, "--log-file").map(PathBuf::from),
        domain_events_file: optional_flag(args, "--domain-events-file").map(PathBuf::from),
        config_file: optional_flag(args, "--config-file").map(PathBuf::from),
        service_name: optional_flag(args, "--service-name").map(str::to_string),
        otlp_file: optional_flag(args, "--otlp-file").map(PathBuf::from),
        journald_jsonl_file: optional_flag(args, "--journald-jsonl-file").map(PathBuf::from),
        perfetto_file: optional_flag(args, "--perfetto-file").map(PathBuf::from),
    };
    adc_core::stage_agent_context_inputs(run_dir, &inputs).map_err(|err| err.to_string())
}

fn write_runtime_snapshots(run_dir: &Path) -> Result<(), String> {
    let raw_dir = run_dir.join("raw");
    write_process_snapshot(&raw_dir.join("process_snapshot.json"))?;
    write_io_snapshot(&raw_dir.join("io_snapshot.json"))?;
    write_thermal_snapshot(&raw_dir.join("thermal_snapshot.json"))?;
    write_fd_thread_snapshot(&raw_dir.join("fd_thread_snapshot.json"))?;
    write_kernel_probe_snapshot(&raw_dir.join("kernel_probe_snapshot.json"))?;
    Ok(())
}

fn write_process_snapshot(output_path: &Path) -> Result<(), String> {
    let mut process_count = 0_u64;
    let mut sampled = Vec::new();
    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.chars().all(|ch| ch.is_ascii_digit()) {
                continue;
            }
            process_count += 1;
            if sampled.len() < 20 {
                let comm = fs::read_to_string(entry.path().join("comm"))
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                sampled.push(serde_json::json!({
                    "pid": name,
                    "comm": comm,
                }));
            }
        }
    }
    let response = serde_json::json!({
        "process_count": process_count,
        "sampled_processes": sampled,
        "root_required": false,
    });
    write_pretty_json(output_path, &response)
}

fn write_io_snapshot(output_path: &Path) -> Result<(), String> {
    let contents = fs::read_to_string("/proc/diskstats").unwrap_or_default();
    let device_count = contents
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    let response = serde_json::json!({
        "device_count": device_count,
        "root_required": false,
        "raw_ref": "artifact://raw/io_snapshot.json"
    });
    write_pretty_json(output_path, &response)
}

fn write_thermal_snapshot(output_path: &Path) -> Result<(), String> {
    let mut zones = Vec::new();
    if let Ok(entries) = fs::read_dir("/sys/class/thermal") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("thermal_zone") {
                continue;
            }
            let zone_path = entry.path();
            let zone_type = fs::read_to_string(zone_path.join("type"))
                .unwrap_or_default()
                .trim()
                .to_string();
            let temp_millic = fs::read_to_string(zone_path.join("temp"))
                .ok()
                .and_then(|text| text.trim().parse::<i64>().ok());
            zones.push(serde_json::json!({
                "zone": name,
                "type": zone_type,
                "temp_millicelsius": temp_millic,
            }));
        }
    }
    let response = serde_json::json!({
        "zone_count": zones.len(),
        "zones": zones,
        "root_required": false,
    });
    write_pretty_json(output_path, &response)
}

fn write_fd_thread_snapshot(output_path: &Path) -> Result<(), String> {
    let mut process_count = 0_u64;
    let mut accessible_process_count = 0_u64;
    let mut inaccessible_process_count = 0_u64;
    let mut total_fd_count = 0_u64;
    let mut total_thread_count = 0_u64;
    let mut sampled = Vec::new();
    let mut data_quality = adc_core::DataQuality {
        clock_confidence: adc_core::ClockConfidence::Medium,
        ..Default::default()
    };

    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let pid = entry.file_name().to_string_lossy().to_string();
            if !pid.chars().all(|ch| ch.is_ascii_digit()) {
                continue;
            }
            process_count += 1;
            let process_dir = entry.path();
            let comm = fs::read_to_string(process_dir.join("comm"))
                .unwrap_or_default()
                .trim()
                .to_string();
            let fd_count = match fs::read_dir(process_dir.join("fd")) {
                Ok(fds) => Some(fds.filter_map(Result::ok).count() as u64),
                Err(err) => {
                    push_limited_missing(
                        &mut data_quality,
                        format!("fd_thread: pid {pid} fd unavailable: {err}"),
                    );
                    None
                }
            };
            let thread_count = match fs::read_dir(process_dir.join("task")) {
                Ok(tasks) => Some(tasks.filter_map(Result::ok).count() as u64),
                Err(err) => {
                    push_limited_missing(
                        &mut data_quality,
                        format!("fd_thread: pid {pid} task unavailable: {err}"),
                    );
                    None
                }
            };

            if let Some(fd_count) = fd_count {
                total_fd_count = total_fd_count.saturating_add(fd_count);
            }
            if let Some(thread_count) = thread_count {
                total_thread_count = total_thread_count.saturating_add(thread_count);
            }
            if fd_count.is_some() || thread_count.is_some() {
                accessible_process_count += 1;
            } else {
                inaccessible_process_count += 1;
            }
            if sampled.len() < 20 {
                sampled.push(serde_json::json!({
                    "pid": pid,
                    "comm": comm,
                    "fd_count": fd_count,
                    "thread_count": thread_count,
                }));
            }
        }
    } else {
        data_quality
            .missing
            .push("fd_thread: /proc is unavailable".to_string());
    }

    let response = serde_json::json!({
        "process_count": process_count,
        "accessible_process_count": accessible_process_count,
        "inaccessible_process_count": inaccessible_process_count,
        "total_fd_count": total_fd_count,
        "total_thread_count": total_thread_count,
        "sampled_processes": sampled,
        "root_required": false,
        "data_quality": data_quality,
    });
    write_pretty_json(output_path, &response)
}

fn write_kernel_probe_snapshot(output_path: &Path) -> Result<(), String> {
    let mut data_quality = adc_core::DataQuality {
        clock_confidence: adc_core::ClockConfidence::Medium,
        ..Default::default()
    };
    let mut response = match adc_core::detect_default_kernel_capabilities() {
        Ok(capabilities) => {
            data_quality = capabilities.data_quality.clone();
            if !capabilities.ftrace_available {
                push_limited_missing(
                    &mut data_quality,
                    "ftrace: tracefs/available_tracers unavailable without optional setup"
                        .to_string(),
                );
            }
            if !capabilities.perf_available {
                push_limited_missing(
                    &mut data_quality,
                    "perf: perf_event_paranoid blocks unprivileged counter use".to_string(),
                );
            }
            if !capabilities.kprobe_available {
                push_limited_missing(
                    &mut data_quality,
                    "kprobe: tracefs kprobe_events unavailable".to_string(),
                );
            }
            let ko_loaded = capabilities
                .loaded_modules
                .iter()
                .any(|module| module == "adc_sensor_probe");
            serde_json::json!({
                "arch": capabilities.arch,
                "kernel_release": capabilities.kernel_release,
                "board_model": capabilities.board_model,
                "tracefs_path": capabilities.tracefs_path,
                "ftrace_available": capabilities.ftrace_available,
                "perf_available": capabilities.perf_available,
                "perf_event_paranoid": capabilities.perf_event_paranoid,
                "kprobe_available": capabilities.kprobe_available,
                "ebpf_available": capabilities.ebpf_available,
                "root_access": capabilities.root_access,
                "thermal_zone_count": capabilities.thermal_zones.len(),
                "pci_device_count": capabilities.pci_devices.len(),
                "ko_loaded": ko_loaded,
            })
        }
        Err(err) => {
            data_quality
                .missing
                .push(format!("kernel_probe: capability detection failed: {err}"));
            serde_json::json!({
                "ftrace_available": false,
                "perf_available": false,
                "kprobe_available": false,
                "ebpf_available": false,
                "root_access": false,
                "ko_loaded": false,
            })
        }
    };

    let ko_source_present = env::var("ADC_KO_SOURCE")
        .ok()
        .map(|path| Path::new(&path).is_file())
        .unwrap_or_else(|| Path::new("kernel/adc_sensor_probe/adc_sensor_probe.c").is_file());
    let ko_installed_present =
        Path::new("/usr/local/libexec/agent-debug-compass/kernel/adc_sensor_probe.ko").is_file();
    if !response
        .get("ko_loaded")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        data_quality
            .notes
            .push("ko optional probe is not loaded".to_string());
    }
    response["ko_source_present"] = serde_json::json!(ko_source_present);
    response["ko_installed_present"] = serde_json::json!(ko_installed_present);
    response["root_required"] = serde_json::json!(false);
    response["optional_privileged_smoke"] = serde_json::json!([
        "scripts/e2e/target/run-privileged-smoke.sh ftrace-perf-smoke",
        "scripts/e2e/target/run-privileged-smoke.sh ko-runtime-smoke",
        "scripts/e2e/target/run-privileged-smoke.sh safe-kprobe-smoke --allow-kprobe-smoke"
    ]);
    response["data_quality"] = serde_json::to_value(data_quality)
        .map_err(|err| format!("failed to serialize kernel probe data_quality: {err}"))?;
    write_pretty_json(output_path, &response)
}

fn push_limited_missing(data_quality: &mut adc_core::DataQuality, message: String) {
    if data_quality.missing.len() < 20 && !data_quality.missing.contains(&message) {
        data_quality.missing.push(message);
    }
}

fn write_pretty_json(output_path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|err| format!("failed to serialize json: {err}"))?;
    fs::write(output_path, bytes)
        .map_err(|err| format!("failed to write {}: {err}", output_path.display()))
}

fn agent_context(args: &[String]) -> Result<(), String> {
    let format = optional_flag(args, "--format").unwrap_or("markdown");
    let artifact_root = adc_core::snapshot::default_artifact_root();
    if let Some(fleet_run_id) = optional_flag(args, "--fleet-run-id") {
        let resolved_fleet_run_id = if fleet_run_id == "latest" {
            adc_core::latest_fleet_run_id(&artifact_root)
                .map_err(|err| err.to_string())?
                .ok_or_else(|| {
                    "no fleet runs are available for --fleet-run-id latest".to_string()
                })?
        } else {
            fleet_run_id.to_string()
        };
        let context = adc_core::build_fleet_agent_context(
            &artifact_root,
            adc_core::FleetAgentContextRequest {
                fleet_run_id: resolved_fleet_run_id,
                max_markdown_bytes: 40 * 1024,
            },
        )
        .map_err(|err| err.to_string())?;
        return match format {
            "json" => {
                serde_json::to_writer_pretty(std::io::stdout(), &context)
                    .map_err(|err| format!("failed to serialize fleet agent context: {err}"))?;
                println!();
                Ok(())
            }
            "markdown" | "md" => {
                println!("# Fleet Agent Context");
                println!();
                println!("- fleet_run_id: `{}`", context.fleet_run_id);
                println!(
                    "- targets: {} captured={} failed={}",
                    context.target_count, context.captured_count, context.failed_count
                );
                println!();
                println!("## Target Matrix");
                println!();
                for target in &context.target_matrix {
                    println!(
                        "- `{}` status={} transport={} run_id={} evidence_ref={}",
                        target.target_id,
                        target.status,
                        target.transport,
                        target.run_id.as_deref().unwrap_or("none"),
                        target.evidence_ref.as_deref().unwrap_or("none")
                    );
                }
                println!();
                println!("## Cross Target Summary");
                println!();
                println!(
                    "- captured={} failed={} total_events={}",
                    context.cross_target_summary.captured_count,
                    context.cross_target_summary.failed_count,
                    context.cross_target_summary.total_event_count
                );
                if !context.cross_target_summary.source_totals.is_empty() {
                    let sources = context
                        .cross_target_summary
                        .source_totals
                        .iter()
                        .take(12)
                        .map(|source| format!("{}={}", source.source, source.event_count))
                        .collect::<Vec<_>>()
                        .join(", ");
                    println!("- source_totals: {sources}");
                }
                if !context
                    .cross_target_summary
                    .targets_with_missing_data_quality
                    .is_empty()
                {
                    println!(
                        "- targets_with_missing_data_quality: {}",
                        context
                            .cross_target_summary
                            .targets_with_missing_data_quality
                            .join(", ")
                    );
                }
                if !context.failure_groups.is_empty() {
                    println!("- failure_groups={}", context.failure_groups.len());
                }
                println!();
                println!("## Target Summaries");
                println!();
                for summary in &context.target_summaries {
                    println!(
                        "- `{}` status={} run_id={} window={} events={} evidence_ref={}",
                        summary.target_id,
                        summary.status,
                        summary.run_id.as_deref().unwrap_or("none"),
                        summary.primary_window_id.as_deref().unwrap_or("none"),
                        summary
                            .event_count
                            .map(|count| count.to_string())
                            .unwrap_or_else(|| "unknown".to_string()),
                        summary.evidence_ref.as_deref().unwrap_or("none")
                    );
                    if !summary.sources.is_empty() {
                        let sources = summary
                            .sources
                            .iter()
                            .take(8)
                            .map(|source| format!("{}={}", source.source, source.event_count))
                            .collect::<Vec<_>>()
                            .join(", ");
                        println!("  sources: {sources}");
                    }
                    if let Some(dossier) = &summary.target_dossier {
                        println!(
                            "  dossier: target_id={} profile={} raw_refs={} root_required={} raw_ref_only={}",
                            dossier.target_id,
                            dossier.profile_id.as_deref().unwrap_or("none"),
                            dossier.raw_ref_count,
                            dossier.root_required,
                            dossier.raw_artifacts_are_ref_only
                        );
                        if !dossier.capability_summary.is_empty() {
                            let capabilities = dossier
                                .capability_summary
                                .iter()
                                .take(6)
                                .map(|(key, value)| format!("{key}={value}"))
                                .collect::<Vec<_>>()
                                .join(", ");
                            println!("  capabilities: {capabilities}");
                        }
                    }
                    for lead in &summary.top_leads {
                        println!(
                            "  lead: {} -> {} ({})",
                            lead.label, lead.raw_ref, lead.reason
                        );
                    }
                    for missing in &summary.data_quality.missing {
                        println!("  missing: {missing}");
                    }
                }
                if !context.failure_groups.is_empty() {
                    println!();
                    println!("## Failure Groups");
                    for group in &context.failure_groups {
                        println!(
                            "- {} targets={} sample={} next_action={}",
                            group.failure_class,
                            group.targets.join(","),
                            group.sample,
                            group.next_action
                        );
                    }
                }
                println!();
                println!("## Remediation Hints");
                for hint in &context.remediation_hints {
                    println!("- {hint}");
                }
                println!();
                println!("## Recommended Refs");
                for reference in &context.recommended_refs {
                    println!("- {}: `{}`", reference.label, reference.raw_ref);
                }
                print!(
                    "{}",
                    adc_core::render_investigation_route_markdown(&context.investigation_route)
                );
                Ok(())
            }
            other => Err(format!(
                "unsupported --format {other}; expected markdown or json"
            )),
        };
    }
    let run_id = required_flag(args, "--run-id")?;
    let resolved_run_id = if run_id == "latest" {
        adc_core::latest_run_id(&artifact_root)
            .map_err(|err| err.to_string())?
            .ok_or_else(|| "no runs are available for --run-id latest".to_string())?
    } else {
        run_id.to_string()
    };
    let context = adc_core::build_run_agent_context(
        &artifact_root,
        adc_core::AgentContextRequest {
            run_id: resolved_run_id,
            service_name: optional_flag(args, "--service-name").map(str::to_string),
            max_markdown_bytes: 40 * 1024,
        },
    )
    .map_err(|err| err.to_string())?;
    match format {
        "json" => {
            serde_json::to_writer_pretty(std::io::stdout(), &context)
                .map_err(|err| format!("failed to serialize agent context: {err}"))?;
            println!();
            Ok(())
        }
        "markdown" | "md" => {
            let markdown =
                adc_core::render_agent_context_markdown(&context).map_err(|err| err.to_string())?;
            print!("{markdown}");
            Ok(())
        }
        "openmetrics" => {
            let metrics = adc_core::render_agent_context_openmetrics(&context)
                .map_err(|err| err.to_string())?;
            print!("{metrics}");
            Ok(())
        }
        "otlp-json" => {
            let otlp =
                adc_core::render_agent_context_otlp_json(&context).map_err(|err| err.to_string())?;
            println!("{otlp}");
            Ok(())
        }
        "journald-jsonl" => {
            let journald = adc_core::render_agent_context_journald_jsonl(&context)
                .map_err(|err| err.to_string())?;
            print!("{journald}");
            Ok(())
        }
        "perfetto-json" => {
            let perfetto = adc_core::render_agent_context_perfetto_json(&context)
                .map_err(|err| err.to_string())?;
            println!("{perfetto}");
            Ok(())
        }
        other => Err(format!(
            "unsupported --format {other}; expected markdown, json, openmetrics, otlp-json, journald-jsonl, or perfetto-json"
        )),
    }
}

fn snapshot(args: &[String]) -> Result<(), String> {
    let run_id = required_flag(args, "--run-id")?;
    let target_id = optional_flag(args, "--target").unwrap_or("local");
    validate_target_id(target_id)?;
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let bundle = adc_core::create_snapshot_for_target(
        &artifact_root,
        run_id,
        adc_core::SnapshotTargetContext {
            target_id: target_id.to_string(),
            fleet_run_id: None,
        },
    )
    .map_err(|err| err.to_string())?;
    let response = serde_json::json!({
        "run_id": bundle.run_id,
        "target_id": target_id,
        "run_dir": bundle.run_dir,
        "manifest": bundle.manifest_path,
        "evidence_index": bundle.evidence_index_path,
        "timeline": bundle.timeline_path,
    });
    serde_json::to_writer_pretty(std::io::stdout(), &response)
        .map_err(|err| format!("failed to serialize snapshot response: {err}"))?;
    println!();
    Ok(())
}

fn capabilities() -> Result<(), String> {
    let map = adc_core::detect_default_kernel_capabilities().map_err(|err| err.to_string())?;
    let report = adc_core::build_capability_report("local", &map);
    serde_json::to_writer_pretty(std::io::stdout(), &report)
        .map_err(|err| format!("failed to serialize capabilities: {err}"))?;
    println!();
    Ok(())
}

fn capture(args: &[String]) -> Result<(), String> {
    let run_id = required_flag(args, "--run-id")?;
    let target_id = optional_flag(args, "--target").unwrap_or("local");
    validate_target_id(target_id)?;
    let duration = capture_duration(args)?;
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let profile_dir = adc_core::default_profile_dir();
    let profile_id = optional_flag(args, "--profile").unwrap_or("manual_capture");
    let profile = if optional_flag(args, "--profile").is_some() {
        Some(adc_core::load_profile(&profile_dir, profile_id).map_err(|err| err.to_string())?)
    } else {
        None
    };
    let interval = optional_flag(args, "--interval-ms")
        .map(parse_millis)
        .transpose()?
        .or_else(|| {
            profile
                .as_ref()
                .map(|profile| Duration::from_millis(profile.sampling.interval_ms))
        })
        .unwrap_or_else(|| Duration::from_millis(1000));
    let collectors = profile
        .as_ref()
        .map(|profile| profile.always_on.collectors.clone())
        .unwrap_or_else(|| {
            vec![
                "cpu".to_string(),
                "memory".to_string(),
                "network".to_string(),
            ]
        });
    let max_artifact_bytes = profile
        .as_ref()
        .map(|profile| {
            profile
                .budgets
                .max_artifact_mb_per_run
                .saturating_mul(1024 * 1024)
        })
        .unwrap_or(512 * 1024 * 1024);

    let bundle = adc_core::capture_for_target(
        &artifact_root,
        adc_core::CaptureOptions {
            run_id: run_id.to_string(),
            profile_id: profile_id.to_string(),
            duration,
            interval,
            collectors,
            max_artifact_bytes,
        },
        adc_core::CaptureTargetContext {
            target_id: target_id.to_string(),
            fleet_run_id: None,
        },
    )
    .map_err(|err| err.to_string())?;
    adc_core::record_run(&artifact_root, run_id).map_err(|err| err.to_string())?;
    let response = serde_json::json!({
        "run_id": bundle.run_id,
        "target_id": target_id,
        "run_dir": bundle.run_dir,
        "manifest": bundle.manifest_path,
        "evidence_index": bundle.evidence_index_path,
        "timeline": bundle.timeline_path,
        "sample_count": bundle.sample_count,
        "duration_ms": bundle.duration_ms,
    });
    serde_json::to_writer_pretty(std::io::stdout(), &response)
        .map_err(|err| format!("failed to serialize capture response: {err}"))?;
    println!();
    Ok(())
}

fn target(args: &[String]) -> Result<(), String> {
    match args {
        [cmd, rest @ ..] if cmd == "preflight" => target_preflight(rest),
        [cmd, rest @ ..] if cmd == "snapshot" => snapshot(rest),
        [cmd, rest @ ..] if cmd == "capture" => capture(rest),
        _ => Err("usage: adc target <preflight|snapshot|capture> --target local ...".to_string()),
    }
}

fn target_preflight(args: &[String]) -> Result<(), String> {
    let target_id = optional_flag(args, "--target").unwrap_or("local");
    validate_target_id(target_id)?;
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let mut checks = Vec::new();
    let mut missing = Vec::new();
    match fs::create_dir_all(&artifact_root) {
        Ok(()) => {
            let write_test_path =
                artifact_root.join(format!(".preflight-write-{}.tmp", process::id()));
            match fs::write(&write_test_path, b"ok") {
                Ok(()) => {
                    let _ = fs::remove_file(&write_test_path);
                    checks.push(serde_json::json!({
                        "name": "artifact_root_writable",
                        "status": "ok",
                        "path": artifact_root,
                    }));
                }
                Err(err) => {
                    missing.push(format!("artifact_root_writable: {err}"));
                    checks.push(serde_json::json!({
                        "name": "artifact_root_writable",
                        "status": "error",
                        "path": artifact_root,
                        "error": err.to_string(),
                    }));
                }
            }
        }
        Err(err) => {
            missing.push(format!("artifact_root_create: {err}"));
            checks.push(serde_json::json!({
                "name": "artifact_root_writable",
                "status": "error",
                "path": artifact_root,
                "error": err.to_string(),
            }));
        }
    }
    checks.push(serde_json::json!({
        "name": "adc_binary",
        "status": "ok",
        "version": adc_core::VERSION,
    }));
    let status = if missing.is_empty() {
        "ready"
    } else {
        "degraded"
    };
    let response = serde_json::json!({
        "schema_version": "obs.target_preflight.v1",
        "target_id": target_id,
        "status": status,
        "root_required": false,
        "checks": checks,
        "data_quality": {
            "dropped": false,
            "drop_count": 0,
            "throttled": false,
            "missing": missing,
            "truncated": false,
            "clock_confidence": "medium",
            "notes": []
        }
    });
    serde_json::to_writer_pretty(std::io::stdout(), &response)
        .map_err(|err| format!("failed to serialize target preflight response: {err}"))?;
    println!();
    Ok(())
}

fn evidence(args: &[String]) -> Result<(), String> {
    match args {
        [cmd, rest @ ..] if cmd == "get" => evidence_get(rest),
        [cmd, rest @ ..] if cmd == "window" => evidence_window(rest),
        [cmd, rest @ ..] if cmd == "series" => evidence_series(rest),
        [cmd, rest @ ..] if cmd == "search" => evidence_search(rest),
        [cmd, rest @ ..] if cmd == "raw-slice" => evidence_raw_slice(rest),
        [cmd, rest @ ..] if cmd == "ref" => evidence_ref(rest),
        _ => Err(
            "usage: adc evidence <get|window|series|search|raw-slice|ref> --run-id <id>"
                .to_string(),
        ),
    }
}

fn evidence_get(args: &[String]) -> Result<(), String> {
    let run_id = required_flag(args, "--run-id")?;
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let evidence = adc_core::read_evidence_index_text(&artifact_root, run_id)
        .map_err(|err| err.to_string())?;
    print!("{evidence}");
    Ok(())
}

fn evidence_series(args: &[String]) -> Result<(), String> {
    let run_id = required_flag(args, "--run-id")?;
    let source = required_flag(args, "--source")?;
    let limit = optional_flag(args, "--limit")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|err| format!("invalid --limit: {err}"))
        })
        .transpose()?
        .unwrap_or(20);
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let series = adc_core::signal_series_for(&artifact_root, run_id, source, limit)
        .map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &series)
        .map_err(|err| format!("failed to serialize signal series: {err}"))?;
    println!();
    Ok(())
}

fn evidence_raw_slice(args: &[String]) -> Result<(), String> {
    let run_id = required_flag(args, "--run-id")?;
    let raw_ref = required_flag(args, "--ref")?;
    let limit = optional_flag(args, "--limit")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|err| format!("invalid --limit: {err}"))
        })
        .transpose()?
        .unwrap_or(20);
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let slice = adc_core::read_raw_slice(&artifact_root, run_id, raw_ref, limit)
        .map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &slice)
        .map_err(|err| format!("failed to serialize raw slice: {err}"))?;
    println!();
    Ok(())
}

fn evidence_ref(args: &[String]) -> Result<(), String> {
    let run_id = required_flag(args, "--run-id")?;
    let ref_uri = required_flag(args, "--ref")?;
    let limit = optional_flag(args, "--limit")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|err| format!("invalid --limit: {err}"))
        })
        .transpose()?
        .unwrap_or(20);
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let resolved = adc_core::resolve_agent_ref(&artifact_root, run_id, ref_uri, limit)
        .map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &resolved)
        .map_err(|err| format!("failed to serialize resolved ref: {err}"))?;
    println!();
    Ok(())
}

fn next_probe(args: &[String]) -> Result<(), String> {
    let run_id = required_flag(args, "--run-id")?;
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let evidence =
        adc_core::read_evidence_index(&artifact_root, run_id).map_err(|err| err.to_string())?;
    let response = serde_json::json!({
        "run_id": evidence.run_id,
        "target_id": evidence.target_id,
        "next_probe_options": evidence.next_probe_options,
        "information_debt": evidence.information_debt,
        "data_quality": evidence.data_quality,
    });
    serde_json::to_writer_pretty(std::io::stdout(), &response)
        .map_err(|err| format!("failed to serialize next probe response: {err}"))?;
    println!();
    Ok(())
}

fn fleet(args: &[String]) -> Result<(), String> {
    match args {
        [cmd] if cmd == "init" => fleet_init(),
        [cmd, rest @ ..] if cmd == "invite" => fleet_invite(rest),
        [cmd, rest @ ..] if cmd == "enroll" => fleet_enroll(rest),
        [cmd, rest @ ..] if cmd == "enroll-kit" => fleet_enroll_kit(rest),
        [cmd] if cmd == "targets" => fleet_targets(),
        [cmd, rest @ ..] if cmd == "discover" => fleet_discover(rest),
        [cmd, rest @ ..] if cmd == "preflight" => fleet_preflight(rest),
        [cmd, rest @ ..] if cmd == "investigate" => fleet_investigate(rest),
        [cmd, rest @ ..] if cmd == "observe" => fleet_capture(rest),
        [cmd, rest @ ..] if cmd == "snapshot" => fleet_snapshot(rest),
        [cmd, rest @ ..] if cmd == "capture" => fleet_capture(rest),
        [cmd, rest @ ..] if cmd == "evidence" => fleet_evidence(rest),
        _ => Err(
            "usage: adc fleet <init|invite|enroll|enroll-kit|targets|discover|preflight|investigate|observe|snapshot|capture|evidence>"
                .to_string(),
        ),
    }
}

fn fleet_init() -> Result<(), String> {
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let registry = adc_core::initialize_managed_fleet_registry(&artifact_root)
        .map_err(|err| err.to_string())?;
    let response = serde_json::json!({
        "schema_version": registry.schema_version,
        "registry_path": adc_core::managed_fleet_registry_path(&artifact_root),
        "target_count": registry.targets.len(),
        "targets": registry.targets,
    });
    serde_json::to_writer_pretty(std::io::stdout(), &response)
        .map_err(|err| format!("failed to serialize fleet init response: {err}"))?;
    println!();
    Ok(())
}

fn fleet_invite(args: &[String]) -> Result<(), String> {
    let ttl = optional_flag(args, "--ttl-sec")
        .map(|value| {
            value
                .parse::<u64>()
                .map(Duration::from_secs)
                .map_err(|err| format!("invalid --ttl-sec value: {err}"))
        })
        .transpose()?
        .unwrap_or_else(|| Duration::from_secs(600));
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let invite = adc_core::create_managed_fleet_invite(
        &artifact_root,
        adc_core::ManagedFleetInviteOptions {
            target_id_hint: optional_flag(args, "--target-id-hint").map(str::to_string),
            ttl,
        },
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &invite)
        .map_err(|err| format!("failed to serialize fleet invite response: {err}"))?;
    println!();
    Ok(())
}

fn fleet_enroll(args: &[String]) -> Result<(), String> {
    let target_id = required_flag(args, "--target-id")?;
    let transport = required_flag(args, "--transport")?;
    let port = optional_flag(args, "--port")
        .map(|value| {
            value
                .parse::<u16>()
                .map_err(|err| format!("invalid --port value: {err}"))
        })
        .transpose()?;
    let target = adc_core::ManagedFleetTarget {
        target_id: target_id.to_string(),
        display_name: optional_flag(args, "--name").map(str::to_string),
        transport: transport.to_string(),
        host: optional_flag(args, "--host").map(str::to_string),
        user: optional_flag(args, "--user").map(str::to_string),
        port,
        profile: optional_flag(args, "--profile").map(str::to_string),
        mcp_server_path: optional_flag(args, "--mcp-server-path").map(str::to_string),
        auth_token_file: optional_flag(args, "--auth-token-file").map(str::to_string),
        tls_ca_file: optional_flag(args, "--tls-ca-file").map(str::to_string),
        tls_client_cert_file: optional_flag(args, "--tls-client-cert-file").map(str::to_string),
        tls_client_key_file: optional_flag(args, "--tls-client-key-file").map(str::to_string),
        tls_server_name: optional_flag(args, "--tls-server-name").map(str::to_string),
        tags: flag_values(args, "--tag")
            .into_iter()
            .map(str::to_string)
            .collect(),
        trust_state: optional_flag(args, "--trust-state")
            .unwrap_or("trusted")
            .to_string(),
        enrollment_mode: optional_flag(args, "--enrollment-mode")
            .unwrap_or("manual")
            .to_string(),
        identity_fingerprint: optional_flag(args, "--identity-fingerprint").map(str::to_string),
    };
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let registry = adc_core::upsert_managed_fleet_target(&artifact_root, target)
        .map_err(|err| err.to_string())?;
    let response = serde_json::json!({
        "schema_version": registry.schema_version,
        "registry_path": adc_core::managed_fleet_registry_path(&artifact_root),
        "target_count": registry.targets.len(),
        "targets": registry.targets,
    });
    serde_json::to_writer_pretty(std::io::stdout(), &response)
        .map_err(|err| format!("failed to serialize fleet enroll response: {err}"))?;
    println!();
    Ok(())
}

fn fleet_enroll_kit(args: &[String]) -> Result<(), String> {
    let kit_path = required_flag(args, "--kit")?;
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let registry = adc_core::enroll_managed_fleet_kit(&artifact_root, kit_path)
        .map_err(|err| err.to_string())?;
    let response = serde_json::json!({
        "schema_version": registry.schema_version,
        "registry_path": adc_core::managed_fleet_registry_path(&artifact_root),
        "target_count": registry.targets.len(),
        "targets": registry.targets,
    });
    serde_json::to_writer_pretty(std::io::stdout(), &response)
        .map_err(|err| format!("failed to serialize fleet enroll-kit response: {err}"))?;
    println!();
    Ok(())
}

fn fleet_targets() -> Result<(), String> {
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let registry =
        adc_core::read_managed_fleet_registry(&artifact_root).map_err(|err| err.to_string())?;
    let response = serde_json::json!({
        "schema_version": registry.schema_version,
        "registry_path": adc_core::managed_fleet_registry_path(&artifact_root),
        "target_count": registry.targets.len(),
        "targets": registry.targets,
    });
    serde_json::to_writer_pretty(std::io::stdout(), &response)
        .map_err(|err| format!("failed to serialize fleet targets response: {err}"))?;
    println!();
    Ok(())
}

fn fleet_discover(args: &[String]) -> Result<(), String> {
    let cidr = required_flag(args, "--cidr")?;
    let neighbor_text = if let Some(path) = optional_flag(args, "--neighbors-file") {
        fs::read_to_string(path).map_err(|err| format!("failed to read {path}: {err}"))?
    } else {
        read_neighbor_table()?
    };
    let result = adc_core::discover_same_network_targets_from_neighbors(cidr, &neighbor_text)
        .map_err(|err| err.to_string())?;
    if let Some(path) = optional_flag(args, "--write-inventory") {
        write_discovered_inventory(path, &result)?;
    }
    serde_json::to_writer_pretty(std::io::stdout(), &result)
        .map_err(|err| format!("failed to serialize discovery response: {err}"))?;
    println!();
    Ok(())
}

fn write_discovered_inventory(
    path: &str,
    result: &adc_core::TargetDiscoveryResult,
) -> Result<(), String> {
    let mut inventory = String::from("targets:\n");
    for target in &result.candidates {
        inventory.push_str(&format!(
            "  - id: {}\n    transport: {}\n    host: {}\n",
            target.target_id, target.transport, target.host
        ));
    }
    let path = Path::new(path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create inventory directory {}: {err}",
                parent.display()
            )
        })?;
    }
    fs::write(path, inventory).map_err(|err| {
        format!(
            "failed to write discovered inventory {}: {err}",
            path.display()
        )
    })
}

fn fleet_preflight(args: &[String]) -> Result<(), String> {
    let inventory = fleet_inventory_path(args)?;
    let result = adc_core::preflight_fleet(inventory).map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &result)
        .map_err(|err| format!("failed to serialize fleet preflight response: {err}"))?;
    println!();
    Ok(())
}

fn fleet_investigate(args: &[String]) -> Result<(), String> {
    match args {
        [scope, service_name, rest @ ..] if scope == "service" => {
            fleet_investigate_service(service_name, rest)
        }
        _ => Err(
            "usage: adc fleet investigate service <name> --fleet-run-id <id> (--inventory PATH|--selector SELECTOR) [--journal-lines N]"
                .to_string(),
        ),
    }
}

fn fleet_investigate_service(service_name: &str, args: &[String]) -> Result<(), String> {
    let inventory = fleet_inventory_path(args)?;
    let fleet_run_id = required_flag(args, "--fleet-run-id")?;
    let max_journal_lines = optional_flag(args, "--journal-lines")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|err| format!("invalid --journal-lines: {err}"))
        })
        .transpose()?
        .unwrap_or(80);
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let result = adc_core::investigate_fleet_service(
        &artifact_root,
        inventory,
        adc_core::FleetServiceInvestigationOptions {
            fleet_run_id: fleet_run_id.to_string(),
            service_name: service_name.to_string(),
            max_journal_lines,
        },
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &result).map_err(|err| {
        format!("failed to serialize fleet service investigation response: {err}")
    })?;
    println!();
    Ok(())
}

fn fleet_snapshot(args: &[String]) -> Result<(), String> {
    let inventory = fleet_inventory_path(args)?;
    let fleet_run_id = required_flag(args, "--fleet-run-id")?;
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let result = adc_core::snapshot_fleet(
        &artifact_root,
        inventory,
        adc_core::FleetSnapshotOptions {
            fleet_run_id: fleet_run_id.to_string(),
        },
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &result)
        .map_err(|err| format!("failed to serialize fleet snapshot response: {err}"))?;
    println!();
    Ok(())
}

fn fleet_capture(args: &[String]) -> Result<(), String> {
    let inventory = fleet_inventory_path(args)?;
    let fleet_run_id = required_flag(args, "--fleet-run-id")?;
    let duration = capture_duration(args)?;
    let interval = optional_flag(args, "--interval-ms")
        .map(parse_millis)
        .transpose()?
        .unwrap_or_else(|| Duration::from_millis(1000));
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let result = adc_core::capture_fleet(
        &artifact_root,
        inventory,
        adc_core::FleetCaptureOptions {
            fleet_run_id: fleet_run_id.to_string(),
            duration,
            interval,
        },
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &result)
        .map_err(|err| format!("failed to serialize fleet capture response: {err}"))?;
    println!();
    Ok(())
}

fn fleet_inventory_path(args: &[String]) -> Result<PathBuf, String> {
    match (
        optional_flag(args, "--inventory"),
        optional_flag(args, "--selector"),
    ) {
        (Some(_), Some(_)) => Err("use only one of --inventory or --selector".to_string()),
        (Some(path), None) => Ok(PathBuf::from(path)),
        (None, Some(selector)) => {
            let artifact_root = adc_core::snapshot::default_artifact_root();
            let materialization =
                adc_core::materialize_managed_fleet_inventory(&artifact_root, selector)
                    .map_err(|err| err.to_string())?;
            Ok(materialization.inventory_path)
        }
        (None, None) => Err("missing required flag: --inventory or --selector".to_string()),
    }
}

fn fleet_evidence(args: &[String]) -> Result<(), String> {
    let fleet_run_id = required_flag(args, "--fleet-run-id")?;
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let evidence = adc_core::read_fleet_evidence_text(&artifact_root, fleet_run_id)
        .map_err(|err| err.to_string())?;
    print!("{evidence}");
    Ok(())
}

fn arm(args: &[String]) -> Result<(), String> {
    let profile_id = required_flag(args, "--profile")?;
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let state = adc_core::arm_profile(&artifact_root, profile_id).map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &state)
        .map_err(|err| format!("failed to serialize arm response: {err}"))?;
    println!();
    Ok(())
}

fn disarm() -> Result<(), String> {
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let state = adc_core::disarm_profile(&artifact_root).map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &state)
        .map_err(|err| format!("failed to serialize disarm response: {err}"))?;
    println!();
    Ok(())
}

fn evidence_window(args: &[String]) -> Result<(), String> {
    let run_id = required_flag(args, "--run-id")?;
    let window_id = required_flag(args, "--window-id")?;
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let window = adc_core::snapshot::read_window(&artifact_root, run_id, window_id)
        .map_err(|err| err.to_string())?;
    print!("{window}");
    Ok(())
}

fn evidence_search(args: &[String]) -> Result<(), String> {
    let run_id = required_flag(args, "--run-id")?;
    let source = optional_flag(args, "--source").map(str::to_string);
    let event_type = optional_flag(args, "--event-type").map(str::to_string);
    let contains = optional_flag(args, "--contains").map(str::to_string);
    let limit = optional_flag(args, "--limit")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|err| format!("invalid --limit: {err}"))
        })
        .transpose()?
        .unwrap_or(20);
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let result = adc_core::search_events(
        &artifact_root,
        run_id,
        &adc_core::SearchEventsQuery {
            source,
            event_type,
            contains,
            limit,
        },
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &result)
        .map_err(|err| format!("failed to serialize search response: {err}"))?;
    println!();
    Ok(())
}

fn compare(args: &[String]) -> Result<(), String> {
    let before = required_flag(args, "--before")?;
    let after = required_flag(args, "--after")?;
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let comparison =
        adc_core::compare_runs(&artifact_root, before, after).map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &comparison)
        .map_err(|err| format!("failed to serialize compare response: {err}"))?;
    println!();
    Ok(())
}

fn list_runs() -> Result<(), String> {
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let runs = adc_core::snapshot::list_runs(&artifact_root).map_err(|err| err.to_string())?;
    let response = serde_json::json!({
        "runs": runs.into_iter().map(|run_id| serde_json::json!({ "run_id": run_id })).collect::<Vec<_>>()
    });
    serde_json::to_writer_pretty(std::io::stdout(), &response)
        .map_err(|err| format!("failed to serialize run list: {err}"))?;
    println!();
    Ok(())
}

fn investigate(args: &[String]) -> Result<(), String> {
    match args {
        [cmd, rest @ ..] if cmd == "bug" => investigate_bug(rest),
        [cmd, rest @ ..] if cmd == "start" => investigate_start(rest),
        [cmd, rest @ ..] if cmd == "continue" => investigate_continue(rest),
        [cmd, rest @ ..] if cmd == "session" => investigate_session(rest),
        [cmd, rest @ ..] if cmd == "cleanup-sessions" => investigate_cleanup_sessions(rest),
        [cmd, rest @ ..] if cmd == "probe-result" => investigate_probe_result(rest),
        [cmd] if cmd == "route-packs" => investigate_route_packs(),
        [scope, kind, rest @ ..] if scope == "service" => investigate_service(kind, rest),
        [cmd, rest @ ..] if cmd == "ref" => investigate_ref(rest),
        _ => Err(
            "usage: adc investigate bug --symptom <text> [--run-id <id>|--fleet-run-id <id>|--duration-ms N] [--service-name NAME] [--inventory PATH] | investigate start (--run-id <id>|--fleet-run-id <id>) [--service-name NAME] [--inventory PATH] [--journal-lines N] [--format json|markdown] | investigate continue (--run-id <id>|--fleet-run-id <id>) --step-id <id> [--ref-label LABEL] [--ref REF] | investigate session (--run-id <id>|--fleet-run-id <id>) --session-id <id> | investigate cleanup-sessions (--run-id <id>|--fleet-run-id <id>) [--max-sessions N] [--max-age-days N] [--dry-run|--execute] | investigate probe-result missing-capability --probe-plan-id ID --probe-id ID --missing-fact FACT [--hypothesis-id H] | investigate probe-result policy-denied --probe-plan-id ID --probe-id ID --reason TEXT [--hypothesis-id H] | investigate route-packs | investigate service <name> [--journal-lines N] | investigate ref --ref artifact://service_investigations/... [--limit N]"
                .to_string(),
        ),
    }
}

fn investigate_bug(args: &[String]) -> Result<(), String> {
    let symptom = required_flag(args, "--symptom")?;
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let mut run_id = optional_flag(args, "--run-id").map(str::to_string);
    let fleet_run_id = optional_flag(args, "--fleet-run-id").map(str::to_string);
    if run_id.is_some() && fleet_run_id.is_some() {
        return Err("use only one of --run-id or --fleet-run-id".to_string());
    }
    if run_id.is_none() && fleet_run_id.is_none() {
        let generated_run_id = generated_symptom_run_id()?;
        let duration = capture_duration(args)?;
        let interval = optional_flag(args, "--interval-ms")
            .map(parse_millis)
            .transpose()?
            .unwrap_or_else(|| Duration::from_millis(1_000));
        let target_id = optional_flag(args, "--target").unwrap_or("local");
        validate_target_id(target_id)?;
        let bundle = adc_core::capture_for_target(
            &artifact_root,
            adc_core::CaptureOptions {
                run_id: generated_run_id.clone(),
                profile_id: optional_flag(args, "--profile")
                    .unwrap_or("symptom_investigation")
                    .to_string(),
                duration,
                interval,
                collectors: vec![
                    "cpu".to_string(),
                    "memory".to_string(),
                    "network".to_string(),
                ],
                max_artifact_bytes: 512 * 1024 * 1024,
            },
            adc_core::CaptureTargetContext {
                target_id: target_id.to_string(),
                fleet_run_id: None,
            },
        )
        .map_err(|err| err.to_string())?;
        adc_core::record_run(&artifact_root, &generated_run_id).map_err(|err| err.to_string())?;
        write_runtime_snapshots(&bundle.run_dir)?;
        stage_agent_context_inputs(args, &bundle.run_dir)?;
        run_id = Some(generated_run_id);
    }

    let max_journal_lines = optional_flag(args, "--journal-lines")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|err| format!("invalid --journal-lines: {err}"))
        })
        .transpose()?;
    let context = adc_core::investigate_bug(
        &artifact_root,
        adc_core::SymptomInvestigationRequest {
            run_id,
            fleet_run_id,
            service_name: optional_flag(args, "--service-name")
                .or_else(|| optional_flag(args, "--service"))
                .map(str::to_string),
            inventory_path: optional_flag(args, "--inventory").map(PathBuf::from),
            symptom: symptom.to_string(),
            max_journal_lines,
            max_markdown_bytes: 40 * 1024,
            max_context_bytes: 64 * 1024,
        },
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &context)
        .map_err(|err| format!("failed to serialize symptom context: {err}"))?;
    println!();
    Ok(())
}

fn generated_symptom_run_id() -> Result<String, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| format!("system clock is before unix epoch: {err}"))?
        .as_millis();
    Ok(format!("R-SYMPTOM-{millis}"))
}

fn investigate_route_packs() -> Result<(), String> {
    let registry = adc_core::default_route_pack_registry();
    serde_json::to_writer_pretty(std::io::stdout(), &registry)
        .map_err(|err| format!("failed to serialize route pack registry: {err}"))?;
    println!();
    Ok(())
}

fn investigate_start(args: &[String]) -> Result<(), String> {
    let max_journal_lines = optional_flag(args, "--journal-lines")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|err| format!("invalid --journal-lines: {err}"))
        })
        .transpose()?;
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let pack = adc_core::start_investigation(
        &artifact_root,
        adc_core::InvestigationStartRequest {
            run_id: optional_flag(args, "--run-id").map(str::to_string),
            fleet_run_id: optional_flag(args, "--fleet-run-id").map(str::to_string),
            service_name: optional_flag(args, "--service-name").map(str::to_string),
            inventory_path: optional_flag(args, "--inventory").map(PathBuf::from),
            max_journal_lines,
            max_markdown_bytes: 40 * 1024,
        },
    )
    .map_err(|err| err.to_string())?;
    match optional_flag(args, "--format").unwrap_or("json") {
        "json" => {
            serde_json::to_writer_pretty(std::io::stdout(), &pack)
                .map_err(|err| format!("failed to serialize investigation start pack: {err}"))?;
            println!();
            Ok(())
        }
        "markdown" | "md" => {
            println!("# Investigation Start");
            println!();
            println!("- scope: `{}`", pack.scope);
            if let Some(run_id) = &pack.run_id {
                println!("- run_id: `{run_id}`");
            }
            if let Some(fleet_run_id) = &pack.fleet_run_id {
                println!("- fleet_run_id: `{fleet_run_id}`");
            }
            if let Some(service_name) = &pack.service_name {
                println!("- service_name: `{service_name}`");
            }
            println!();
            println!("## Investigation Route");
            for step in &pack.investigation_route.steps {
                println!(
                    "- `{}` {} expected=\"{}\" refs={}",
                    step.step_id,
                    step.title,
                    step.expected_answer,
                    step.refs
                        .iter()
                        .map(|reference| format!("{}=`{}`", reference.label, reference.raw_ref))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            Ok(())
        }
        other => Err(format!(
            "unsupported --format {other}; expected json or markdown"
        )),
    }
}

fn investigate_continue(args: &[String]) -> Result<(), String> {
    let max_ref_lines = optional_flag(args, "--max-ref-lines")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|err| format!("invalid --max-ref-lines: {err}"))
        })
        .transpose()?
        .unwrap_or(80);
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let pack = adc_core::continue_investigation(
        &artifact_root,
        adc_core::InvestigationContinuationRequest {
            run_id: optional_flag(args, "--run-id").map(str::to_string),
            fleet_run_id: optional_flag(args, "--fleet-run-id").map(str::to_string),
            service_name: optional_flag(args, "--service-name").map(str::to_string),
            route_id: optional_flag(args, "--route-id").map(str::to_string),
            session_id: optional_flag(args, "--session-id").map(str::to_string),
            current_step_id: required_flag(args, "--step-id")?.to_string(),
            open_ref_labels: flag_values(args, "--ref-label")
                .into_iter()
                .map(str::to_string)
                .collect(),
            open_raw_refs: flag_values(args, "--ref")
                .into_iter()
                .map(str::to_string)
                .collect(),
            max_context_bytes: 10 * 1024,
            max_ref_lines,
        },
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &pack)
        .map_err(|err| format!("failed to serialize investigation continuation pack: {err}"))?;
    println!();
    Ok(())
}

fn investigate_session(args: &[String]) -> Result<(), String> {
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let state = adc_core::get_investigation_session_state(
        &artifact_root,
        adc_core::InvestigationSessionRequest {
            run_id: optional_flag(args, "--run-id").map(str::to_string),
            fleet_run_id: optional_flag(args, "--fleet-run-id").map(str::to_string),
            session_id: required_flag(args, "--session-id")?.to_string(),
        },
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &state)
        .map_err(|err| format!("failed to serialize investigation session state: {err}"))?;
    println!();
    Ok(())
}

fn investigate_cleanup_sessions(args: &[String]) -> Result<(), String> {
    let max_sessions = optional_flag(args, "--max-sessions")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|err| format!("invalid --max-sessions: {err}"))
        })
        .transpose()?
        .unwrap_or(64);
    let max_age_days = optional_flag(args, "--max-age-days")
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|err| format!("invalid --max-age-days: {err}"))
        })
        .transpose()?;
    let dry_run = !has_flag(args, "--execute") || has_flag(args, "--dry-run");
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let report = adc_core::cleanup_investigation_sessions(
        &artifact_root,
        adc_core::InvestigationSessionCleanupRequest {
            run_id: optional_flag(args, "--run-id").map(str::to_string),
            fleet_run_id: optional_flag(args, "--fleet-run-id").map(str::to_string),
            max_sessions,
            max_age_days,
            dry_run,
        },
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &report)
        .map_err(|err| format!("failed to serialize investigation session cleanup: {err}"))?;
    println!();
    Ok(())
}

fn investigate_probe_result(args: &[String]) -> Result<(), String> {
    let (result_kind, rest) = args.split_first().ok_or_else(probe_result_usage)?;
    match result_kind.as_str() {
        "missing-capability" => investigate_probe_result_missing_capability(rest),
        "policy-denied" => investigate_probe_result_policy_denied(rest),
        _ => Err(probe_result_usage()),
    }
}

fn probe_result_usage() -> String {
    "usage: adc investigate probe-result missing-capability --probe-plan-id ID --probe-id ID --missing-fact FACT [--hypothesis-id H] | adc investigate probe-result policy-denied --probe-plan-id ID --probe-id ID --reason TEXT [--hypothesis-id H]".to_string()
}

fn investigate_probe_result_missing_capability(args: &[String]) -> Result<(), String> {
    let probe_plan_id = required_flag(args, "--probe-plan-id")?;
    let probe_id = required_flag(args, "--probe-id")?;
    let missing_fact = required_flag(args, "--missing-fact")?;
    let hypothesis_ids = flag_values(args, "--hypothesis-id")
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let data_quality = adc_core::DataQuality {
        missing: vec![format!(
            "{missing_fact} unavailable in recorded probe result"
        )],
        clock_confidence: adc_core::ClockConfidence::Medium,
        ..Default::default()
    };
    let result = adc_core::probe_result_for_unavailable_capability(
        probe_plan_id,
        probe_id,
        &hypothesis_ids,
        missing_fact,
        &data_quality,
    );
    serde_json::to_writer_pretty(std::io::stdout(), &result)
        .map_err(|err| format!("failed to serialize probe result: {err}"))?;
    println!();
    Ok(())
}

fn investigate_probe_result_policy_denied(args: &[String]) -> Result<(), String> {
    let probe_plan_id = required_flag(args, "--probe-plan-id")?;
    let probe_id = required_flag(args, "--probe-id")?;
    let reason = required_flag(args, "--reason")?;
    let hypothesis_ids = flag_values(args, "--hypothesis-id")
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let data_quality = adc_core::DataQuality {
        missing: vec![format!(
            "{probe_id} was not executed because policy denied the probe"
        )],
        clock_confidence: adc_core::ClockConfidence::Medium,
        notes: vec![reason.to_string()],
        ..Default::default()
    };
    let result = adc_core::probe_result_for_policy_denied(
        probe_plan_id,
        probe_id,
        &hypothesis_ids,
        reason,
        &data_quality,
    );
    serde_json::to_writer_pretty(std::io::stdout(), &result)
        .map_err(|err| format!("failed to serialize probe result: {err}"))?;
    println!();
    Ok(())
}

fn investigate_service(service_name: &str, args: &[String]) -> Result<(), String> {
    let max_journal_lines = optional_flag(args, "--journal-lines")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|err| format!("invalid --journal-lines: {err}"))
        })
        .transpose()?
        .unwrap_or(80);
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let pack = adc_core::investigate_service(
        &artifact_root,
        adc_core::ServiceInvestigationRequest {
            service_name: service_name.to_string(),
            max_journal_lines,
        },
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &pack)
        .map_err(|err| format!("failed to serialize service investigation pack: {err}"))?;
    println!();
    Ok(())
}

fn investigate_ref(args: &[String]) -> Result<(), String> {
    let ref_uri = required_flag(args, "--ref")?;
    let limit = optional_flag(args, "--limit")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|err| format!("invalid --limit: {err}"))
        })
        .transpose()?
        .unwrap_or(20);
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let resolved = adc_core::resolve_global_agent_ref(&artifact_root, ref_uri, limit)
        .map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &resolved)
        .map_err(|err| format!("failed to serialize resolved ref: {err}"))?;
    println!();
    Ok(())
}

fn bundle(args: &[String]) -> Result<(), String> {
    let run_id = required_flag(args, "--run-id")?;
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let manifest = adc_core::snapshot::manifest_path_for(&artifact_root, run_id)
        .map_err(|err| err.to_string())?;
    let response = serde_json::json!({
        "run_id": run_id,
        "manifest": manifest,
    });
    serde_json::to_writer_pretty(std::io::stdout(), &response)
        .map_err(|err| format!("failed to serialize bundle response: {err}"))?;
    println!();
    Ok(())
}

fn read_neighbor_table() -> Result<String, String> {
    match run_command_with_timeout("ip", &["-4", "neigh", "show"], Duration::from_secs(2)) {
        Ok(output) if output.status_success && !output.stdout.trim().is_empty() => {
            Ok(output.stdout)
        }
        Ok(output) => fs::read_to_string("/proc/net/arp").map_err(|err| {
            format!(
                "failed to read neighbor table: ip stderr='{}'; /proc/net/arp: {err}",
                output.stderr.trim()
            )
        }),
        Err(err) => fs::read_to_string("/proc/net/arp")
            .map_err(|fallback_err| format!("{err}; /proc/net/arp: {fallback_err}")),
    }
}

struct ProcessOutput {
    status_success: bool,
    stdout: String,
    stderr: String,
}

fn run_command_with_timeout(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<ProcessOutput, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to start {program}: {err}"))?;
    let started = Instant::now();
    loop {
        match child
            .try_wait()
            .map_err(|err| format!("failed to poll {program}: {err}"))?
        {
            Some(_) => {
                let output = child
                    .wait_with_output()
                    .map_err(|err| format!("failed to read {program} output: {err}"))?;
                return Ok(ProcessOutput {
                    status_success: output.status.success(),
                    stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                });
            }
            None if started.elapsed() >= timeout => {
                let _ = child.kill();
                let output = child
                    .wait_with_output()
                    .map_err(|err| format!("failed to read timed-out {program} output: {err}"))?;
                return Err(format!(
                    "{program} timed out after {} ms; stderr='{}'",
                    timeout.as_millis(),
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    }
}
