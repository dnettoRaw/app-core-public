// =============================================================================
//        #######
//     ###       ###     F: storage_atomic_file.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/05 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/05 00:00:00 by dnettoRaw
//      ###########      S: 1.0.4-rc
// =============================================================================

//! Bounded atomic replacement of one deployment-owned file outside provider roots.

use super::storage_file_fs::{
    ensure_real_directory, metadata_is_link, tmp_path_for, write_atomic_file_using,
};
use super::{StorageError, StorageResult};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

/// Maximum accepted single-file write budget (64 MiB).
pub const MAX_ATOMIC_FILE_BYTES: usize = 64 * 1024 * 1024;

/// Atomic file replacement with explicit path and byte budget.
///
/// Parent directories must already exist, contain no symlinks/reparse points,
/// and be protected by the deployment against untrusted replacement. Concurrent
/// writers use exclusive temporary files; the last successful rename wins.
/// This is not a transaction or a lock across processes. A directory-sync error
/// after rename is returned as failure even though new bytes may be visible.
#[derive(Debug)]
pub struct AtomicFileWriter {
    path: PathBuf,
    max_bytes: usize,
}
impl AtomicFileWriter {
    /// Validates an absolute destination without creating directories.
    pub fn new(path: impl Into<PathBuf>, max_bytes: usize) -> StorageResult<Self> {
        let path = path.into();
        if max_bytes == 0 || max_bytes > MAX_ATOMIC_FILE_BYTES {
            return Err(StorageError::TransactionFailed(
                "invalid atomic file byte budget".into(),
            ));
        }
        validate_destination(&path)?;
        Ok(Self { path, max_bytes })
    }
    /// Revalidates the destination, writes an exclusive sibling, syncs and renames.
    /// Over-budget input is rejected before creating a temporary file.
    pub fn write(&self, bytes: &[u8]) -> StorageResult<()> {
        if bytes.len() > self.max_bytes {
            return Err(StorageError::TransactionFailed(
                "atomic file exceeds byte budget".into(),
            ));
        }
        validate_destination(&self.path)?;
        let tmp = tmp_path_for(&self.path);
        write_atomic_file_using(&tmp, &self.path, |file| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(fs::Permissions::from_mode(0o600))?;
            }
            file.write_all(bytes)
        })
        .map_err(|error| {
            StorageError::TransactionFailed(format!("atomic file write failed: {}", error.kind()))
        })
    }
}

fn validate_destination(path: &Path) -> StorageResult<()> {
    if !path.is_absolute()
        || path.file_name().is_none()
        || path.components().any(|c| matches!(c, Component::ParentDir))
    {
        return Err(StorageError::InvalidPath(
            "absolute file destination required".into(),
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| StorageError::InvalidPath("missing parent".into()))?;
    let mut current = PathBuf::new();
    for component in parent.components() {
        current.push(component);
        ensure_real_directory(&current)?;
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata_is_link(&metadata) || !metadata.is_file() => Err(
            StorageError::InvalidPath("destination must be a regular file".into()),
        ),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(StorageError::NotAvailable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = fs::canonicalize(std::env::temp_dir())
                .unwrap()
                .join(format!(
                    "appcore-atomic-{}-{:?}",
                    std::process::id(),
                    std::thread::current().id()
                ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn replacement_is_complete_and_budget_failure_preserves_previous_content() {
        let fixture = Fixture::new();
        let path = fixture.0.join("settings");
        let writer = AtomicFileWriter::new(&path, 8).unwrap();
        writer.write(b"before").unwrap();
        writer.write(b"after").unwrap();
        assert!(writer.write(b"oversized").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"after");
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
    #[test]
    fn failed_exclusive_creation_never_removes_another_writers_temporary() {
        let fixture = Fixture::new();
        let temporary = fixture.0.join("reserved.tmp");
        fs::write(&temporary, b"another writer").unwrap();
        let result = write_atomic_file_using(&temporary, &fixture.0.join("final"), |file| {
            file.write_all(b"ours")
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&temporary).unwrap(), b"another writer");
    }
    #[test]
    fn write_failure_discards_only_owned_staging_and_preserves_final() {
        let fixture = Fixture::new();
        let temporary = fixture.0.join("owned.tmp");
        let final_path = fixture.0.join("final");
        fs::write(&final_path, b"before").unwrap();
        let result = write_atomic_file_using(&temporary, &final_path, |file| {
            file.write_all(b"partial")?;
            Err(std::io::Error::other("injected failure"))
        });
        assert!(result.is_err());
        assert!(!temporary.exists());
        assert_eq!(fs::read(&final_path).unwrap(), b"before");
    }
    #[cfg(unix)]
    #[test]
    fn links_and_replaced_destinations_fail_without_touching_the_target() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let target = fixture.0.join("target");
        fs::write(&target, b"original").unwrap();
        let path = fixture.0.join("destination");
        let writer = AtomicFileWriter::new(&path, 16).unwrap();
        symlink(&target, &path).unwrap();
        assert!(writer.write(b"changed").is_err());
        assert_eq!(fs::read(&target).unwrap(), b"original");
        let linked = fixture.0.join("linked");
        symlink(&fixture.0, &linked).unwrap();
        assert!(AtomicFileWriter::new(linked.join("new"), 16).is_err());
    }
}
