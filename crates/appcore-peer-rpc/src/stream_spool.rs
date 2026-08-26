// =============================================================================
//        #######
//     ###       ###     F: stream_spool.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Exclusive automatically removed spool used by V2 stream sessions.

use crate::stream_spool_security::{validate_private_directory, validate_private_file};
use crate::v2::PeerRpcStreamErrorV2;
use std::fmt::{Debug, Formatter};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// File-backed bounded stream payload removed automatically on drop.
pub struct PeerRpcStreamPayload {
    file: Option<File>,
    path: PathBuf,
    bytes: u64,
}

impl PeerRpcStreamPayload {
    pub(crate) fn create(directory: &Path) -> Result<Self, PeerRpcStreamErrorV2> {
        validate_private_directory(directory)?;
        let directory = directory
            .canonicalize()
            .map_err(|_| PeerRpcStreamErrorV2::Io)?;
        for _ in 0..32 {
            let path = directory.join(next_spool_name());
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            match options.open(&path) {
                Ok(file) => {
                    if let Err(error) = validate_private_file(&file) {
                        drop(file);
                        let _ = fs::remove_file(&path);
                        return Err(error);
                    }
                    return Ok(Self {
                        file: Some(file),
                        path,
                        bytes: 0,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return Err(PeerRpcStreamErrorV2::Io),
            }
        }
        Err(PeerRpcStreamErrorV2::Io)
    }

    /// Returns decoded payload bytes written to this spool.
    pub fn len(&self) -> u64 {
        self.bytes
    }

    /// Reports whether the payload is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes == 0
    }

    /// Rewinds the payload for bounded sequential reading.
    pub fn rewind(&mut self) -> Result<(), PeerRpcStreamErrorV2> {
        self.file_mut()?
            .seek(SeekFrom::Start(0))
            .map_err(|_| PeerRpcStreamErrorV2::Io)?;
        Ok(())
    }

    fn file_mut(&mut self) -> Result<&mut File, PeerRpcStreamErrorV2> {
        self.file.as_mut().ok_or(PeerRpcStreamErrorV2::Closed)
    }
}

impl Debug for PeerRpcStreamPayload {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PeerRpcStreamPayload")
            .field("bytes", &self.bytes)
            .finish()
    }
}

impl Read for PeerRpcStreamPayload {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.file
            .as_mut()
            .ok_or_else(|| std::io::Error::other("stream payload is closed"))?
            .read(buffer)
    }
}

impl Write for PeerRpcStreamPayload {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let written = self
            .file
            .as_mut()
            .ok_or_else(|| std::io::Error::other("stream payload is closed"))?
            .write(buffer)?;
        self.bytes = self.bytes.saturating_add(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file
            .as_mut()
            .ok_or_else(|| std::io::Error::other("stream payload is closed"))?
            .flush()
    }
}

impl Seek for PeerRpcStreamPayload {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.file
            .as_mut()
            .ok_or_else(|| std::io::Error::other("stream payload is closed"))?
            .seek(position)
    }
}

impl Drop for PeerRpcStreamPayload {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = fs::remove_file(&self.path);
    }
}

fn next_spool_name() -> String {
    // appcore-norm: allow(global-state) reason: atomic sequence prevents process-local temporary path collisions
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("peer-v2-{}-{sequence}.part", std::process::id())
}
