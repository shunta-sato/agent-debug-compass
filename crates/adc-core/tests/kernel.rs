use std::fs;

use adc_core::{detect_kernel_capabilities, KernelCapabilityPaths};

#[test]
fn detects_tracefs_perf_and_pi_capabilities_from_paths() {
    let temp = tempfile::tempdir().expect("tempdir");
    let proc_root = temp.path().join("proc");
    let sys_root = temp.path().join("sys");
    fs::create_dir_all(proc_root.join("sys/kernel")).expect("proc sys kernel");
    fs::create_dir_all(proc_root.join("device-tree")).expect("device tree");
    fs::create_dir_all(sys_root.join("kernel/tracing")).expect("tracefs");
    fs::create_dir_all(sys_root.join("bus/pci/devices/0000:01:00.0")).expect("pci");
    fs::create_dir_all(sys_root.join("class/thermal/thermal_zone0")).expect("thermal");
    fs::write(proc_root.join("sys/kernel/osrelease"), "6.6.0-test\n").expect("osrelease");
    fs::write(proc_root.join("sys/kernel/perf_event_paranoid"), "2\n").expect("perf");
    fs::write(
        proc_root.join("device-tree/model"),
        "Raspberry Pi 5 Model B\0",
    )
    .expect("model");
    fs::write(
        sys_root.join("kernel/tracing/available_tracers"),
        "nop function\n",
    )
    .expect("available tracers");
    fs::write(sys_root.join("kernel/tracing/kprobe_events"), "").expect("kprobe events");

    let map = detect_kernel_capabilities(&KernelCapabilityPaths {
        proc_root: proc_root.clone(),
        sys_root: sys_root.clone(),
    })
    .expect("capabilities");

    assert_eq!(map.kernel_release.as_deref(), Some("6.6.0-test"));
    assert_eq!(map.tracefs_path.as_deref(), Some("kernel/tracing"));
    assert!(map.ftrace_available);
    assert!(map.perf_available);
    assert!(map.kprobe_available);
    assert_eq!(map.thermal_zones, vec!["thermal_zone0"]);
    assert_eq!(map.pci_devices, vec!["0000:01:00.0"]);
    assert!(map.board_model.expect("model").contains("Raspberry Pi 5"));
}
