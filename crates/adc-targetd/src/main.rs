use std::{env, process, time::Duration};

fn main() {
    if let Err(err) = run(env::args().skip(1)) {
        eprintln!("{err}");
        process::exit(2);
    }
}

fn run(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let args = args.into_iter().collect::<Vec<_>>();
    match args.as_slice() {
        [] => print_status(),
        [flag] if flag == "--status-json" => print_status(),
        [flag] if flag == "--service" => service_loop(),
        [flag] if flag == "--service-once" => service_once(),
        [flag, duration_ms] if flag == "--service-for-ms" => service_for_ms(duration_ms),
        [flag] if flag == "-h" || flag == "--help" => {
            print_help();
            Ok(())
        }
        _ => Err(
            "usage: adc-targetd [--status-json|--service|--service-once|--service-for-ms <ms>]"
                .to_string(),
        ),
    }
}

fn print_status() -> Result<(), String> {
    let status = adc_core::status_for("adc-targetd", adc_core::VERSION);
    serde_json::to_writer_pretty(std::io::stdout(), &status)
        .map_err(|err| format!("failed to serialize status: {err}"))?;
    println!();
    Ok(())
}

fn print_help() {
    println!("Usage: adc-targetd [--status-json|--service|--service-once|--service-for-ms <ms>]");
}

fn service_once() -> Result<(), String> {
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let state = adc_core::initialize_state(&artifact_root).map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &state)
        .map_err(|err| format!("failed to serialize daemon state: {err}"))?;
    println!();
    Ok(())
}

fn service_loop() -> Result<(), String> {
    loop {
        let artifact_root = adc_core::snapshot::default_artifact_root();
        let profile_dir = adc_core::default_profile_dir();
        adc_core::run_service_for(&artifact_root, &profile_dir, Duration::from_secs(60))
            .map_err(|err| err.to_string())?;
    }
}

fn service_for_ms(duration_ms: &str) -> Result<(), String> {
    let duration_ms = duration_ms
        .parse::<u64>()
        .map_err(|err| format!("invalid --service-for-ms value: {err}"))?;
    if duration_ms == 0 {
        return Err("--service-for-ms must be greater than zero".to_string());
    }
    let artifact_root = adc_core::snapshot::default_artifact_root();
    let profile_dir = adc_core::default_profile_dir();
    let summary = adc_core::run_service_for(
        &artifact_root,
        &profile_dir,
        Duration::from_millis(duration_ms),
    )
    .map_err(|err| err.to_string())?;
    serde_json::to_writer_pretty(std::io::stdout(), &summary)
        .map_err(|err| format!("failed to serialize service summary: {err}"))?;
    println!();
    Ok(())
}
