use std::{
    env, fs,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    process, thread,
    time::{Duration, Instant},
};

use serde_json::json;

fn main() {
    if let Err(err) = run(env::args().skip(1)) {
        eprintln!("{err}");
        process::exit(2);
    }
}

fn run(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let args = args.into_iter().collect::<Vec<_>>();
    match args.as_slice() {
        [cmd, rest @ ..] if cmd == "cpu-spike" => cpu_spike(rest),
        [cmd, rest @ ..] if cmd == "memory-pressure" => memory_pressure(rest),
        [cmd, rest @ ..] if cmd == "network-loopback" => network_loopback(rest),
        [cmd, rest @ ..] if cmd == "kmsg-mock" => kmsg_mock(rest),
        [flag] if flag == "-h" || flag == "--help" => {
            print_help();
            Ok(())
        }
        _ => Err(
            "usage: adc-workload <cpu-spike|memory-pressure|network-loopback|kmsg-mock>"
                .to_string(),
        ),
    }
}

fn print_help() {
    println!("Usage: adc-workload <cpu-spike|memory-pressure|network-loopback|kmsg-mock>");
}

fn cpu_spike(args: &[String]) -> Result<(), String> {
    let duration = Duration::from_millis(parse_u64_flag(args, "--duration-ms")?);
    require_nonzero_duration(duration)?;
    let deadline = Instant::now() + duration;
    let mut counter = 0_u64;
    while Instant::now() < deadline {
        counter = counter.wrapping_add(1);
        std::hint::black_box(counter);
    }
    print_completed(json!({
        "workload": "cpu-spike",
        "duration_ms": duration.as_millis(),
        "iterations": counter,
    }))
}

fn memory_pressure(args: &[String]) -> Result<(), String> {
    let mb = parse_u64_flag(args, "--mb")?;
    if mb == 0 || mb > 512 {
        return Err("--mb must be between 1 and 512".to_string());
    }
    let duration = Duration::from_millis(parse_u64_flag(args, "--duration-ms")?);
    require_nonzero_duration(duration)?;
    let bytes = mb
        .checked_mul(1024 * 1024)
        .ok_or_else(|| "--mb is too large".to_string())? as usize;
    let mut allocation = vec![0_u8; bytes];
    for index in (0..allocation.len()).step_by(4096) {
        allocation[index] = allocation[index].wrapping_add(1);
    }
    thread::sleep(duration);
    std::hint::black_box(&allocation);
    print_completed(json!({
        "workload": "memory-pressure",
        "duration_ms": duration.as_millis(),
        "mb": mb,
    }))
}

fn network_loopback(args: &[String]) -> Result<(), String> {
    let bytes = parse_u64_flag(args, "--bytes")?;
    if bytes == 0 || bytes > 256 * 1024 * 1024 {
        return Err("--bytes must be between 1 and 268435456".to_string());
    }

    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|err| format!("bind loopback: {err}"))?;
    let addr = listener
        .local_addr()
        .map_err(|err| format!("read loopback address: {err}"))?;
    let server = thread::spawn(move || -> Result<u64, String> {
        let (mut stream, _) = listener
            .accept()
            .map_err(|err| format!("accept loopback: {err}"))?;
        let mut buffer = [0_u8; 8192];
        let mut received = 0_u64;
        loop {
            let read = stream
                .read(&mut buffer)
                .map_err(|err| format!("read loopback: {err}"))?;
            if read == 0 {
                break;
            }
            received += read as u64;
        }
        Ok(received)
    });

    let mut stream = TcpStream::connect(addr).map_err(|err| format!("connect loopback: {err}"))?;
    let chunk = vec![0_u8; 8192];
    let mut remaining = bytes;
    while remaining > 0 {
        let write_len = remaining.min(chunk.len() as u64) as usize;
        stream
            .write_all(&chunk[..write_len])
            .map_err(|err| format!("write loopback: {err}"))?;
        remaining -= write_len as u64;
    }
    stream
        .shutdown(Shutdown::Write)
        .map_err(|err| format!("shutdown loopback writer: {err}"))?;
    let received = server
        .join()
        .map_err(|_| "loopback server thread panicked".to_string())??;
    if received != bytes {
        return Err(format!(
            "loopback received {received} bytes, expected {bytes}"
        ));
    }

    print_completed(json!({
        "workload": "network-loopback",
        "bytes": bytes,
        "received_bytes": received,
    }))
}

fn kmsg_mock(args: &[String]) -> Result<(), String> {
    let message = required_flag(args, "--message")?;
    let output = required_flag(args, "--output")?;
    fs::write(output, format!("{message}\n"))
        .map_err(|err| format!("failed to write kmsg mock {output}: {err}"))?;
    print_completed(json!({
        "workload": "kmsg-mock",
        "output": output,
    }))
}

fn print_completed(mut payload: serde_json::Value) -> Result<(), String> {
    payload["status"] = json!("completed");
    let text = serde_json::to_string(&payload)
        .map_err(|err| format!("failed to serialize workload result: {err}"))?;
    println!("{text}");
    Ok(())
}

fn parse_u64_flag(args: &[String], flag: &str) -> Result<u64, String> {
    let value = required_flag(args, flag)?;
    value
        .parse::<u64>()
        .map_err(|err| format!("invalid {flag}: {err}"))
}

fn required_flag<'a>(args: &'a [String], flag: &str) -> Result<&'a str, String> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].as_str())
        .ok_or_else(|| format!("missing required flag: {flag}"))
}

fn require_nonzero_duration(duration: Duration) -> Result<(), String> {
    if duration.is_zero() {
        return Err("duration must be greater than zero".to_string());
    }
    Ok(())
}
