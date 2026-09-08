// =============================================================================
//        #######
//     ###       ###     F: store_io.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/03 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/03 00:00:00 by dnettoRaw
//      ###########      S: 1.0.3-rc
// =============================================================================

//! Atomic bounded writers for update artifacts and V1 store metadata.

use crate::filesystem::open_regular_file;
use crate::{UpdateError, UpdateResult};
use serde::de::DeserializeOwned;
use serde::Serialize;
#[cfg(unix)]
use std::fs::File;
use std::fs::{self, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) const MAX_UPDATE_METADATA_BYTES: usize = 1024 * 1024;
const UPDATE_METADATA_BUFFER_BYTES: usize = 16 * 1024;
// appcore-norm: allow(global-state) reason: atomic sequence prevents process-local temporary path collisions
static UPDATE_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub(crate) enum JsonReadError {
    Io(io::Error),
    Decode(serde_json::Error),
}

pub(crate) fn read_json_bounded<T: DeserializeOwned>(
    path: &Path,
    max_bytes: usize,
) -> Result<Option<T>, JsonReadError> {
    let file = match open_regular_file(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(JsonReadError::Io(error)),
    };
    let declared_length = file.metadata().map_err(JsonReadError::Io)?.len();
    decode_json_bounded(file, declared_length, max_bytes).map(Some)
}

pub(crate) fn decode_json_bounded<T: DeserializeOwned>(
    reader: impl Read,
    declared_length: u64,
    max_bytes: usize,
) -> Result<T, JsonReadError> {
    let max_bytes =
        u64::try_from(max_bytes).map_err(|_| JsonReadError::Io(invalid_read_limit()))?;
    if declared_length > max_bytes {
        return Err(JsonReadError::Io(metadata_size_error()));
    }
    let read_limit = max_bytes
        .checked_add(1)
        .ok_or_else(|| JsonReadError::Io(invalid_read_limit()))?;
    let mut reader =
        BufReader::with_capacity(UPDATE_METADATA_BUFFER_BYTES, reader.take(read_limit));
    let mut deserializer = serde_json::Deserializer::from_reader(&mut reader);
    let value = T::deserialize(&mut deserializer).map_err(JsonReadError::Decode)?;
    deserializer.end().map_err(JsonReadError::Decode)?;
    if reader.get_ref().limit() == 0 {
        return Err(JsonReadError::Io(metadata_size_error()));
    }
    Ok(value)
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> UpdateResult<()> {
    atomic_write_with(path, |file| {
        file.write_all(bytes)
            .map_err(|error| UpdateError::Store(error.to_string()))
    })
}

pub(crate) fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> UpdateResult<()> {
    let mut sizing = BoundedWriter::new(io::sink(), MAX_UPDATE_METADATA_BYTES);
    serde_json::to_writer_pretty(&mut sizing, value)
        .map_err(|error| UpdateError::Store(error.to_string()))?;
    let expected = sizing.written();
    atomic_write_with(path, |file| write_json(file, value, expected))
}

fn write_json<T: Serialize>(file: &mut fs::File, value: &T, expected: usize) -> UpdateResult<()> {
    let mut buffered = BufWriter::with_capacity(UPDATE_METADATA_BUFFER_BYTES, file);
    let actual = {
        let mut writer = BoundedWriter::new(&mut buffered, expected);
        serde_json::to_writer_pretty(&mut writer, value)
            .map_err(|error| UpdateError::Store(error.to_string()))?;
        writer
            .flush()
            .map_err(|error| UpdateError::Store(error.to_string()))?;
        writer.written()
    };
    if actual != expected {
        return Err(UpdateError::Store(
            "update metadata serialization changed between passes".to_string(),
        ));
    }
    buffered
        .flush()
        .map_err(|error| UpdateError::Store(error.to_string()))
}

fn atomic_write_with(
    path: &Path,
    write: impl FnOnce(&mut fs::File) -> UpdateResult<()>,
) -> UpdateResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| UpdateError::Store("path has no parent".to_string()))?;
    fs::create_dir_all(parent).map_err(|error| UpdateError::Store(error.to_string()))?;
    reject_directory(parent)?;
    reject_optional_regular_file(path)?;
    let temporary = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        UPDATE_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| UpdateError::Store(error.to_string()))?;
        write(&mut file)?;
        file.sync_all()
            .map_err(|error| UpdateError::Store(error.to_string()))?;
        fs::rename(&temporary, path).map_err(|error| UpdateError::Store(error.to_string()))?;
        sync_parent_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(unix)]
pub(crate) fn sync_parent_directory(path: &Path) -> UpdateResult<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| UpdateError::Store(error.to_string()))
}

#[cfg(not(unix))]
pub(crate) fn sync_parent_directory(_path: &Path) -> UpdateResult<()> {
    Ok(())
}

pub(crate) fn remove_if_exists(path: &Path) -> UpdateResult<()> {
    match fs::remove_file(path) {
        Ok(()) => path
            .parent()
            .map(sync_parent_directory)
            .transpose()
            .map(|_| ()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(UpdateError::Store(error.to_string())),
    }
}

fn reject_optional_regular_file(path: &Path) -> UpdateResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => Err(
            UpdateError::Store("update path is not a regular file".to_string()),
        ),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(UpdateError::Store(error.to_string())),
    }
}

pub(crate) fn reject_directory(path: &Path) -> UpdateResult<()> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| UpdateError::Store(error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(UpdateError::Store(
            "update root is not a regular directory".to_string(),
        ));
    }
    Ok(())
}

struct BoundedWriter<W> {
    inner: W,
    maximum: usize,
    written: usize,
}

impl<W> BoundedWriter<W> {
    const fn new(inner: W, maximum: usize) -> Self {
        Self {
            inner,
            maximum,
            written: 0,
        }
    }

    const fn written(&self) -> usize {
        self.written
    }
}

impl<W: Write> Write for BoundedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(next) = self.written.checked_add(bytes.len()) else {
            return Err(metadata_size_error());
        };
        if next > self.maximum {
            return Err(metadata_size_error());
        }
        self.inner.write_all(bytes)?;
        self.written = next;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn metadata_size_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "update metadata exceeds size limit",
    )
}

fn invalid_read_limit() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "update metadata read limit exceeds platform range",
    )
}

#[cfg(test)]
#[path = "store_io_tests.rs"]
mod tests;
