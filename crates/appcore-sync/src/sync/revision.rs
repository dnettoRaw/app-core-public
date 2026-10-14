// =============================================================================
//        #######
//     ###       ###     F: revision.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/12 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/12 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Bounded structural causal evidence for opaque object revisions.
//!
//! Deployment must authenticate each event and bind it to one tenant before
//! constructing a chain. Structural validation does not authenticate evidence.
//! Content equality, timestamps and business schemas never decide ancestry.

use super::{SyncError, SyncResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Maximum revisions inspected in one causal decision.
pub const MAX_REVISION_CHAIN_V1: usize = 1_024;

/// Opaque revision identity, independent of document schema or content digest.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectRevisionV1 {
    /// Stable tenant-scoped opaque object identity, at most 128 bytes.
    pub object_id: String,
    /// Unique immutable revision identity, at most 128 bytes.
    pub revision_id: String,
    /// Direct parent; absent only for the initial revision.
    pub base_revision_id: Option<String>,
    /// Unique authenticated event identity, at most 128 bytes.
    pub event_id: String,
}

impl std::fmt::Debug for ObjectRevisionV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjectRevisionV1").finish_non_exhaustive()
    }
}

impl ObjectRevisionV1 {
    /// Validates identity bounds without revealing their contents in errors.
    pub fn validate(&self) -> SyncResult<()> {
        for id in [&self.object_id, &self.revision_id, &self.event_id] {
            validate_id(id)?;
        }
        if let Some(base) = &self.base_revision_id {
            validate_id(base)?;
            if base == &self.revision_id {
                return invalid();
            }
        }
        Ok(())
    }
}

/// Structural chain supplied oldest-first after envelope/tenant authentication.
/// Missing earlier history is permitted; missing links inside the chain are not.
#[derive(Clone)]
pub struct RevisionChainV1 {
    revisions: Vec<ObjectRevisionV1>,
}

impl RevisionChainV1 {
    /// Checks the bounded chain, object binding, unique events and parent links.
    pub fn new(revisions: Vec<ObjectRevisionV1>) -> SyncResult<Self> {
        if revisions.is_empty() || revisions.len() > MAX_REVISION_CHAIN_V1 {
            return invalid();
        }
        let mut ids = BTreeSet::new();
        let mut events = BTreeSet::new();
        for (index, revision) in revisions.iter().enumerate() {
            revision.validate()?;
            if revision.object_id != revisions[0].object_id
                || !ids.insert(&revision.revision_id)
                || !events.insert(&revision.event_id)
                || revisions[..index].iter().any(|previous| {
                    previous.base_revision_id.as_ref() == Some(&revision.revision_id)
                })
            {
                return invalid();
            }
            if index > 0
                && revision.base_revision_id.as_ref() != Some(&revisions[index - 1].revision_id)
            {
                return invalid();
            }
        }
        Ok(Self { revisions })
    }

    fn position(&self, revision: &ObjectRevisionV1) -> SyncResult<Option<usize>> {
        let position = self.revisions.iter().position(|entry| {
            entry.revision_id == revision.revision_id || entry.event_id == revision.event_id
        });
        if position.is_some_and(|index| self.revisions[index] != *revision) {
            return invalid();
        }
        Ok(position)
    }
}

/// Payload-free causal conclusion. It never performs materialization or merging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RevisionDecisionV1 {
    /// The authenticated immutable event is already applied.
    AlreadyApplied,
    /// Remote is a proven ancestor; retain the newer local revision.
    AncestorEcho,
    /// Remote descends from local through explicit evidence.
    Applicable,
    /// Both revisions directly descend from the same known parent.
    ConcurrentBranch,
    /// Evidence does not establish a relation; request bounded history/snapshot.
    SnapshotRequired,
}

/// Classifies two authenticated object revisions using explicit causal evidence.
/// Advance the transport checkpoint independently when an ancestor echo is accepted.
/// An intentional revert must have a fresh revision/event descended from local.
pub fn decide_object_revision_v1(
    local: &ObjectRevisionV1,
    remote: &ObjectRevisionV1,
    chain: &RevisionChainV1,
) -> SyncResult<RevisionDecisionV1> {
    local.validate()?;
    remote.validate()?;
    if local.object_id != remote.object_id || local.object_id != chain.revisions[0].object_id {
        return invalid();
    }
    let left = chain.position(local)?;
    let right = chain.position(remote)?;
    if local.revision_id == remote.revision_id || local.event_id == remote.event_id {
        return if local == remote {
            Ok(RevisionDecisionV1::AlreadyApplied)
        } else {
            invalid()
        };
    }
    if let (Some(left), Some(right)) = (left, right) {
        return Ok(if right < left {
            RevisionDecisionV1::AncestorEcho
        } else {
            RevisionDecisionV1::Applicable
        });
    }
    if remote.base_revision_id.as_ref() == Some(&local.revision_id) {
        return Ok(RevisionDecisionV1::Applicable);
    }
    if local.base_revision_id.is_some() && local.base_revision_id == remote.base_revision_id {
        return Ok(RevisionDecisionV1::ConcurrentBranch);
    }
    Ok(RevisionDecisionV1::SnapshotRequired)
}

/// Rejects a checkpoint beyond the authenticated observed head before any mutation.
pub fn validate_checkpoint_head_v1(checkpoint: u64, observed_head: u64) -> SyncResult<()> {
    if checkpoint > observed_head {
        return Err(SyncError::InvalidSequence(checkpoint));
    }
    Ok(())
}

fn validate_id(id: &str) -> SyncResult<()> {
    if id.is_empty() || id.len() > 128 || id.chars().any(char::is_control) {
        return invalid();
    }
    Ok(())
}

fn invalid<T>() -> SyncResult<T> {
    Err(SyncError::InvalidSyncMessage(
        "invalid object revision evidence",
    ))
}
