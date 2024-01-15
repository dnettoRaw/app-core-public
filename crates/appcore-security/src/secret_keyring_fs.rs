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
    fs::create_dir_all(path).map_err(|_| SecretAccessError::Io)?;
    set_private_directory_permissions(path)?;
    validate_private_directory(path)
}

pub(super) fn read_private_file(path: &Path, max_bytes: u64) -> SecretAccessResult<Vec<u8>> {
    validate_private_file(path)?;
    let mut file = File::open(path).map_err(|_| SecretAccessError::Unavailable)?;
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
    set_private_file_permissions(&file)?;
    file.write_all(contents)
        .and_then(|_| file.sync_all())
        .map_err(|_| SecretAccessError::Io)?;
    drop(file);
    replace_file(&temporary, path)?;
    sync_directory(parent)
}

fn replace_file(temporary: &Path, target: &Path) -> SecretAccessResult<()> {
    #[cfg(windows)]
    if target.exists() {
        fs::remove_file(target).map_err(|_| SecretAccessError::Io)?;
    }
    fs::rename(temporary, target).map_err(|_| SecretAccessError::Io)
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
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|_| SecretAccessError::Io)
}

pub(super) fn reject_symlink(path: &Path) -> SecretAccessResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(SecretAccessError::InvalidPath),
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
fn set_private_directory_permissions(_path: &Path) -> SecretAccessResult<()> {
    Ok(())
}

#[cfg(unix)]
pub(super) fn set_private_file_permissions(file: &File) -> SecretAccessResult<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|_| SecretAccessError::Io)
}

#[cfg(not(unix))]
pub(super) fn set_private_file_permissions(_file: &File) -> SecretAccessResult<()> {
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
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(SecretAccessError::InsecurePermissions);
    }
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
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(SecretAccessError::InsecurePermissions);
    }
    Ok(())
}

pub(super) fn sync_directory(path: &Path) -> SecretAccessResult<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| SecretAccessError::Io)
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
