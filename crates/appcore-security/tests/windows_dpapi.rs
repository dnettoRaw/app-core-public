// =============================================================================
//        #######
//     ###       ###     F: windows_dpapi.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================
// appcore-norm: test

#![cfg(windows)]

use appcore_security::{
    format_secret_material, new_rotated_secret, FileSecretKeyring, SecretAccessError,
    WindowsDpapiSecretKeyring,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn dpapi_keyring_encrypts_rotates_revokes_and_redacts() {
    let root = test_root("lifecycle");
    let keyring = WindowsDpapiSecretKeyring::open(&root).unwrap();
    assert!(matches!(
        FileSecretKeyring::open(&root),
        Err(SecretAccessError::InvalidMaterial)
    ));
    let first = new_rotated_secret(None).unwrap();
    let first_text = format_secret_material(&first);
    keyring.install_initial(&first).unwrap();

    let persisted = fs::read(key_path(&root, &first.metadata.key_id)).unwrap();
    assert!(!contains(&persisted, first_text.as_bytes()));
    assert!(!format!("{keyring:?}").contains(&root.to_string_lossy().to_string()));
    assert_eq!(
        keyring.resolve_active(now_ms()).unwrap().metadata.key_id,
        first.metadata.key_id
    );

    let second = new_rotated_secret(None).unwrap();
    assert_eq!(
        keyring.rotate(&second, now_ms()).unwrap().as_deref(),
        Some(first.metadata.key_id.as_str())
    );
    assert!(keyring
        .resolve_for_validation(&first.metadata.key_id, now_ms())
        .is_ok());
    keyring.revoke(&first.metadata.key_id).unwrap();
    assert_eq!(
        keyring.resolve_for_validation(&first.metadata.key_id, now_ms()),
        Err(SecretAccessError::Revoked)
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dpapi_backup_restores_only_verifiable_records() {
    let root = test_root("backup-source");
    let keyring = WindowsDpapiSecretKeyring::open(&root).unwrap();
    let material = new_rotated_secret(None).unwrap();
    keyring.install_initial(&material).unwrap();

    let restored_root = test_root("backup-restored");
    WindowsDpapiSecretKeyring::open(&restored_root).unwrap();
    fs::copy(root.join("active"), restored_root.join("active")).unwrap();
    fs::copy(
        key_path(&root, &material.metadata.key_id),
        key_path(&restored_root, &material.metadata.key_id),
    )
    .unwrap();
    let restored = WindowsDpapiSecretKeyring::open(&restored_root).unwrap();
    assert_eq!(
        restored.resolve_active(now_ms()).unwrap().metadata.key_id,
        material.metadata.key_id
    );

    let path = key_path(&restored_root, &material.metadata.key_id);
    let mut corrupt = fs::read(&path).unwrap();
    corrupt[0] ^= 0x55;
    fs::write(path, corrupt).unwrap();
    assert_eq!(
        restored.resolve_active(now_ms()),
        Err(SecretAccessError::InvalidMaterial)
    );
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(restored_root).unwrap();
}

#[test]
fn dpapi_keyring_rejects_a_broadened_acl() {
    let root = test_root("broad-acl");
    WindowsDpapiSecretKeyring::open(&root).unwrap();
    let status = Command::new("icacls")
        .arg(&root)
        .args(["/grant", "*S-1-1-0:(OI)(CI)F"])
        .status()
        .expect("icacls must be available on the Windows certification host");
    assert!(status.success());
    assert!(matches!(
        WindowsDpapiSecretKeyring::open(&root),
        Err(SecretAccessError::InsecurePermissions)
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dpapi_keyring_rejects_a_directory_junction() {
    let target = test_root("junction-target");
    WindowsDpapiSecretKeyring::open(&target).unwrap();
    let junction = test_root("junction");
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(&target)
        .status()
        .expect("cmd must be available on the Windows certification host");
    assert!(status.success());
    assert!(matches!(
        WindowsDpapiSecretKeyring::open(&junction),
        Err(SecretAccessError::InvalidPath)
    ));
    fs::remove_dir(junction).unwrap();
    fs::remove_dir_all(target).unwrap();
}

fn key_path(root: &Path, key_id: &str) -> PathBuf {
    root.join("keys").join(format!("{key_id}.secret"))
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|part| part == needle)
}

fn test_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "appcore-dpapi-{label}-{}-{}",
        std::process::id(),
        now_ms()
    ))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
