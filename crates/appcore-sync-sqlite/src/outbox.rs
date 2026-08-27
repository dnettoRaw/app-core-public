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
use appcore_sync::{
    SyncError, SyncMessage, SyncOutbox, SyncOutboxReceipt, SyncOutboxStats, SyncResult,
    MAX_OUTBOX_PAGE_BYTES, MAX_OUTBOX_PAGE_MESSAGES,
};
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

    fn read_page(
        &self,
        limit: usize,
        max_bytes: usize,
        ready_at_ms: Option<u64>,
    ) -> SyncResult<Vec<SyncMessage>> {
        validate_page_limits(limit, max_bytes)?;
        self.store
            .with_connection(|connection| {
                let limit = limit.min(self.store.config().max_outbox_entries);
                let sql_limit = i64::try_from(limit)
                    .map_err(|_| SqliteSyncError::CapacityExceeded("outbox page"))?;
                let mut statement = connection
                    .prepare(
                        "SELECT position, length(encoded), next_ready_at_ms
                         FROM appcore_sync_outbox ORDER BY position LIMIT ?1",
                    )
                    .map_err(SqliteSyncError::database)?;
                let rows = statement
                    .query_map([sql_limit], |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    })
                    .map_err(SqliteSyncError::database)?;
                let mut selected = 0usize;
                let mut selected_bytes = 0usize;
                let mut last_position = None;
                for row in rows {
                    let (position, encoded_bytes, next_ready) =
                        row.map_err(SqliteSyncError::database)?;
                    let encoded_bytes = usize::try_from(encoded_bytes)
                        .map_err(|_| SqliteSyncError::CorruptRecord("outbox byte"))?;
                    if ready_at_ms.is_some_and(|now| {
                        u64::try_from(next_ready).map_or(true, |ready| ready > now)
                    }) || selected_bytes
                        .checked_add(encoded_bytes)
                        .is_none_or(|bytes| bytes > max_bytes)
                    {
                        break;
                    }
                    selected_bytes += encoded_bytes;
                    selected += 1;
                    last_position = Some(position);
                }
                drop(statement);
                let Some(last_position) = last_position else {
                    return Ok(Vec::new());
                };
                read_selected_messages(connection, last_position, selected, selected_bytes)
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
        let receipt = SyncOutboxReceipt::new(vec![batch_id.to_string()])?;
        self.acknowledge_receipt(&receipt).map(|_| ())
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

    fn peek(&self, limit: usize, max_bytes: usize) -> SyncResult<Vec<SyncMessage>> {
        self.read_page(limit, max_bytes, None)
    }

    fn stats(&self) -> SyncResult<SyncOutboxStats> {
        self.store
            .with_connection(|connection| {
                let values: (i64, i64, i64, i64, Option<i64>) = connection
                    .query_row(
                        "SELECT COUNT(*), COALESCE(SUM(length(encoded)), 0),
                                COALESCE(SUM(CASE WHEN attempts > 0 THEN 1 ELSE 0 END), 0),
                                COALESCE(SUM(attempts), 0),
                                (SELECT next_ready_at_ms FROM appcore_sync_outbox
                                 ORDER BY position LIMIT 1)
                         FROM appcore_sync_outbox",
                        [],
                        |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                            ))
                        },
                    )
                    .map_err(SqliteSyncError::database)?;
                Ok(SyncOutboxStats {
                    pending_messages: checked_usize(values.0, "outbox count")?,
                    pending_bytes: Some(checked_usize(values.1, "outbox byte")?),
                    attempted_messages: Some(checked_usize(values.2, "outbox attempt")?),
                    total_attempts: Some(
                        u64::try_from(values.3)
                            .map_err(|_| SqliteSyncError::CorruptRecord("outbox attempt"))?,
                    ),
                    next_ready_at_ms: values
                        .4
                        .map(|value| {
                            u64::try_from(value)
                                .map_err(|_| SqliteSyncError::CorruptRecord("outbox readiness"))
                        })
                        .transpose()?,
                })
            })
            .map_err(SqliteSyncError::sync)
    }

    fn mark_attempt(&self, batch_id: &str, next_ready_at_ms: u64) -> SyncResult<u32> {
        validate_batch_id(batch_id)?;
        let next_ready_at_ms = i64::try_from(next_ready_at_ms)
            .map_err(|_| SyncError::InvalidSyncMessage("outbox readiness overflow"))?;
        self.store
            .with_connection(|connection| {
                let transaction = connection
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .map_err(SqliteSyncError::database)?;
                let front: Option<(i64, String, i64)> = transaction
                    .query_row(
                        "SELECT position, batch_id, attempts FROM appcore_sync_outbox
                         ORDER BY position LIMIT 1",
                        [],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()
                    .map_err(SqliteSyncError::database)?;
                let Some((position, current_id, attempts)) = front else {
                    return Err(SqliteSyncError::CorruptRecord("outbox attempt"));
                };
                if current_id != batch_id {
                    return Err(SqliteSyncError::CorruptRecord("outbox attempt"));
                }
                let attempts = attempts
                    .checked_add(1)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or(SqliteSyncError::CapacityExceeded("outbox attempt"))?;
                transaction
                    .execute(
                        "UPDATE appcore_sync_outbox
                         SET attempts = ?1, next_ready_at_ms = ?2 WHERE position = ?3",
                        params![attempts, next_ready_at_ms, position],
                    )
                    .map_err(SqliteSyncError::database)?;
                transaction.commit().map_err(SqliteSyncError::database)?;
                Ok(attempts)
            })
            .map_err(|error| match error {
                SqliteSyncError::CorruptRecord("outbox attempt") => {
                    SyncError::InvalidSyncMessage("outbox attempt mismatch")
                }
                other => other.sync(),
            })
    }

    fn next_ready(
        &self,
        now_ms: u64,
        limit: usize,
        max_bytes: usize,
    ) -> SyncResult<Vec<SyncMessage>> {
        self.read_page(limit, max_bytes, Some(now_ms))
    }

    fn acknowledge_receipt(&self, receipt: &SyncOutboxReceipt) -> SyncResult<usize> {
        self.store
            .with_connection(|connection| {
                let transaction = connection
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .map_err(SqliteSyncError::database)?;
                let limit = i64::try_from(receipt.batch_ids().len())
                    .map_err(|_| SqliteSyncError::CapacityExceeded("outbox receipt"))?;
                let mut statement = transaction
                    .prepare(
                        "SELECT position, batch_id FROM appcore_sync_outbox
                         ORDER BY position LIMIT ?1",
                    )
                    .map_err(SqliteSyncError::database)?;
                let rows = statement
                    .query_map([limit], |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                    })
                    .map_err(SqliteSyncError::database)?;
                let selected = rows
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(SqliteSyncError::database)?;
                if selected.len() != receipt.batch_ids().len()
                    || selected
                        .iter()
                        .zip(receipt.batch_ids())
                        .any(|((_, current), expected)| current != expected)
                {
                    return Err(SqliteSyncError::CorruptRecord("outbox acknowledgement"));
                }
                let last_position = selected
                    .last()
                    .map(|(position, _)| *position)
                    .ok_or(SqliteSyncError::CorruptRecord("outbox acknowledgement"))?;
                drop(statement);
                let removed = transaction
                    .execute(
                        "DELETE FROM appcore_sync_outbox WHERE position <= ?1",
                        [last_position],
                    )
                    .map_err(SqliteSyncError::database)?;
                if removed != receipt.batch_ids().len() {
                    return Err(SqliteSyncError::CorruptRecord("outbox acknowledgement"));
                }
                transaction.commit().map_err(SqliteSyncError::database)?;
                Ok(removed)
            })
            .map_err(|error| match error {
                SqliteSyncError::CorruptRecord("outbox acknowledgement") => {
                    SyncError::InvalidSyncMessage("outbox acknowledgement mismatch")
                }
                other => other.sync(),
            })
    }
}

fn read_selected_messages(
    connection: &rusqlite::Connection,
    last_position: i64,
    selected: usize,
    selected_bytes: usize,
) -> Result<Vec<SyncMessage>, SqliteSyncError> {
    let limit =
        i64::try_from(selected).map_err(|_| SqliteSyncError::CapacityExceeded("outbox page"))?;
    let mut statement = connection
        .prepare(
            "SELECT encoded FROM appcore_sync_outbox
             WHERE position <= ?1 ORDER BY position LIMIT ?2",
        )
        .map_err(SqliteSyncError::database)?;
    let rows = statement
        .query_map(params![last_position, limit], |row| {
            row.get::<_, Vec<u8>>(0)
        })
        .map_err(SqliteSyncError::database)?;
    let mut messages = Vec::with_capacity(selected);
    let mut actual_bytes = 0usize;
    for encoded in rows {
        let encoded = encoded.map_err(SqliteSyncError::database)?;
        actual_bytes = actual_bytes
            .checked_add(encoded.len())
            .ok_or(SqliteSyncError::CapacityExceeded("outbox byte"))?;
        messages.push(
            serde_json::from_slice(&encoded)
                .map_err(|_| SqliteSyncError::CorruptRecord("outbox"))?,
        );
    }
    if messages.len() != selected || actual_bytes != selected_bytes {
        return Err(SqliteSyncError::CorruptRecord("outbox page"));
    }
    Ok(messages)
}

fn validate_page_limits(limit: usize, max_bytes: usize) -> SyncResult<()> {
    if limit > MAX_OUTBOX_PAGE_MESSAGES || max_bytes > MAX_OUTBOX_PAGE_BYTES {
        return Err(SyncError::InvalidSyncMessage("invalid outbox page limits"));
    }
    Ok(())
}

fn checked_usize(value: i64, reason: &'static str) -> Result<usize, SqliteSyncError> {
    usize::try_from(value).map_err(|_| SqliteSyncError::CorruptRecord(reason))
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
