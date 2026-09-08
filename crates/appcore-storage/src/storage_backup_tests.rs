// =============================================================================
//        #######
//     ###       ###     F: storage_backup_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::storage_file_fs::{
    copy_atomic_file_with_fault, tmp_path_for, write_atomic_file_with_fault, AtomicCopyFault,
    AtomicWriteFault,
};
use super::{
    FileStorageProvider, StorageError, StorageProvider, MAX_STORAGE_BACKUP_FILE_BYTES,
    STORAGE_BACKUP_FORMAT_V1,
};
use std::fs;
use std::sync::{Arc, Barrier};

fn temp_provider(name: &str) -> (FileStorageProvider, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "appcore-storage-backup-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    (
        FileStorageProvider::new(root.join("data"), root.join("backups")),
        root,
    )
}

#[test]
fn complete_snapshot_restore_replaces_the_whole_data_root() {
    let (provider, root) = temp_provider("complete");
    provider.create_dirs().unwrap();
    provider.write_bytes("a/one.txt", b"one").unwrap();
    provider.write_bytes("two.txt", b"two").unwrap();
    let backup = provider.create_snapshot_backup("baseline").unwrap();
    provider.write_bytes("a/one.txt", b"changed").unwrap();
    provider.write_bytes("extra.txt", b"remove-me").unwrap();

    let restored = provider.restore_snapshot_backup("baseline").unwrap();

    assert_eq!(backup, restored);
    assert_eq!(provider.read_bytes("a/one.txt").unwrap(), b"one");
    assert_eq!(provider.read_bytes("two.txt").unwrap(), b"two");
    assert!(!provider.exists("extra.txt").unwrap());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn snapshot_manifest_is_versioned_and_verifiable() {
    let (provider, root) = temp_provider("manifest");
    provider.create_dirs().unwrap();
    provider.write_bytes("state.json", b"{}").unwrap();
    provider.create_snapshot_backup("snapshot-a").unwrap();

    let text = fs::read_to_string(root.join("backups/snapshot-a/manifest.json")).unwrap();
    assert!(text.contains(STORAGE_BACKUP_FORMAT_V1));
    assert_eq!(
        provider.verify_snapshot_backup("snapshot-a").unwrap().name,
        "snapshot-a"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn snapshot_rejects_oversized_file_before_copying() {
    let (provider, root) = temp_provider("snapshot-file-limit");
    provider.create_dirs().unwrap();
    let file = fs::File::create(root.join("data/oversized.bin")).unwrap();
    file.set_len(MAX_STORAGE_BACKUP_FILE_BYTES + 1).unwrap();

    assert!(matches!(
        provider.create_snapshot_backup("oversized"),
        Err(StorageError::BackupFailed(_))
    ));
    assert!(!root.join("backups/oversized").exists());
    assert_eq!(provider.cleanup_temp_files().unwrap(), 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn listed_snapshot_timestamps_come_from_the_persisted_manifests() {
    let (provider, root) = temp_provider("list-created-at");
    provider.create_dirs().unwrap();
    provider.write_bytes("state.json", b"{}").unwrap();
    let first = provider.create_snapshot_backup("snapshot-a").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let second = provider.create_snapshot_backup("snapshot-b").unwrap();

    let listed_once = provider.list_backups();
    assert_eq!(listed_once.len(), 2);
    assert_eq!(listed_once[0], first);
    assert_eq!(listed_once[1], second);
    assert!(listed_once[0].created_at_ms < listed_once[1].created_at_ms);

    std::thread::sleep(std::time::Duration::from_millis(5));
    assert_eq!(provider.list_backups(), listed_once);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn tampered_snapshot_is_rejected_without_modifying_current_data() {
    let (provider, root) = temp_provider("tamper");
    provider.create_dirs().unwrap();
    provider.write_bytes("state.txt", b"original").unwrap();
    provider.create_snapshot_backup("snapshot-a").unwrap();
    provider.write_bytes("state.txt", b"current").unwrap();
    fs::write(root.join("backups/snapshot-a/data/state.txt"), b"tampered").unwrap();

    assert!(matches!(
        provider.restore_snapshot_backup("snapshot-a"),
        Err(StorageError::BackupFailed(_) | StorageError::NotAvailable)
    ));
    assert_eq!(provider.read_bytes("state.txt").unwrap(), b"current");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn snapshot_with_unlisted_file_is_rejected_without_path_inventory() {
    let (provider, root) = temp_provider("unlisted-file");
    provider.create_dirs().unwrap();
    provider.write_bytes("state.txt", b"state").unwrap();
    provider.create_snapshot_backup("snapshot-a").unwrap();
    fs::write(
        root.join("backups/snapshot-a/data/unlisted.txt"),
        b"unlisted",
    )
    .unwrap();

    assert!(matches!(
        provider.verify_snapshot_backup("snapshot-a"),
        Err(StorageError::BackupFailed(_))
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn truncated_or_future_manifest_is_rejected() {
    let (provider, root) = temp_provider("format");
    provider.create_dirs().unwrap();
    provider.write_bytes("state.txt", b"state").unwrap();
    provider.create_snapshot_backup("snapshot-a").unwrap();
    let manifest = root.join("backups/snapshot-a/manifest.json");
    fs::write(&manifest, b"{").unwrap();
    assert!(provider.verify_snapshot_backup("snapshot-a").is_err());
    let future = "{\"format\":\"appcore-storage-backup-v2\",\"name\":\"snapshot-a\",\"created_at_ms\":1,\"files\":[]}".to_string();
    fs::write(manifest, future).unwrap();
    assert!(provider.verify_snapshot_backup("snapshot-a").is_err());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn symlink_in_source_or_backup_is_rejected() {
    use std::os::unix::fs::symlink;

    let (provider, root) = temp_provider("symlink");
    provider.create_dirs().unwrap();
    fs::write(root.join("outside"), b"outside").unwrap();
    symlink(root.join("outside"), root.join("data/link")).unwrap();
    assert!(matches!(
        provider.create_snapshot_backup("snapshot-a"),
        Err(StorageError::InvalidPath(_))
    ));
    fs::remove_file(root.join("data/link")).unwrap();
    provider.write_bytes("state.txt", b"state").unwrap();
    provider.create_snapshot_backup("snapshot-a").unwrap();
    fs::remove_file(root.join("backups/snapshot-a/data/state.txt")).unwrap();
    symlink(
        root.join("outside"),
        root.join("backups/snapshot-a/data/state.txt"),
    )
    .unwrap();
    assert!(matches!(
        provider.verify_snapshot_backup("snapshot-a"),
        Err(StorageError::InvalidPath(_))
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn interrupted_restore_promotes_verified_pending_root() {
    let (provider, root) = temp_provider("pending-recovery");
    provider.create_dirs().unwrap();
    fs::remove_dir_all(root.join("data")).unwrap();
    fs::create_dir(root.join("data.restore.pending")).unwrap();
    fs::write(root.join("data.restore.pending/state.txt"), b"pending").unwrap();
    fs::create_dir(root.join("data.restore.previous")).unwrap();
    fs::write(root.join("data.restore.previous/state.txt"), b"previous").unwrap();

    provider.recover_snapshot_restore().unwrap();

    assert_eq!(provider.read_bytes("state.txt").unwrap(), b"pending");
    assert!(!root.join("data.restore.pending").exists());
    assert!(!root.join("data.restore.previous").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn interrupted_restore_rolls_back_when_no_pending_root_exists() {
    let (provider, root) = temp_provider("previous-recovery");
    provider.create_dirs().unwrap();
    fs::remove_dir_all(root.join("data")).unwrap();
    fs::create_dir(root.join("data.restore.previous")).unwrap();
    fs::write(root.join("data.restore.previous/state.txt"), b"previous").unwrap();

    provider.recover_snapshot_restore().unwrap();

    assert_eq!(provider.read_bytes("state.txt").unwrap(), b"previous");
    assert!(!root.join("data.restore.previous").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn atomic_write_faults_preserve_the_previous_complete_file() {
    let (provider, root) = temp_provider("atomic-faults");
    provider.create_dirs().unwrap();
    let final_path = root.join("data/state.txt");
    fs::write(&final_path, b"stable").unwrap();
    let faults = [
        AtomicWriteFault::DiskFull,
        AtomicWriteFault::PermissionDenied,
        AtomicWriteFault::AfterPartialWrite,
        AtomicWriteFault::AfterFileSync,
        AtomicWriteFault::BeforeRename,
    ];
    for fault in faults {
        let tmp = tmp_path_for(&final_path);
        assert!(write_atomic_file_with_fault(&tmp, &final_path, b"replacement", fault).is_err());
        assert_eq!(fs::read(&final_path).unwrap(), b"stable");
    }
    assert!(provider.cleanup_temp_files().unwrap() >= 3);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn streaming_copy_faults_preserve_the_previous_file_and_remove_temp() {
    let (provider, root) = temp_provider("stream-copy-faults");
    provider.create_dirs().unwrap();
    let source_path = root.join("data/source.bin");
    let final_path = root.join("backups/source.bak");
    fs::write(&source_path, b"replacement").unwrap();
    fs::write(&final_path, b"stable").unwrap();
    let faults = [
        AtomicCopyFault::DiskFull,
        AtomicCopyFault::AfterPartialWrite,
        AtomicCopyFault::AfterFileSync,
        AtomicCopyFault::SourceGrowth,
    ];
    for fault in faults {
        fs::write(&source_path, b"replacement").unwrap();
        let mut source = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&source_path)
            .unwrap();
        let tmp = tmp_path_for(&final_path);
        assert!(copy_atomic_file_with_fault(&mut source, &tmp, &final_path, 1024, fault).is_err());
        assert_eq!(fs::read(&final_path).unwrap(), b"stable");
        assert!(!tmp.exists());
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn concurrent_provider_instances_never_publish_partial_payloads() {
    let (provider, root) = temp_provider("concurrent");
    provider.create_dirs().unwrap();
    let provider = Arc::new(provider);
    let barrier = Arc::new(Barrier::new(8));
    let mut workers = Vec::new();
    for index in 0..8u8 {
        let provider = Arc::clone(&provider);
        let barrier = Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            let payload = vec![index; 128 * 1024];
            barrier.wait();
            provider.write_bytes("shared.bin", &payload).unwrap();
        }));
    }
    for worker in workers {
        worker.join().unwrap();
    }

    let result = provider.read_bytes("shared.bin").unwrap();
    assert_eq!(result.len(), 128 * 1024);
    assert!(result.iter().all(|byte| *byte == result[0]));
    fs::remove_dir_all(root).unwrap();
}
