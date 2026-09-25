// =============================================================================
//        #######
//     ###       ###     F: conflict.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/25 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/25 00:00:00 by dnettoRaw
//      ###########      S: 1.0.3-rc
// =============================================================================

//! Bounded UI-ready sync conflicts and idempotent resolution records.

use super::error::{SyncError, SyncResult};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Maximum UTF-8 bytes accepted for a conflict identity field.
pub const MAX_CONFLICT_ID_BYTES: usize = 128;
/// Maximum UTF-8 bytes accepted for a conflict reason.
pub const MAX_CONFLICT_REASON_BYTES: usize = 512;
/// Maximum conflicts retained by the in-memory resolution registry.
pub const MAX_SYNC_CONFLICTS: usize = 4_096;

/// Generic replication conflict category, independent of application schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncConflictKind {
    /// One sequence contains different bytes for the same position.
    SequencePayload,
    /// A checkpoint hash disagrees with the observed leader chain.
    CheckpointHash,
    /// A batch identity was reused with different metadata or events.
    BatchIdentity,
}

/// Payload-free conflict observation suitable for a UI or diagnostics API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncConflict {
    /// Stable conflict identity used by resolution requests.
    pub conflict_id: String,
    /// Peer that produced the conflicting observation.
    pub peer_id: String,
    /// Generic conflict category.
    pub kind: SyncConflictKind,
    /// Conflicting replication sequence, when applicable.
    pub sequence: Option<u64>,
    /// Local content digest, never the content payload.
    pub local_hash: String,
    /// Remote content digest, never the content payload.
    pub remote_hash: String,
    /// Bounded operator-visible explanation.
    pub reason: String,
    /// Detection timestamp supplied by the caller.
    pub detected_at_ms: u64,
}

impl SyncConflict {
    /// Validates the bounded, payload-free conflict contract.
    pub fn validate(&self) -> SyncResult<()> {
        for (name, value, max) in [
            (
                "conflict_id",
                self.conflict_id.as_str(),
                MAX_CONFLICT_ID_BYTES,
            ),
            ("peer_id", self.peer_id.as_str(), MAX_CONFLICT_ID_BYTES),
            ("reason", self.reason.as_str(), MAX_CONFLICT_REASON_BYTES),
        ] {
            validate_text(name, value, max)?;
        }
        validate_hash(&self.local_hash)?;
        validate_hash(&self.remote_hash)?;
        Ok(())
    }
}

/// Operator or application choice recorded for one conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncConflictResolution {
    /// Preserve the local replication state.
    KeepLocal,
    /// Accept the remote observation through an owner-controlled operation.
    AcceptRemote,
    /// Defer the decision for manual review.
    Defer,
}

/// Idempotent resolution command for one conflict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncConflictResolutionRequest {
    /// Conflict being resolved.
    pub conflict_id: String,
    /// Stable caller-generated idempotency key.
    pub idempotency_key: String,
    /// Resolution selected by the owner.
    pub resolution: SyncConflictResolution,
    /// Resolution timestamp supplied by the caller.
    pub resolved_at_ms: u64,
}

impl SyncConflictResolutionRequest {
    /// Validates the bounded idempotency and conflict identity fields.
    pub fn validate(&self) -> SyncResult<()> {
        validate_text("conflict_id", &self.conflict_id, MAX_CONFLICT_ID_BYTES)?;
        validate_text(
            "idempotency_key",
            &self.idempotency_key,
            MAX_CONFLICT_ID_BYTES,
        )
    }
}

/// Result of applying or replaying a conflict resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncConflictResolutionReceipt {
    /// Conflict resolved by this receipt.
    pub conflict_id: String,
    /// Idempotency key accepted for the resolution.
    pub idempotency_key: String,
    /// Recorded resolution.
    pub resolution: SyncConflictResolution,
    /// Whether this call replayed an already-recorded identical resolution.
    pub idempotent: bool,
    /// Resolution timestamp.
    pub resolved_at_ms: u64,
}

#[derive(Debug, Clone)]
struct ConflictRecord {
    conflict: SyncConflict,
    resolution: Option<SyncConflictResolutionReceipt>,
}

/// Conflict registry contract with idempotent recording and resolution.
pub trait SyncConflictStore: Send + Sync {
    /// Records one conflict, accepting an identical replay.
    fn record(&self, conflict: SyncConflict) -> SyncResult<()>;
    /// Applies or replays one resolution without mutating replication payloads.
    fn resolve(
        &self,
        request: SyncConflictResolutionRequest,
    ) -> SyncResult<SyncConflictResolutionReceipt>;
    /// Lists payload-free conflicts in stable identity order.
    fn list(&self) -> SyncResult<Vec<SyncConflict>>;
}

/// Bounded process-local conflict store for Runtime/UI coordination.
#[derive(Debug, Clone)]
pub struct InMemorySyncConflictStore {
    records: Arc<Mutex<BTreeMap<String, ConflictRecord>>>,
}

impl Default for InMemorySyncConflictStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemorySyncConflictStore {
    /// Creates an empty bounded conflict store.
    pub fn new() -> Self {
        Self {
            records: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }
}

impl SyncConflictStore for InMemorySyncConflictStore {
    fn record(&self, conflict: SyncConflict) -> SyncResult<()> {
        conflict.validate()?;
        let mut records = self.records.lock();
        if let Some(existing) = records.get(&conflict.conflict_id) {
            if existing.conflict == conflict {
                return Ok(());
            }
            return Err(SyncError::InvalidConflict(
                "conflict identity was reused with different evidence".to_string(),
            ));
        }
        if records.len() >= MAX_SYNC_CONFLICTS {
            return Err(SyncError::ConflictStoreFull);
        }
        records.insert(
            conflict.conflict_id.clone(),
            ConflictRecord {
                conflict,
                resolution: None,
            },
        );
        Ok(())
    }

    fn resolve(
        &self,
        request: SyncConflictResolutionRequest,
    ) -> SyncResult<SyncConflictResolutionReceipt> {
        request.validate()?;
        let mut records = self.records.lock();
        let record = records
            .get_mut(&request.conflict_id)
            .ok_or_else(|| SyncError::ConflictNotFound(request.conflict_id.clone()))?;
        if let Some(existing) = &record.resolution {
            if existing.idempotency_key == request.idempotency_key
                && existing.resolution == request.resolution
            {
                return Ok(SyncConflictResolutionReceipt {
                    idempotent: true,
                    ..existing.clone()
                });
            }
            return Err(SyncError::ConflictAlreadyResolved(
                request.conflict_id.clone(),
            ));
        }
        let receipt = SyncConflictResolutionReceipt {
            conflict_id: request.conflict_id,
            idempotency_key: request.idempotency_key,
            resolution: request.resolution,
            idempotent: false,
            resolved_at_ms: request.resolved_at_ms,
        };
        record.resolution = Some(receipt.clone());
        Ok(receipt)
    }

    fn list(&self) -> SyncResult<Vec<SyncConflict>> {
        Ok(self
            .records
            .lock()
            .values()
            .map(|record| record.conflict.clone())
            .collect())
    }
}

fn validate_text(name: &str, value: &str, max: usize) -> SyncResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(SyncError::InvalidConflict(format!(
            "{name} is empty, too long or contains control characters"
        )));
    }
    Ok(())
}

fn validate_hash(value: &str) -> SyncResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(SyncError::InvalidConflict(
            "conflict hash must be lowercase SHA-256".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conflict() -> SyncConflict {
        SyncConflict {
            conflict_id: "conflict-a".to_string(),
            peer_id: "peer-a".to_string(),
            kind: SyncConflictKind::SequencePayload,
            sequence: Some(42),
            local_hash: "11".repeat(32),
            remote_hash: "22".repeat(32),
            reason: "same sequence has different bytes".to_string(),
            detected_at_ms: 100,
        }
    }

    #[test]
    fn resolution_replay_is_idempotent_and_payload_free() {
        let store = InMemorySyncConflictStore::new();
        store.record(conflict()).unwrap();
        let request = SyncConflictResolutionRequest {
            conflict_id: "conflict-a".to_string(),
            idempotency_key: "resolve-a".to_string(),
            resolution: SyncConflictResolution::KeepLocal,
            resolved_at_ms: 200,
        };
        let first = store.resolve(request.clone()).unwrap();
        let replay = store.resolve(request).unwrap();
        assert!(!first.idempotent);
        assert!(replay.idempotent);
        assert_eq!(store.list().unwrap()[0].local_hash, "11".repeat(32));
    }

    #[test]
    fn conflicting_resolution_is_rejected_without_overwrite() {
        let store = InMemorySyncConflictStore::new();
        store.record(conflict()).unwrap();
        store
            .resolve(SyncConflictResolutionRequest {
                conflict_id: "conflict-a".to_string(),
                idempotency_key: "resolve-a".to_string(),
                resolution: SyncConflictResolution::KeepLocal,
                resolved_at_ms: 200,
            })
            .unwrap();
        assert!(matches!(
            store.resolve(SyncConflictResolutionRequest {
                conflict_id: "conflict-a".to_string(),
                idempotency_key: "resolve-b".to_string(),
                resolution: SyncConflictResolution::AcceptRemote,
                resolved_at_ms: 300,
            }),
            Err(SyncError::ConflictAlreadyResolved(id)) if id == "conflict-a"
        ));
    }
}
