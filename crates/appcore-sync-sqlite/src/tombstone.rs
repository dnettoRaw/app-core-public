// =============================================================================
//        #######
//     ###       ###     F: tombstone.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

use crate::{SqliteSyncError, SqliteSyncResult, SqliteSyncStore};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

const MAX_NAMESPACE_BYTES: usize = 128;
const MAX_OPAQUE_KEY_BYTES: usize = 512;

/// One opaque Runtime sync deletion marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteSyncTombstone {
    /// Provider-neutral namespace owned by synchronization infrastructure.
    pub namespace: String,
    /// Opaque deletion identity; its business meaning is not interpreted.
    pub opaque_key: String,
    /// Replication sequence at which deletion occurred.
    pub deleted_sequence: u64,
    /// SHA-256 hash bound to the deleted opaque payload.
    pub payload_hash: String,
    /// Expiry time in Unix epoch milliseconds.
    pub expires_at_ms: u64,
}

/// Bounded SQLite tombstone storage for conservative deletion replication.
#[derive(Debug, Clone)]
pub struct SqliteSyncTombstoneStore {
    store: SqliteSyncStore,
}

impl SqliteSyncTombstoneStore {
    pub(crate) fn new(store: SqliteSyncStore) -> Self {
        Self { store }
    }

    /// Atomically inserts or advances one opaque deletion marker.
    pub fn record(&self, tombstone: &SqliteSyncTombstone) -> SqliteSyncResult<bool> {
        validate_tombstone(tombstone)?;
        self.store.with_connection(|connection| {
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(SqliteSyncError::database)?;
            let existing: Option<(i64, String, i64)> = transaction
                .query_row(
                    "SELECT deleted_sequence, payload_hash, expires_at_ms
                     FROM appcore_sync_tombstone WHERE namespace = ?1 AND opaque_key = ?2",
                    params![tombstone.namespace, tombstone.opaque_key],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()
                .map_err(SqliteSyncError::database)?;
            let incoming_sequence = to_i64(tombstone.deleted_sequence, "tombstone sequence")?;
            let incoming_expiry = to_i64(tombstone.expires_at_ms, "tombstone expiry")?;
            if let Some((sequence, hash, expiry)) = existing {
                if incoming_sequence < sequence {
                    return Ok(false);
                }
                if incoming_sequence == sequence {
                    return if hash == tombstone.payload_hash && expiry == incoming_expiry {
                        Ok(false)
                    } else {
                        Err(SqliteSyncError::CorruptRecord("tombstone conflict"))
                    };
                }
            } else if count_tombstones(&transaction)? >= self.store.config().max_tombstones {
                return Err(SqliteSyncError::CapacityExceeded("tombstone"));
            }
            let changed = transaction
                .execute(
                    "INSERT INTO appcore_sync_tombstone
                     (namespace, opaque_key, deleted_sequence, payload_hash, expires_at_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(namespace, opaque_key) DO UPDATE SET
                     deleted_sequence = excluded.deleted_sequence,
                     payload_hash = excluded.payload_hash,
                     expires_at_ms = excluded.expires_at_ms
                     WHERE excluded.deleted_sequence >= appcore_sync_tombstone.deleted_sequence",
                    params![
                        tombstone.namespace,
                        tombstone.opaque_key,
                        incoming_sequence,
                        tombstone.payload_hash,
                        incoming_expiry
                    ],
                )
                .map_err(SqliteSyncError::database)?;
            transaction.commit().map_err(SqliteSyncError::database)?;
            Ok(changed == 1)
        })
    }

    /// Returns at most `limit` unexpired markers in deterministic order.
    pub fn active(&self, now_ms: u64, limit: usize) -> SqliteSyncResult<Vec<SqliteSyncTombstone>> {
        if limit == 0 || limit > self.store.config().max_tombstones {
            return Err(SqliteSyncError::CapacityExceeded("tombstone read"));
        }
        self.store.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT namespace, opaque_key, deleted_sequence, payload_hash, expires_at_ms
                     FROM appcore_sync_tombstone WHERE expires_at_ms > ?1
                     ORDER BY namespace, opaque_key LIMIT ?2",
                )
                .map_err(SqliteSyncError::database)?;
            let rows = statement
                .query_map(
                    params![
                        to_i64(now_ms, "current time")?,
                        to_i64(limit as u64, "limit")?
                    ],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, i64>(4)?,
                        ))
                    },
                )
                .map_err(SqliteSyncError::database)?;
            let mut tombstones = Vec::new();
            for row in rows {
                let (namespace, opaque_key, sequence, payload_hash, expiry) =
                    row.map_err(SqliteSyncError::database)?;
                tombstones.push(SqliteSyncTombstone {
                    namespace,
                    opaque_key,
                    deleted_sequence: to_u64(sequence, "tombstone sequence")?,
                    payload_hash,
                    expires_at_ms: to_u64(expiry, "tombstone expiry")?,
                });
            }
            Ok(tombstones)
        })
    }

    /// Deletes at most `limit` expired markers and returns the deletion count.
    pub fn prune_expired(&self, now_ms: u64, limit: usize) -> SqliteSyncResult<usize> {
        if limit == 0 || limit > self.store.config().max_tombstones {
            return Err(SqliteSyncError::CapacityExceeded("tombstone prune"));
        }
        self.store.with_connection(|connection| {
            connection
                .execute(
                    "DELETE FROM appcore_sync_tombstone WHERE (namespace, opaque_key) IN (
                         SELECT namespace, opaque_key FROM appcore_sync_tombstone
                         WHERE expires_at_ms <= ?1 ORDER BY expires_at_ms LIMIT ?2
                     )",
                    params![
                        to_i64(now_ms, "current time")?,
                        to_i64(limit as u64, "limit")?
                    ],
                )
                .map_err(SqliteSyncError::database)
        })
    }

    /// Returns the current retained marker count.
    pub fn len(&self) -> SqliteSyncResult<usize> {
        self.store
            .with_connection(|connection| count_tombstones(connection))
    }

    /// Reports whether no markers are retained.
    pub fn is_empty(&self) -> SqliteSyncResult<bool> {
        self.len().map(|length| length == 0)
    }
}

fn count_tombstones(connection: &rusqlite::Connection) -> SqliteSyncResult<usize> {
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM appcore_sync_tombstone", [], |row| {
            row.get(0)
        })
        .map_err(SqliteSyncError::database)?;
    usize::try_from(count).map_err(|_| SqliteSyncError::CorruptRecord("tombstone count"))
}

pub(crate) fn validate_tombstone(tombstone: &SqliteSyncTombstone) -> SqliteSyncResult<()> {
    if !valid_identifier(&tombstone.namespace, MAX_NAMESPACE_BYTES)
        || !valid_identifier(&tombstone.opaque_key, MAX_OPAQUE_KEY_BYTES)
        || tombstone.deleted_sequence == 0
        || tombstone.expires_at_ms == 0
        || tombstone.payload_hash.len() != 64
        || !tombstone
            .payload_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(SqliteSyncError::CorruptRecord("tombstone input"));
    }
    Ok(())
}

fn valid_identifier(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value
            .chars()
            .all(|character| character.is_ascii_graphic() && !character.is_ascii_control())
}

fn to_i64(value: u64, resource: &'static str) -> SqliteSyncResult<i64> {
    i64::try_from(value).map_err(|_| SqliteSyncError::CapacityExceeded(resource))
}

fn to_u64(value: i64, resource: &'static str) -> SqliteSyncResult<u64> {
    u64::try_from(value).map_err(|_| SqliteSyncError::CorruptRecord(resource))
}
