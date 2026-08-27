// =============================================================================
//        #######
//     ###       ###     F: backup.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

use crate::store::normalize_path;
use crate::{
    SqliteSyncError, SqliteSyncResult, SqliteSyncStore, SQLITE_SYNC_SCHEMA_V1,
    SQLITE_SYNC_SCHEMA_V2,
};
use rusqlite::backup::Backup;
use rusqlite::{Connection, OpenFlags};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Evidence returned after an integrity-checked online backup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SqliteBackupReport {
    /// Internal schema version copied to the backup.
    pub schema_version: u32,
    /// Final backup file size in bytes.
    pub bytes: u64,
}

impl SqliteSyncStore {
    /// Creates an integrity-checked online backup and atomically publishes it.
    pub fn online_backup(
        &self,
        destination: impl AsRef<Path>,
    ) -> SqliteSyncResult<SqliteBackupReport> {
        let destination = normalize_backup_destination(destination.as_ref())?;
        let temporary = reserve_temporary(&destination)?;
        let result = self.copy_backup(&temporary).and_then(|report| {
            sync_file(&temporary)?;
            publish_temporary(&temporary, &destination)?;
            Ok(report)
        });
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn copy_backup(&self, temporary: &Path) -> SqliteSyncResult<SqliteBackupReport> {
        self.with_connection(|source| {
            copy_connection(source, temporary, self.config().backup_pages_per_step)
        })
    }

    /// Restores a verified backup into a new, previously absent database path.
    pub fn restore_backup_to_new(
        backup_path: impl AsRef<Path>,
        destination: impl AsRef<Path>,
    ) -> SqliteSyncResult<SqliteBackupReport> {
        let original_backup_path = backup_path.as_ref();
        let backup_path = normalize_path(original_backup_path)?;
        if !backup_path.is_file() {
            return Err(SqliteSyncError::UnsafePath);
        }
        let destination = normalize_backup_destination(destination.as_ref())?;
        let source = Connection::open_with_flags(
            &backup_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
        )
        .map_err(SqliteSyncError::database)?;
        validate_backup_connection(&source)?;
        let temporary = reserve_temporary(&destination)?;
        let result = copy_connection(&source, &temporary, 128).and_then(|report| {
            sync_file(&temporary)?;
            publish_temporary(&temporary, &destination)?;
            Ok(report)
        });
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

fn copy_connection(
    source: &Connection,
    temporary: &Path,
    pages_per_step: i32,
) -> SqliteSyncResult<SqliteBackupReport> {
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_FULL_MUTEX;
    let mut destination =
        Connection::open_with_flags(temporary, flags).map_err(SqliteSyncError::database)?;
    let backup = Backup::new(source, &mut destination).map_err(SqliteSyncError::database)?;
    backup
        .run_to_completion(pages_per_step, Duration::from_millis(2), None)
        .map_err(SqliteSyncError::database)?;
    drop(backup);
    let schema_version = validate_backup_connection(&destination)?;
    drop(destination);
    let bytes = fs::metadata(temporary)
        .map_err(|_| SqliteSyncError::DatabaseOperation)?
        .len();
    Ok(SqliteBackupReport {
        schema_version,
        bytes,
    })
}

fn normalize_backup_destination(path: &Path) -> SqliteSyncResult<PathBuf> {
    if path.as_os_str().is_empty() {
        return Err(SqliteSyncError::UnsafePath);
    }
    let normalized = normalize_path(path)?;
    if normalized.exists() {
        return Err(SqliteSyncError::UnsafePath);
    }
    Ok(normalized)
}

fn reserve_temporary(destination: &Path) -> SqliteSyncResult<PathBuf> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(SqliteSyncError::UnsafePath)?;
    for attempt in 0..16u8 {
        let candidate = parent.join(format!(".{name}.{}-{attempt}.tmp", std::process::id()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                drop(file);
                return Ok(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(SqliteSyncError::DatabaseOperation),
        }
    }
    Err(SqliteSyncError::CapacityExceeded("backup temporary"))
}

fn publish_temporary(temporary: &Path, destination: &Path) -> SqliteSyncResult<()> {
    fs::hard_link(temporary, destination).map_err(|_| SqliteSyncError::DatabaseOperation)?;
    if fs::remove_file(temporary).is_err() {
        let _ = fs::remove_file(destination);
        return Err(SqliteSyncError::DatabaseOperation);
    }
    if sync_parent(destination).is_err() {
        let _ = fs::remove_file(destination);
        let _ = sync_parent(destination);
        return Err(SqliteSyncError::DatabaseOperation);
    }
    Ok(())
}

fn validate_backup_connection(connection: &Connection) -> SqliteSyncResult<u32> {
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(SqliteSyncError::database)?;
    let integrity: String = connection
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(SqliteSyncError::database)?;
    if matches!(version, SQLITE_SYNC_SCHEMA_V1 | SQLITE_SYNC_SCHEMA_V2) && integrity == "ok" {
        Ok(version)
    } else if !matches!(version, SQLITE_SYNC_SCHEMA_V1 | SQLITE_SYNC_SCHEMA_V2) {
        Err(SqliteSyncError::UpdateRequired)
    } else {
        Err(SqliteSyncError::IntegrityFailed)
    }
}

fn sync_file(path: &Path) -> SqliteSyncResult<()> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|_| SqliteSyncError::DatabaseOperation)
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> SqliteSyncResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| SqliteSyncError::DatabaseOperation)
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> SqliteSyncResult<()> {
    Ok(())
}
