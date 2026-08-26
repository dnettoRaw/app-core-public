// =============================================================================
//        #######
//     ###       ###     F: outbox.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

use crate::{SqliteSyncError, SqliteSyncStore};
use appcore_sync::{SyncError, SyncMessage, SyncOutbox, SyncResult};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

const MAX_BATCH_ID_BYTES: usize = 1_024;

/// SQLite-backed bounded synchronization outbox.
#[derive(Debug, Clone)]
pub struct SqliteSyncOutbox {
    store: SqliteSyncStore,
}

impl SqliteSyncOutbox {
    pub(crate) fn new(store: SqliteSyncStore) -> Self {
        Self { store }
    }

    fn read_messages(&self, limit: usize) -> SyncResult<Vec<SyncMessage>> {
        self.store
            .with_connection(|connection| {
                let limit = i64::try_from(limit)
                    .map_err(|_| SqliteSyncError::CapacityExceeded("outbox"))?;
                let mut statement = connection
                    .prepare(
                        "SELECT position, encoded FROM appcore_sync_outbox
                         ORDER BY position LIMIT ?1",
                    )
                    .map_err(SqliteSyncError::database)?;
                let rows = statement
                    .query_map([limit], |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
                    })
                    .map_err(SqliteSyncError::database)?;
                let mut messages = Vec::new();
                let mut bytes = 0usize;
                for row in rows {
                    let (_position, encoded) = row.map_err(SqliteSyncError::database)?;
                    bytes = bytes
                        .checked_add(encoded.len())
                        .ok_or(SqliteSyncError::CapacityExceeded("outbox byte"))?;
                    if bytes as u64 > self.store.config().max_database_bytes {
                        return Err(SqliteSyncError::CapacityExceeded("outbox byte"));
                    }
                    let message = serde_json::from_slice(&encoded)
                        .map_err(|_| SqliteSyncError::CorruptRecord("outbox"))?;
                    messages.push(message);
                }
                Ok(messages)
            })
            .map_err(SqliteSyncError::sync)
    }
}

impl SyncOutbox for SqliteSyncOutbox {
    fn try_enqueue(&self, message: SyncMessage, max_len: usize) -> SyncResult<bool> {
        validate_batch_id(&message.batch_id)?;
        let encoded = serde_json::to_vec(&message)
            .map_err(|_| SyncError::InvalidSyncMessage("outbox serialization failed"))?;
        if encoded.len() > self.store.config().max_outbox_record_bytes {
            return Err(SqliteSyncError::CapacityExceeded("outbox record").sync());
        }
        let limit = max_len.min(self.store.config().max_outbox_entries);
        self.store
            .with_connection(|connection| {
                let transaction = connection
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .map_err(SqliteSyncError::database)?;
                let existing: Option<Vec<u8>> = transaction
                    .query_row(
                        "SELECT encoded FROM appcore_sync_outbox WHERE batch_id = ?1",
                        [&message.batch_id],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(SqliteSyncError::database)?;
                if let Some(existing) = existing {
                    return if existing == encoded {
                        Ok(false)
                    } else {
                        Err(SqliteSyncError::CorruptRecord("outbox conflict"))
                    };
                }
                let count = count_entries(&transaction)?;
                if count >= limit {
                    return Ok(false);
                }
                let inserted = transaction
                    .execute(
                        "INSERT INTO appcore_sync_outbox(batch_id, encoded)
                         VALUES (?1, ?2)",
                        params![message.batch_id, encoded],
                    )
                    .map_err(SqliteSyncError::database)?;
                transaction.commit().map_err(SqliteSyncError::database)?;
                Ok(inserted == 1)
            })
            .map_err(|error| match error {
                SqliteSyncError::CorruptRecord("outbox conflict") => {
                    SyncError::InvalidSyncMessage("outbox batch conflict")
                }
                other => other.sync(),
            })
    }

    fn front(&self) -> SyncResult<Option<SyncMessage>> {
        Ok(self.read_messages(1)?.into_iter().next())
    }

    fn acknowledge_front(&self, batch_id: &str) -> SyncResult<()> {
        validate_batch_id(batch_id)?;
        self.store
            .with_connection(|connection| {
                let transaction = connection
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .map_err(SqliteSyncError::database)?;
                let front: Option<(i64, String)> = transaction
                    .query_row(
                        "SELECT position, batch_id FROM appcore_sync_outbox
                         ORDER BY position LIMIT 1",
                        [],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()
                    .map_err(SqliteSyncError::database)?;
                let Some((position, current_id)) = front else {
                    return Err(SqliteSyncError::CorruptRecord("outbox acknowledgement"));
                };
                if current_id != batch_id {
                    return Err(SqliteSyncError::CorruptRecord("outbox acknowledgement"));
                }
                transaction
                    .execute(
                        "DELETE FROM appcore_sync_outbox WHERE position = ?1",
                        [position],
                    )
                    .map_err(SqliteSyncError::database)?;
                transaction.commit().map_err(SqliteSyncError::database)
            })
            .map_err(|error| match error {
                SqliteSyncError::CorruptRecord("outbox acknowledgement") => {
                    SyncError::InvalidSyncMessage("outbox acknowledgement mismatch")
                }
                other => other.sync(),
            })
    }

    fn messages(&self) -> SyncResult<Vec<SyncMessage>> {
        let length = self.len()?;
        if length > self.store.config().max_outbox_entries {
            return Err(SqliteSyncError::CapacityExceeded("outbox").sync());
        }
        self.read_messages(length)
    }

    fn len(&self) -> SyncResult<usize> {
        self.store
            .with_connection(|connection| count_entries(connection))
            .map_err(SqliteSyncError::sync)
    }
}

fn count_entries(connection: &rusqlite::Connection) -> Result<usize, SqliteSyncError> {
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM appcore_sync_outbox", [], |row| {
            row.get(0)
        })
        .map_err(SqliteSyncError::database)?;
    usize::try_from(count).map_err(|_| SqliteSyncError::CorruptRecord("outbox count"))
}

pub(crate) fn validate_batch_id(batch_id: &str) -> SyncResult<()> {
    if batch_id.is_empty()
        || batch_id.len() > MAX_BATCH_ID_BYTES
        || batch_id.chars().any(char::is_control)
    {
        return Err(SyncError::InvalidSyncMessage("invalid outbox batch id"));
    }
    Ok(())
}
