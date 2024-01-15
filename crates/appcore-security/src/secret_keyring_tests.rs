// =============================================================================
//        #######
//     ###       ###     F: secret_keyring_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use crate::{SecuritySecretMetadata, SecuritySecretStatus};
use std::sync::Arc;

fn test_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "appcore-secret-keyring-{name}-{}-{}",
        std::process::id(),
        now_ms()
    ))
}

fn material(
    key_id: &str,
    byte: u8,
    status: SecuritySecretStatus,
    expires_at_ms: Option<u64>,
) -> SecuritySecretMaterial {
    SecuritySecretMaterial {
        secret: vec![byte; 32],
        metadata: SecuritySecretMetadata {
            key_id: key_id.to_string(),
            created_at_ms: 10,
            expires_at_ms,
            status,
        },
    }
}

#[test]
fn rotation_keeps_previous_key_available_for_validation() {
    let root = test_root("rotation");
    let keyring = FileSecretKeyring::open(&root).unwrap();
    keyring
        .install_initial(&material("key-a", b'a', SecuritySecretStatus::Active, None))
        .unwrap();

    let previous = keyring
        .rotate(
            &material("key-b", b'b', SecuritySecretStatus::Active, None),
            20,
        )
        .unwrap();

    assert_eq!(previous.as_deref(), Some("key-a"));
    assert_eq!(keyring.resolve_active(20).unwrap().metadata.key_id, "key-b");
    assert_eq!(
        keyring
            .resolve_for_validation("key-a", 20)
            .unwrap()
            .metadata
            .status,
        SecuritySecretStatus::Deprecated
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn expired_revoked_missing_and_partial_material_fail_closed() {
    let root = test_root("fail-closed");
    let keyring = FileSecretKeyring::open(&root).unwrap();
    let expiry = now_ms().saturating_add(1_000);
    keyring
        .install_initial(&material(
            "key-a",
            b'a',
            SecuritySecretStatus::Active,
            Some(expiry),
        ))
        .unwrap();

    assert_eq!(
        keyring.resolve_active(expiry),
        Err(SecretAccessError::Expired)
    );
    assert_eq!(
        keyring.resolve_for_validation("missing", 10),
        Err(SecretAccessError::Unavailable)
    );

    keyring.revoke("key-a").unwrap();
    assert_eq!(
        keyring.resolve_for_validation("key-a", 10),
        Err(SecretAccessError::Revoked)
    );
    fs::write(root.join("keys/partial.secret"), b"key_id=partial\n").unwrap();
    set_private_path(&root.join("keys/partial.secret"), 0o600);
    assert_eq!(
        keyring.resolve_for_validation("partial", 10),
        Err(SecretAccessError::InvalidMaterial)
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restart_and_absent_pointer_recovery_are_durable() {
    let root = test_root("recovery");
    let keyring = FileSecretKeyring::open(&root).unwrap();
    keyring
        .install_initial(&material("key-a", b'a', SecuritySecretStatus::Active, None))
        .unwrap();
    fs::remove_file(root.join("active")).unwrap();

    let reopened = FileSecretKeyring::open(&root).unwrap();
    assert_eq!(reopened.recover(20).unwrap(), "key-a");
    assert_eq!(
        reopened.resolve_active(20).unwrap().metadata.key_id,
        "key-a"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn concurrent_rotation_serializes_and_keeps_one_active_key() {
    let root = test_root("concurrent");
    let keyring = Arc::new(FileSecretKeyring::open(&root).unwrap());
    keyring
        .install_initial(&material("key-a", b'a', SecuritySecretStatus::Active, None))
        .unwrap();
    let first = Arc::clone(&keyring);
    let second = Arc::clone(&keyring);
    let first_thread = std::thread::spawn(move || {
        first.rotate(
            &material("key-b", b'b', SecuritySecretStatus::Active, None),
            20,
        )
    });
    let second_thread = std::thread::spawn(move || {
        second.rotate(
            &material("key-c", b'c', SecuritySecretStatus::Active, None),
            20,
        )
    });

    assert!(first_thread.join().unwrap().is_ok());
    assert!(second_thread.join().unwrap().is_ok());
    let active = keyring.resolve_active(20).unwrap();
    assert!(matches!(active.metadata.key_id.as_str(), "key-b" | "key-c"));
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn permissions_and_symlinks_are_rechecked_for_every_operation() {
    use std::os::unix::fs::symlink;

    let root = test_root("permissions");
    let keyring = FileSecretKeyring::open(&root).unwrap();
    keyring
        .install_initial(&material("key-a", b'a', SecuritySecretStatus::Active, None))
        .unwrap();
    set_private_path(&root, 0o755);
    assert_eq!(
        keyring.resolve_active(20),
        Err(SecretAccessError::InsecurePermissions)
    );
    set_private_path(&root, 0o700);

    let target = root.join("target");
    fs::write(&target, b"not-a-secret").unwrap();
    set_private_path(&target, 0o600);
    symlink(&target, root.join("keys/link.secret")).unwrap();
    assert_eq!(
        keyring.resolve_for_validation("link", 20),
        Err(SecretAccessError::InsecurePermissions)
    );
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
fn set_private_path(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

#[cfg(not(unix))]
fn set_private_path(_path: &Path, _mode: u32) {}
