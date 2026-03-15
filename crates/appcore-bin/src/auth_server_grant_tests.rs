// =============================================================================
//        #######
//     ###       ###     F: auth_server_grant_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/07 08:56:37 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/07 09:02:12 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use appcore_security::{format_secret_material, new_rotated_secret, SecuritySecretStatus};
use std::path::PathBuf;

fn temp_secret(name: &str, status: SecuritySecretStatus, expires: Option<u64>) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("appcore-grant-{name}-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    let mut material = new_rotated_secret(expires).expect("secret");
    material.metadata.status = status;
    let path = dir.join("auth.secret");
    fs::write(&path, format_secret_material(&material)).expect("write secret");
    path
}

fn temp_secret_pair(name: &str) -> (PathBuf, PathBuf) {
    let data = temp_secret(&format!("{name}-data"), SecuritySecretStatus::Active, None);
    let transport = temp_secret(
        &format!("{name}-transport"),
        SecuritySecretStatus::Active,
        None,
    );
    (data, transport)
}

#[test]
fn auth_grant_roundtrip_uses_hashtoken_envelope() {
    let path = temp_secret("roundtrip", SecuritySecretStatus::Active, None);

    let token = issue_auth_grant(&path, "storage/private.txt", 1_000, 100).expect("grant");
    let grant = open_auth_grant(&path, &token, 200).expect("open grant");

    assert_eq!(grant.resource, "storage/private.txt");
    assert_eq!(grant.issued_at_ms, 100);
    assert_eq!(grant.expires_at_ms, 1_100);
    let _ = fs::remove_dir_all(path.parent().expect("dir"));
}

#[test]
fn auth_grant_uses_transport_secret_not_data_secret() {
    let (data_secret, transport_secret) = temp_secret_pair("separated");

    let token =
        issue_auth_grant(&transport_secret, "storage/private.txt", 1_000, 100).expect("grant");
    let wrong_open = open_auth_grant(&data_secret, &token, 200);
    let right_open = open_auth_grant(&transport_secret, &token, 200);

    assert!(matches!(wrong_open, Err(error) if error.contains("verification")));
    assert!(right_open.is_ok());
    let _ = fs::remove_dir_all(data_secret.parent().expect("data dir"));
    let _ = fs::remove_dir_all(transport_secret.parent().expect("transport dir"));
}

#[test]
fn auth_grant_rejects_missing_secret() {
    let path = PathBuf::from("/tmp/appcore-missing-auth.secret");

    let grant = issue_auth_grant(&path, "storage/private.txt", 1_000, 100);

    assert!(matches!(grant, Err(error) if error.contains("missing")));
}

#[test]
fn auth_grant_rejects_revoked_secret() {
    let path = temp_secret("revoked", SecuritySecretStatus::Revoked, None);

    let grant = issue_auth_grant(&path, "storage/private.txt", 1_000, 100);

    assert!(matches!(grant, Err(error) if error.contains("revoked")));
    let _ = fs::remove_dir_all(path.parent().expect("dir"));
}

#[test]
fn auth_grant_rejects_expired_secret() {
    let path = temp_secret("expired", SecuritySecretStatus::Active, Some(99));

    let grant = issue_auth_grant(&path, "storage/private.txt", 1_000, 100);

    assert!(matches!(grant, Err(error) if error.contains("expired")));
    let _ = fs::remove_dir_all(path.parent().expect("dir"));
}

#[test]
fn auth_grant_rejects_invalid_resource_and_ttl() {
    let path = temp_secret("invalid", SecuritySecretStatus::Active, None);

    let empty = issue_auth_grant(&path, "", 1_000, 100);
    let zero_ttl = issue_auth_grant(&path, "storage/private.txt", 0, 100);

    assert!(matches!(empty, Err(error) if error.contains("resource")));
    assert!(matches!(zero_ttl, Err(error) if error.contains("ttl")));
    let _ = fs::remove_dir_all(path.parent().expect("dir"));
}
