// =============================================================================
//        #######
//     ###       ###     F: storage_file_fs.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/20 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/20 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Filesystem path validation and no-follow file operations.

use super::{StorageError, StorageResult};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) fn resolve_under_root(root: &Path, relative: &str) -> StorageResult<PathBuf> {
    let rel = Path::new(relative);
    if rel.as_os_str().is_empty() || rel.is_absolute() {
        return Err(StorageError::InvalidPath(relative.to_string()));
    }
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata_is_link(&metadata) || !metadata.is_dir() => {
            return Err(StorageError::InvalidPath(root.display().to_string()));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(_) => return Err(StorageError::NotAvailable),
    }
    let mut current = root.to_path_buf();
    for component in rel.components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return Err(StorageError::InvalidPath(relative.to_string()));
        }
        if let Component::Normal(part) = component {
            current.push(part);
            match fs::symlink_metadata(&current) {
                Ok(metadata) if metadata_is_link(&metadata) => {
                    return Err(StorageError::InvalidPath(relative.to_string()));
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(_) => return Err(StorageError::NotAvailable),
            }
        }
    }
    Ok(current)
}

pub(super) fn ensure_real_directory(path: &Path) -> StorageResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| StorageError::NotAvailable)?;
    if metadata_is_link(&metadata) || !metadata.is_dir() {
        return Err(StorageError::InvalidPath(path.display().to_string()));
    }
    Ok(())
}

pub(super) fn create_real_directory_all(path: &Path) -> StorageResult<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => return ensure_real_directory(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(_) => return Err(StorageError::NotAvailable),
    }
    fs::create_dir_all(path).map_err(|_| StorageError::NotAvailable)?;
    ensure_real_directory(path)
}

pub(super) fn path_exists_no_follow(path: &Path) -> StorageResult<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata_is_link(&metadata) => {
            Err(StorageError::InvalidPath(path.display().to_string()))
        }
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(StorageError::NotAvailable),
    }
}

pub(super) fn path_is_real_directory(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.is_dir() && !metadata_is_link(&metadata))
        .unwrap_or(false)
}

pub(super) fn open_regular_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    let file = open_no_follow(&mut options, path)?;
    validate_open_file(&file, path, false)?;
    Ok(file)
}

pub(super) fn create_new_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    let file = open_no_follow(&mut options, path)?;
    validate_open_file(&file, path, false)?;
    Ok(file)
}

pub(super) fn open_lock_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    let file = open_no_follow(&mut options, path)?;
    validate_open_file(&file, path, false)?;
    Ok(file)
}

#[cfg(unix)]
fn open_directory(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    let file = open_no_follow(&mut options, path)?;
    validate_open_file(&file, path, true)?;
    Ok(file)
}

fn validate_open_file(file: &File, path: &Path, directory: bool) -> io::Result<()> {
    let path_metadata = fs::symlink_metadata(path)?;
    let file_metadata = file.metadata()?;
    if metadata_is_link(&path_metadata)
        || metadata_is_link(&file_metadata)
        || (directory && !file_metadata.is_dir())
        || (!directory && !file_metadata.is_file())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "filesystem object type is not allowed",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn open_no_follow(options: &mut OpenOptions, path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    options.custom_flags(libc::O_NOFOLLOW).open(path)
}

#[cfg(windows)]
fn open_no_follow(options: &mut OpenOptions, path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
    options
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(all(not(unix), not(windows)))]
fn open_no_follow(_options: &mut OpenOptions, _path: &Path) -> io::Result<File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "no-follow file opening is unavailable on this platform",
    ))
}

pub(super) fn metadata_is_link(metadata: &Metadata) -> bool {
    metadata.file_type().is_symlink() || metadata_is_reparse_point(metadata)
}

#[cfg(windows)]
fn metadata_is_reparse_point(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn metadata_is_reparse_point(_metadata: &Metadata) -> bool {
    false
}

// appcore-norm: allow(global-state) reason: atomic sequence prevents process-local temporary path collisions
static TMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn tmp_path_for(path: &Path) -> PathBuf {
    let pid = std::process::id();
    let thread_id = format!("{:?}", std::thread::current().id());
    let clean_thread_id: String = thread_id
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect();
    let counter = TMP_COUNTER.fetch_add(1, Ordering::SeqCst);

    let mut tmp_name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "tmp".into());
    tmp_name.push(format!(".{pid}_{clean_thread_id}_{counter}.tmp"));
    path.with_file_name(tmp_name)
}

#[cfg(unix)]
pub(super) fn sync_directory(path: &Path) -> io::Result<()> {
    open_directory(path)?.sync_all()
}

#[cfg(not(unix))]
pub(super) fn sync_directory(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata_is_link(&metadata) || !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "filesystem object type is not allowed",
        ));
    }
    Ok(())
}

pub(super) fn fsync_parent(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        sync_directory(parent)?;
    }
    Ok(())
}

pub(super) fn write_atomic_file(tmp: &Path, final_path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic_file_inner(tmp, final_path, bytes, None)
}

pub(super) fn write_atomic_file_using(
    tmp: &Path,
    final_path: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    let result = (|| {
        let mut file = create_new_file(tmp)?;
        write(&mut file)?;
        file.sync_all()?;
        drop(file);
        reject_link_if_present(final_path)?;
        fs::rename(tmp, final_path)?;
        fsync_parent(final_path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}

pub(super) fn copy_atomic_file(
    source: &mut File,
    tmp: &Path,
    final_path: &Path,
    max_bytes: u64,
) -> io::Result<u64> {
    let result = copy_atomic_file_inner(source, tmp, final_path, max_bytes, None);
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}

pub(super) fn copy_file_bounded(
    source: &mut File,
    destination: &mut File,
    max_bytes: u64,
) -> io::Result<u64> {
    copy_file_bounded_inner(source, destination, max_bytes, None)
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy)]
pub(crate) enum AtomicWriteFault {
    DiskFull,
    PermissionDenied,
    AfterPartialWrite,
    AfterFileSync,
    BeforeRename,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy)]
pub(crate) enum AtomicCopyFault {
    DiskFull,
    AfterPartialWrite,
    AfterFileSync,
    SourceGrowth,
}

#[cfg(test)]
pub(crate) fn write_atomic_file_with_fault(
    tmp: &Path,
    final_path: &Path,
    bytes: &[u8],
    fault: AtomicWriteFault,
) -> io::Result<()> {
    write_atomic_file_inner(tmp, final_path, bytes, Some(fault))
}

#[cfg(test)]
pub(crate) fn copy_atomic_file_with_fault(
    source: &mut File,
    tmp: &Path,
    final_path: &Path,
    max_bytes: u64,
    fault: AtomicCopyFault,
) -> io::Result<u64> {
    let result = copy_atomic_file_inner(source, tmp, final_path, max_bytes, Some(fault));
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}

fn copy_atomic_file_inner(
    source: &mut File,
    tmp: &Path,
    final_path: &Path,
    max_bytes: u64,
    #[cfg_attr(not(test), allow(unused_variables))] fault: Option<AtomicCopyFault>,
) -> io::Result<u64> {
    #[cfg(test)]
    if matches!(fault, Some(AtomicCopyFault::DiskFull)) {
        return Err(io::Error::from_raw_os_error(28));
    }
    let mut destination = create_new_file(tmp)?;
    #[cfg(test)]
    if matches!(fault, Some(AtomicCopyFault::AfterPartialWrite)) {
        let expected = source.metadata()?.len();
        io::copy(&mut source.take(expected.div_ceil(2)), &mut destination)?;
        destination.sync_all()?;
        return Err(io::Error::new(
            io::ErrorKind::WriteZero,
            "injected partial copy",
        ));
    }
    let copied = copy_file_bounded_inner(source, &mut destination, max_bytes, fault)?;
    destination.sync_all()?;
    #[cfg(test)]
    if matches!(fault, Some(AtomicCopyFault::AfterFileSync)) {
        return Err(io::Error::other("injected post-sync copy failure"));
    }
    drop(destination);
    reject_link_if_present(final_path)?;
    fs::rename(tmp, final_path)?;
    fsync_parent(final_path)?;
    Ok(copied)
}

fn copy_file_bounded_inner(
    source: &mut File,
    destination: &mut File,
    max_bytes: u64,
    #[cfg_attr(not(test), allow(unused_variables))] fault: Option<AtomicCopyFault>,
) -> io::Result<u64> {
    let before = source.metadata()?;
    if before.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::FileTooLarge,
            "source exceeds the configured copy limit",
        ));
    }
    let modified_before = before.modified().ok();
    #[cfg(test)]
    if matches!(fault, Some(AtomicCopyFault::SourceGrowth)) {
        source.set_len(before.len().saturating_add(1))?;
    }
    let read_limit = max_bytes
        .checked_add(1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "copy limit is too large"))?;
    let copied = io::copy(&mut source.take(read_limit), destination)?;
    let after = source.metadata()?;
    let modified_after = after.modified().ok();
    let modified = matches!(
        (modified_before, modified_after),
        (Some(before), Some(after)) if before != after
    );
    if copied > max_bytes || copied != before.len() || after.len() != before.len() || modified {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "source changed while it was copied",
        ));
    }
    Ok(copied)
}

fn write_atomic_file_inner(
    tmp: &Path,
    final_path: &Path,
    bytes: &[u8],
    #[cfg_attr(not(test), allow(unused_variables))] fault: Option<AtomicWriteFault>,
) -> io::Result<()> {
    #[cfg(test)]
    match fault {
        Some(AtomicWriteFault::DiskFull) => return Err(io::Error::from_raw_os_error(28)),
        Some(AtomicWriteFault::PermissionDenied) => {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "injected permission failure",
            ));
        }
        _ => {}
    }
    let mut file = create_new_file(tmp)?;
    #[cfg(test)]
    if matches!(fault, Some(AtomicWriteFault::AfterPartialWrite)) {
        file.write_all(&bytes[..bytes.len() / 2])?;
        file.sync_all()?;
        return Err(io::Error::new(
            io::ErrorKind::WriteZero,
            "injected partial write",
        ));
    }
    file.write_all(bytes)?;
    file.sync_all()?;
    #[cfg(test)]
    if matches!(
        fault,
        Some(AtomicWriteFault::AfterFileSync | AtomicWriteFault::BeforeRename)
    ) {
        return Err(io::Error::other("injected pre-rename failure"));
    }
    drop(file);
    reject_link_if_present(final_path)?;
    fs::rename(tmp, final_path)?;
    fsync_parent(final_path)
}

fn reject_link_if_present(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata_is_link(&metadata) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to replace a filesystem link",
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
