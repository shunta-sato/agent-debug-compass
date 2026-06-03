use adc_core::{extract_evidence_facts_from_ref, DataQuality};
use serde_json::json;

fn dq() -> DataQuality {
    DataQuality {
        clock_confidence: "medium".to_string(),
        ..Default::default()
    }
}

#[test]
fn extracts_service_state_facts_without_port_conflation() {
    let text = r#"{
      "service": "ssh",
      "availability": "available",
      "active_state": "active",
      "sub_state": "running",
      "load_state": "loaded",
      "unit_id": "ssh.service",
      "main_pid": 1234,
      "fragment_path": "/usr/lib/systemd/system/ssh.service"
    }"#;

    let facts = extract_evidence_facts_from_ref(
        "service_state",
        "artifact://raw/service_state.json",
        "raw",
        "application/json",
        text,
        &dq(),
    );

    assert!(facts
        .iter()
        .any(|fact| fact.fact_id == "service.availability" && fact.value == json!("available")));
    assert!(facts.iter().all(|fact| fact.fact_id != "port.availability"));
}

#[test]
fn extracts_port_availability_as_a_separate_fact() {
    let text = r#"{
      "availability": "unavailable",
      "unavailable_reason": "pid 1234 fd unavailable: Permission denied",
      "socket_inode_count": null,
      "matched_socket_table_count": null
    }"#;

    let facts = extract_evidence_facts_from_ref(
        "service.port_summary",
        "artifact://service_investigations/ssh/port_summary.json",
        "service_investigation",
        "application/json",
        text,
        &dq(),
    );

    assert!(facts
        .iter()
        .any(|fact| fact.fact_id == "port.availability" && fact.value == json!("unavailable")));
    assert!(facts
        .iter()
        .all(|fact| fact.fact_id != "service.availability"));
}

#[test]
fn extracts_bounded_text_signal_facts() {
    let text = "info start\nwarning retrying request\nerror timeout request_id=abc\n";

    let facts = extract_evidence_facts_from_ref(
        "log",
        "artifact://raw/app.log",
        "raw",
        "text/plain",
        text,
        &dq(),
    );

    assert!(facts
        .iter()
        .any(|fact| fact.fact_id == "signal.has_signal_words" && fact.value == json!(true)));
    assert!(facts
        .iter()
        .any(|fact| fact.fact_id == "signal.signal_line_count" && fact.value == json!(2)));
}

#[test]
fn extracts_resource_facts_from_runtime_refs() {
    let cpu = concat!(
        r#"{"sample_index":0,"sample":{"total_jiffies":100,"idle_jiffies":80}}"#,
        "\n",
        r#"{"sample_index":1,"sample":{"total_jiffies":200,"idle_jiffies":120}}"#,
        "\n"
    );
    let cpu_facts = extract_evidence_facts_from_ref(
        "cpu",
        "artifact://raw/cpu.jsonl",
        "raw",
        "application/jsonl",
        cpu,
        &dq(),
    );
    assert!(cpu_facts
        .iter()
        .any(|fact| { fact.fact_id == "resource.cpu_busy_percent" && fact.value == json!(60.0) }));

    let memory = r#"{"sample":{"mem_available_kb":2048}}"#;
    let memory_facts = extract_evidence_facts_from_ref(
        "memory",
        "artifact://raw/memory.jsonl",
        "raw",
        "application/jsonl",
        memory,
        &dq(),
    );
    assert!(memory_facts.iter().any(|fact| {
        fact.fact_id == "resource.memory_available_bytes" && fact.value == json!(2_097_152)
    }));

    let network =
        r#"{"sample":{"interfaces":[{"rx_bytes":10,"tx_bytes":20},{"rx_bytes":5,"tx_bytes":7}]}}"#;
    let network_facts = extract_evidence_facts_from_ref(
        "network",
        "artifact://raw/network.jsonl",
        "raw",
        "application/jsonl",
        network,
        &dq(),
    );
    assert!(network_facts
        .iter()
        .any(|fact| fact.fact_id == "resource.network_rx_bytes" && fact.value == json!(15)));
    assert!(network_facts
        .iter()
        .any(|fact| fact.fact_id == "resource.network_tx_bytes" && fact.value == json!(27)));

    let thermal = r#"{"zones":[{"temp_millicelsius":55100}],"throttled_state":"none"}"#;
    let thermal_facts = extract_evidence_facts_from_ref(
        "thermal",
        "artifact://raw/thermal_snapshot.json",
        "raw",
        "application/json",
        thermal,
        &dq(),
    );
    assert!(thermal_facts.iter().any(|fact| {
        fact.fact_id == "resource.thermal_millidegree_c" && fact.value == json!(55100)
    }));
    assert!(thermal_facts
        .iter()
        .any(|fact| fact.fact_id == "resource.throttled_state" && fact.value == json!("none")));
}
