// =============================================================================
//        #######
//     ###       ###     F: secret_keyring_fs.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::{SecretAccessError, SecretAccessResult};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn reject_unsafe_root(root: &Path) -> SecretAccessResult<()> {
    if root.as_os_str().is_empty()
        || root
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(SecretAccessError::InvalidPath);
    }
    reject_symlink(root)
}

pub(super) fn create_private_directory(path: &Path) -> SecretAccessResult<()> {
    reject_symlink(path)?;
    if create_directory(path)? {
        set_private_directory_permissions(path)?;
    }
    validate_private_directory(path)
}

pub(super) fn read_private_file(path: &Path, max_bytes: u64) -> SecretAccessResult<Vec<u8>> {
    validate_private_file(path)?;
    let mut file = open_read_file(path)?;
    let metadata = file.metadata().map_err(|_| SecretAccessError::Io)?;
    if metadata.len() > max_bytes {
        return Err(SecretAccessError::InvalidMaterial);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|_| SecretAccessError::Io)?;
    Ok(bytes)
}

pub(super) fn atomic_write(path: &Path, contents: &[u8]) -> SecretAccessResult<()> {
    let parent = path.parent().ok_or(SecretAccessError::InvalidPath)?;
    validate_private_directory(parent)?;
    reject_symlink(path)?;
    if path.exists() {
        validate_private_file(path)?;
    }
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .ok_or(SecretAccessError::InvalidPath)?,
        unique_suffix()
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|_| SecretAccessError::Io)?;
    set_private_file_permissions(&temporary, &file)?;
    file.write_all(contents)
        .and_then(|_| file.sync_all())
        .map_err(|_| SecretAccessError::Io)?;
    drop(file);
    replace_file(&temporary, path)?;
    sync_directory(parent)
}

#[cfg(not(windows))]
fn replace_file(temporary: &Path, target: &Path) -> SecretAccessResult<()> {
    fs::rename(temporary, target).map_err(|_| SecretAccessError::Io)
}

#[cfg(windows)]
fn replace_file(temporary: &Path, target: &Path) -> SecretAccessResult<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let source = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(SecretAccessError::Io);
    }
    Ok(())
}

pub(super) fn remove_file_if_present(path: &Path) -> SecretAccessResult<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(SecretAccessError::Io),
    }
}

pub(super) fn open_lock(path: &Path) -> SecretAccessResult<File> {
    validate_private_file(path)?;
    open_lock_file(path)
}

pub(super) fn reject_symlink(path: &Path) -> SecretAccessResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || is_reparse_point(&metadata) => {
            Err(SecretAccessError::InvalidPath)
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(SecretAccessError::Io),
    }
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> SecretAccessResult<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|_| SecretAccessError::Io)
}

#[cfg(not(unix))]
fn set_private_directory_permissions(path: &Path) -> SecretAccessResult<()> {
    #[cfg(windows)]
    crate::secret_file::windows_acl::set_path_owner_acl(path, true)
        .map_err(|_| SecretAccessError::InsecurePermissions)?;
    Ok(())
}

#[cfg(unix)]
pub(super) fn set_private_file_permissions(_path: &Path, file: &File) -> SecretAccessResult<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|_| SecretAccessError::Io)
}

#[cfg(not(unix))]
pub(super) fn set_private_file_permissions(path: &Path, _file: &File) -> SecretAccessResult<()> {
    #[cfg(windows)]
    crate::secret_file::windows_acl::set_path_owner_acl(path, false)
        .map_err(|_| SecretAccessError::InsecurePermissions)?;
    Ok(())
}

#[cfg(unix)]
pub(super) fn validate_private_directory(path: &Path) -> SecretAccessResult<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let metadata = fs::symlink_metadata(path).map_err(|_| SecretAccessError::Unavailable)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err(SecretAccessError::InsecurePermissions);
    }
    Ok(())
}

#[cfg(not(unix))]
pub(super) fn validate_private_directory(path: &Path) -> SecretAccessResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| SecretAccessError::Unavailable)?;
    if metadata.file_type().is_symlink() || is_reparse_point(&metadata) || !metadata.is_dir() {
        return Err(SecretAccessError::InsecurePermissions);
    }
    #[cfg(windows)]
    crate::secret_file::windows_acl::validate_path_owner_acl(path)
        .map_err(|_| SecretAccessError::InsecurePermissions)?;
    Ok(())
}

#[cfg(unix)]
pub(super) fn validate_private_file(path: &Path) -> SecretAccessResult<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let metadata = fs::symlink_metadata(path).map_err(|_| SecretAccessError::Unavailable)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err(SecretAccessError::InsecurePermissions);
    }
    Ok(())
}

#[cfg(not(unix))]
pub(super) fn validate_private_file(path: &Path) -> SecretAccessResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| SecretAccessError::Unavailable)?;
    if metadata.file_type().is_symlink() || is_reparse_point(&metadata) || !metadata.is_file() {
        return Err(SecretAccessError::InsecurePermissions);
    }
    #[cfg(windows)]
    crate::secret_file::windows_acl::validate_file_owner_acl(&open_lock_file(path)?)
        .map_err(|_| SecretAccessError::InsecurePermissions)?;
    Ok(())
}

#[cfg(not(windows))]
pub(super) fn sync_directory(path: &Path) -> SecretAccessResult<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| SecretAccessError::Io)
}

#[cfg(windows)]
pub(super) fn sync_directory(path: &Path) -> SecretAccessResult<()> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS;
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| SecretAccessError::Io)
}

#[cfg(windows)]
fn create_directory(path: &Path) -> SecretAccessResult<bool> {
    match fs::create_dir(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(_) => Err(SecretAccessError::Io),
    }
}

#[cfg(not(windows))]
fn create_directory(path: &Path) -> SecretAccessResult<bool> {
    match fs::create_dir(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(_) => Err(SecretAccessError::Io),
    }
}

#[cfg(unix)]
fn open_read_file(path: &Path) -> SecretAccessResult<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|_| SecretAccessError::Unavailable)
}

#[cfg(windows)]
fn open_read_file(path: &Path) -> SecretAccessResult<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| SecretAccessError::Unavailable)
}

#[cfg(all(not(unix), not(windows)))]
fn open_read_file(_path: &Path) -> SecretAccessResult<File> {
    Err(SecretAccessError::Unavailable)
}

#[cfg(unix)]
fn open_lock_file(path: &Path) -> SecretAccessResult<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|_| SecretAccessError::Io)
}

#[cfg(windows)]
fn open_lock_file(path: &Path) -> SecretAccessResult<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
    OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| SecretAccessError::Io)
}

#[cfg(all(not(unix), not(windows)))]
fn open_lock_file(_path: &Path) -> SecretAccessResult<File> {
    Err(SecretAccessError::Io)
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{}-{nanos}", std::process::id())
}

pub(super) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
