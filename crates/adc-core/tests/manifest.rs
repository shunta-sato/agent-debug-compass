use std::{fs, path::Path};

use adc_core::artifact::ArtifactManifest;

#[test]
fn manifest_records_artifact_hash_size_source_and_json_round_trip() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_file(temp.path().join("raw/cpu.jsonl"), "sample\n");

    let mut manifest = ArtifactManifest::new("R001", "pi5_basic");
    let entry = manifest
        .add_file(temp.path(), "raw/cpu.jsonl", "cpu")
        .expect("add artifact");

    assert_eq!(entry.path, "raw/cpu.jsonl");
    assert_eq!(entry.source, "cpu");
    assert_eq!(entry.size_bytes, 7);
    assert_eq!(
        entry.sha256,
        "aaf9ff488e0767da5ea1d56118e6f65a16c5633b0cefc1fa089bd3ab1810613d"
    );

    let manifest_path = temp.path().join("manifest.json");
    manifest.write_json(&manifest_path).expect("write manifest");
    let decoded = ArtifactManifest::read_json(&manifest_path).expect("read manifest");

    assert_eq!(decoded.run_id, "R001");
    assert_eq!(decoded.profile_id, "pi5_basic");
    assert_eq!(decoded.artifacts.len(), 1);
    assert_eq!(decoded.artifacts[0].sha256, entry.sha256);
}

#[test]
fn manifest_rejects_paths_outside_artifact_root() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut manifest = ArtifactManifest::new("R001", "pi5_basic");

    let err = manifest
        .add_file(temp.path(), "../outside.log", "cpu")
        .expect_err("path traversal must be rejected");

    assert!(err.to_string().contains("relative artifact path"));
}

fn write_file(path: impl AsRef<Path>, contents: &str) {
    if let Some(parent) = path.as_ref().parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, contents).expect("write fixture");
}
