use std::{
    env, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{AdcError, AdcResult, DataQuality};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelCapabilityPaths {
    pub proc_root: PathBuf,
    pub sys_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelCapabilityMap {
    pub arch: String,
    pub kernel_release: Option<String>,
    pub board_model: Option<String>,
    pub tracefs_path: Option<String>,
    pub ftrace_available: bool,
    pub perf_available: bool,
    pub perf_event_paranoid: Option<i32>,
    pub kprobe_available: bool,
    pub ebpf_available: bool,
    pub root_access: bool,
    pub loaded_modules: Vec<String>,
    pub pci_devices: Vec<String>,
    pub thermal_zones: Vec<String>,
    pub data_quality: DataQuality,
}

impl Default for KernelCapabilityPaths {
    fn default() -> Self {
        Self {
            proc_root: PathBuf::from("/proc"),
            sys_root: PathBuf::from("/sys"),
        }
    }
}

pub fn detect_kernel_capabilities(paths: &KernelCapabilityPaths) -> AdcResult<KernelCapabilityMap> {
    let mut data_quality = DataQuality {
        clock_confidence: crate::ClockConfidence::Medium,
        ..Default::default()
    };
    let kernel_release = read_optional_trimmed(
        &paths.proc_root.join("sys/kernel/osrelease"),
        "kernel_release",
        &mut data_quality,
    );
    let board_model = read_optional_trimmed(
        &paths.proc_root.join("device-tree/model"),
        "board_model",
        &mut data_quality,
    )
    .map(|model| model.trim_matches(char::from(0)).to_string());
    let tracefs_path = find_tracefs(&paths.sys_root);
    if tracefs_path.is_none() {
        data_quality
            .missing
            .push("tracefs: no tracing directory visible".to_string());
    }
    let ftrace_available = tracefs_path.as_ref().is_some_and(|path| {
        paths
            .sys_root
            .join(path)
            .join("available_tracers")
            .is_file()
    });
    let kprobe_available = tracefs_path
        .as_ref()
        .is_some_and(|path| paths.sys_root.join(path).join("kprobe_events").exists());
    let perf_event_paranoid = read_optional_trimmed(
        &paths.proc_root.join("sys/kernel/perf_event_paranoid"),
        "perf_event_paranoid",
        &mut data_quality,
    )
    .and_then(|value| value.parse::<i32>().ok());
    let perf_available = perf_event_paranoid.is_some_and(|value| value <= 3);

    Ok(KernelCapabilityMap {
        arch: env::consts::ARCH.to_string(),
        kernel_release,
        board_model,
        tracefs_path,
        ftrace_available,
        perf_available,
        perf_event_paranoid,
        kprobe_available,
        ebpf_available: paths.sys_root.join("fs/bpf").is_dir(),
        root_access: effective_uid(&paths.proc_root).is_some_and(|uid| uid == 0),
        loaded_modules: read_modules(&paths.proc_root, &mut data_quality),
        pci_devices: list_dir_names(&paths.sys_root.join("bus/pci/devices")),
        thermal_zones: list_dir_names(&paths.sys_root.join("class/thermal")),
        data_quality,
    })
}

pub fn detect_default_kernel_capabilities() -> AdcResult<KernelCapabilityMap> {
    detect_kernel_capabilities(&KernelCapabilityPaths::default())
}

fn find_tracefs(sys_root: &Path) -> Option<String> {
    ["kernel/tracing", "kernel/debug/tracing"]
        .into_iter()
        .find(|relative| sys_root.join(relative).is_dir())
        .map(str::to_string)
}

fn read_optional_trimmed(
    path: &Path,
    label: &str,
    data_quality: &mut DataQuality,
) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(value) => Some(value.trim().to_string()),
        Err(err) => {
            data_quality.missing.push(format!("{label}: {err}"));
            None
        }
    }
}

fn read_modules(proc_root: &Path, data_quality: &mut DataQuality) -> Vec<String> {
    match fs::read_to_string(proc_root.join("modules")) {
        Ok(contents) => contents
            .lines()
            .filter_map(|line| line.split_whitespace().next().map(str::to_string))
            .collect(),
        Err(err) => {
            data_quality.missing.push(format!("loaded_modules: {err}"));
            Vec::new()
        }
    }
}

fn list_dir_names(path: &Path) -> Vec<String> {
    fs::read_dir(path)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect()
}

fn effective_uid(proc_root: &Path) -> Option<u32> {
    let status = fs::read_to_string(proc_root.join("self/status")).ok()?;
    let uid_line = status.lines().find(|line| line.starts_with("Uid:"))?;
    uid_line.split_whitespace().nth(2)?.parse().ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrivilegedOperation {
    FtraceStart,
    FtraceStop,
    PerfStart,
    PerfStop,
    KmsgRead,
    CapabilityReport,
}

pub fn parse_privileged_operation(name: &str) -> AdcResult<PrivilegedOperation> {
    match name {
        "ftrace-start" => Ok(PrivilegedOperation::FtraceStart),
        "ftrace-stop" => Ok(PrivilegedOperation::FtraceStop),
        "perf-start" => Ok(PrivilegedOperation::PerfStart),
        "perf-stop" => Ok(PrivilegedOperation::PerfStop),
        "kmsg-read" => Ok(PrivilegedOperation::KmsgRead),
        "capability-report" => Ok(PrivilegedOperation::CapabilityReport),
        _ => Err(AdcError::ProfileValidation(format!(
            "privileged operation is not allowlisted: {name}"
        ))),
    }
}
