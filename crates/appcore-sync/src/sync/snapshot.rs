// =============================================================================
//        #######
//     ###       ###     F: snapshot.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Versioned replication snapshot contract and integrity validation.

use crate::sync::codec::bytes_to_hex;
use crate::sync::error::{SyncError, SyncResult};
use crate::sync::log::{
    replication_record_hash, validate_record_count, validate_record_size, ReplicationRecord,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

/// Stable format version for portable replication snapshots.
pub const SYNC_SNAPSHOT_FORMAT_V1: u16 = 1;

/// Sequence-addressed event stored in a replication snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicationSnapshotRecord {
    /// Source replication sequence.
    pub sequence: u64,
    /// Opaque serialized event bytes.
    pub payload: Vec<u8>,
}

/// Integrity-protected portable image of a replication log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicationSnapshot {
    /// Snapshot encoding version.
    pub format_version: u16,
    /// One-based index of the final record.
    pub last_index: u64,
    /// Replication records in log order.
    pub records: Vec<ReplicationSnapshotRecord>,
    /// SHA-256 checksum over version, index, sequences, and payloads.
    pub checksum: String,
}

impl ReplicationSnapshot {
    /// Creates a validated snapshot while taking ownership of every payload.
    ///
    /// Payload allocations are moved into the snapshot instead of cloned. The
    /// same record-count, record-size, sequence-uniqueness and checksum
    /// contracts used by durable replication logs are applied.
    pub fn try_from_records(records: impl IntoIterator<Item = (u64, Vec<u8>)>) -> SyncResult<Self> {
        snapshot_from_payloads(records.into_iter().map(Ok))
    }

    /// Validates this snapshot without cloning record payloads.
    pub fn validate(&self) -> SyncResult<()> {
        validate_snapshot_contract(self)
    }
}

pub(super) fn snapshot_from_records(records: &[ReplicationRecord]) -> ReplicationSnapshot {
    let records = records
        .iter()
        .map(|record| ReplicationSnapshotRecord {
            sequence: record.sequence,
            payload: record.payload.clone(),
        })
        .collect::<Vec<_>>();
    let last_index = records.len() as u64;
    let checksum = snapshot_checksum(SYNC_SNAPSHOT_FORMAT_V1, last_index, &records);
    ReplicationSnapshot {
        format_version: SYNC_SNAPSHOT_FORMAT_V1,
        last_index,
        records,
        checksum,
    }
}

pub(super) fn validate_snapshot(
    snapshot: &ReplicationSnapshot,
) -> SyncResult<Vec<ReplicationRecord>> {
    validate_snapshot_contract(snapshot)?;
    snapshot_records(snapshot)
}

pub(super) fn validate_snapshot_contract(snapshot: &ReplicationSnapshot) -> SyncResult<()> {
    if snapshot.format_version != SYNC_SNAPSHOT_FORMAT_V1 {
        return Err(SyncError::InvalidSnapshot("unsupported format version"));
    }
    if snapshot.last_index != snapshot.records.len() as u64 {
        return Err(SyncError::InvalidSnapshot("last index mismatch"));
    }
    validate_record_count(snapshot.records.len())?;
    if snapshot.checksum
        != snapshot_checksum(
            snapshot.format_version,
            snapshot.last_index,
            &snapshot.records,
        )
    {
        return Err(SyncError::InvalidSnapshot("checksum mismatch"));
    }
    let mut sequences = HashSet::new();
    for record in &snapshot.records {
        validate_record_size(&record.payload)?;
        if record.sequence > 0 && !sequences.insert(record.sequence) {
            return Err(SyncError::InvalidSnapshot("duplicate sequence"));
        }
    }
    Ok(())
}

pub(super) fn snapshot_from_payloads(
    records: impl Iterator<Item = SyncResult<(u64, Vec<u8>)>>,
) -> SyncResult<ReplicationSnapshot> {
    let mut output = Vec::new();
    let mut sequences = HashSet::new();
    for record in records {
        validate_record_count(output.len().saturating_add(1))?;
        let (sequence, payload) = record?;
        validate_record_size(&payload)?;
        if sequence > 0 && !sequences.insert(sequence) {
            return Err(SyncError::InvalidSnapshot("duplicate sequence"));
        }
        output.push(ReplicationSnapshotRecord { sequence, payload });
    }
    let last_index = output.len() as u64;
    let checksum = snapshot_checksum(SYNC_SNAPSHOT_FORMAT_V1, last_index, &output);
    Ok(ReplicationSnapshot {
        format_version: SYNC_SNAPSHOT_FORMAT_V1,
        last_index,
        records: output,
        checksum,
    })
}

fn snapshot_records(snapshot: &ReplicationSnapshot) -> SyncResult<Vec<ReplicationRecord>> {
    let mut sequences = HashSet::new();
    let mut records = Vec::with_capacity(snapshot.records.len());
    let mut previous_hash = String::new();
    for (offset, record) in snapshot.records.iter().enumerate() {
        validate_record_size(&record.payload)?;
        if record.sequence > 0 && !sequences.insert(record.sequence) {
            return Err(SyncError::InvalidSnapshot("duplicate sequence"));
        }
        let record_hash = replication_record_hash(&previous_hash, record.sequence, &record.payload);
        records.push(ReplicationRecord {
            index: offset + 1,
            sequence: record.sequence,
            payload: record.payload.clone(),
            previous_hash,
            record_hash: record_hash.clone(),
        });
        previous_hash = record_hash;
    }
    Ok(records)
}

pub(super) fn into_replication_records(
    snapshot: ReplicationSnapshot,
) -> SyncResult<Vec<ReplicationRecord>> {
    validate_snapshot_contract(&snapshot)?;
    let mut sequences = HashSet::new();
    let mut records = Vec::with_capacity(snapshot.records.len());
    let mut previous_hash = String::new();
    for (offset, record) in snapshot.records.into_iter().enumerate() {
        if record.sequence > 0 && !sequences.insert(record.sequence) {
            return Err(SyncError::InvalidSnapshot("duplicate sequence"));
        }
        validate_record_size(&record.payload)?;
        let record_hash = replication_record_hash(&previous_hash, record.sequence, &record.payload);
        records.push(ReplicationRecord {
            index: offset + 1,
            sequence: record.sequence,
            payload: record.payload,
            previous_hash,
            record_hash: record_hash.clone(),
        });
        previous_hash = record_hash;
    }
    Ok(records)
}

fn snapshot_checksum(
    format_version: u16,
    last_index: u64,
    records: &[ReplicationSnapshotRecord],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format_version.to_be_bytes());
    hasher.update(last_index.to_be_bytes());
    hasher.update((records.len() as u64).to_be_bytes());
    for record in records {
        hasher.update(record.sequence.to_be_bytes());
        hasher.update((record.payload.len() as u64).to_be_bytes());
        hasher.update(&record.payload);
    }
    bytes_to_hex(&hasher.finalize())
}
