use crate::{AdcError, AdcResult};

pub(super) fn validate_preservation_reason_name(name: &str) -> AdcResult<()> {
    let normalized = name.to_ascii_lowercase().replace(['_', '-'], " ");
    let forbidden = [
        "root cause",
        "caused",
        "cause detected",
        "driver bug",
        "bad firmware",
    ];
    if forbidden.iter().any(|needle| normalized.contains(needle)) {
        return Err(AdcError::Artifact(format!(
            "recorder trigger name must be symptom/event oriented, not a root-cause claim: {name}"
        )));
    }
    Ok(())
}

pub fn validate_recorder_file_segment(value: &str, label: &str) -> AdcResult<()> {
    if value.trim().is_empty() {
        return Err(AdcError::Artifact(format!("{label} must not be empty")));
    }
    let valid = value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if !valid || value == "." || value == ".." {
        return Err(AdcError::Artifact(format!(
            "{label} must be a single safe recorder file segment"
        )));
    }
    Ok(())
}
