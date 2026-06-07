use std::{fs, path::Path};

use serde::Serialize;

use crate::{AdcError, AdcResult};

pub(super) fn write_json(path: &Path, value: &impl Serialize) -> AdcResult<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|err| AdcError::Artifact(format!("recorder json serialization failed: {err}")))?;
    fs::write(path, bytes)
        .map_err(|err| AdcError::Artifact(format!("failed to write {}: {err}", path.display())))
}

pub(super) fn write_jsonl<T: Serialize>(path: &Path, samples: &[T]) -> AdcResult<()> {
    let mut lines = String::new();
    for sample in samples {
        let line = serde_json::to_string(sample).map_err(|err| {
            AdcError::Artifact(format!("recorder sample serialization failed: {err}"))
        })?;
        lines.push_str(&line);
        lines.push('\n');
    }
    fs::write(path, lines)
        .map_err(|err| AdcError::Artifact(format!("failed to write {}: {err}", path.display())))
}
