// =============================================================================
//        #######
//     ###       ###     F: outbox_stream.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0
// =============================================================================

//! Two-pass JSON framing without an encoded payload allocation.

use crate::sync::error::{SyncError, SyncResult};
use crate::sync::outbox_format::{
    io_error, outbox_full, validate_record_data_len, FRAME_BODY_FIXED_BYTES, HASH_BYTES,
};
use crate::sync::persistence::reject_symlink;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::Path;

const SERIALIZATION_BUFFER_BYTES: usize = 64 * 1024;

pub(super) struct SerializedMeasurement {
    pub(super) bytes: usize,
    pub(super) digest: [u8; HASH_BYTES],
}

pub(super) fn measure_json(value: &impl Serialize) -> SyncResult<SerializedMeasurement> {
    let mut writer = DigestWriter::new(io::sink());
    {
        let mut buffered = BufWriter::with_capacity(SERIALIZATION_BUFFER_BYTES, &mut writer);
        serde_json::to_writer(&mut buffered, value).map_err(serialization_error)?;
        buffered.flush().map_err(io_error)?;
    }
    validate_record_data_len(writer.bytes)?;
    Ok(SerializedMeasurement {
        bytes: writer.bytes,
        digest: writer.hasher.finalize().into(),
    })
}

pub(super) fn append_json_record(
    path: &Path,
    ordinal: u64,
    kind: u8,
    value: &impl Serialize,
    measurement: &SerializedMeasurement,
    previous: [u8; HASH_BYTES],
) -> SyncResult<[u8; HASH_BYTES]> {
    reject_symlink(path)?;
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(io_error)?;
    let hash = write_json_frame(&mut file, ordinal, kind, value, measurement.bytes, previous)?;
    file.sync_data().map_err(io_error)?;
    Ok(hash)
}

pub(super) fn write_json_frame(
    file: &mut File,
    ordinal: u64,
    kind: u8,
    value: &impl Serialize,
    data_len: usize,
    previous: [u8; HASH_BYTES],
) -> SyncResult<[u8; HASH_BYTES]> {
    validate_record_data_len(data_len)?;
    let frame_len = FRAME_BODY_FIXED_BYTES
        .checked_add(data_len)
        .and_then(|length| u32::try_from(length).ok())
        .ok_or_else(outbox_full)?;
    let encoded_data_len = u32::try_from(data_len).map_err(|_| outbox_full())?;
    file.write_all(&frame_len.to_be_bytes())
        .and_then(|_| file.write_all(&ordinal.to_be_bytes()))
        .and_then(|_| file.write_all(&[kind]))
        .and_then(|_| file.write_all(&encoded_data_len.to_be_bytes()))
        .map_err(io_error)?;

    let mut hasher = Sha256::new();
    hasher.update(ordinal.to_be_bytes());
    hasher.update([kind]);
    hasher.update((data_len as u64).to_be_bytes());
    let written = {
        let mut writer = DigestWriter::with_hasher(&mut *file, hasher);
        {
            let mut buffered = BufWriter::with_capacity(SERIALIZATION_BUFFER_BYTES, &mut writer);
            serde_json::to_writer(&mut buffered, value).map_err(serialization_error)?;
            buffered.flush().map_err(io_error)?;
        }
        hasher = writer.hasher;
        writer.bytes
    };
    if written != data_len {
        return Err(SyncError::ReplicationFailed(
            "outbox serialization length changed".to_string(),
        ));
    }
    hasher.update(previous);
    let hash: [u8; HASH_BYTES] = hasher.finalize().into();
    file.write_all(&previous)
        .and_then(|_| file.write_all(&hash))
        .and_then(|_| file.write_all(&frame_len.to_be_bytes()))
        .map_err(io_error)?;
    Ok(hash)
}

struct DigestWriter<W> {
    inner: W,
    hasher: Sha256,
    bytes: usize,
}

impl<W> DigestWriter<W> {
    fn new(inner: W) -> Self {
        Self::with_hasher(inner, Sha256::new())
    }

    fn with_hasher(inner: W, hasher: Sha256) -> Self {
        Self {
            inner,
            hasher,
            bytes: 0,
        }
    }
}

impl<W: Write> Write for DigestWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(bytes)?;
        self.bytes = self
            .bytes
            .checked_add(written)
            .ok_or_else(|| io::Error::other("outbox serialization length overflow"))?;
        self.hasher.update(&bytes[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn serialization_error(error: serde_json::Error) -> SyncError {
    SyncError::ReplicationFailed(error.to_string())
}
