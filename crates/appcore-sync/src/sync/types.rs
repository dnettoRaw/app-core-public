// =============================================================================
//        #######
//     ###       ###     F: types.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Shared sync roles, peer metadata, messages, and election contract.

use crate::sync::error::{SyncError, SyncResult};
use appcore_core::{NodeId, RuntimeIdentity};
use appcore_ops::Heartbeat;
use sha2::{Digest, Sha256};

/// Node role used by leader election and write routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeRole {
    /// The node currently owns write leadership.
    Leader,
    /// The node receives replicated state from a leader.
    Follower,
    /// The node is participating in a leadership transition.
    Candidate,
    /// The node can serve reads but must not accept writes.
    ReadOnly,
}

/// Coarse sync health and progress status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncStatus {
    /// No discovery or replication work is active.
    Idle,
    /// The runtime is resolving or selecting peers.
    DiscoveringPeers,
    /// A replication exchange is in progress.
    Replicating,
    /// Sync is available with the supplied degradation reason.
    Degraded(String),
    /// Sync has been intentionally stopped.
    Stopped,
}

/// Peer metadata used by sync orchestration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerInfo {
    /// Application-scoped runtime identity advertised by the peer.
    pub identity: RuntimeIdentity,
    /// Current synchronization role reported by the peer.
    pub role: NodeRole,
    /// Last observed peer timestamp in Unix epoch milliseconds.
    pub last_seen_ms: u64,
}

impl PeerInfo {
    /// Verifies that the peer belongs to the local application sync group.
    pub fn ensure_compatible_with(&self, local: &RuntimeIdentity) -> SyncResult<()> {
        if local.ensure_compatible(&self.identity).is_ok() {
            return Ok(());
        }
        Err(SyncError::IncompatiblePeer)
    }
}

/// Transport-neutral heartbeat message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatMessage {
    /// Node emitting the heartbeat.
    pub node_id: NodeId,
    /// Heartbeat creation time in Unix epoch milliseconds.
    pub timestamp_ms: u64,
}

impl From<Heartbeat> for HeartbeatMessage {
    fn from(value: Heartbeat) -> Self {
        Self {
            node_id: value.node_id,
            timestamp_ms: value.timestamp_ms,
        }
    }
}

/// Leader election contract.
pub trait LeaderElection {
    /// Returns the node's current election role.
    fn current_role(&self) -> NodeRole;
    /// Records a vote for `candidate`.
    fn vote(&mut self, candidate: &NodeId) -> SyncResult<()>;
    /// Transitions the local node to leader when election rules permit it.
    fn become_leader(&mut self) -> SyncResult<()>;
}

/// Computes the SHA-256 hash of events payload deterministically including size prefixes and batch metadata.
pub fn compute_events_hash(
    batch_id: &str,
    source_node_id: &NodeId,
    sequence_start: u64,
    sequence_end: u64,
    created_at_ms: u64,
    previous_batch_hash: Option<&str>,
    events: &[Vec<u8>],
) -> String {
    let mut hasher = Sha256::new();

    fn write_prefixed(hasher: &mut Sha256, bytes: &[u8]) {
        hasher.update((bytes.len() as u32).to_be_bytes());
        hasher.update(bytes);
    }

    // Metadata
    write_prefixed(&mut hasher, batch_id.as_bytes());
    write_prefixed(&mut hasher, source_node_id.as_str().as_bytes());
    hasher.update(sequence_start.to_be_bytes());
    hasher.update(sequence_end.to_be_bytes());
    hasher.update(created_at_ms.to_be_bytes());

    if let Some(prev) = previous_batch_hash {
        hasher.update([1u8]);
        write_prefixed(&mut hasher, prev.as_bytes());
    } else {
        hasher.update([0u8]);
    }

    // Size-prefixed events
    for event in events {
        write_prefixed(&mut hasher, event);
    }

    let result = hasher.finalize();
    let mut hex = String::with_capacity(result.len() * 2);
    for byte in result {
        hex.push_str(&format!("{:02x}", byte));
    }
    hex
}

/// Mensagem de replicação enviada do líder para o seguidor.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SyncMessage {
    /// Idempotency identifier for this replication batch.
    pub batch_id: String,
    /// Node that produced the batch.
    pub source_node_id: NodeId,
    /// Inclusive sequence of the first event in the batch.
    pub sequence_start: u64,
    /// Inclusive sequence of the final event in the batch.
    pub sequence_end: u64,
    /// Declared event count, validated against `events`.
    pub event_count: usize,
    /// SHA-256 integrity hash over metadata and event bytes.
    pub events_hash: String,
    /// Batch creation time in Unix epoch milliseconds.
    pub created_at_ms: u64,
    /// Hash of the preceding accepted batch, when one exists.
    pub previous_batch_hash: Option<String>,
    /// Opaque serialized event payloads in sequence order.
    pub events: Vec<Vec<u8>>,
}

impl SyncMessage {
    /// Builds a batch and computes its count and integrity hash.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        batch_id: String,
        source_node_id: NodeId,
        sequence_start: u64,
        sequence_end: u64,
        created_at_ms: u64,
        previous_batch_hash: Option<String>,
        events: Vec<Vec<u8>>,
    ) -> Self {
        let event_count = events.len();
        let events_hash = compute_events_hash(
            &batch_id,
            &source_node_id,
            sequence_start,
            sequence_end,
            created_at_ms,
            previous_batch_hash.as_deref(),
            &events,
        );
        Self {
            batch_id,
            source_node_id,
            sequence_start,
            sequence_end,
            event_count,
            events_hash,
            created_at_ms,
            previous_batch_hash,
            events,
        }
    }

    /// Builds an unchained batch with a deterministic identifier and zero timestamp.
    pub fn new_simple(source_node_id: NodeId, sequence: u64, events: Vec<Vec<u8>>) -> Self {
        let batch_id = format!("batch-{}-{}", source_node_id.as_str(), sequence);
        let event_count = events.len();
        let sequence_end = if event_count > 0 {
            sequence.saturating_add((event_count as u64).saturating_sub(1))
        } else {
            sequence
        };
        Self::new(
            batch_id,
            source_node_id,
            sequence,
            sequence_end,
            0,
            None,
            events,
        )
    }
}
