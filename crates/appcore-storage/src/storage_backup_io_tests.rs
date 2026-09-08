// =============================================================================
//        #######
//     ###       ###     F: storage_backup_io_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/03 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/03 00:00:00 by dnettoRaw
//      ###########      S: 1.0.3-rc
// =============================================================================
// appcore-norm: test

use super::super::storage_backup::{StorageBackupManifestFileV1, STORAGE_BACKUP_FORMAT_V1};
use super::*;
use std::io::{self, Cursor, Read};

#[test]
fn manifest_stream_preserves_pretty_encoding_and_roundtrip() {
    let root = test_root("roundtrip");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let manifest = manifest();

    write_manifest(&root, &manifest).unwrap();

    assert_eq!(
        std::fs::read(root.join(BACKUP_MANIFEST)).unwrap(),
        serde_json::to_vec_pretty(&manifest).unwrap()
    );
    assert_eq!(read_manifest(&root, &manifest.name).unwrap(), manifest);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn manifest_stream_accepts_exact_limit_and_rejects_growth() {
    let manifest = manifest();
    let mut encoded = serde_json::to_vec_pretty(&manifest).unwrap();
    let exact = encoded.len() as u64;
    assert_eq!(
        deserialize_manifest(Cursor::new(&encoded), exact, exact, &manifest.name).unwrap(),
        manifest
    );

    encoded.push(b' ');
    assert!(deserialize_manifest(Cursor::new(encoded), exact, exact, "snapshot-a").is_err());
}

#[test]
fn manifest_writer_enforces_limit_and_atomic_failure_removes_temporary() {
    let root = test_root("writer-limit");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let manifest = manifest();
    let final_path = root.join(BACKUP_MANIFEST);
    std::fs::write(&final_path, b"stable").unwrap();
    let temporary = tmp_path_for(&final_path);

    let encoded = serde_json::to_vec_pretty(&manifest).unwrap();
    let result = write_atomic_file_using(&temporary, &final_path, |file| {
        serialize_manifest(file, &manifest, encoded.len().saturating_sub(1) as u64)
    });

    assert!(result.is_err());
    assert_eq!(std::fs::read(&final_path).unwrap(), b"stable");
    assert!(!temporary.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn declared_oversize_manifest_is_rejected_before_read() {
    struct PanicReader;

    impl Read for PanicReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            panic!("oversized backup manifest must not be read");
        }
    }

    assert!(deserialize_manifest(PanicReader, 17, 16, "snapshot-a").is_err());
}

fn manifest() -> StorageBackupManifestV1 {
    StorageBackupManifestV1 {
        format: STORAGE_BACKUP_FORMAT_V1.to_string(),
        name: "snapshot-a".to_string(),
        created_at_ms: 1,
        files: vec![StorageBackupManifestFileV1 {
            path: "state.json".to_string(),
            size: 2,
            sha256: "a".repeat(64),
        }],
    }
}

fn test_root(suffix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "appcore-storage-manifest-io-{suffix}-{}",
        std::process::id()
    ))
}
