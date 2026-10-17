// =============================================================================
//        #######
//     ###       ###     F: durability.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 10:22:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 2.0.0-alpha.6
// =============================================================================

//! Canonical-copy confirmation; authenticate receipt issuers before evaluation.
//! This contract neither authenticates JSON nor acknowledges application effects.
use super::{invalid, validate_identifier, CanonicalHeadV1};
use crate::sync::{types::is_valid_sync_batch_id, SyncResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Maximum encoded receipt size, checked before JSON allocation.
pub const MAX_DURABILITY_RECEIPT_BYTES: usize = 16 * 1024;
/// Maximum evidence items admitted in one decision; duplicates consume slots too.
pub const MAX_DURABILITY_RECEIPTS: usize = 16;

/// Immutable batch boundary whose canonical bytes must be durable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurabilityTargetV1 {
    /// Original canonical batch ID, not a new request ID on retry.
    pub batch_id: String,
    /// Exact original batch boundary, including both chain digests and service.
    pub head: CanonicalHeadV1,
    /// Original writer's service epoch, not current discovery state.
    pub writer_epoch: u64,
    /// Original writer's Core, used by the master requirement.
    pub writer_core_id: String,
}

impl DurabilityTargetV1 {
    /// Validates structure, not authentication, storage or current leadership.
    pub fn validate(&self) -> SyncResult<()> {
        self.head.stream.validate()?;
        validate_identifier(&self.writer_core_id)?;
        if !is_valid_sync_batch_id(&self.batch_id)
            || self.writer_epoch == 0
            || self.head.sequence == 0
        {
            return Err(invalid("invalid durability target"));
        }
        for hash in [&self.head.batch_hash, &self.head.history_hash] {
            if !hash.as_ref().is_some_and(|hash| {
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            }) {
                return Err(invalid("invalid durability head digest"));
            }
        }
        Ok(())
    }
}

/// Explicit confirmation policy. No timeout or outage may silently weaken it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DurabilityPolicyV1 {
    /// Require a canonical copy on the explicitly selected local Core.
    /// This does not describe a pre-canonical local outbox or application transaction.
    Local {
        /// Deployment-owned local storage identity.
        core_id: String,
    },
    /// Require the original writer's durable canonical copy.
    Master,
    /// Require the original writer and one explicitly selected, distinct replica Core.
    MasterAndReplica {
        /// Replica selected by trusted deployment policy, never by receipt count alone.
        replica_core_id: String,
    },
}

/// Reconstructible receipt for one durable canonical copy, not a transport ACK.
/// Authenticate the entire body and bind persisted_by_core_id to the sender's identity.
/// Different Core IDs alone do not prove independent disks or failure domains.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurabilityReceiptV1 {
    /// Must be 1; unknown versions hit the update wall.
    pub schema_version: u16,
    /// Exact immutable operation/batch target.
    pub target: DurabilityTargetV1,
    /// Authenticated storage owner provisioned by the deployment.
    pub persisted_by_core_id: String,
}

impl DurabilityReceiptV1 {
    /// Checks shape only; caller must verify issuer identity and authenticity separately.
    pub fn validate(&self) -> SyncResult<()> {
        if self.schema_version != 1 {
            return Err(invalid("NO MORE SUPPORTED PLEASE UPDATE"));
        }
        self.target.validate()?;
        validate_identifier(&self.persisted_by_core_id)
    }
}

/// Structured result for a fixed target and policy. Pending is not a write rollback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurabilityStatusV1 {
    /// Policy evaluated without downgrade.
    pub policy: DurabilityPolicyV1,
    /// Target to preserve for retries/recovery and, when trusted, promotion requirements.
    pub target: DurabilityTargetV1,
    /// True only if every required Core supplied matching authenticated evidence.
    pub confirmed: bool,
    /// Sorted required Cores whose copies are confirmed (at most two).
    pub persisted_core_ids: Vec<String>,
    /// Sorted required Cores without matching evidence (at most two).
    pub missing_core_ids: Vec<String>,
}

/// Evaluates at most 16 already-authenticated receipts without I/O, waiting or retained state.
/// Identical duplicates are idempotent; mismatched targets fail rather than mixing heads.
/// Unselected Cores cannot satisfy a requirement. Persist the selected policy/target and
/// authenticated confirmations in the deployment's recovery boundary before reporting success.
/// A confirmed historical receipt is not evidence of current availability or leadership.
pub fn evaluate_durability_v1(
    target: &DurabilityTargetV1,
    policy: &DurabilityPolicyV1,
    receipts: &[DurabilityReceiptV1],
) -> SyncResult<DurabilityStatusV1> {
    target.validate()?;
    if receipts.len() > MAX_DURABILITY_RECEIPTS {
        return Err(invalid("too many durability receipts"));
    }
    let required = required_cores(target, policy)?;
    let mut observed = BTreeSet::new();
    for receipt in receipts {
        receipt.validate()?;
        if &receipt.target != target {
            return Err(invalid("durability receipt target mismatch"));
        }
        observed.insert(receipt.persisted_by_core_id.as_str());
    }
    let (persisted, missing): (Vec<_>, Vec<_>) = required
        .into_iter()
        .partition(|core| observed.contains(core.as_str()));
    Ok(DurabilityStatusV1 {
        policy: policy.clone(),
        target: target.clone(),
        confirmed: missing.is_empty(),
        persisted_core_ids: persisted,
        missing_core_ids: missing,
    })
}

fn required_cores(
    target: &DurabilityTargetV1,
    policy: &DurabilityPolicyV1,
) -> SyncResult<BTreeSet<String>> {
    let mut cores = BTreeSet::new();
    match policy {
        DurabilityPolicyV1::Local { core_id } => {
            validate_identifier(core_id)?;
            cores.insert(core_id.clone());
        }
        DurabilityPolicyV1::Master => {
            cores.insert(target.writer_core_id.clone());
        }
        DurabilityPolicyV1::MasterAndReplica { replica_core_id } => {
            validate_identifier(replica_core_id)?;
            if replica_core_id == &target.writer_core_id {
                return Err(invalid("durability replica must differ from writer"));
            }
            cores.insert(target.writer_core_id.clone());
            cores.insert(replica_core_id.clone());
        }
    }
    Ok(cores)
}

/// Decodes bounded V1 receipt JSON. Decoding does not authenticate the issuer.
pub fn decode_durability_receipt_v1(bytes: &[u8]) -> SyncResult<DurabilityReceiptV1> {
    if bytes.len() > MAX_DURABILITY_RECEIPT_BYTES {
        return Err(invalid("durability receipt size exceeded"));
    }
    #[derive(Deserialize)]
    struct Version {
        #[serde(default)]
        schema_version: u16,
    }
    let version: Version =
        serde_json::from_slice(bytes).map_err(|_| invalid("invalid durability receipt JSON"))?;
    if version.schema_version != 1 {
        return Err(invalid("NO MORE SUPPORTED PLEASE UPDATE"));
    }
    let receipt: DurabilityReceiptV1 =
        serde_json::from_slice(bytes).map_err(|_| invalid("invalid durability receipt JSON"))?;
    receipt.validate()?;
    Ok(receipt)
}
