use std::{
    collections::BTreeSet,
    fmt::Write as _,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{AdcError, AdcResult, FleetTargetConfig};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedFleetRegistry {
    pub schema_version: String,
    pub targets: Vec<ManagedFleetTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedFleetTarget {
    pub target_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub transport: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_server_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_token_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_ca_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_client_cert_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_client_key_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_server_name: Option<String>,
    pub tags: Vec<String>,
    pub trust_state: String,
    pub enrollment_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedFleetInviteOptions {
    pub target_id_hint: Option<String>,
    pub ttl: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedFleetInvite {
    pub schema_version: String,
    pub invite_id: String,
    pub join_code: String,
    pub controller_pin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id_hint: Option<String>,
    pub expires_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedFleetInventoryMaterialization {
    pub schema_version: String,
    pub selector: String,
    pub target_count: usize,
    pub registry_path: PathBuf,
    pub inventory_path: PathBuf,
    pub targets: Vec<FleetTargetConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedFleetEnrollmentKit {
    pub schema_version: String,
    pub target: ManagedFleetTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ManagedFleetControllerIdentity {
    schema_version: String,
    controller_id: String,
    controller_pin: String,
    created_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StoredManagedFleetInvite {
    schema_version: String,
    invite_id: String,
    join_code_sha256: String,
    controller_pin: String,
    target_id_hint: Option<String>,
    expires_at_unix: u64,
    used_at_unix: Option<u64>,
}

#[derive(Debug, Serialize)]
struct ManagedFleetInventoryFile {
    targets: Vec<FleetTargetConfig>,
}

pub fn managed_fleet_registry_path(artifact_root: impl AsRef<Path>) -> PathBuf {
    artifact_root.as_ref().join("fleet").join("targets.json")
}

pub fn initialize_managed_fleet_registry(
    artifact_root: impl AsRef<Path>,
) -> AdcResult<ManagedFleetRegistry> {
    let artifact_root = artifact_root.as_ref();
    let registry = if managed_fleet_registry_path(artifact_root).is_file() {
        read_managed_fleet_registry(artifact_root)?
    } else {
        ManagedFleetRegistry {
            schema_version: "obs.managed_fleet_registry.v1".to_string(),
            targets: Vec::new(),
        }
    };
    write_managed_fleet_registry(artifact_root, &registry)?;
    ensure_controller_identity(artifact_root)?;
    Ok(registry)
}

pub fn read_managed_fleet_registry(
    artifact_root: impl AsRef<Path>,
) -> AdcResult<ManagedFleetRegistry> {
    let path = managed_fleet_registry_path(artifact_root);
    if !path.is_file() {
        return Ok(ManagedFleetRegistry {
            schema_version: "obs.managed_fleet_registry.v1".to_string(),
            targets: Vec::new(),
        });
    }
    let bytes = fs::read(&path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read managed fleet registry {}: {err}",
            path.display()
        ))
    })?;
    serde_json::from_slice(&bytes)
        .map_err(|err| AdcError::Artifact(format!("managed fleet registry parse failed: {err}")))
}

pub fn upsert_managed_fleet_target(
    artifact_root: impl AsRef<Path>,
    mut target: ManagedFleetTarget,
) -> AdcResult<ManagedFleetRegistry> {
    validate_managed_target(&mut target)?;
    let artifact_root = artifact_root.as_ref();
    let mut registry = read_managed_fleet_registry(artifact_root)?;
    if registry.schema_version.is_empty() {
        registry.schema_version = "obs.managed_fleet_registry.v1".to_string();
    }
    match registry
        .targets
        .iter_mut()
        .find(|existing| existing.target_id == target.target_id)
    {
        Some(existing) => *existing = target,
        None => registry.targets.push(target),
    }
    registry
        .targets
        .sort_by(|left, right| left.target_id.cmp(&right.target_id));
    write_managed_fleet_registry(artifact_root, &registry)?;
    Ok(registry)
}

pub fn enroll_managed_fleet_kit(
    artifact_root: impl AsRef<Path>,
    kit_path: impl AsRef<Path>,
) -> AdcResult<ManagedFleetRegistry> {
    let path = kit_path.as_ref();
    let bytes = fs::read(path).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to read managed MCP enrollment kit {}: {err}",
            path.display()
        ))
    })?;
    let kit: ManagedFleetEnrollmentKit = serde_json::from_slice(&bytes).map_err(|err| {
        AdcError::ProfileValidation(format!("managed MCP enrollment kit parse failed: {err}"))
    })?;
    if kit.schema_version != "obs.managed_mcp_enrollment_kit.v1" {
        return Err(AdcError::ProfileValidation(format!(
            "unsupported managed MCP enrollment kit schema {}",
            kit.schema_version
        )));
    }
    upsert_managed_fleet_target(artifact_root, kit.target)
}

pub fn create_managed_fleet_invite(
    artifact_root: impl AsRef<Path>,
    options: ManagedFleetInviteOptions,
) -> AdcResult<ManagedFleetInvite> {
    let artifact_root = artifact_root.as_ref();
    if let Some(target_id) = options.target_id_hint.as_deref() {
        validate_segment(target_id, "target_id_hint")?;
    }
    let identity = ensure_controller_identity(artifact_root)?;
    let now = unix_now();
    let invite_id = random_hex(8)?;
    let join_code = format_join_code(&random_bytes(16)?);
    let expires_at_unix = if options.ttl.is_zero() {
        now.saturating_sub(1)
    } else {
        now.saturating_add(options.ttl.as_secs())
    };
    let stored = StoredManagedFleetInvite {
        schema_version: "obs.managed_fleet_invite_record.v1".to_string(),
        invite_id: invite_id.clone(),
        join_code_sha256: join_code_hash(&invite_id, &join_code),
        controller_pin: identity.controller_pin.clone(),
        target_id_hint: options.target_id_hint.clone(),
        expires_at_unix,
        used_at_unix: None,
    };
    write_json_private(invite_path(artifact_root, &invite_id), &stored)?;
    Ok(ManagedFleetInvite {
        schema_version: "obs.managed_fleet_invite.v1".to_string(),
        invite_id,
        join_code,
        controller_pin: identity.controller_pin,
        target_id_hint: options.target_id_hint,
        expires_at_unix,
    })
}

pub fn verify_and_consume_managed_fleet_invite(
    artifact_root: impl AsRef<Path>,
    invite_id: &str,
    join_code: &str,
) -> AdcResult<()> {
    validate_segment(invite_id, "invite_id")?;
    if join_code.trim().is_empty() {
        return Err(AdcError::ProfileValidation(
            "join code must not be empty".to_string(),
        ));
    }
    let path = invite_path(artifact_root.as_ref(), invite_id);
    let bytes = fs::read(&path).map_err(|err| {
        AdcError::ProfileValidation(format!("managed fleet invite is not readable: {err}"))
    })?;
    let mut stored: StoredManagedFleetInvite = serde_json::from_slice(&bytes).map_err(|err| {
        AdcError::ProfileValidation(format!("managed fleet invite parse failed: {err}"))
    })?;
    if stored.used_at_unix.is_some() {
        return Err(AdcError::ProfileValidation(
            "managed fleet invite was already used".to_string(),
        ));
    }
    if stored.expires_at_unix <= unix_now() {
        return Err(AdcError::ProfileValidation(
            "managed fleet invite expired".to_string(),
        ));
    }
    if stored.join_code_sha256 != join_code_hash(invite_id, join_code) {
        return Err(AdcError::ProfileValidation(
            "managed fleet invite join code mismatch".to_string(),
        ));
    }
    stored.used_at_unix = Some(unix_now());
    write_json_private(path, &stored)
}

pub fn materialize_managed_fleet_inventory(
    artifact_root: impl AsRef<Path>,
    selector: &str,
) -> AdcResult<ManagedFleetInventoryMaterialization> {
    let artifact_root = artifact_root.as_ref();
    let registry = read_managed_fleet_registry(artifact_root)?;
    let selected = select_managed_targets(&registry, selector)?;
    let targets = selected
        .into_iter()
        .map(managed_target_to_fleet_config)
        .collect::<Vec<_>>();
    if targets.is_empty() {
        return Err(AdcError::ProfileValidation(
            "managed fleet selector did not match any targets".to_string(),
        ));
    }
    let inventory_path = artifact_root
        .join("fleet")
        .join("generated")
        .join(format!("{}.yaml", selector_filename(selector)?));
    write_json_parent_private(&inventory_path)?;
    let inventory = ManagedFleetInventoryFile {
        targets: targets.clone(),
    };
    let bytes = yaml_serde::to_string(&inventory).map_err(|err| {
        AdcError::Artifact(format!(
            "managed fleet inventory serialization failed: {err}"
        ))
    })?;
    fs::write(&inventory_path, bytes).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to write managed fleet inventory {}: {err}",
            inventory_path.display()
        ))
    })?;
    set_private_file_permissions(&inventory_path)?;
    Ok(ManagedFleetInventoryMaterialization {
        schema_version: "obs.managed_fleet_inventory.v1".to_string(),
        selector: selector.to_string(),
        target_count: targets.len(),
        registry_path: managed_fleet_registry_path(artifact_root),
        inventory_path,
        targets,
    })
}

fn select_managed_targets<'a>(
    registry: &'a ManagedFleetRegistry,
    selector: &str,
) -> AdcResult<Vec<&'a ManagedFleetTarget>> {
    let selector = selector.trim();
    if selector.is_empty() {
        return Err(AdcError::ProfileValidation(
            "managed fleet selector must not be empty".to_string(),
        ));
    }
    let targets = registry
        .targets
        .iter()
        .filter(|target| target.trust_state != "revoked")
        .filter(|target| match selector {
            "all" | "enrolled" => true,
            _ if selector.starts_with("target=") => {
                target.target_id == selector.trim_start_matches("target=")
            }
            _ if selector.starts_with("tag=") => target
                .tags
                .iter()
                .any(|tag| tag == selector.trim_start_matches("tag=")),
            _ if selector.starts_with("transport=") => {
                target.transport == selector.trim_start_matches("transport=")
            }
            _ => false,
        })
        .collect::<Vec<_>>();
    if !matches!(selector, "all" | "enrolled")
        && !selector.starts_with("target=")
        && !selector.starts_with("tag=")
        && !selector.starts_with("transport=")
    {
        return Err(AdcError::ProfileValidation(
            "managed fleet selector must be all, enrolled, target=<id>, tag=<tag>, or transport=<transport>".to_string(),
        ));
    }
    Ok(targets)
}

fn managed_target_to_fleet_config(target: &ManagedFleetTarget) -> FleetTargetConfig {
    FleetTargetConfig {
        id: target.target_id.clone(),
        transport: target.transport.clone(),
        host: target.host.clone(),
        user: target.user.clone(),
        port: target.port,
        profile: target.profile.clone(),
        mcp_server_path: target.mcp_server_path.clone(),
        auth_token_file: target.auth_token_file.clone(),
        tls_ca_file: target.tls_ca_file.clone(),
        tls_client_cert_file: target.tls_client_cert_file.clone(),
        tls_client_key_file: target.tls_client_key_file.clone(),
        tls_server_name: target.tls_server_name.clone(),
    }
}

fn write_managed_fleet_registry(
    artifact_root: &Path,
    registry: &ManagedFleetRegistry,
) -> AdcResult<()> {
    write_json_private(managed_fleet_registry_path(artifact_root), registry)
}

fn ensure_controller_identity(artifact_root: &Path) -> AdcResult<ManagedFleetControllerIdentity> {
    let path = artifact_root.join("fleet").join("controller_identity.json");
    if path.is_file() {
        let bytes = fs::read(&path).map_err(|err| {
            AdcError::Artifact(format!(
                "failed to read controller identity {}: {err}",
                path.display()
            ))
        })?;
        return serde_json::from_slice(&bytes)
            .map_err(|err| AdcError::Artifact(format!("controller identity parse failed: {err}")));
    }
    let controller_id = random_hex(32)?;
    let controller_pin = format!("sha256:{}", sha256_hex(controller_id.as_bytes()));
    let identity = ManagedFleetControllerIdentity {
        schema_version: "obs.managed_fleet_controller_identity.v1".to_string(),
        controller_id,
        controller_pin,
        created_at_unix: unix_now(),
    };
    write_json_private(path, &identity)?;
    Ok(identity)
}

fn validate_managed_target(target: &mut ManagedFleetTarget) -> AdcResult<()> {
    validate_segment(&target.target_id, "target_id")?;
    validate_transport(&target.transport)?;
    if let Some(host) = target.host.as_deref() {
        validate_plain(host, "host")?;
    }
    if let Some(user) = target.user.as_deref() {
        validate_plain(user, "user")?;
        if user.contains('@') {
            return Err(AdcError::ProfileValidation(
                "user must not contain @".to_string(),
            ));
        }
    }
    if let Some(profile) = target.profile.as_deref() {
        validate_segment(profile, "profile")?;
    }
    if let Some(path) = target.mcp_server_path.as_deref() {
        validate_plain(path, "mcp_server_path")?;
    }
    if let Some(path) = target.auth_token_file.as_deref() {
        validate_plain(path, "auth_token_file")?;
    }
    if let Some(path) = target.tls_ca_file.as_deref() {
        validate_plain(path, "tls_ca_file")?;
    }
    if let Some(path) = target.tls_client_cert_file.as_deref() {
        validate_plain(path, "tls_client_cert_file")?;
    }
    if let Some(path) = target.tls_client_key_file.as_deref() {
        validate_plain(path, "tls_client_key_file")?;
    }
    if let Some(name) = target.tls_server_name.as_deref() {
        validate_plain(name, "tls_server_name")?;
    }
    if target.transport == "mcp_stdio_over_ssh" && target.host.is_none() {
        return Err(AdcError::ProfileValidation(
            "mcp_stdio_over_ssh target requires host".to_string(),
        ));
    }
    if target.transport == "managed_mcp" {
        if target.host.is_none() {
            return Err(AdcError::ProfileValidation(
                "managed_mcp target requires host".to_string(),
            ));
        }
        if target.port.is_none() {
            return Err(AdcError::ProfileValidation(
                "managed_mcp target requires port".to_string(),
            ));
        }
        if target.auth_token_file.is_none() {
            return Err(AdcError::ProfileValidation(
                "managed_mcp target requires auth_token_file".to_string(),
            ));
        }
    }
    validate_trust_state(&target.trust_state)?;
    validate_plain(&target.enrollment_mode, "enrollment_mode")?;
    let mut tags = BTreeSet::new();
    for tag in &target.tags {
        validate_tag(tag)?;
        tags.insert(tag.clone());
    }
    target.tags = tags.into_iter().collect();
    Ok(())
}

fn validate_transport(value: &str) -> AdcResult<()> {
    match value {
        "local" | "mcp_stdio_over_ssh" | "managed_mcp" => Ok(()),
        other => Err(AdcError::ProfileValidation(format!(
            "unsupported managed fleet transport {other}"
        ))),
    }
}

fn validate_trust_state(value: &str) -> AdcResult<()> {
    match value {
        "trusted" | "pending" | "revoked" => Ok(()),
        other => Err(AdcError::ProfileValidation(format!(
            "unsupported managed fleet trust_state {other}"
        ))),
    }
}

fn validate_segment(value: &str, label: &str) -> AdcResult<()> {
    if value.trim().is_empty() {
        return Err(AdcError::ProfileValidation(format!(
            "{label} must not be empty"
        )));
    }
    let path = Path::new(value);
    let mut components = path.components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(AdcError::ProfileValidation(format!(
            "{label} must be a single relative path segment"
        ))),
    }
}

fn validate_plain(value: &str, label: &str) -> AdcResult<()> {
    if value.trim().is_empty()
        || value.starts_with('-')
        || value
            .chars()
            .any(|ch| ch.is_whitespace() || matches!(ch, '"' | '\'' | '`' | '$' | ';' | '|'))
    {
        return Err(AdcError::ProfileValidation(format!(
            "{label} must be a plain value without shell metacharacters"
        )));
    }
    Ok(())
}

fn validate_tag(value: &str) -> AdcResult<()> {
    if value.trim().is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(AdcError::ProfileValidation(
            "tag must contain only ascii alphanumeric, dash, underscore, or dot".to_string(),
        ));
    }
    Ok(())
}

fn selector_filename(selector: &str) -> AdcResult<String> {
    if selector.trim().is_empty() {
        return Err(AdcError::ProfileValidation(
            "managed fleet selector must not be empty".to_string(),
        ));
    }
    Ok(selector
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect())
}

fn invite_path(artifact_root: &Path, invite_id: &str) -> PathBuf {
    artifact_root
        .join("fleet")
        .join("enrollment")
        .join("invites")
        .join(format!("{invite_id}.json"))
}

fn write_json_private(path: impl AsRef<Path>, value: &impl Serialize) -> AdcResult<()> {
    let path = path.as_ref();
    write_json_parent_private(path)?;
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|err| AdcError::Artifact(format!("json serialization failed: {err}")))?;
    fs::write(path, bytes)
        .map_err(|err| AdcError::Artifact(format!("failed to write {}: {err}", path.display())))?;
    set_private_file_permissions(path)
}

fn write_json_parent_private(path: &Path) -> AdcResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            AdcError::Artifact(format!("failed to create {}: {err}", parent.display()))
        })?;
        set_private_dir_permissions(parent)?;
    }
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> AdcResult<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to set private file permissions {}: {err}",
            path.display()
        ))
    })
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> AdcResult<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> AdcResult<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|err| {
        AdcError::Artifact(format!(
            "failed to set private directory permissions {}: {err}",
            path.display()
        ))
    })
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &Path) -> AdcResult<()> {
    Ok(())
}

fn random_hex(byte_count: usize) -> AdcResult<String> {
    Ok(hex_lower(&random_bytes(byte_count)?))
}

fn random_bytes(byte_count: usize) -> AdcResult<Vec<u8>> {
    let mut bytes = vec![0_u8; byte_count];
    fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(|err| AdcError::Artifact(format!("failed to read random bytes: {err}")))?;
    Ok(bytes)
}

fn format_join_code(bytes: &[u8]) -> String {
    hex_lower(bytes)
        .as_bytes()
        .chunks(4)
        .map(|chunk| String::from_utf8_lossy(chunk).to_ascii_uppercase())
        .collect::<Vec<_>>()
        .join("-")
}

fn join_code_hash(invite_id: &str, join_code: &str) -> String {
    sha256_hex(format!("{invite_id}:{join_code}").as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_lower(&hasher.finalize())
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
