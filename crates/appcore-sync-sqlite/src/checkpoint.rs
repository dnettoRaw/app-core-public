// =============================================================================
//        #######
//     ###       ###     F: checkpoint.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

use crate::{SqliteSyncError, SqliteSyncStore};
use appcore_sync::{SyncCheckpointStore, SyncError, SyncResult};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

const MAX_PEER_ID_BYTES: usize = 256;

/// SQLite-backed per-peer synchronization checkpoints.
#[derive(Debug, Clone)]
pub struct SqliteSyncCheckpointStore {
    store: SqliteSyncStore,
}

impl SqliteSyncCheckpointStore {
    pub(crate) fn new(store: SqliteSyncStore) -> Self {
        Self { store }
    }
}

impl SyncCheckpointStore for SqliteSyncCheckpointStore {
    fn get_checkpoint(&self, peer_id: &str) -> SyncResult<Option<(u64, String)>> {
        validate_peer_id(peer_id)?;
        self.store
            .with_connection(|connection| {
                connection
                    .query_row(
                        "SELECT sequence, batch_hash FROM appcore_sync_checkpoint
                         WHERE peer_id = ?1",
                        [peer_id],
                        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                    )
                    .optional()
                    .map_err(SqliteSyncError::database)?
                    .map(|(sequence, hash)| {
                        u64::try_from(sequence)
                            .map(|sequence| (sequence, hash))
                            .map_err(|_| SqliteSyncError::CorruptRecord("checkpoint"))
                    })
                    .transpose()
            })
            .map_err(SqliteSyncError::sync)
    }

    fn set_checkpoint(&self, peer_id: &str, sequence: u64, hash: &str) -> SyncResult<()> {
        validate_peer_id(peer_id)?;
        validate_hash(hash)?;
        let sequence_u64 = sequence;
        let sequence = i64::try_from(sequence).map_err(|_| {
            SyncError::ReplicationFailed("checkpoint capacity exceeded".to_string())
        })?;
        self.store
            .with_connection(|connection| {
                let transaction = connection
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .map_err(SqliteSyncError::database)?;
                let existing: Option<(i64, String)> = transaction
                    .query_row(
                        "SELECT sequence, batch_hash FROM appcore_sync_checkpoint WHERE peer_id = ?1",
                        [peer_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()
                    .map_err(SqliteSyncError::database)?;
                if let Some((current_sequence, current_hash)) = existing {
                    if sequence < current_sequence
                        || (sequence == current_sequence && hash != current_hash)
                    {
                        return Err(SqliteSyncError::CorruptRecord("checkpoint conflict"));
                    }
                    if sequence == current_sequence {
                        return Ok(());
                    }
                } else if checkpoint_count(&transaction)? >= self.store.config().max_checkpoints {
                    return Err(SqliteSyncError::CapacityExceeded("checkpoint"));
                }
                transaction
                    .execute(
                        "INSERT INTO appcore_sync_checkpoint(peer_id, sequence, batch_hash)
                         VALUES (?1, ?2, ?3)
                         ON CONFLICT(peer_id) DO UPDATE SET
                         sequence = excluded.sequence, batch_hash = excluded.batch_hash",
                        params![peer_id, sequence, hash],
                    )
                    .map_err(SqliteSyncError::database)?;
                transaction.commit().map_err(SqliteSyncError::database)
            })
            .map_err(|error| match error {
                SqliteSyncError::CorruptRecord("checkpoint conflict") => {
                    SyncError::InvalidSequence(sequence_u64)
                }
                other => other.sync(),
            })
    }
}

pub(crate) fn validate_peer_id(peer_id: &str) -> SyncResult<()> {
    if peer_id.is_empty()
        || peer_id.len() > MAX_PEER_ID_BYTES
        || !peer_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | ':' | '-')
        })
    {
        return Err(SyncError::InvalidPeerId);
    }
    Ok(())
}

pub(crate) fn validate_hash(hash: &str) -> SyncResult<()> {
    if hash.is_empty() || (hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())) {
        Ok(())
    } else {
        Err(SyncError::ReplicationFailed(
            "invalid checkpoint hash".to_string(),
        ))
    }
}

fn checkpoint_count(connection: &rusqlite::Connection) -> Result<usize, SqliteSyncError> {
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM appcore_sync_checkpoint", [], |row| {
            row.get(0)
        })
        .map_err(SqliteSyncError::database)?;
    usize::try_from(count).map_err(|_| SqliteSyncError::CorruptRecord("checkpoint count"))
}
