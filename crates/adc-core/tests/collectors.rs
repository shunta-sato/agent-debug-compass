use adc_core::collectors::{
    parse_meminfo, parse_net_dev, parse_proc_stat, CpuSample, MemorySample, NetworkSample,
};

#[test]
fn parses_proc_stat_cpu_totals_and_usage_delta() {
    let before = parse_proc_stat(
        r#"
cpu  100 0 50 850 0 0 0 0 0 0
cpu0 25 0 10 215 0 0 0 0 0 0
cpu1 25 0 10 215 0 0 0 0 0 0
"#,
    )
    .expect("parse before");
    let after = parse_proc_stat(
        r#"
cpu  130 0 70 900 0 0 0 0 0 0
cpu0 30 0 20 240 0 0 0 0 0 0
cpu1 30 0 20 240 0 0 0 0 0 0
"#,
    )
    .expect("parse after");

    assert_eq!(before.cpu_count, 2);
    assert_eq!(after.cpu_count, 2);
    assert_eq!(after.total_jiffies, 1100);
    assert_eq!(after.idle_jiffies, 900);
    assert_eq!(
        CpuSample::usage_percent_between(&before, &after),
        Some(50.0)
    );
}

#[test]
fn parses_meminfo_required_fields() {
    let sample = parse_meminfo(
        r#"
MemTotal:        8065432 kB
MemFree:          123456 kB
MemAvailable:    654321 kB
Buffers:           11111 kB
Cached:           222222 kB
"#,
    )
    .expect("parse meminfo");

    assert_eq!(
        sample,
        MemorySample {
            mem_total_kb: 8_065_432,
            mem_free_kb: 123_456,
            mem_available_kb: 654_321,
        }
    );
}

#[test]
fn parses_net_dev_interfaces_and_counters() {
    let sample = parse_net_dev(
        r#"
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 1000 10 0 0 0 0 0 0 1000 10 0 0 0 0 0 0
  eth0: 2048 20 1 2 0 0 0 0 4096 40 3 4 0 0 0 0
"#,
    )
    .expect("parse net dev");

    assert_eq!(
        sample.interfaces,
        vec![
            NetworkSample {
                interface: "lo".to_string(),
                rx_bytes: 1000,
                rx_packets: 10,
                rx_errors: 0,
                rx_drops: 0,
                tx_bytes: 1000,
                tx_packets: 10,
                tx_errors: 0,
                tx_drops: 0,
            },
            NetworkSample {
                interface: "eth0".to_string(),
                rx_bytes: 2048,
                rx_packets: 20,
                rx_errors: 1,
                rx_drops: 2,
                tx_bytes: 4096,
                tx_packets: 40,
                tx_errors: 3,
                tx_drops: 4,
            }
        ]
    );
}

#[test]
fn meminfo_reports_missing_required_field() {
    let err =
        parse_meminfo("MemTotal: 100 kB\nMemFree: 50 kB\n").expect_err("missing field must fail");

    assert!(err.to_string().contains("MemAvailable"));
}
