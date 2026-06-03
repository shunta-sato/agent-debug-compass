use std::{fs, process::Command};

#[test]
fn kmsg_mock_writes_requested_message_to_output_file() {
    let temp = tempfile::tempdir().expect("tempdir");
    let output_path = temp.path().join("kmsg.log");

    let output = Command::new(env!("CARGO_BIN_EXE_adc-workload"))
        .args([
            "kmsg-mock",
            "--message",
            "warning: synthetic timeout",
            "--output",
            output_path.to_str().expect("path"),
        ])
        .output()
        .expect("run kmsg mock");

    assert!(
        output.status.success(),
        "kmsg mock failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let contents = fs::read_to_string(output_path).expect("kmsg output");
    assert!(contents.contains("warning: synthetic timeout"));
}

#[test]
fn bounded_workloads_complete_successfully() {
    for args in [
        &["cpu-spike", "--duration-ms", "10"][..],
        &["memory-pressure", "--mb", "1", "--duration-ms", "10"][..],
        &["network-loopback", "--bytes", "1024"][..],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_adc-workload"))
            .args(args)
            .output()
            .expect("run workload");

        assert!(
            output.status.success(),
            "workload {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
        assert!(stdout.contains("\"status\":\"completed\""));
    }
}
