// =============================================================================
//        #######
//     ###       ###     F: integrity.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

use crate::checkpoint::{validate_hash, validate_peer_id};
use crate::log::{record_hash, MAX_REPLICATION_RECORD_BYTES};
use crate::outbox::validate_batch_id;
use crate::tombstone::validate_tombstone;
use crate::{SqliteSyncConfig, SqliteSyncError, SqliteSyncResult, SqliteSyncTombstone};
use appcore_sync::SyncMessage;
use rusqlite::Connection;

pub(crate) fn validate_internal_records(
    connection: &Connection,
    config: &SqliteSyncConfig,
) -> SqliteSyncResult<()> {
    validate_replication_log(connection, config)?;
    validate_outbox(connection, config)?;
    validate_checkpoints(connection, config)?;
    validate_tombstones(connection, config)
}

fn validate_replication_log(
    connection: &Connection,
    config: &SqliteSyncConfig,
) -> SqliteSyncResult<()> {
    let mut statement = connection
        .prepare(
            "SELECT log_index, source_sequence, payload, previous_hash, record_hash
             FROM appcore_replication_log ORDER BY log_index",
        )
        .map_err(SqliteSyncError::database)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(SqliteSyncError::database)?;
    let mut previous_hash = String::new();
    let mut bytes = 0u64;
    for (expected_index, row) in (1i64..).zip(rows) {
        let (index, sequence, payload, stored_previous, stored_hash) =
            row.map_err(SqliteSyncError::database)?;
        bytes = bytes
            .checked_add(payload.len() as u64)
            .ok_or(SqliteSyncError::IntegrityFailed)?;
        let sequence = u64::try_from(sequence).map_err(|_| SqliteSyncError::IntegrityFailed)?;
        let expected_hash = record_hash(&previous_hash, sequence, &payload);
        if index != expected_index
            || payload.len() > MAX_REPLICATION_RECORD_BYTES
            || bytes > config.max_database_bytes
            || stored_previous != previous_hash
            || stored_hash != expected_hash
        {
            return Err(SqliteSyncError::IntegrityFailed);
        }
        previous_hash = expected_hash;
    }
    Ok(())
}

fn validate_outbox(connection: &Connection, config: &SqliteSyncConfig) -> SqliteSyncResult<()> {
    let mut statement = connection
        .prepare("SELECT batch_id, encoded FROM appcore_sync_outbox ORDER BY position")
        .map_err(SqliteSyncError::database)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .map_err(SqliteSyncError::database)?;
    let mut count = 0usize;
    let mut bytes = 0u64;
    for row in rows {
        let (batch_id, encoded) = row.map_err(SqliteSyncError::database)?;
        count = count
            .checked_add(1)
            .ok_or(SqliteSyncError::IntegrityFailed)?;
        bytes = bytes
            .checked_add(encoded.len() as u64)
            .ok_or(SqliteSyncError::IntegrityFailed)?;
        let message: SyncMessage =
            serde_json::from_slice(&encoded).map_err(|_| SqliteSyncError::IntegrityFailed)?;
        if count > config.max_outbox_entries
            || encoded.len() > config.max_outbox_record_bytes
            || bytes > config.max_database_bytes
            || message.batch_id != batch_id
            || validate_batch_id(&batch_id).is_err()
        {
            return Err(SqliteSyncError::IntegrityFailed);
        }
    }
    Ok(())
}

fn validate_checkpoints(
    connection: &Connection,
    config: &SqliteSyncConfig,
) -> SqliteSyncResult<()> {
    let mut statement = connection
        .prepare("SELECT peer_id, sequence, batch_hash FROM appcore_sync_checkpoint")
        .map_err(SqliteSyncError::database)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(SqliteSyncError::database)?;
    let mut count = 0usize;
    for row in rows {
        let (peer_id, sequence, hash) = row.map_err(SqliteSyncError::database)?;
        count = count
            .checked_add(1)
            .ok_or(SqliteSyncError::IntegrityFailed)?;
        if count > config.max_checkpoints
            || sequence < 0
            || validate_peer_id(&peer_id).is_err()
            || validate_hash(&hash).is_err()
        {
            return Err(SqliteSyncError::IntegrityFailed);
        }
    }
    Ok(())
}

fn validate_tombstones(connection: &Connection, config: &SqliteSyncConfig) -> SqliteSyncResult<()> {
    let mut statement = connection
        .prepare(
            "SELECT namespace, opaque_key, deleted_sequence, payload_hash, expires_at_ms
             FROM appcore_sync_tombstone",
        )
        .map_err(SqliteSyncError::database)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(SqliteSyncError::database)?;
    let mut count = 0usize;
    for row in rows {
        let (namespace, opaque_key, sequence, payload_hash, expiry) =
            row.map_err(SqliteSyncError::database)?;
        count = count
            .checked_add(1)
            .ok_or(SqliteSyncError::IntegrityFailed)?;
        let tombstone = SqliteSyncTombstone {
            namespace,
            opaque_key,
            deleted_sequence: u64::try_from(sequence)
                .map_err(|_| SqliteSyncError::IntegrityFailed)?,
            payload_hash,
            expires_at_ms: u64::try_from(expiry).map_err(|_| SqliteSyncError::IntegrityFailed)?,
        };
        if count > config.max_tombstones || validate_tombstone(&tombstone).is_err() {
            return Err(SqliteSyncError::IntegrityFailed);
        }
    }
    Ok(())
}
