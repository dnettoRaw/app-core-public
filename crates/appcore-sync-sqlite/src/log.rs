// =============================================================================
//        #######
//     ###       ###     F: log.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

use crate::{SqliteSyncError, SqliteSyncStore};
use appcore_sync::{
    InMemoryReplicationLog, ReplicationLog, ReplicationSnapshot, SyncError, SyncResult,
    REPLICATION_LOG_FORMAT_V1,
};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use sha2::{Digest, Sha256};

pub(crate) const MAX_REPLICATION_RECORD_BYTES: usize = 1024 * 1024;

/// SQLite-backed implementation of the AppCore replication-log contract.
#[derive(Debug, Clone)]
pub struct SqliteReplicationLog {
    store: SqliteSyncStore,
}

impl SqliteReplicationLog {
    pub(crate) fn new(store: SqliteSyncStore) -> Self {
        Self { store }
    }

    /// Reads one bounded page after the supplied zero-based log offset.
    pub fn events_page(&self, index: usize, max_records: usize) -> SyncResult<Vec<Vec<u8>>> {
        if max_records == 0 || max_records > self.store.config().max_read_records {
            return Err(capacity_error("read record"));
        }
        self.store
            .with_connection(|connection| {
                let length = count_records(connection)?;
                if index > length {
                    return Err(SqliteSyncError::CorruptRecord("log index"));
                }
                let start = i64::try_from(index)
                    .map_err(|_| SqliteSyncError::CapacityExceeded("log index"))?;
                let limit = i64::try_from(max_records)
                    .map_err(|_| SqliteSyncError::CapacityExceeded("read record"))?;
                let mut statement = connection
                    .prepare(
                        "SELECT payload FROM appcore_replication_log
                         WHERE log_index > ?1 ORDER BY log_index LIMIT ?2",
                    )
                    .map_err(SqliteSyncError::database)?;
                let rows = statement
                    .query_map(params![start, limit], |row| row.get::<_, Vec<u8>>(0))
                    .map_err(SqliteSyncError::database)?;
                let mut payloads = Vec::with_capacity(max_records.min(length - index));
                let mut bytes = 0usize;
                for row in rows {
                    let payload = row.map_err(SqliteSyncError::database)?;
                    bytes = bytes
                        .checked_add(payload.len())
                        .ok_or(SqliteSyncError::CapacityExceeded("read byte"))?;
                    if bytes > self.store.config().max_read_bytes {
                        return Err(SqliteSyncError::CapacityExceeded("read byte"));
                    }
                    payloads.push(payload);
                }
                Ok(payloads)
            })
            .map_err(SqliteSyncError::sync)
    }

    fn append_record(&self, payload: Vec<u8>, sequence: u64) -> SyncResult<usize> {
        validate_payload(&payload)?;
        self.store
            .with_connection(|connection| {
                let transaction = connection
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .map_err(SqliteSyncError::database)?;
                if let Some(existing) = existing_sequence(&transaction, sequence)? {
                    return if existing.1 == payload {
                        Ok(existing.0)
                    } else {
                        Err(SqliteSyncError::CorruptRecord("sequence conflict"))
                    };
                }
                let previous_hash: String = transaction
                    .query_row(
                        "SELECT record_hash FROM appcore_replication_log
                         ORDER BY log_index DESC LIMIT 1",
                        [],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(SqliteSyncError::database)?
                    .unwrap_or_default();
                let record_hash = record_hash(&previous_hash, sequence, &payload);
                transaction
                    .execute(
                        "INSERT INTO appcore_replication_log
                         (source_sequence, payload, previous_hash, record_hash)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![
                            sequence_to_i64(sequence)?,
                            payload,
                            previous_hash,
                            record_hash
                        ],
                    )
                    .map_err(SqliteSyncError::database)?;
                let index = usize::try_from(transaction.last_insert_rowid())
                    .map_err(|_| SqliteSyncError::CapacityExceeded("log index"))?;
                transaction.commit().map_err(SqliteSyncError::database)?;
                Ok(index)
            })
            .map_err(|error| match error {
                SqliteSyncError::CorruptRecord("sequence conflict") => {
                    SyncError::SequenceConflict(sequence)
                }
                other => other.sync(),
            })
    }

    fn snapshot_records(&self) -> SyncResult<Vec<(u64, Vec<u8>)>> {
        self.store
            .with_connection(|connection| {
                let mut statement = connection
                    .prepare(
                        "SELECT source_sequence, payload FROM appcore_replication_log
                         ORDER BY log_index",
                    )
                    .map_err(SqliteSyncError::database)?;
                let rows = statement
                    .query_map([], |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
                    })
                    .map_err(SqliteSyncError::database)?;
                let mut records = Vec::new();
                let mut bytes = 0usize;
                for row in rows {
                    let (sequence, payload) = row.map_err(SqliteSyncError::database)?;
                    bytes = bytes
                        .checked_add(payload.len())
                        .ok_or(SqliteSyncError::CapacityExceeded("snapshot byte"))?;
                    if bytes as u64 > self.store.config().max_database_bytes {
                        return Err(SqliteSyncError::CapacityExceeded("snapshot byte"));
                    }
                    records.push((
                        u64::try_from(sequence)
                            .map_err(|_| SqliteSyncError::CorruptRecord("sequence"))?,
                        payload,
                    ));
                }
                Ok(records)
            })
            .map_err(SqliteSyncError::sync)
    }
}

impl ReplicationLog for SqliteReplicationLog {
    fn append(&mut self, record: Vec<u8>) -> SyncResult<usize> {
        self.append_record(record, 0)
    }

    fn append_with_sequence(&mut self, record: Vec<u8>, sequence: u64) -> SyncResult<usize> {
        self.append_record(record, sequence)
    }

    fn event_at_sequence(&self, sequence: u64) -> SyncResult<Option<Vec<u8>>> {
        if sequence == 0 {
            return Ok(None);
        }
        self.store
            .with_connection(|connection| {
                connection
                    .query_row(
                        "SELECT payload FROM appcore_replication_log WHERE source_sequence = ?1",
                        [sequence_to_i64(sequence)?],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(SqliteSyncError::database)
            })
            .map_err(SqliteSyncError::sync)
    }

    fn events_since(&self, index: usize) -> SyncResult<Vec<Vec<u8>>> {
        let length = self.len()?;
        if length.saturating_sub(index) > self.store.config().max_read_records {
            return Err(capacity_error("read record"));
        }
        self.events_page(index, self.store.config().max_read_records)
    }

    fn last_index(&self) -> SyncResult<usize> {
        self.len()
    }

    fn len(&self) -> SyncResult<usize> {
        self.store
            .with_connection(|connection| count_records(connection))
            .map_err(SqliteSyncError::sync)
    }

    fn is_empty(&self) -> SyncResult<bool> {
        self.len().map(|length| length == 0)
    }

    fn create_snapshot(&self) -> SyncResult<ReplicationSnapshot> {
        let mut memory = InMemoryReplicationLog::new();
        for (sequence, payload) in self.snapshot_records()? {
            let _ = memory.append_with_sequence(payload, sequence)?;
        }
        memory.create_snapshot()
    }

    fn restore_snapshot(&mut self, snapshot: &ReplicationSnapshot) -> SyncResult<()> {
        let mut validated = InMemoryReplicationLog::new();
        validated.restore_snapshot(snapshot)?;
        let records = snapshot.records.clone();
        self.store
            .with_connection(|connection| {
                let transaction = connection
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .map_err(SqliteSyncError::database)?;
                transaction
                    .execute("DELETE FROM appcore_replication_log", [])
                    .map_err(SqliteSyncError::database)?;
                let mut previous_hash = String::new();
                for (offset, record) in records.iter().enumerate() {
                    validate_payload(&record.payload)
                        .map_err(|_| SqliteSyncError::CapacityExceeded("replication record"))?;
                    let hash = record_hash(&previous_hash, record.sequence, &record.payload);
                    transaction
                        .execute(
                            "INSERT INTO appcore_replication_log
                             (log_index, source_sequence, payload, previous_hash, record_hash)
                             VALUES (?1, ?2, ?3, ?4, ?5)",
                            params![
                                i64::try_from(offset + 1).map_err(|_| {
                                    SqliteSyncError::CapacityExceeded("log index")
                                })?,
                                sequence_to_i64(record.sequence)?,
                                &record.payload,
                                previous_hash,
                                hash
                            ],
                        )
                        .map_err(SqliteSyncError::database)?;
                    previous_hash = hash;
                }
                transaction.commit().map_err(SqliteSyncError::database)
            })
            .map_err(SqliteSyncError::sync)
    }
}

fn existing_sequence(
    transaction: &Transaction<'_>,
    sequence: u64,
) -> Result<Option<(usize, Vec<u8>)>, SqliteSyncError> {
    if sequence == 0 {
        return Ok(None);
    }
    transaction
        .query_row(
            "SELECT log_index, payload FROM appcore_replication_log WHERE source_sequence = ?1",
            [sequence_to_i64(sequence)?],
            |row| Ok((row.get::<_, i64>(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(SqliteSyncError::database)?
        .map(|(index, payload)| {
            usize::try_from(index)
                .map(|index| (index, payload))
                .map_err(|_| SqliteSyncError::CorruptRecord("log index"))
        })
        .transpose()
}

fn count_records(connection: &rusqlite::Connection) -> Result<usize, SqliteSyncError> {
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM appcore_replication_log", [], |row| {
            row.get(0)
        })
        .map_err(SqliteSyncError::database)?;
    usize::try_from(count).map_err(|_| SqliteSyncError::CorruptRecord("log count"))
}

fn validate_payload(payload: &[u8]) -> SyncResult<()> {
    if payload.len() > MAX_REPLICATION_RECORD_BYTES {
        return Err(capacity_error("replication record"));
    }
    Ok(())
}

fn sequence_to_i64(sequence: u64) -> Result<i64, SqliteSyncError> {
    i64::try_from(sequence).map_err(|_| SqliteSyncError::CapacityExceeded("sequence"))
}

fn capacity_error(resource: &'static str) -> SyncError {
    SqliteSyncError::CapacityExceeded(resource).sync()
}

pub(crate) fn record_hash(previous_hash: &str, sequence: u64, payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(REPLICATION_LOG_FORMAT_V1.as_bytes());
    hasher.update((previous_hash.len() as u64).to_be_bytes());
    hasher.update(previous_hash.as_bytes());
    hasher.update(sequence.to_be_bytes());
    hasher.update((payload.len() as u64).to_be_bytes());
    hasher.update(payload);
    let digest = hasher.finalize();
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}
