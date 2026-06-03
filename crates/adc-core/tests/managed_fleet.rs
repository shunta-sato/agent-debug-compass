use std::{fs, time::Duration};

use adc_core::{
    create_managed_fleet_invite, enroll_managed_fleet_kit, initialize_managed_fleet_registry,
    materialize_managed_fleet_inventory, read_managed_fleet_registry, upsert_managed_fleet_target,
    verify_and_consume_managed_fleet_invite, ManagedFleetInviteOptions, ManagedFleetTarget,
};

#[test]
fn managed_registry_initializes_and_enrolls_target_without_plain_join_code() {
    let temp = tempfile::tempdir().expect("tempdir");

    let registry = initialize_managed_fleet_registry(temp.path()).expect("init registry");
    assert_eq!(registry.schema_version, "obs.managed_fleet_registry.v1");
    assert!(registry.targets.is_empty());

    let invite = create_managed_fleet_invite(
        temp.path(),
        ManagedFleetInviteOptions {
            target_id_hint: Some("pi4-a".to_string()),
            ttl: Duration::from_secs(600),
        },
    )
    .expect("create invite");
    assert_eq!(invite.schema_version, "obs.managed_fleet_invite.v1");
    assert!(invite.join_code.contains('-'));
    assert!(invite.controller_pin.starts_with("sha256:"));

    let invite_path = temp
        .path()
        .join("fleet/enrollment/invites")
        .join(format!("{}.json", invite.invite_id));
    let stored_invite = fs::read_to_string(invite_path).expect("stored invite");
    assert!(stored_invite.contains("join_code_sha256"));
    assert!(!stored_invite.contains(&invite.join_code));

    let target = ManagedFleetTarget {
        target_id: "pi4-a".to_string(),
        display_name: Some("Raspberry Pi 4 A".to_string()),
        transport: "mcp_stdio_over_ssh".to_string(),
        host: Some("example-target".to_string()),
        user: None,
        port: None,
        profile: Some("pi_basic".to_string()),
        mcp_server_path: Some("/home/pi/.local/bin/adc-mcp".to_string()),
        auth_token_file: None,
        tls_ca_file: None,
        tls_client_cert_file: None,
        tls_client_key_file: None,
        tls_server_name: None,
        tags: vec!["pi".to_string(), "edge".to_string()],
        trust_state: "trusted".to_string(),
        enrollment_mode: "manual".to_string(),
        identity_fingerprint: Some("sha256:test-fingerprint".to_string()),
    };
    let registry = upsert_managed_fleet_target(temp.path(), target).expect("upsert target");
    assert_eq!(registry.targets.len(), 1);
    assert_eq!(registry.targets[0].target_id, "pi4-a");
    assert_eq!(registry.targets[0].tags, vec!["edge", "pi"]);

    let reread = read_managed_fleet_registry(temp.path()).expect("read registry");
    assert_eq!(reread.targets[0].transport, "mcp_stdio_over_ssh");
}

#[test]
fn managed_fleet_selector_materializes_inventory_targets() {
    let temp = tempfile::tempdir().expect("tempdir");
    initialize_managed_fleet_registry(temp.path()).expect("init registry");
    upsert_managed_fleet_target(
        temp.path(),
        ManagedFleetTarget {
            target_id: "local-a".to_string(),
            display_name: None,
            transport: "local".to_string(),
            host: None,
            user: None,
            port: None,
            profile: Some("pi5_basic".to_string()),
            mcp_server_path: None,
            auth_token_file: None,
            tls_ca_file: None,
            tls_client_cert_file: None,
            tls_client_key_file: None,
            tls_server_name: None,
            tags: vec!["pi5".to_string(), "edge".to_string()],
            trust_state: "trusted".to_string(),
            enrollment_mode: "manual".to_string(),
            identity_fingerprint: None,
        },
    )
    .expect("upsert local");
    upsert_managed_fleet_target(
        temp.path(),
        ManagedFleetTarget {
            target_id: "managed-c".to_string(),
            display_name: None,
            transport: "managed_mcp".to_string(),
            host: Some("192.0.2.10".to_string()),
            user: None,
            port: Some(32145),
            profile: None,
            mcp_server_path: None,
            auth_token_file: Some("/tmp/adc-managed.token".to_string()),
            tls_ca_file: Some("/tmp/adc-ca.pem".to_string()),
            tls_client_cert_file: Some("/tmp/adc-client.pem".to_string()),
            tls_client_key_file: Some("/tmp/adc-client.key".to_string()),
            tls_server_name: Some("example-target.local".to_string()),
            tags: vec!["managed".to_string()],
            trust_state: "trusted".to_string(),
            enrollment_mode: "manual".to_string(),
            identity_fingerprint: None,
        },
    )
    .expect("upsert managed");
    upsert_managed_fleet_target(
        temp.path(),
        ManagedFleetTarget {
            target_id: "remote-b".to_string(),
            display_name: None,
            transport: "mcp_stdio_over_ssh".to_string(),
            host: Some("example-target".to_string()),
            user: None,
            port: None,
            profile: None,
            mcp_server_path: Some("/home/pi/.local/bin/adc-mcp".to_string()),
            auth_token_file: None,
            tls_ca_file: None,
            tls_client_cert_file: None,
            tls_client_key_file: None,
            tls_server_name: None,
            tags: vec!["pi4".to_string()],
            trust_state: "trusted".to_string(),
            enrollment_mode: "manual".to_string(),
            identity_fingerprint: None,
        },
    )
    .expect("upsert remote");

    let all = materialize_managed_fleet_inventory(temp.path(), "all").expect("all selector");
    assert_eq!(all.target_count, 3);
    assert!(all.inventory_path.is_file());

    let lab = materialize_managed_fleet_inventory(temp.path(), "tag=edge").expect("tag selector");
    assert_eq!(lab.target_count, 1);
    assert_eq!(lab.targets[0].id, "local-a");
    let inventory = fs::read_to_string(&lab.inventory_path).expect("inventory yaml");
    assert!(inventory.contains("transport: local"));
    assert!(inventory.contains("profile: pi5_basic"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = fs::metadata(&lab.inventory_path)
            .expect("inventory metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    let by_id =
        materialize_managed_fleet_inventory(temp.path(), "target=remote-b").expect("id selector");
    assert_eq!(by_id.targets[0].host.as_deref(), Some("example-target"));

    let managed =
        materialize_managed_fleet_inventory(temp.path(), "transport=managed_mcp").expect("managed");
    assert_eq!(managed.target_count, 1);
    assert_eq!(managed.targets[0].port, Some(32145));
    assert_eq!(
        managed.targets[0].auth_token_file.as_deref(),
        Some("/tmp/adc-managed.token")
    );
    assert_eq!(
        managed.targets[0].tls_ca_file.as_deref(),
        Some("/tmp/adc-ca.pem")
    );
    assert_eq!(
        managed.targets[0].tls_client_cert_file.as_deref(),
        Some("/tmp/adc-client.pem")
    );
    assert_eq!(
        managed.targets[0].tls_client_key_file.as_deref(),
        Some("/tmp/adc-client.key")
    );
    assert_eq!(
        managed.targets[0].tls_server_name.as_deref(),
        Some("example-target.local")
    );
    let managed_inventory = fs::read_to_string(&managed.inventory_path).expect("managed inventory");
    assert!(managed_inventory.contains("transport: managed_mcp"));
    assert!(managed_inventory.contains("auth_token_file: /tmp/adc-managed.token"));
    assert!(managed_inventory.contains("tls_ca_file: /tmp/adc-ca.pem"));
    assert!(managed_inventory.contains("tls_client_cert_file: /tmp/adc-client.pem"));
    assert!(managed_inventory.contains("tls_client_key_file: /tmp/adc-client.key"));
    assert!(managed_inventory.contains("tls_server_name: example-target.local"));

    let miss =
        materialize_managed_fleet_inventory(temp.path(), "tag=missing").expect_err("selector miss");
    assert!(miss
        .to_string()
        .contains("managed fleet selector did not match any targets"));
}

#[test]
fn managed_fleet_invite_is_single_use_and_expires() {
    let temp = tempfile::tempdir().expect("tempdir");
    initialize_managed_fleet_registry(temp.path()).expect("init registry");
    let invite = create_managed_fleet_invite(
        temp.path(),
        ManagedFleetInviteOptions {
            target_id_hint: None,
            ttl: Duration::from_secs(600),
        },
    )
    .expect("create invite");

    verify_and_consume_managed_fleet_invite(temp.path(), &invite.invite_id, &invite.join_code)
        .expect("consume invite");
    let reused =
        verify_and_consume_managed_fleet_invite(temp.path(), &invite.invite_id, &invite.join_code)
            .expect_err("reused invite");
    assert!(reused.to_string().contains("already used"));

    let expired = create_managed_fleet_invite(
        temp.path(),
        ManagedFleetInviteOptions {
            target_id_hint: None,
            ttl: Duration::ZERO,
        },
    )
    .expect("create expired invite");
    let err = verify_and_consume_managed_fleet_invite(
        temp.path(),
        &expired.invite_id,
        &expired.join_code,
    )
    .expect_err("expired invite");
    assert!(err.to_string().contains("expired"));
}

#[test]
fn managed_fleet_enrolls_target_from_enrollment_kit() {
    let temp = tempfile::tempdir().expect("tempdir");
    initialize_managed_fleet_registry(temp.path()).expect("init registry");
    let kit_path = temp.path().join("enrollment-kit.json");
    fs::write(
        &kit_path,
        r#"
{
  "schema_version": "obs.managed_mcp_enrollment_kit.v1",
  "target": {
    "target_id": "kit-target",
    "transport": "managed_mcp",
    "host": "192.0.2.55",
    "port": 39245,
    "auth_token_file": "/tmp/kit/managed.token",
    "tls_ca_file": "/tmp/kit/ca.pem",
    "tls_client_cert_file": "/tmp/kit/controller.pem",
    "tls_client_key_file": "/tmp/kit/controller.key",
    "tls_server_name": "kit-target.local",
    "tags": ["kit", "lab"],
    "trust_state": "trusted",
    "enrollment_mode": "kit"
  }
}
"#,
    )
    .expect("kit");

    let registry = enroll_managed_fleet_kit(temp.path(), &kit_path).expect("enroll kit");
    assert_eq!(registry.targets.len(), 1);
    let target = &registry.targets[0];
    assert_eq!(target.target_id, "kit-target");
    assert_eq!(target.transport, "managed_mcp");
    assert_eq!(target.enrollment_mode, "kit");
    assert_eq!(target.tags, vec!["kit", "lab"]);
    assert_eq!(target.tls_server_name.as_deref(), Some("kit-target.local"));
}
