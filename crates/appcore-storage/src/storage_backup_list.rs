// =============================================================================
//        #######
//     ###       ###     F: storage_backup_list.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/20 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/20 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Stable backup catalog descriptors derived from persisted metadata.

use super::storage_backup::{descriptor, read_manifest, STORAGE_BACKUP_FORMAT_V1};
use super::storage_file_fs::{ensure_real_directory, metadata_is_link};
use super::BackupDescriptor;
use std::fs::{self, Metadata};
use std::path::Path;
use std::time::UNIX_EPOCH;

pub(super) fn list_backup_descriptors(root: &Path) -> Vec<BackupDescriptor> {
    if ensure_real_directory(root).is_err() {
        return Vec::new();
    }
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut backups = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".tmp") {
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if metadata_is_link(&metadata) {
            continue;
        }
        let backup = if metadata.is_dir() {
            read_manifest(&entry.path(), &name)
                .ok()
                .filter(|manifest| {
                    manifest.format == STORAGE_BACKUP_FORMAT_V1 && manifest.name == name
                })
                .map(|manifest| descriptor(&manifest))
        } else if metadata.is_file() {
            metadata_created_at_ms(&metadata).map(|created_at_ms| BackupDescriptor {
                name,
                created_at_ms,
            })
        } else {
            None
        };
        if let Some(backup) = backup {
            backups.push(backup);
        }
    }
    backups.sort_by(|left, right| {
        left.created_at_ms
            .cmp(&right.created_at_ms)
            .then_with(|| left.name.cmp(&right.name))
    });
    backups
}

fn metadata_created_at_ms(metadata: &Metadata) -> Option<u64> {
    metadata
        .created()
        .or_else(|_| metadata.modified())
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}
