// =============================================================================
//        #######
//     ###       ###     F: storage_tree.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/20 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/20 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Bounded iterative traversal for provider-owned directory trees.

use super::storage_file_fs::{ensure_real_directory, metadata_is_link};
use super::{StorageError, StorageResult};
use std::fs;
use std::path::Path;

const MAX_STORAGE_TREE_DEPTH: usize = 128;
const MAX_STORAGE_TREE_DIRECTORIES: usize = 16_384;
const MAX_STORAGE_TREE_ENTRIES: usize = 200_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StorageTreeEntryKind {
    File,
    Directory,
    Link,
    Other,
}

pub(super) fn visit_bounded_tree(
    root: &Path,
    mut visit: impl FnMut(&Path, StorageTreeEntryKind) -> StorageResult<()>,
) -> StorageResult<()> {
    ensure_real_directory(root)?;
    let mut pending = vec![(root.to_path_buf(), 0usize)];
    let mut directory_count = 1usize;
    let mut entry_count = 0usize;

    while let Some((directory, depth)) = pending.pop() {
        ensure_real_directory(&directory)?;
        for entry in fs::read_dir(&directory).map_err(|_| StorageError::NotAvailable)? {
            let entry = entry.map_err(|_| StorageError::NotAvailable)?;
            if entry_count >= MAX_STORAGE_TREE_ENTRIES {
                return Err(limit_error("entry"));
            }
            entry_count += 1;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|_| StorageError::NotAvailable)?;
            let kind = if metadata_is_link(&metadata) {
                StorageTreeEntryKind::Link
            } else if metadata.is_dir() {
                if depth >= MAX_STORAGE_TREE_DEPTH {
                    return Err(limit_error("depth"));
                }
                directory_count = directory_count.saturating_add(1);
                if directory_count > MAX_STORAGE_TREE_DIRECTORIES {
                    return Err(limit_error("directory"));
                }
                pending.push((path.clone(), depth + 1));
                StorageTreeEntryKind::Directory
            } else if metadata.is_file() {
                StorageTreeEntryKind::File
            } else {
                StorageTreeEntryKind::Other
            };
            visit(&path, kind)?;
        }
    }
    Ok(())
}

fn limit_error(limit: &str) -> StorageError {
    StorageError::InvalidPath(format!("storage traversal {limit} limit exceeded"))
}
