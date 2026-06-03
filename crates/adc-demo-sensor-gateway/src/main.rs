use std::{
    env, fs,
    io::Write,
    net::UdpSocket,
    path::{Path, PathBuf},
    process, thread,
    time::{Duration, Instant},
};

use serde_json::json;

const MAX_DURATION_MS: u64 = 30_000;
const MAX_RETAINED_KB: u64 = 128 * 1024;
const MAX_PACKET_ATTEMPTS: u64 = 100_000;

fn main() {
    if let Err(err) = run(env::args().skip(1)) {
        eprintln!("{err}");
        process::exit(2);
    }
}

fn run(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let args = args.into_iter().collect::<Vec<_>>();
    match args.as_slice() {
        [cmd, rest @ ..] if cmd == "baseline" => run_baseline(rest),
        [cmd, rest @ ..] if cmd == "retry-storm" => run_retry_storm(rest),
        [cmd, rest @ ..] if cmd == "memory-leak" => run_memory_leak(rest),
        [flag] if flag == "-h" || flag == "--help" => {
            print_help();
            Ok(())
        }
        _ => Err("usage: adc-demo-sensor-gateway <baseline|retry-storm|memory-leak>".to_string()),
    }
}

fn print_help() {
    println!("Usage: adc-demo-sensor-gateway <baseline|retry-storm|memory-leak>");
}

fn run_baseline(args: &[String]) -> Result<(), String> {
    let config = Config::from_args(args)?;
    let started = Instant::now();
    let mut events = EventWriter::new(config.events_jsonl.clone())?;
    events.write(
        "baseline",
        "startup",
        started,
        "demo gateway accepted baseline traffic",
    )?;
    thread::sleep(config.duration);
    events.write(
        "baseline",
        "outcome",
        started,
        "baseline completed without warnings",
    )?;
    print_summary(json!({
        "scenario": "baseline",
        "status": "completed",
        "duration_ms": config.duration.as_millis(),
        "event_count": events.count(),
        "warning_count": 0,
        "retained_bytes": 0,
        "packet_attempts": 0,
    }))
}

fn run_retry_storm(args: &[String]) -> Result<(), String> {
    let config = Config::from_args(args)?;
    let packet_attempts = parse_u64_flag(args, "--packet-attempts")
        .unwrap_or_else(|_| config.duration.as_millis().max(1) as u64);
    if packet_attempts == 0 || packet_attempts > MAX_PACKET_ATTEMPTS {
        return Err(format!(
            "--packet-attempts must be between 1 and {MAX_PACKET_ATTEMPTS}"
        ));
    }

    let started = Instant::now();
    let mut events = EventWriter::new(config.events_jsonl.clone())?;
    events.write(
        "retry-storm",
        "startup",
        started,
        "demo gateway started retry-storm scenario",
    )?;
    let socket = UdpSocket::bind("127.0.0.1:0")
        .map_err(|err| format!("failed to bind localhost udp socket: {err}"))?;
    let payload = [0_u8; 64];
    for attempt in 1..=packet_attempts {
        socket
            .send_to(&payload, "127.0.0.1:9")
            .map_err(|err| format!("failed to send localhost udp packet: {err}"))?;
        events.write(
            "retry-storm",
            "retry_attempt",
            started,
            &format!("retry attempt {attempt} without backoff"),
        )?;
    }
    let warning = format!(
        "warning: demo retry storm observed {packet_attempts} immediate retries without backoff"
    );
    eprintln!("{warning}");
    if let Some(path) = config.kmsg_fixture {
        append_line(&path, &warning)?;
    }
    thread::sleep(config.duration);
    events.write(
        "retry-storm",
        "outcome",
        started,
        "retry-storm completed with warnings",
    )?;
    print_summary(json!({
        "scenario": "retry-storm",
        "status": "completed",
        "duration_ms": config.duration.as_millis(),
        "event_count": events.count(),
        "warning_count": 1,
        "retained_bytes": 0,
        "packet_attempts": packet_attempts,
    }))
}

fn run_memory_leak(args: &[String]) -> Result<(), String> {
    let config = Config::from_args(args)?;
    let retained_kb = parse_u64_flag(args, "--retained-kb")?;
    if retained_kb == 0 || retained_kb > MAX_RETAINED_KB {
        return Err(format!(
            "--retained-kb must be between 1 and {MAX_RETAINED_KB}"
        ));
    }
    let retained_bytes = retained_kb
        .checked_mul(1024)
        .ok_or_else(|| "--retained-kb is too large".to_string())? as usize;

    let started = Instant::now();
    let mut events = EventWriter::new(config.events_jsonl.clone())?;
    events.write(
        "memory-leak",
        "startup",
        started,
        "demo gateway started memory-leak scenario",
    )?;
    let mut retained = vec![0_u8; retained_bytes];
    for index in (0..retained.len()).step_by(4096) {
        retained[index] = retained[index].wrapping_add(1);
    }
    events.write(
        "memory-leak",
        "buffer_retained",
        started,
        &format!("retained {retained_bytes} bytes after synthetic error"),
    )?;
    thread::sleep(config.duration);
    std::hint::black_box(&retained);
    events.write(
        "memory-leak",
        "outcome",
        started,
        "memory-leak scenario completed with retained buffer",
    )?;
    print_summary(json!({
        "scenario": "memory-leak",
        "status": "completed",
        "duration_ms": config.duration.as_millis(),
        "event_count": events.count(),
        "warning_count": 0,
        "retained_bytes": retained_bytes,
        "packet_attempts": 0,
    }))
}

#[derive(Debug)]
struct Config {
    duration: Duration,
    events_jsonl: Option<PathBuf>,
    kmsg_fixture: Option<PathBuf>,
}

impl Config {
    fn from_args(args: &[String]) -> Result<Self, String> {
        let duration_ms = parse_u64_flag(args, "--duration-ms")?;
        if duration_ms == 0 {
            return Err("duration must be greater than zero".to_string());
        }
        if duration_ms > MAX_DURATION_MS {
            return Err(format!("--duration-ms must be <= {MAX_DURATION_MS}"));
        }
        Ok(Self {
            duration: Duration::from_millis(duration_ms),
            events_jsonl: optional_flag(args, "--events-jsonl").map(PathBuf::from),
            kmsg_fixture: optional_flag(args, "--kmsg-fixture").map(PathBuf::from),
        })
    }
}

struct EventWriter {
    path: Option<PathBuf>,
    count: u64,
}

impl EventWriter {
    fn new(path: Option<PathBuf>) -> Result<Self, String> {
        if let Some(path) = &path {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
            }
            fs::write(path, "")
                .map_err(|err| format!("failed to initialize {}: {err}", path.display()))?;
        }
        Ok(Self { path, count: 0 })
    }

    fn count(&self) -> u64 {
        self.count
    }

    fn write(
        &mut self,
        scenario: &str,
        event_type: &str,
        started: Instant,
        message: &str,
    ) -> Result<(), String> {
        self.count += 1;
        let Some(path) = &self.path else {
            return Ok(());
        };
        let event = json!({
            "scenario": scenario,
            "event_type": event_type,
            "elapsed_ms": started.elapsed().as_millis(),
            "message": message,
        });
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|err| format!("failed to open {}: {err}", path.display()))?;
        writeln!(file, "{event}")
            .map_err(|err| format!("failed to write {}: {err}", path.display()))
    }
}

fn print_summary(mut summary: serde_json::Value) -> Result<(), String> {
    summary["app"] = json!("adc-demo-sensor-gateway");
    let text = serde_json::to_string(&summary)
        .map_err(|err| format!("failed to serialize summary: {err}"))?;
    println!("{text}");
    Ok(())
}

fn append_line(path: &Path, line: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| format!("failed to open {}: {err}", path.display()))?;
    writeln!(file, "{line}").map_err(|err| format!("failed to write {}: {err}", path.display()))
}

fn parse_u64_flag(args: &[String], flag: &str) -> Result<u64, String> {
    required_flag(args, flag)?
        .parse::<u64>()
        .map_err(|err| format!("invalid {flag}: {err}"))
}

fn required_flag<'a>(args: &'a [String], flag: &str) -> Result<&'a str, String> {
    optional_flag(args, flag).ok_or_else(|| format!("missing required flag: {flag}"))
}

fn optional_flag<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].as_str())
}
