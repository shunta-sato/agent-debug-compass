use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Component, Path},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{AdcError, AdcResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactManifest {
    pub run_id: String,
    pub profile_id: String,
    pub target_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fleet_run_id: Option<String>,
    pub created_at_unix_ms: u128,
    pub artifacts: Vec<ArtifactEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactEntry {
    pub path: String,
    pub uri: String,
    pub source: String,
    pub size_bytes: u64,
    pub sha256: String,
}

impl ArtifactManifest {
    pub fn new(run_id: impl Into<String>, profile_id: impl Into<String>) -> Self {
        Self::new_for_target(run_id, profile_id, "local", None)
    }

    pub fn new_for_target(
        run_id: impl Into<String>,
        profile_id: impl Into<String>,
        target_id: impl Into<String>,
        fleet_run_id: Option<String>,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            profile_id: profile_id.into(),
            target_id: target_id.into(),
            fleet_run_id,
            created_at_unix_ms: unix_epoch_ms(),
            artifacts: Vec::new(),
        }
    }

    pub fn add_file(
        &mut self,
        artifact_root: impl AsRef<Path>,
        relative_path: impl AsRef<Path>,
        source: impl Into<String>,
    ) -> AdcResult<ArtifactEntry> {
        validate_relative_artifact_path(relative_path.as_ref())?;
        let path = artifact_root.as_ref().join(relative_path.as_ref());
        let metadata = fs::metadata(&path).map_err(|err| {
            AdcError::Artifact(format!("metadata failed for {}: {err}", path.display()))
        })?;
        if !metadata.is_file() {
            return Err(AdcError::Artifact(format!(
                "artifact path is not a regular file: {}",
                path.display()
            )));
        }

        let path_string = relative_path.as_ref().to_string_lossy().to_string();
        let entry = ArtifactEntry {
            uri: format!("artifact://{}", path_string),
            path: path_string,
            source: source.into(),
            size_bytes: metadata.len(),
            sha256: sha256_file(&path).map_err(|err| {
                AdcError::Artifact(format!("sha256 failed for {}: {err}", path.display()))
            })?,
        };
        self.artifacts.push(entry.clone());
        Ok(entry)
    }

    pub fn write_json(&self, path: impl AsRef<Path>) -> AdcResult<()> {
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent).map_err(|err| {
                AdcError::Artifact(format!(
                    "failed to create manifest directory {}: {err}",
                    parent.display()
                ))
            })?;
        }
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|err| AdcError::Artifact(format!("manifest serialization failed: {err}")))?;
        fs::write(path.as_ref(), bytes).map_err(|err| {
            AdcError::Artifact(format!(
                "failed to write manifest {}: {err}",
                path.as_ref().display()
            ))
        })
    }

    pub fn read_json(path: impl AsRef<Path>) -> AdcResult<Self> {
        let bytes = fs::read(path.as_ref()).map_err(|err| {
            AdcError::Artifact(format!(
                "failed to read manifest {}: {err}",
                path.as_ref().display()
            ))
        })?;
        serde_json::from_slice(&bytes)
            .map_err(|err| AdcError::Artifact(format!("manifest parse failed: {err}")))
    }
}

fn validate_relative_artifact_path(path: &Path) -> AdcResult<()> {
    if path.as_os_str().is_empty() {
        return Err(AdcError::Artifact(
            "relative artifact path must not be empty".to_string(),
        ));
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => {
                return Err(AdcError::Artifact(
                    "relative artifact path must stay within artifact root".to_string(),
                ));
            }
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn unix_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}
