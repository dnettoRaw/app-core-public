// =============================================================================
//        #######
//     ###       ###     F: conflict.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/12 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/12 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Durable conflict evidence and idempotent resolution without domain payloads.
use crate::{SqliteSyncError, SqliteSyncResult, SqliteSyncStore};
use appcore_sync::{
    SyncConflict, SyncConflictResolutionReceipt, SyncConflictResolutionRequest, SyncConflictStore,
    SyncError, SyncResult, MAX_SYNC_CONFLICTS,
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

const MAX_CONFLICT_BYTES: usize = 8_192;
const MAX_RESOLUTION_BYTES: usize = 4_096;

/// SQLite-backed bounded conflict registry. It records decisions, never applies them.
#[derive(Clone, Debug)]
pub struct SqliteSyncConflictStore {
    store: SqliteSyncStore,
}
impl SqliteSyncStore {
    /// Returns a conflict store sharing this database and its integrity/backup boundary.
    pub fn conflict_store(&self) -> SqliteSyncConflictStore {
        SqliteSyncConflictStore {
            store: self.clone(),
        }
    }
}
impl SyncConflictStore for SqliteSyncConflictStore {
    fn record(&self, conflict: SyncConflict) -> SyncResult<()> {
        conflict.validate()?;
        self.store
            .with_connection(|connection| {
                let transaction = connection
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .map_err(SqliteSyncError::database)?;
                let existing: Option<String> = transaction
                    .query_row(
                        "SELECT CASE WHEN length(CAST(evidence AS BLOB)) <= 8192 THEN evidence ELSE NULL END FROM appcore_sync_conflict WHERE conflict_id = ?1",
                        [&conflict.conflict_id],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(SqliteSyncError::database)?;
                if let Some(existing) = existing {
                    if decode_conflict(&existing)? == conflict {
                        return Ok(());
                    }
                    return Err(SqliteSyncError::CorruptRecord("conflict identity"));
                }
                let count: i64 = transaction
                    .query_row("SELECT COUNT(*) FROM appcore_sync_conflict", [], |row| {
                        row.get(0)
                    })
                    .map_err(SqliteSyncError::database)?;
                if count >= MAX_SYNC_CONFLICTS as i64 {
                    return Err(SqliteSyncError::CapacityExceeded("conflict"));
                }
                let encoded = serde_json::to_string(&conflict)
                    .map_err(|_| SqliteSyncError::CorruptRecord("conflict"))?;
                if encoded.len() > MAX_CONFLICT_BYTES {
                    return Err(SqliteSyncError::CapacityExceeded("conflict record"));
                }
                transaction
                    .execute(
                        "INSERT INTO appcore_sync_conflict(conflict_id, evidence) VALUES (?1, ?2)",
                        params![conflict.conflict_id, encoded],
                    )
                    .map_err(SqliteSyncError::database)?;
                transaction.commit().map_err(SqliteSyncError::database)
            })
            .map_err(|error| match error {
                SqliteSyncError::CapacityExceeded("conflict") => SyncError::ConflictStoreFull,
                other => other.sync(),
            })
    }
    fn resolve(
        &self,
        request: SyncConflictResolutionRequest,
    ) -> SyncResult<SyncConflictResolutionReceipt> {
        request.validate()?;
        self.store
            .with_connection(|connection| {
                let transaction = connection
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .map_err(SqliteSyncError::database)?;
                let row: Option<(String, Option<String>)> = transaction.query_row(
                "SELECT CASE WHEN length(CAST(evidence AS BLOB)) <= 8192 AND (resolution IS NULL OR length(CAST(resolution AS BLOB)) <= 4096) THEN evidence ELSE NULL END, CASE WHEN length(CAST(resolution AS BLOB)) <= 4096 THEN resolution ELSE NULL END FROM appcore_sync_conflict WHERE conflict_id = ?1",
                [&request.conflict_id], |row| Ok((row.get(0)?, row.get(1)?)),
            ).optional().map_err(SqliteSyncError::database)?;
                let Some((evidence, resolution)) = row else {
                    return Err(SqliteSyncError::CorruptRecord("conflict missing"));
                };
                let conflict = decode_conflict(&evidence)?;
            if conflict.conflict_id != request.conflict_id { return Err(SqliteSyncError::IntegrityFailed); }
                if let Some(resolution) = resolution {
                    let mut receipt = decode_resolution(&resolution, &conflict.conflict_id)?;
                    if receipt.idempotency_key != request.idempotency_key
                        || receipt.resolution != request.resolution
                    {
                        return Err(SqliteSyncError::CorruptRecord("conflict resolved"));
                    }
                    receipt.idempotent = true;
                    return Ok(receipt);
                }
                let receipt = SyncConflictResolutionReceipt {
                    conflict_id: request.conflict_id.clone(),
                    idempotency_key: request.idempotency_key,
                    resolution: request.resolution,
                    idempotent: false,
                    resolved_at_ms: request.resolved_at_ms,
                };
                let encoded = serde_json::to_string(&receipt)
                    .map_err(|_| SqliteSyncError::CorruptRecord("resolution"))?;
                if encoded.len() > MAX_RESOLUTION_BYTES {
                    return Err(SqliteSyncError::CapacityExceeded("resolution"));
                }
                transaction
                    .execute(
                        "UPDATE appcore_sync_conflict SET resolution = ?1 WHERE conflict_id = ?2",
                        params![encoded, receipt.conflict_id],
                    )
                    .map_err(SqliteSyncError::database)?;
                transaction.commit().map_err(SqliteSyncError::database)?;
                Ok(receipt)
            })
            .map_err(|error| match error {
                SqliteSyncError::CorruptRecord("conflict missing") => {
                    SyncError::ConflictNotFound(request.conflict_id.clone())
                }
                SqliteSyncError::CorruptRecord("conflict resolved") => {
                    SyncError::ConflictAlreadyResolved(request.conflict_id.clone())
                }
                other => other.sync(),
            })
    }
    fn list(&self) -> SyncResult<Vec<SyncConflict>> {
        self.store
            .with_connection(|connection| {
                read_conflicts(connection)
                    .map(|records| records.into_iter().map(|(conflict, _)| conflict).collect())
            })
            .map_err(SqliteSyncError::sync)
    }
}

pub(crate) fn validate_conflicts(connection: &Connection) -> SqliteSyncResult<()> {
    read_conflicts(connection).map(|_| ())
}
fn read_conflicts(
    connection: &Connection,
) -> SqliteSyncResult<Vec<(SyncConflict, Option<SyncConflictResolutionReceipt>)>> {
    let mut statement = connection.prepare("SELECT conflict_id, CASE WHEN length(CAST(evidence AS BLOB)) <= 8192 AND (resolution IS NULL OR length(CAST(resolution AS BLOB)) <= 4096) THEN evidence ELSE NULL END, resolution FROM appcore_sync_conflict ORDER BY conflict_id LIMIT 4097")
        .map_err(SqliteSyncError::database)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(SqliteSyncError::database)?;
    let mut records = Vec::new();
    for row in rows {
        let (id, encoded, resolution) = row.map_err(SqliteSyncError::database)?;
        let conflict = decode_conflict(&encoded)?;
        if conflict.conflict_id != id || records.len() >= MAX_SYNC_CONFLICTS {
            return Err(SqliteSyncError::IntegrityFailed);
        }
        let resolution = resolution
            .map(|value| decode_resolution(&value, &id))
            .transpose()?;
        records.push((conflict, resolution));
    }
    Ok(records)
}
fn decode_conflict(encoded: &str) -> SqliteSyncResult<SyncConflict> {
    if encoded.len() > MAX_CONFLICT_BYTES {
        return Err(SqliteSyncError::IntegrityFailed);
    }
    let value: SyncConflict =
        serde_json::from_str(encoded).map_err(|_| SqliteSyncError::IntegrityFailed)?;
    value
        .validate()
        .map_err(|_| SqliteSyncError::IntegrityFailed)?;
    Ok(value)
}
fn decode_resolution(encoded: &str, id: &str) -> SqliteSyncResult<SyncConflictResolutionReceipt> {
    if encoded.len() > MAX_RESOLUTION_BYTES {
        return Err(SqliteSyncError::IntegrityFailed);
    }
    let value: SyncConflictResolutionReceipt =
        serde_json::from_str(encoded).map_err(|_| SqliteSyncError::IntegrityFailed)?;
    if value.conflict_id != id || value.idempotent {
        return Err(SqliteSyncError::IntegrityFailed);
    }
    SyncConflictResolutionRequest {
        conflict_id: value.conflict_id.clone(),
        idempotency_key: value.idempotency_key.clone(),
        resolution: value.resolution,
        resolved_at_ms: value.resolved_at_ms,
    }
    .validate()
    .map_err(|_| SqliteSyncError::IntegrityFailed)?;
    Ok(value)
}
