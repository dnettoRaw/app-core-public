// =============================================================================
//        #######
//     ###       ###     F: outbox_blob.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/03 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/03 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-alpha.4
// =============================================================================

//! Streams canonical outbox JSON into and out of `SQLite` BLOBs.

use crate::{SqliteSyncError, SqliteSyncResult};
use appcore_sync::{encoded_sync_message_bytes, write_sync_message_json, SyncMessage, SyncResult};
use rusqlite::blob::{Blob, ZeroBlob};
use rusqlite::{params, Connection, MAIN_DB};
use std::io::{self, BufReader, BufWriter, Read, Write};

const MAX_BLOB_BUFFER_BYTES: usize = 1024 * 1024;
const COMPARE_BUFFER_BYTES: usize = 8 * 1024;
const MAX_READ_BUFFER_BYTES: usize = 64 * 1024;

pub(crate) fn encoded_message_bytes(message: &SyncMessage, max_bytes: usize) -> SyncResult<usize> {
    let encoded_bytes = encoded_sync_message_bytes(message)?;
    if encoded_bytes > max_bytes {
        return Err(SqliteSyncError::CapacityExceeded("outbox record").sync());
    }
    Ok(encoded_bytes)
}

pub(crate) fn insert_message_blob(
    connection: &Connection,
    message: &SyncMessage,
    encoded_bytes: usize,
) -> SqliteSyncResult<()> {
    let blob_bytes = i32::try_from(encoded_bytes)
        .map_err(|_| SqliteSyncError::CapacityExceeded("outbox record"))?;
    let inserted = connection
        .execute(
            "INSERT INTO appcore_sync_outbox(batch_id, encoded) VALUES (?1, ?2)",
            params![&message.batch_id, ZeroBlob(blob_bytes)],
        )
        .map_err(SqliteSyncError::database)?;
    if inserted != 1 {
        return Err(SqliteSyncError::DatabaseOperation);
    }
    write_message_blob(
        connection,
        connection.last_insert_rowid(),
        message,
        encoded_bytes,
    )
}

pub(crate) fn message_blob_matches(
    connection: &Connection,
    row_id: i64,
    message: &SyncMessage,
    encoded_bytes: usize,
) -> SqliteSyncResult<bool> {
    let blob = open_blob(connection, row_id, true)?;
    if blob.len() != encoded_bytes {
        return Err(SqliteSyncError::CorruptRecord("outbox byte"));
    }
    let mut comparator = BlobComparator::new(blob, encoded_bytes);
    write_sync_message_json(&mut comparator, message)
        .map_err(|_| SqliteSyncError::DatabaseOperation)?;
    comparator.finish()
}

pub(crate) fn read_message_blob(
    connection: &Connection,
    row_id: i64,
    encoded_bytes: usize,
) -> SqliteSyncResult<SyncMessage> {
    let blob = open_blob(connection, row_id, true)?;
    if blob.len() != encoded_bytes {
        return Err(SqliteSyncError::CorruptRecord("outbox byte"));
    }
    let mut reader = BufReader::with_capacity(
        bounded_buffer_capacity(encoded_bytes, MAX_READ_BUFFER_BYTES),
        blob,
    );
    let message = serde_json::from_reader(&mut reader)
        .map_err(|_| SqliteSyncError::CorruptRecord("outbox"))?;
    reader
        .into_inner()
        .close()
        .map_err(SqliteSyncError::database)?;
    Ok(message)
}

fn write_message_blob(
    connection: &Connection,
    row_id: i64,
    message: &SyncMessage,
    encoded_bytes: usize,
) -> SqliteSyncResult<()> {
    let blob = open_blob(connection, row_id, false)?;
    if blob.len() != encoded_bytes {
        return Err(SqliteSyncError::CorruptRecord("outbox byte"));
    }
    let mut writer = BufWriter::with_capacity(
        bounded_buffer_capacity(encoded_bytes, MAX_BLOB_BUFFER_BYTES),
        blob,
    );
    write_sync_message_json(&mut writer, message)
        .map_err(|_| SqliteSyncError::DatabaseOperation)?;
    writer
        .flush()
        .map_err(|_| SqliteSyncError::DatabaseOperation)?;
    let blob = writer
        .into_inner()
        .map_err(|_| SqliteSyncError::DatabaseOperation)?;
    blob.close().map_err(SqliteSyncError::database)
}

fn open_blob(connection: &Connection, row_id: i64, read_only: bool) -> SqliteSyncResult<Blob<'_>> {
    connection
        .blob_open(MAIN_DB, "appcore_sync_outbox", "encoded", row_id, read_only)
        .map_err(SqliteSyncError::database)
}

struct BlobComparator<'a> {
    reader: BufReader<Blob<'a>>,
    expected_bytes: usize,
    written: usize,
    matches: bool,
}

impl<'a> BlobComparator<'a> {
    fn new(blob: Blob<'a>, expected_bytes: usize) -> Self {
        Self {
            reader: BufReader::with_capacity(
                bounded_buffer_capacity(expected_bytes, MAX_BLOB_BUFFER_BYTES),
                blob,
            ),
            expected_bytes,
            written: 0,
            matches: true,
        }
    }

    fn finish(self) -> SqliteSyncResult<bool> {
        if self.written != self.expected_bytes {
            return Err(SqliteSyncError::CorruptRecord("outbox byte"));
        }
        let matches = self.matches;
        self.reader
            .into_inner()
            .close()
            .map_err(SqliteSyncError::database)?;
        Ok(matches)
    }
}

fn bounded_buffer_capacity(encoded_bytes: usize, maximum: usize) -> usize {
    encoded_bytes.min(maximum).max(1)
}

impl Write for BlobComparator<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.written = self
            .written
            .checked_add(bytes.len())
            .filter(|written| *written <= self.expected_bytes)
            .ok_or_else(|| io::Error::other("outbox comparison overflow"))?;
        if !self.matches {
            return Ok(bytes.len());
        }
        let mut compared = 0usize;
        let mut buffer = [0u8; COMPARE_BUFFER_BYTES];
        while compared < bytes.len() {
            let count = (bytes.len() - compared).min(buffer.len());
            self.reader.read_exact(&mut buffer[..count])?;
            if buffer[..count] != bytes[compared..compared + count] {
                self.matches = false;
                break;
            }
            compared += count;
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{bounded_buffer_capacity, MAX_BLOB_BUFFER_BYTES, MAX_READ_BUFFER_BYTES};

    #[test]
    fn blob_buffers_match_small_records_and_cap_large_records() {
        assert_eq!(bounded_buffer_capacity(0, MAX_READ_BUFFER_BYTES), 1);
        assert_eq!(bounded_buffer_capacity(257, MAX_READ_BUFFER_BYTES), 257);
        assert_eq!(
            bounded_buffer_capacity(MAX_READ_BUFFER_BYTES + 1, MAX_READ_BUFFER_BYTES),
            MAX_READ_BUFFER_BYTES
        );
        assert_eq!(
            bounded_buffer_capacity(MAX_BLOB_BUFFER_BYTES + 1, MAX_BLOB_BUFFER_BYTES),
            MAX_BLOB_BUFFER_BYTES
        );
    }
}
