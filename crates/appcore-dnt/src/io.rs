// =============================================================================
//        #######
//     ###       ###     F: io.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 00:04:12 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:07:11 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! DNT filesystem helpers.

use crate::{
    open_owned, seal, verify, DntCodec, DntError, DntKeyProvider, DntOpenOptions, DntResult,
    DntSealOptions, OpenedDnt,
};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

// appcore-norm: allow(global-state) reason: atomic sequence prevents process-local temporary path collisions
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

impl DntOpenOptions {
    /// Returns the largest complete file buffer allowed by these options.
    ///
    /// File-based readers require an explicit payload bound so they can reject
    /// an oversized envelope before allocating its complete contents.
    pub fn max_envelope_bytes(&self) -> DntResult<u64> {
        self.max_payload_bytes
            .ok_or(DntError::PayloadTooLarge)?
            .checked_add(crate::DNT_MAX_HEADER_BYTES as u64)
            .and_then(|length| length.checked_add(crate::DNT_MAX_ENCRYPTED_METADATA_BYTES as u64))
            .and_then(|length| length.checked_add(20))
            .ok_or(DntError::PayloadTooLarge)
    }
}

/// Seals, verifies and atomically replaces one DNT file.
pub fn write_atomic<P, C>(
    path: impl AsRef<Path>,
    payload: &[u8],
    key_provider: &P,
    codec: &C,
    seal_options: DntSealOptions,
    open_options: &DntOpenOptions,
) -> DntResult<()>
where
    P: DntKeyProvider,
    C: DntCodec,
{
    let envelope = seal(payload, key_provider, codec, seal_options)?;
    verify(&envelope, key_provider, codec, open_options)?;
    atomic_replace(path.as_ref(), &envelope)
}

/// Reads, authenticates and opens one DNT file.
pub fn read_verified<P, C>(
    path: impl AsRef<Path>,
    key_provider: &P,
    codec: &C,
    options: &DntOpenOptions,
) -> DntResult<OpenedDnt>
where
    P: DntKeyProvider,
    C: DntCodec,
{
    let bytes = read_bounded(path.as_ref(), options.max_envelope_bytes()?)?;
    open_owned(bytes, key_provider, codec, options)
}

fn read_bounded(path: &Path, max_bytes: u64) -> DntResult<Vec<u8>> {
    reject_symlink(path)?;
    let file = File::open(path).map_err(|_| DntError::Io)?;
    let metadata = file.metadata().map_err(|_| DntError::Io)?;
    if !metadata.is_file() {
        return Err(DntError::Io);
    }
    if metadata.len() > max_bytes {
        return Err(DntError::PayloadTooLarge);
    }
    let capacity = usize::try_from(metadata.len()).map_err(|_| DntError::PayloadTooLarge)?;
    let read_limit = max_bytes.checked_add(1).ok_or(DntError::PayloadTooLarge)?;
    let mut bytes = Vec::with_capacity(capacity);
    file.take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|_| DntError::Io)?;
    if bytes.len() as u64 > max_bytes {
        return Err(DntError::PayloadTooLarge);
    }
    Ok(bytes)
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> DntResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|_| DntError::Io)?;
    reject_symlink(path)?;
    let temporary = temporary_path(path, parent);
    let result = write_and_rename(&temporary, path, parent, bytes);
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn write_and_rename(
    temporary: &Path,
    final_path: &Path,
    parent: &Path,
    bytes: &[u8],
) -> DntResult<()> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(temporary)
        .map_err(|_| DntError::Io)?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| DntError::Io)?;
    fs::rename(temporary, final_path).map_err(|_| DntError::Io)?;
    sync_parent(parent)
}

fn temporary_path(path: &Path, parent: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("appcore-dnt");
    parent.join(format!(
        ".{name}.{}-{}.tmp",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

fn reject_symlink(path: &Path) -> DntResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(DntError::Io),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(DntError::Io),
    }
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> DntResult<()> {
    fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| DntError::Io)
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> DntResult<()> {
    Ok(())
}
