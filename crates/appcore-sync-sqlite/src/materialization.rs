// =============================================================================
//        #######
//     ###       ###     F: materialization.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/12 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/12 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! `SQLite` write-ahead intents for recoverable opaque-object materialization.
//! This is not an atomic filesystem/SQLite transaction. Deployment maps opaque
//! keys to safe paths and syncs/revalidates bytes before acknowledging the intent.
use crate::{SqliteSyncError, SqliteSyncResult, SqliteSyncStore};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

const MAX_INTENTS: usize = 4_096;
const MAX_PAYLOAD_BYTES: usize = 1024 * 1024;
/// Bounded immutable materialization intent, scoped to an installation.
#[derive(Clone, PartialEq, Eq)]
pub struct ObjectWriteIntentV1 {
    /// Opaque idempotency identity, 1..=128 safe identifier bytes.
    pub intent_id: String,
    /// Installation allowed to replay this intent.
    pub installation_id: String,
    /// Opaque tenant-scoped object key; never an unchecked filesystem path.
    pub object_key: String,
    /// Opaque envelope bytes, at most 1 MiB. Deployment authenticates them.
    pub envelope: Vec<u8>,
}
impl std::fmt::Debug for ObjectWriteIntentV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjectWriteIntentV1")
            .field("envelope_bytes", &self.envelope.len())
            .finish_non_exhaustive()
    }
}
impl ObjectWriteIntentV1 {
    /// Checks identity and payload limits without interpreting document content.
    pub fn validate(&self) -> SqliteSyncResult<()> {
        for id in [&self.intent_id, &self.installation_id, &self.object_key] {
            validate_id(id)?;
        }
        if self.envelope.is_empty() || self.envelope.len() > MAX_PAYLOAD_BYTES {
            return Err(SqliteSyncError::CapacityExceeded("object envelope"));
        }
        Ok(())
    }
    /// Digest of opaque envelope bytes. Prefer encrypted envelopes; never export
    /// predictable plaintext digests as diagnostics or treat them as authentication.
    pub fn envelope_sha256(&self) -> String {
        digest(&self.envelope)
    }
}
impl SqliteSyncStore {
    /// Returns a FIFO bounded page of outstanding intents for one installation.
    /// Materialize idempotently in this order under application-owned object locks.
    pub fn pending_object_writes(
        &self,
        installation: &str,
        limit: usize,
        max_bytes: usize,
    ) -> SqliteSyncResult<Vec<ObjectWriteIntentV1>> {
        validate_id(installation)?;
        if limit > 1_024 || max_bytes > 16 * 1024 * 1024 {
            return Err(SqliteSyncError::CapacityExceeded("object page"));
        }
        self.with_connection(|connection| {
            let transaction = connection.transaction().map_err(SqliteSyncError::database)?;
            let mut statement = transaction.prepare(
                "SELECT intent_id, object_key, length(envelope) FROM appcore_object_write_intent WHERE installation_id=?1 ORDER BY position LIMIT ?2"
            ).map_err(SqliteSyncError::database)?;
            let rows = statement.query_map(params![installation, limit as i64], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)))
                .map_err(SqliteSyncError::database)?;
            let mut result = Vec::new();
            let mut bytes = 0usize;
            for row in rows {
                let (id, key, length) = row.map_err(SqliteSyncError::database)?;
                let length = usize::try_from(length).map_err(|_| SqliteSyncError::IntegrityFailed)?;
                if length > MAX_PAYLOAD_BYTES { return Err(SqliteSyncError::IntegrityFailed); }
                if bytes.checked_add(length).is_none_or(|value| value > max_bytes) { break; }
                let (envelope, hash): (Vec<u8>, String) = transaction.query_row(
                    "SELECT envelope, envelope_hash FROM appcore_object_write_intent WHERE intent_id=?1", [&id], |row| Ok((row.get(0)?, row.get(1)?))
                ).map_err(SqliteSyncError::database)?;
                if digest(&envelope) != hash { return Err(SqliteSyncError::IntegrityFailed); }
                let intent = ObjectWriteIntentV1 { intent_id: id, installation_id: installation.into(), object_key: key, envelope };
                intent.validate()?;
                bytes += length;
                result.push(intent);
            }
            Ok(result)
        })
    }
    /// Removes only the installation's oldest intent after the caller has synced
    /// and verified the actual materialized envelope. Same receipt replay returns
    /// false after removal; an ID must never be reused once an intent is completed.
    /// This records caller evidence, not an independent filesystem verification.
    pub fn acknowledge_object_write(
        &self,
        installation: &str,
        id: &str,
        observed_sha256: &str,
    ) -> SqliteSyncResult<bool> {
        validate_id(installation)?;
        validate_id(id)?;
        self.with_connection(|connection| {
            let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(SqliteSyncError::database)?;
            let owner: Option<String> = transaction.query_row(
                "SELECT installation_id FROM appcore_object_write_intent WHERE intent_id=?1", [id], |row| row.get(0)
            ).optional().map_err(SqliteSyncError::database)?;
            let Some(owner) = owner else { return Ok(false); };
            if owner != installation { return Err(SqliteSyncError::CorruptRecord("object installation")); }
            let existing: Option<(String, String)> = transaction.query_row(
                "SELECT intent_id, envelope_hash FROM appcore_object_write_intent WHERE installation_id=?1 ORDER BY position LIMIT 1", [installation],
                |row| Ok((row.get(0)?, row.get(1)?))
            ).optional().map_err(SqliteSyncError::database)?;
            let Some((front, hash)) = existing else { return Ok(false); };
            if front != id || hash != observed_sha256 { return Err(SqliteSyncError::CorruptRecord("object receipt")); }
            transaction.execute("DELETE FROM appcore_object_write_intent WHERE intent_id=?1 AND installation_id=?2", params![id, installation]).map_err(SqliteSyncError::database)?;
            transaction.commit().map_err(SqliteSyncError::database)?;
            Ok(true)
        })
    }
}
pub(crate) fn stage(
    connection: &Connection,
    intent: &ObjectWriteIntentV1,
) -> SqliteSyncResult<bool> {
    intent.validate()?;
    let existing: Option<(String, String, Vec<u8>)> = connection.query_row(
        "SELECT installation_id, object_key, CASE WHEN length(envelope) BETWEEN 1 AND 1048576 THEN envelope ELSE NULL END FROM appcore_object_write_intent WHERE intent_id=?1", [&intent.intent_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    ).optional().map_err(SqliteSyncError::database)?;
    if let Some((installation, key, bytes)) = existing {
        if installation != intent.installation_id
            || key != intent.object_key
            || bytes != intent.envelope
        {
            return Err(SqliteSyncError::CorruptRecord("object intent"));
        }
        return Ok(false);
    }
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM appcore_object_write_intent",
            [],
            |row| row.get(0),
        )
        .map_err(SqliteSyncError::database)?;
    if count >= MAX_INTENTS as i64 {
        return Err(SqliteSyncError::CapacityExceeded("object intent"));
    }
    connection.execute("INSERT INTO appcore_object_write_intent(intent_id, installation_id, object_key, envelope, envelope_hash) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![intent.intent_id, intent.installation_id, intent.object_key, intent.envelope, digest(&intent.envelope)]).map_err(SqliteSyncError::database)?;
    Ok(true)
}
pub(crate) fn validate_records(connection: &Connection) -> SqliteSyncResult<()> {
    let mut statement = connection.prepare("SELECT intent_id, installation_id, object_key, CASE WHEN length(envelope) BETWEEN 1 AND 1048576 THEN envelope ELSE NULL END, envelope_hash FROM appcore_object_write_intent ORDER BY position LIMIT 4097").map_err(SqliteSyncError::database)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                ObjectWriteIntentV1 {
                    intent_id: row.get(0)?,
                    installation_id: row.get(1)?,
                    object_key: row.get(2)?,
                    envelope: row.get(3)?,
                },
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(SqliteSyncError::database)?;
    for (index, row) in rows.enumerate() {
        let (intent, hash) = row.map_err(SqliteSyncError::database)?;
        if index >= MAX_INTENTS || intent.validate().is_err() || digest(&intent.envelope) != hash {
            return Err(SqliteSyncError::IntegrityFailed);
        }
    }
    Ok(())
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn validate_id(id: &str) -> SqliteSyncResult<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
    {
        return Err(SqliteSyncError::CorruptRecord("object identity"));
    }
    Ok(())
}
