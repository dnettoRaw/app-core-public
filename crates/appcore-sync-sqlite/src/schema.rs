// =============================================================================
//        #######
//     ###       ###     F: schema.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

use crate::{SqliteSyncError, SqliteSyncResult, SQLITE_SYNC_SCHEMA_V1, SQLITE_SYNC_SCHEMA_V2};
use rusqlite::{Connection, TransactionBehavior};

const APPLICATION_ID: i64 = 0x4150_4353;

pub(crate) fn migrate(connection: &mut Connection) -> SqliteSyncResult<()> {
    let version = schema_version(connection)?;
    if version > SQLITE_SYNC_SCHEMA_V2 {
        return Err(SqliteSyncError::UpdateRequired);
    }
    if version == 0 && has_user_tables(connection)? {
        return Err(SqliteSyncError::UpdateRequired);
    }
    if version == 0 {
        create_schema_v2(connection)?;
        return Ok(());
    }
    if version == SQLITE_SYNC_SCHEMA_V1 {
        migrate_v1_to_v2(connection)?;
    }
    if schema_version(connection)? != SQLITE_SYNC_SCHEMA_V2 {
        return Err(SqliteSyncError::UpdateRequired);
    }
    let application_id: i64 = connection
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .map_err(SqliteSyncError::database)?;
    if application_id != APPLICATION_ID {
        return Err(SqliteSyncError::UpdateRequired);
    }
    Ok(())
}

fn schema_version(connection: &Connection) -> SqliteSyncResult<u32> {
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(SqliteSyncError::database)
}

fn has_user_tables(connection: &Connection) -> SqliteSyncResult<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%')",
            [],
            |row| row.get(0),
        )
        .map_err(SqliteSyncError::database)
}

fn create_schema_v2(connection: &mut Connection) -> SqliteSyncResult<()> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(SqliteSyncError::database)?;
    transaction
        .execute_batch(
            "CREATE TABLE appcore_replication_log (
                log_index INTEGER PRIMARY KEY AUTOINCREMENT,
                source_sequence INTEGER NOT NULL CHECK(source_sequence >= 0),
                payload BLOB NOT NULL,
                previous_hash TEXT NOT NULL,
                record_hash TEXT NOT NULL
            );
            CREATE UNIQUE INDEX appcore_replication_sequence
                ON appcore_replication_log(source_sequence)
                WHERE source_sequence > 0;
            CREATE TABLE appcore_sync_outbox (
                position INTEGER PRIMARY KEY AUTOINCREMENT,
                batch_id TEXT NOT NULL UNIQUE,
                encoded BLOB NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts >= 0),
                next_ready_at_ms INTEGER NOT NULL DEFAULT 0 CHECK(next_ready_at_ms >= 0)
            );
            CREATE TABLE appcore_sync_checkpoint (
                peer_id TEXT PRIMARY KEY,
                sequence INTEGER NOT NULL CHECK(sequence >= 0),
                batch_hash TEXT NOT NULL
            ) WITHOUT ROWID;
            CREATE TABLE appcore_sync_tombstone (
                namespace TEXT NOT NULL,
                opaque_key TEXT NOT NULL,
                deleted_sequence INTEGER NOT NULL CHECK(deleted_sequence > 0),
                payload_hash TEXT NOT NULL,
                expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms > 0),
                PRIMARY KEY(namespace, opaque_key)
            ) WITHOUT ROWID;
            CREATE INDEX appcore_sync_tombstone_expiry
                ON appcore_sync_tombstone(expires_at_ms);",
        )
        .map_err(SqliteSyncError::database)?;
    transaction
        .pragma_update(None, "application_id", APPLICATION_ID)
        .map_err(SqliteSyncError::database)?;
    transaction
        .pragma_update(None, "user_version", SQLITE_SYNC_SCHEMA_V2)
        .map_err(SqliteSyncError::database)?;
    transaction.commit().map_err(SqliteSyncError::database)
}

fn migrate_v1_to_v2(connection: &mut Connection) -> SqliteSyncResult<()> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(SqliteSyncError::database)?;
    transaction
        .execute_batch(
            "ALTER TABLE appcore_sync_outbox
                 ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts >= 0);
             ALTER TABLE appcore_sync_outbox
                 ADD COLUMN next_ready_at_ms INTEGER NOT NULL DEFAULT 0
                 CHECK(next_ready_at_ms >= 0);",
        )
        .map_err(SqliteSyncError::database)?;
    transaction
        .pragma_update(None, "user_version", SQLITE_SYNC_SCHEMA_V2)
        .map_err(SqliteSyncError::database)?;
    transaction.commit().map_err(SqliteSyncError::database)
}
