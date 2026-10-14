// =============================================================================
//        #######
//     ###       ###     F: filesystem.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/06 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/06 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Owner-protected spool paths and exclusive atomic publication.
use crate::{TransferError as E, TransferResult};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
#[cfg(windows)]
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
pub(crate) fn is_link(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}
pub(crate) fn validate_root(root: &Path) -> TransferResult<()> {
    if !root.is_absolute() || root.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(E::UnsafePath);
    }
    let mut current = PathBuf::new();
    for part in root.components() {
        current.push(part);
        let m = fs::symlink_metadata(&current).map_err(|_| E::Storage)?;
        if is_link(&m) || !m.is_dir() {
            return Err(E::UnsafePath);
        }
    }
    private_directory(root)
}
pub(crate) fn private_directory(path: &Path) -> TransferResult<()> {
    let m = fs::symlink_metadata(path).map_err(|_| E::Storage)?;
    if is_link(&m) || !m.is_dir() {
        return Err(E::UnsafePath);
    }
    #[cfg(unix)]
    {
        // SAFETY: geteuid has no arguments and returns the current effective UID.
        if m.mode() & 0o777 != 0o700 || m.uid() != unsafe { libc::geteuid() } {
            return Err(E::UnsafePath);
        }
    }
    #[cfg(windows)]
    crate::filesystem_windows::validate_path_owner_acl(path)?;
    #[cfg(any(unix, windows))]
    {
        Ok(())
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(E::UnsafePath)
    }
}
pub(crate) fn make_directory(path: &Path) -> TransferResult<()> {
    #[cfg(windows)]
    private_directory(path.parent().ok_or(E::UnsafePath)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .mode(0o700)
            .create(path)
            .map_err(|_| E::Storage)?;
    }
    #[cfg(not(unix))]
    fs::create_dir(path).map_err(|_| E::Storage)?;
    #[cfg(windows)]
    crate::filesystem_windows::set_path_owner_acl(path, true)?;
    sync_parent(path)?;
    private_directory(path)
}
pub(crate) fn regular(path: &Path) -> TransferResult<fs::Metadata> {
    let m = fs::symlink_metadata(path).map_err(|_| E::Storage)?;
    if is_link(&m) || !m.is_file() {
        return Err(E::UnsafePath);
    }
    #[cfg(unix)]
    {
        // SAFETY: geteuid only reads the effective process identity.
        if m.nlink() != 1 || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
            return Err(E::UnsafePath);
        }
    }
    #[cfg(windows)]
    crate::filesystem_windows::validate_path_owner_acl(path)?;
    Ok(m)
}
pub(crate) fn open(path: &Path) -> TransferResult<File> {
    regular(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW);
    #[cfg(windows)]
    options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    let file = options.open(path).map_err(|_| E::Storage)?;
    if is_link(&file.metadata().map_err(|_| E::Storage)?) {
        return Err(E::UnsafePath);
    }
    #[cfg(windows)]
    crate::filesystem_windows::validate_file_owner_acl(&file)?;
    Ok(file)
}
pub(crate) fn exclusive(path: &Path) -> TransferResult<File> {
    #[cfg(windows)]
    private_directory(path.parent().ok_or(E::UnsafePath)?)?;
    let mut options = OpenOptions::new();
    options.write(true).read(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    #[cfg(windows)]
    options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    let file = options.open(path).map_err(|_| E::Storage)?;
    #[cfg(windows)]
    {
        // An elevated Windows token may assign BUILTIN\Administrators as the
        // default owner even inside an owner-only parent. Keep the new handle
        // open, set the current user's protected ACL, then validate it.
        crate::filesystem_windows::set_path_owner_acl(path, false)?;
        crate::filesystem_windows::validate_file_owner_acl(&file)?;
    }
    Ok(file)
}
pub(crate) fn read(path: &Path, max: u64) -> TransferResult<Vec<u8>> {
    let mut file = open(path)?;
    if file.metadata().map_err(|_| E::Storage)?.len() > max {
        return Err(E::Limit);
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| E::Storage)?;
    if bytes.len() as u64 > max {
        return Err(E::Limit);
    }
    Ok(bytes)
}
pub(crate) fn atomic(
    path: &Path,
    write: impl FnOnce(&mut File) -> TransferResult<()>,
) -> TransferResult<()> {
    let temporary = path.with_extension("tmp");
    let mut file = exclusive(&temporary)?;
    let result = (|| {
        write(&mut file)?;
        file.sync_all().map_err(|_| E::Storage)?;
        if path.try_exists().map_err(|_| E::Storage)? {
            regular(path)?;
        }
        fs::rename(&temporary, path).map_err(|_| E::Storage)?;
        sync_parent(path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
pub(crate) fn write(path: &Path, bytes: &[u8]) -> TransferResult<()> {
    atomic(path, |file| file.write_all(bytes).map_err(|_| E::Storage))
}
pub(crate) fn sync_parent(path: &Path) -> TransferResult<()> {
    #[cfg(unix)]
    File::open(path.parent().ok_or(E::UnsafePath)?)
        .and_then(|f| f.sync_all())
        .map_err(|_| E::Storage)?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
