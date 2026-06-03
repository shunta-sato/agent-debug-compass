use std::{env, process};

const OPS: [&str; 6] = [
    "capability-report",
    "ftrace-start",
    "ftrace-stop",
    "perf-start",
    "perf-stop",
    "kmsg-read",
];

fn main() {
    if let Err(err) = run(env::args().skip(1)) {
        eprintln!("{err}");
        process::exit(2);
    }
}

fn run(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let args = args.into_iter().collect::<Vec<_>>();
    match args.as_slice() {
        [flag] if flag == "--list-ops" => print_ops(),
        [op] if op == "capability-report" => capability_report(),
        [op] => {
            adc_core::parse_privileged_operation(op).map_err(|err| err.to_string())?;
            Err(format!(
                "operation requires an explicit implementation: {op}"
            ))
        }
        _ => Err("usage: adc-priv-helper <--list-ops|capability-report>".to_string()),
    }
}

fn print_ops() -> Result<(), String> {
    serde_json::to_writer_pretty(std::io::stdout(), &serde_json::json!({ "operations": OPS }))
        .map_err(|err| format!("failed to serialize operations: {err}"))?;
    println!();
    Ok(())
}

fn capability_report() -> Result<(), String> {
    let map = adc_core::detect_default_kernel_capabilities().map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &map)
        .map_err(|err| format!("failed to serialize capability report: {err}"))?;
    println!();
    Ok(())
}
