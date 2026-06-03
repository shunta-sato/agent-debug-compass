use std::{
    collections::BTreeMap,
    env, fs,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{AdcError, AdcResult};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[serde(rename = "profile")]
    pub id: String,
    pub sampling: Sampling,
    pub always_on: AlwaysOn,
    pub budgets: Budgets,
    #[serde(default)]
    pub triggers: Vec<TriggerRule>,
    #[serde(default)]
    pub capture_profiles: BTreeMap<String, CaptureProfile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sampling {
    pub interval_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlwaysOn {
    pub collectors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budgets {
    pub max_daemon_cpu_percent: u8,
    pub max_memory_mb: u64,
    pub max_artifact_mb_per_run: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerRule {
    pub name: String,
    #[serde(rename = "type")]
    pub rule_type: RuleType,
    #[serde(default)]
    pub signal: Option<String>,
    #[serde(default)]
    pub op: Option<String>,
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default)]
    pub duration_sec: Option<u64>,
    #[serde(default)]
    pub capture_profile: Option<String>,
    #[serde(default)]
    pub severity_at_least: Option<String>,
    #[serde(default)]
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleType {
    Threshold,
    ThresholdDuration,
    Delta,
    DeltaRate,
    MissingSample,
    KmsgPattern,
    Burst,
    BaselineDeviation,
    SequencePattern,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureProfile {
    pub duration_sec: u64,
    #[serde(default)]
    pub collectors: Vec<String>,
}

pub fn parse_profile(input: &str) -> AdcResult<Profile> {
    let profile: Profile =
        yaml_serde::from_str(input).map_err(|err| AdcError::ProfileParse(err.to_string()))?;
    validate_profile(&profile)?;
    Ok(profile)
}

pub fn load_profile(profile_dir: impl AsRef<Path>, profile_id: &str) -> AdcResult<Profile> {
    validate_profile_id(profile_id)?;
    let profile_dir = profile_dir.as_ref();
    let candidates = [
        profile_dir.join(format!("{profile_id}.yaml")),
        profile_dir.join(format!("{profile_id}.yml")),
    ];
    let path = candidates
        .iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            AdcError::ProfileValidation(format!(
                "profile {profile_id} not found in {}",
                profile_dir.display()
            ))
        })?;
    let contents = fs::read_to_string(path).map_err(|err| {
        AdcError::ProfileParse(format!("failed to read profile {}: {err}", path.display()))
    })?;
    parse_profile(&contents)
}

pub fn default_profile_dir() -> PathBuf {
    env::var_os("ADC_PROFILE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("profiles"))
}

fn validate_profile(profile: &Profile) -> AdcResult<()> {
    require_non_empty("profile", &profile.id)?;
    if profile.sampling.interval_ms == 0 {
        return validation_error("sampling.interval_ms must be greater than zero");
    }
    if profile.always_on.collectors.is_empty() {
        return validation_error("always_on.collectors must contain at least one collector");
    }
    for collector in &profile.always_on.collectors {
        require_non_empty("always_on.collectors[]", collector)?;
    }
    if profile.budgets.max_daemon_cpu_percent == 0 || profile.budgets.max_daemon_cpu_percent > 100 {
        return validation_error("budgets.max_daemon_cpu_percent must be between 1 and 100");
    }
    if profile.budgets.max_memory_mb == 0 {
        return validation_error("budgets.max_memory_mb must be greater than zero");
    }
    if profile.budgets.max_artifact_mb_per_run == 0 {
        return validation_error("budgets.max_artifact_mb_per_run must be greater than zero");
    }
    for trigger in &profile.triggers {
        validate_trigger(trigger)?;
    }
    Ok(())
}

fn validate_trigger(trigger: &TriggerRule) -> AdcResult<()> {
    require_non_empty("triggers[].name", &trigger.name)?;
    if let Some(capture_profile) = &trigger.capture_profile {
        require_non_empty("triggers[].capture_profile", capture_profile)?;
    }
    Ok(())
}

fn validate_profile_id(profile_id: &str) -> AdcResult<()> {
    if profile_id.trim().is_empty() {
        return validation_error("profile id must not be empty");
    }
    let path = Path::new(profile_id);
    let mut components = path.components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => validation_error("profile id must be a single relative path segment"),
    }
}

fn require_non_empty(field: &str, value: &str) -> AdcResult<()> {
    if value.trim().is_empty() {
        return validation_error(format!("{field} must not be empty"));
    }
    Ok(())
}

fn validation_error<T>(message: impl Into<String>) -> AdcResult<T> {
    Err(AdcError::ProfileValidation(message.into()))
}
