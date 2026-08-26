// =============================================================================
//        #######
//     ###       ###     F: storage_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:42:05 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use std::fs;

use appcore_contracts::{ProviderConfig, ProviderId};
use appcore_security::{HashTokenProvider, TokenClaims};

use super::{
    BackupDescriptor, FileStorageProvider, Migration, MigrationId, Repository, RepositoryName,
    StorageCapabilityCatalogV1, StorageCapabilityError, StorageCapabilityProviderV1,
    StorageCapabilityRequirementsV1, StorageCapabilityV1, StorageError, StorageHealth,
    StorageProvider, StorageStatus, Transaction, MAX_STORAGE_CAPABILITY_PROVIDERS_V1,
    STORAGE_CAPABILITY_DESCRIPTOR_VERSION_V1,
};

struct MockTx {
    committed: bool,
    rolled_back: bool,
}

impl Transaction for MockTx {
    fn commit(&mut self) -> super::StorageResult<()> {
        self.committed = true;
        Ok(())
    }

    fn rollback(&mut self) -> super::StorageResult<()> {
        self.rolled_back = true;
        Ok(())
    }
}

struct MockRepo {
    name: RepositoryName,
}

impl Repository for MockRepo {
    fn name(&self) -> &RepositoryName {
        &self.name
    }
}

struct MockMigration {
    id: MigrationId,
}

impl Migration for MockMigration {
    fn id(&self) -> &MigrationId {
        &self.id
    }

    fn apply(&self, tx: &mut dyn Transaction) -> super::StorageResult<()> {
        tx.commit()
    }
}

struct MockStorage {
    open: bool,
}

impl StorageProvider for MockStorage {
    fn status(&self) -> StorageStatus {
        if self.open {
            StorageStatus::Online
        } else {
            StorageStatus::Offline
        }
    }

    fn health(&self) -> StorageHealth {
        StorageHealth {
            status: self.status(),
            message: None,
        }
    }

    fn open(&mut self) -> super::StorageResult<()> {
        self.open = true;
        Ok(())
    }

    fn close(&mut self) -> super::StorageResult<()> {
        self.open = false;
        Ok(())
    }

    fn begin_transaction(&mut self) -> super::StorageResult<Box<dyn Transaction>> {
        Ok(Box::new(MockTx {
            committed: false,
            rolled_back: false,
        }))
    }

    fn list_backups(&self) -> Vec<BackupDescriptor> {
        vec![BackupDescriptor {
            name: "backup-a".to_string(),
            created_at_ms: 1,
        }]
    }
}

#[test]
fn storage_health_mock() {
    let storage = MockStorage { open: false };
    assert_eq!(storage.health().status, StorageStatus::Offline);
}

#[test]
fn repository_name_mock() {
    let repo = MockRepo {
        name: RepositoryName("runtime-records".to_string()),
    };
    assert_eq!(repo.name(), &RepositoryName("runtime-records".to_string()));
}

#[test]
fn migration_apply_mock() {
    let migration = MockMigration {
        id: MigrationId("m1".to_string()),
    };
    let mut tx = MockTx {
        committed: false,
        rolled_back: false,
    };
    assert!(migration.apply(&mut tx).is_ok());
    assert!(tx.committed);
}

#[test]
fn storage_provider_transaction_mock() {
    let mut storage = MockStorage { open: true };
    let tx = storage.begin_transaction();
    assert!(tx.is_ok());
}

#[test]
fn file_storage_rejects_unsupported_transactions() {
    let (storage, backups) = temp_paths("transactions-unsupported");
    let mut provider = FileStorageProvider::new(&storage, &backups);
    assert!(provider.open().is_ok());
    assert!(matches!(
        provider.begin_transaction(),
        Err(StorageError::TransactionsUnsupported)
    ));
    let _ = fs::remove_dir_all(storage.parent().unwrap_or(std::path::Path::new("")));
}

#[test]
fn file_storage_descriptor_is_versioned_and_conservative() {
    let (storage, backups) = temp_paths("capability-descriptor");
    let provider = FileStorageProvider::new(storage, backups);
    let descriptor = provider.storage_capabilities_v1().unwrap();
    assert_eq!(
        descriptor.descriptor_version(),
        STORAGE_CAPABILITY_DESCRIPTOR_VERSION_V1
    );
    assert_eq!(descriptor.provider_id().as_str(), "file");
    assert!(descriptor.supports(StorageCapabilityV1::Snapshot));
    assert!(!descriptor.supports(StorageCapabilityV1::Transactions));
    assert!(!descriptor.supports(StorageCapabilityV1::MultiProcess));
    assert!(!descriptor.supports(StorageCapabilityV1::MultiHost));
}

#[test]
fn capability_requirements_parse_strict_bounded_spellings() {
    let config = ProviderConfig::new(ProviderId::new("file").unwrap())
        .with_setting(
            "required_capabilities",
            "transactions,locking,snapshot,streaming,online_backup,multi_process,multi_host",
        )
        .unwrap();
    let requirements = StorageCapabilityRequirementsV1::from_provider_config(&config).unwrap();
    assert_eq!(requirements.capabilities().len(), 7);

    let duplicate = ProviderConfig::new(ProviderId::new("file").unwrap())
        .with_setting("required_capabilities", "snapshot,snapshot")
        .unwrap();
    assert!(matches!(
        StorageCapabilityRequirementsV1::from_provider_config(&duplicate),
        Err(StorageCapabilityError::DuplicateRequirement(
            StorageCapabilityV1::Snapshot
        ))
    ));

    let unknown = ProviderConfig::new(ProviderId::new("file").unwrap())
        .with_setting("required_capabilities", "provider_specific_magic")
        .unwrap();
    assert_eq!(
        StorageCapabilityRequirementsV1::from_provider_config(&unknown),
        Err(StorageCapabilityError::UnknownRequirement)
    );
}

#[test]
fn capability_catalog_never_falls_back_to_weaker_provider() {
    let (storage, backups) = temp_paths("capability-catalog");
    let provider = FileStorageProvider::new(storage, backups);
    let mut catalog = StorageCapabilityCatalogV1::new();
    catalog
        .register(provider.storage_capabilities_v1().unwrap())
        .unwrap();
    let mut requirements = StorageCapabilityRequirementsV1::new();
    requirements
        .require(StorageCapabilityV1::Transactions)
        .unwrap();
    assert!(matches!(
        catalog.validate(&ProviderId::new("file").unwrap(), &requirements),
        Err(StorageCapabilityError::MissingCapability {
            capability: StorageCapabilityV1::Transactions,
            ..
        })
    ));
    assert!(matches!(
        catalog.validate(&ProviderId::new("unknown").unwrap(), &requirements),
        Err(StorageCapabilityError::ProviderUnavailable(_))
    ));
}

#[test]
fn capability_catalog_has_a_fixed_provider_bound() {
    let mut catalog = StorageCapabilityCatalogV1::new();
    for index in 0..MAX_STORAGE_CAPABILITY_PROVIDERS_V1 {
        catalog
            .register(super::StorageCapabilityDescriptorV1::new(
                ProviderId::new(format!("provider-{index}")).unwrap(),
                [],
            ))
            .unwrap();
    }
    assert_eq!(
        catalog.register(super::StorageCapabilityDescriptorV1::new(
            ProviderId::new("provider-overflow").unwrap(),
            [],
        )),
        Err(StorageCapabilityError::CatalogFull)
    );
}

#[cfg(unix)]
#[test]
fn file_storage_rejects_symlink_path_components() {
    use std::os::unix::fs::symlink;

    let (storage, backups) = temp_paths("symlink-escape");
    let outside = storage.parent().unwrap().join("outside");
    let provider = FileStorageProvider::new(&storage, &backups);
    assert!(provider.create_dirs().is_ok());
    assert!(fs::create_dir_all(&outside).is_ok());
    assert!(symlink(&outside, storage.join("linked")).is_ok());

    assert!(matches!(
        provider.write_bytes("linked/escape.txt", b"blocked"),
        Err(StorageError::InvalidPath(_))
    ));
    assert!(!outside.join("escape.txt").exists());
    let _ = fs::remove_dir_all(storage.parent().unwrap_or(std::path::Path::new("")));
}

#[cfg(unix)]
#[test]
fn file_storage_rejects_a_symlink_storage_root() {
    use std::os::unix::fs::symlink;

    let (storage, backups) = temp_paths("symlink-root");
    let root = storage.parent().unwrap();
    let outside = root.join("outside-root");
    assert!(fs::create_dir_all(&outside).is_ok());
    assert!(fs::create_dir_all(&backups).is_ok());
    assert!(symlink(&outside, &storage).is_ok());
    let provider = FileStorageProvider::new(&storage, &backups);

    assert!(matches!(
        provider.write_bytes("escape.txt", b"blocked"),
        Err(StorageError::InvalidPath(_))
    ));
    assert!(!outside.join("escape.txt").exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn typed_storage_error_exists() {
    let err = StorageError::NotAvailable;
    assert_eq!(err, StorageError::NotAvailable);
}

fn temp_paths(prefix: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let base =
        std::env::temp_dir().join(format!("appcore-storage-{prefix}-{}", std::process::id()));
    let storage = base.join("storage");
    let backups = base.join("backups");
    (storage, backups)
}

#[test]
fn file_storage_write_read_backup() {
    let (storage, backups) = temp_paths("basic");
    let mut provider = FileStorageProvider::new(&storage, &backups);
    assert!(provider.open().is_ok());
    assert!(provider.write_bytes("a/b.txt", b"hello").is_ok());
    let read = provider.read_bytes("a/b.txt");
    assert_eq!(read.ok(), Some(b"hello".to_vec()));
    assert!(provider.backup_file("a/b.txt", "bkp.txt").is_ok());
    assert!(backups.join("bkp.txt").exists());
    let _ = fs::remove_dir_all(storage.parent().unwrap_or(std::path::Path::new("")));
}

#[test]
fn file_storage_rejects_path_traversal() {
    let (storage, backups) = temp_paths("traversal");
    let provider = FileStorageProvider::new(storage, backups);
    let result = provider.write_bytes("../evil.txt", b"x");
    assert!(matches!(result, Err(StorageError::InvalidPath(_))));
}

#[test]
fn file_storage_rejects_empty_relative_path() {
    let (storage, backups) = temp_paths("empty-path");
    let provider = FileStorageProvider::new(storage, backups);

    assert_eq!(
        provider.write_bytes("", b"x"),
        Err(StorageError::InvalidPath(String::new()))
    );
}

#[test]
fn write_atomic_creates_final_destination() {
    let (storage, backups) = temp_paths("atomic-write");
    let provider = FileStorageProvider::new(&storage, &backups);
    assert!(provider.create_dirs().is_ok());
    assert!(provider
        .write_bytes_atomic("records/data.txt", b"ok")
        .is_ok());
    assert!(storage.join("records/data.txt").exists());
    assert!(!storage.join("records/data.txt.tmp").exists());
    let _ = fs::remove_dir_all(storage.parent().unwrap_or(std::path::Path::new("")));
}

#[test]
fn cleanup_temp_files_removes_orphans_only() {
    let (storage, backups) = temp_paths("cleanup");
    let provider = FileStorageProvider::new(&storage, &backups);
    assert!(provider.create_dirs().is_ok());
    assert!(fs::write(storage.join("keep.txt"), b"k").is_ok());
    assert!(fs::write(storage.join("orphan.tmp"), b"o").is_ok());
    assert!(fs::write(backups.join("orphan2.tmp"), b"o").is_ok());

    let removed = provider.cleanup_temp_files();
    assert_eq!(removed.ok(), Some(2));
    assert!(storage.join("keep.txt").exists());
    assert!(!storage.join("orphan.tmp").exists());
    assert!(!backups.join("orphan2.tmp").exists());
    let _ = fs::remove_dir_all(storage.parent().unwrap_or(std::path::Path::new("")));
}

#[cfg(unix)]
#[test]
fn cleanup_and_health_do_not_follow_directory_symlinks() {
    use std::os::unix::fs::symlink;

    let (storage, backups) = temp_paths("cleanup-symlink");
    let root = storage.parent().unwrap();
    let outside = root.join("outside-cleanup");
    let provider = FileStorageProvider::new(&storage, &backups);
    assert!(provider.create_dirs().is_ok());
    assert!(fs::create_dir_all(storage.join("real/nested")).is_ok());
    assert!(fs::create_dir_all(&outside).is_ok());
    assert!(fs::write(storage.join("real/nested/inside.tmp"), b"inside").is_ok());
    assert!(fs::write(outside.join("outside.tmp"), b"outside").is_ok());
    assert!(symlink(&outside, storage.join("external-link")).is_ok());

    assert_eq!(provider.cleanup_temp_files().ok(), Some(1));
    assert!(outside.join("outside.tmp").exists());
    assert!(storage.join("external-link").exists());
    assert_eq!(provider.health().status, StorageStatus::Online);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn cleanup_rejects_a_tree_beyond_the_depth_bound() {
    let (storage, backups) = temp_paths("cleanup-depth-bound");
    let provider = FileStorageProvider::new(&storage, &backups);
    assert!(provider.create_dirs().is_ok());
    let mut current = storage.clone();
    for _ in 0..130 {
        current.push("d");
        assert!(fs::create_dir(&current).is_ok());
    }
    assert!(fs::write(current.join("orphan.tmp"), b"orphan").is_ok());

    assert!(matches!(
        provider.cleanup_temp_files(),
        Err(StorageError::InvalidPath(message)) if message.contains("depth limit")
    ));
    let health = provider.health();
    assert_eq!(health.status, StorageStatus::Degraded);
    assert_eq!(
        health.message.as_deref(),
        Some("temporary file scan failed")
    );
    let _ = fs::remove_dir_all(storage.parent().unwrap());
}

#[cfg(unix)]
#[test]
fn no_follow_open_rejects_a_file_replaced_after_path_validation() {
    use super::storage_file_fs::{open_regular_file, resolve_under_root};
    use std::os::unix::fs::symlink;

    let (storage, backups) = temp_paths("read-check-use");
    let root = storage.parent().unwrap();
    let outside = root.join("outside.txt");
    let provider = FileStorageProvider::new(&storage, &backups);
    assert!(provider.create_dirs().is_ok());
    assert!(fs::write(storage.join("value.txt"), b"inside").is_ok());
    assert!(fs::write(&outside, b"outside").is_ok());
    let checked = resolve_under_root(&storage, "value.txt").unwrap();

    assert!(fs::remove_file(&checked).is_ok());
    assert!(symlink(&outside, &checked).is_ok());
    assert!(open_regular_file(&checked).is_err());
    assert_eq!(fs::read(&outside).ok(), Some(b"outside".to_vec()));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn backup_atomic_creates_backup_file() {
    let (storage, backups) = temp_paths("atomic-backup");
    let provider = FileStorageProvider::new(&storage, &backups);
    assert!(provider.create_dirs().is_ok());
    assert!(provider.write_bytes("x.txt", b"hello").is_ok());
    assert!(provider.backup_file_atomic("x.txt", "x.bak").is_ok());
    assert!(backups.join("x.bak").exists());
    let _ = fs::remove_dir_all(storage.parent().unwrap_or(std::path::Path::new("")));
}

#[test]
fn backup_source_missing_returns_typed_error() {
    let (storage, backups) = temp_paths("backup-missing");
    let provider = FileStorageProvider::new(&storage, &backups);
    assert!(provider.create_dirs().is_ok());
    let result = provider.backup_file_atomic("missing.txt", "x.bak");
    assert!(matches!(result, Err(StorageError::RepositoryNotFound(_))));
    let _ = fs::remove_dir_all(storage.parent().unwrap_or(std::path::Path::new("")));
}

#[test]
fn health_reports_orphan_tmp_files() {
    let (storage, backups) = temp_paths("health-tmp");
    let provider = FileStorageProvider::new(&storage, &backups);
    assert!(provider.create_dirs().is_ok());
    assert!(fs::write(storage.join("leftover.tmp"), b"x").is_ok());
    let health = provider.health();
    assert_eq!(health.status, StorageStatus::Degraded);
    assert!(health
        .message
        .unwrap_or_default()
        .contains("orphan temp files"));
    let _ = fs::remove_dir_all(storage.parent().unwrap_or(std::path::Path::new("")));
}

#[test]
fn cleanup_recovers_partial_temp_and_preserves_final_file() {
    let (storage, backups) = temp_paths("partial-recovery");
    let provider = FileStorageProvider::new(&storage, &backups);
    assert!(provider.create_dirs().is_ok());
    assert!(fs::write(storage.join("records.txt"), b"valid").is_ok());
    assert!(fs::write(storage.join("records.txt.tmp"), b"partial").is_ok());

    assert_eq!(provider.cleanup_temp_files().ok(), Some(1));
    assert_eq!(
        fs::read(storage.join("records.txt")).ok(),
        Some(b"valid".to_vec())
    );
    assert!(!storage.join("records.txt.tmp").exists());
    let _ = fs::remove_dir_all(storage.parent().unwrap_or(std::path::Path::new("")));
}

#[test]
fn atomic_write_replaces_existing_file_without_leaving_temp() {
    let (storage, backups) = temp_paths("atomic-replace");
    let provider = FileStorageProvider::new(&storage, &backups);
    assert!(provider.create_dirs().is_ok());
    assert!(provider.write_bytes_atomic("state.txt", b"before").is_ok());
    assert!(provider.write_bytes_atomic("state.txt", b"after").is_ok());

    assert_eq!(
        fs::read(storage.join("state.txt")).ok(),
        Some(b"after".to_vec())
    );
    assert!(!storage.join("state.txt.tmp").exists());
    let _ = fs::remove_dir_all(storage.parent().unwrap_or(std::path::Path::new("")));
}

#[test]
fn cleanup_discards_truncated_temp_file_without_touching_final_file() {
    let (storage, backups) = temp_paths("truncated-temp");
    let provider = FileStorageProvider::new(&storage, &backups);
    assert!(provider.create_dirs().is_ok());
    assert!(fs::write(storage.join("state.txt"), b"complete").is_ok());
    assert!(fs::write(storage.join("state.txt.tmp"), b"tru").is_ok());

    assert_eq!(provider.cleanup_temp_files().ok(), Some(1));
    assert_eq!(
        fs::read(storage.join("state.txt")).ok(),
        Some(b"complete".to_vec())
    );
    let _ = fs::remove_dir_all(storage.parent().unwrap_or(std::path::Path::new("")));
}

#[test]
fn cleanup_discards_incomplete_backup_temp_file() {
    let (storage, backups) = temp_paths("incomplete-backup");
    let provider = FileStorageProvider::new(&storage, &backups);
    assert!(provider.create_dirs().is_ok());
    assert!(fs::write(backups.join("state.bak.tmp"), b"partial").is_ok());

    assert_eq!(provider.cleanup_temp_files().ok(), Some(1));
    assert!(!backups.join("state.bak.tmp").exists());
    assert!(!backups.join("state.bak").exists());
    let _ = fs::remove_dir_all(storage.parent().unwrap_or(std::path::Path::new("")));
}

#[test]
fn concurrent_atomic_writes_have_distinct_tmp_paths() {
    let path = std::path::Path::new("some_file.txt");
    let path1 = super::tmp_path_for(path);
    let path2 = super::tmp_path_for(path);
    assert_ne!(path1, path2);
}

fn secure_claims() -> TokenClaims {
    TokenClaims {
        issuer: "storage-test".to_string(),
        audience: "local-file".to_string(),
        salt: "secure-write".to_string(),
        ttl_ms: 0,
    }
}

fn secure_provider(secret: &[u8]) -> HashTokenProvider {
    HashTokenProvider::from_secret(secret.to_vec()).expect("secure storage provider")
}

#[test]
fn secure_write_does_not_store_plaintext() {
    let (storage, backups) = temp_paths("secure-no-plaintext");
    let provider = FileStorageProvider::new(&storage, &backups);
    let key_provider = secure_provider(b"storage-secret-1234567890");
    assert!(provider.create_dirs().is_ok());

    let write = provider.write_secure_bytes(
        "private.bin",
        b"sensitive-test-payload",
        &key_provider,
        &secure_claims(),
    );

    assert!(write.is_ok());
    let raw = fs::read(storage.join("private.bin")).expect("raw sealed file");
    assert_ne!(raw, b"sensitive-test-payload".to_vec());
    assert!(!String::from_utf8_lossy(&raw).contains("sensitive-test-payload"));
    let _ = fs::remove_dir_all(storage.parent().unwrap_or(std::path::Path::new("")));
}

#[test]
fn auth_required_write_fails_without_auth_provider() {
    let (storage, backups) = temp_paths("auth-required-write-no-auth");
    let provider = FileStorageProvider::new(&storage, &backups);
    assert!(provider.create_dirs().is_ok());

    let write = provider.write_auth_required_bytes::<HashTokenProvider>(
        "sensitive.bin",
        b"payload",
        None,
        &secure_claims(),
    );

    assert!(matches!(write, Err(StorageError::AuthUnavailable(_))));
    assert!(!storage.join("sensitive.bin").exists());
    let _ = fs::remove_dir_all(storage.parent().unwrap_or(std::path::Path::new("")));
}

#[test]
fn auth_required_read_fails_without_auth_provider() {
    let (storage, backups) = temp_paths("auth-required-read-no-auth");
    let provider = FileStorageProvider::new(&storage, &backups);
    let auth = secure_provider(b"auth-required-secret-1234");
    assert!(provider.create_dirs().is_ok());
    assert!(provider
        .write_auth_required_bytes("sensitive.bin", b"payload", Some(&auth), &secure_claims())
        .is_ok());

    let read = provider.read_auth_required_bytes::<HashTokenProvider>(
        "sensitive.bin",
        None,
        &secure_claims(),
    );

    assert!(matches!(read, Err(StorageError::AuthUnavailable(_))));
    let _ = fs::remove_dir_all(storage.parent().unwrap_or(std::path::Path::new("")));
}

#[test]
fn auth_required_read_recovers_when_auth_provider_returns() {
    let (storage, backups) = temp_paths("auth-required-read-recovers");
    let provider = FileStorageProvider::new(&storage, &backups);
    let auth = secure_provider(b"auth-required-secret-1234");
    assert!(provider.create_dirs().is_ok());
    assert!(provider
        .write_auth_required_bytes("sensitive.bin", b"payload", Some(&auth), &secure_claims())
        .is_ok());
    assert!(provider
        .read_auth_required_bytes::<HashTokenProvider>("sensitive.bin", None, &secure_claims())
        .is_err());

    let read = provider.read_auth_required_bytes("sensitive.bin", Some(&auth), &secure_claims());

    assert_eq!(read.ok(), Some(b"payload".to_vec()));
    let _ = fs::remove_dir_all(storage.parent().unwrap_or(std::path::Path::new("")));
}

#[test]
fn secure_read_recovers_payload_with_same_key() {
    let (storage, backups) = temp_paths("secure-read");
    let provider = FileStorageProvider::new(&storage, &backups);
    let key_provider = secure_provider(b"storage-secret-1234567890");
    assert!(provider.create_dirs().is_ok());
    assert!(provider
        .write_secure_bytes("private.bin", b"payload", &key_provider, &secure_claims())
        .is_ok());

    let read = provider.read_secure_bytes("private.bin", &key_provider, &secure_claims());

    assert_eq!(read.ok(), Some(b"payload".to_vec()));
    let _ = fs::remove_dir_all(storage.parent().unwrap_or(std::path::Path::new("")));
}

#[test]
fn secure_read_with_wrong_key_returns_typed_error() {
    let (storage, backups) = temp_paths("secure-wrong-key");
    let provider = FileStorageProvider::new(&storage, &backups);
    let good = secure_provider(b"storage-secret-1234567890");
    let wrong = secure_provider(b"other-storage-secret-1234");
    assert!(provider.create_dirs().is_ok());
    assert!(provider
        .write_secure_bytes("private.bin", b"payload", &good, &secure_claims())
        .is_ok());

    let read = provider.read_secure_bytes("private.bin", &wrong, &secure_claims());

    assert!(matches!(read, Err(StorageError::SecurityFailed(_))));
    let _ = fs::remove_dir_all(storage.parent().unwrap_or(std::path::Path::new("")));
}
