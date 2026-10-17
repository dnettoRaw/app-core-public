// =============================================================================
//        #######
//     ###       ###     F: canonical.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 10:22:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 2.0.0-alpha.6
// =============================================================================

//! Bounded canonical history, independent of per-source receiver checkpoints.
//!
//! These are trusted-storage primitives, not an election or authentication layer.
//! Hosts authenticate replication and fence writer commits at the storage boundary.
mod durability;
mod file;
mod format;

use super::{SyncError, SyncResult};
use appcore_distributed_contracts::ServiceLeaderLease;
pub use durability::{
    decode_durability_receipt_v1, evaluate_durability_v1, DurabilityPolicyV1, DurabilityReceiptV1,
    DurabilityStatusV1, DurabilityTargetV1, MAX_DURABILITY_RECEIPTS, MAX_DURABILITY_RECEIPT_BYTES,
};
pub use file::{CanonicalHistoryTransactionV1, FileCanonicalHistoryV1};
use serde::{Deserialize, Serialize};

/// Maximum encoded canonical history size (16 MiB).
pub const MAX_CANONICAL_HISTORY_BYTES: usize = 16 * 1024 * 1024;
/// Maximum retained batches; reaching this limit stops writes, never evicts evidence.
pub const MAX_CANONICAL_BATCHES: usize = 1_024;

/// Stable logical identity, independent of the current writer or network address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalStreamV1 {
    /// Tenant selected by the authenticated deployment.
    pub tenant_id: String,
    /// Cluster selected by the authenticated deployment.
    pub cluster_id: String,
    /// Service whose authority owns this history.
    pub service_id: String,
}

impl CanonicalStreamV1 {
    /// Extracts and validates a scope. This does not authenticate or check the lease.
    pub fn from_lease(lease: &ServiceLeaderLease) -> SyncResult<Self> {
        let scope = Self {
            tenant_id: lease.tenant_id.as_str().into(),
            cluster_id: lease.cluster_id.as_str().into(),
            service_id: lease.service_id.as_str().into(),
        };
        scope.validate()?;
        Ok(scope)
    }

    pub(super) fn validate(&self) -> SyncResult<()> {
        for value in [&self.tenant_id, &self.cluster_id, &self.service_id] {
            validate_identifier(value)?;
        }
        Ok(())
    }
}

/// Transferable original batch and its canonical lineage. Authenticate before ingestion.
/// Integrity digests are not signatures or proof of authority.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalBatchV1 {
    /// Original writer's service epoch.
    pub epoch: u64,
    /// Original writer's Core identity.
    pub writer: String,
    /// Original batch metadata and opaque event bytes.
    pub message: crate::sync::SyncMessage,
    /// Canonical digest binding scope, previous digest, epoch, writer and batch hash.
    pub digest: String,
}

impl std::fmt::Debug for CanonicalBatchV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CanonicalBatchV1")
            .field("epoch", &self.epoch)
            .finish_non_exhaustive()
    }
}

/// Exact durable head. Both digests and the logical scope participate in equality.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalHeadV1 {
    /// Stable stream identity.
    pub stream: CanonicalStreamV1,
    /// Last event sequence, zero only for an empty history.
    pub sequence: u64,
    /// Original SyncMessage digest, absent only at genesis.
    pub batch_hash: Option<String>,
    /// Digest binding the stream, epoch, writer and complete batch chain.
    pub history_hash: Option<String>,
}

pub(super) fn invalid(reason: &'static str) -> SyncError {
    SyncError::InvalidSyncMessage(reason)
}

pub(super) fn validate_identifier(value: &str) -> SyncResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    {
        return Err(invalid("invalid canonical identity"));
    }
    Ok(())
}
