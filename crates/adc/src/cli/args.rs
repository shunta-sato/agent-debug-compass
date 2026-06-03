use std::{
    path::{Component, Path},
    time::Duration,
};

pub(super) fn required_flag<'a>(args: &'a [String], flag: &str) -> Result<&'a str, String> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].as_str())
        .ok_or_else(|| format!("missing required flag: {flag}"))
}

pub(super) fn optional_flag<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].as_str())
}

pub(super) fn flag_values<'a>(args: &'a [String], flag: &str) -> Vec<&'a str> {
    args.windows(2)
        .filter(|window| window[0] == flag)
        .map(|window| window[1].as_str())
        .collect()
}

pub(super) fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

pub(super) fn capture_duration(args: &[String]) -> Result<Duration, String> {
    let duration_ms = optional_flag(args, "--duration-ms");
    let duration_sec = optional_flag(args, "--duration-sec");
    match (duration_ms, duration_sec) {
        (Some(_), Some(_)) => {
            Err("use only one of --duration-ms or --duration-sec for capture".to_string())
        }
        (Some(value), None) => parse_millis(value),
        (None, Some(value)) => parse_seconds(value),
        (None, None) => {
            Err("missing required capture duration: --duration-sec or --duration-ms".to_string())
        }
    }
}

pub(super) fn parse_millis(value: &str) -> Result<Duration, String> {
    let millis = value
        .parse::<u64>()
        .map_err(|err| format!("invalid millisecond value {value}: {err}"))?;
    if millis == 0 {
        return Err("duration/interval milliseconds must be greater than zero".to_string());
    }
    Ok(Duration::from_millis(millis))
}

fn parse_seconds(value: &str) -> Result<Duration, String> {
    let seconds = value
        .parse::<u64>()
        .map_err(|err| format!("invalid second value {value}: {err}"))?;
    if seconds == 0 {
        return Err("duration seconds must be greater than zero".to_string());
    }
    let millis = seconds
        .checked_mul(1000)
        .ok_or_else(|| format!("duration seconds value is too large: {value}"))?;
    Ok(Duration::from_millis(millis))
}

pub(super) fn validate_target_id(target_id: &str) -> Result<(), String> {
    if target_id.trim().is_empty() {
        return Err("target id must not be empty".to_string());
    }
    let path = Path::new(target_id);
    let mut components = path.components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err("target id must be a single relative path segment".to_string()),
    }
}
