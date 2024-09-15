// =============================================================================
//        #######
//     ###       ###     F: receiver.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Follower receiver state and acknowledgements.

use crate::sync::checkpoint::SyncCheckpointStore;
use crate::sync::error::{SyncError, SyncResult};
use crate::sync::log::{validate_record_size, ReplicationLog};
use crate::sync::types::SyncMessage;
use crate::sync::wire::SyncEnvelopeV1;
use appcore_core::CoreIdentity;
use parking_lot::Mutex;
use std::sync::Arc;

const DEFAULT_MAX_EVENTS: usize = 10_000;

#[derive(Debug, Clone, Default)]
struct ProcessedBatches {
    set: std::collections::HashSet<String>,
    queue: std::collections::VecDeque<String>,
}

/// In-memory receiver state for follower sync endpoint.
#[derive(Clone)]
pub struct SyncReceiverState {
    replication_log: Arc<Mutex<Box<dyn ReplicationLog + Send>>>,
    checkpoint_store: Arc<dyn SyncCheckpointStore>,
    processed_batches: Arc<Mutex<ProcessedBatches>>,
    local_identity: Option<CoreIdentity>,
}

/// Ack returned by `/v1/sync/events`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncReceiveAck {
    /// Whether the batch passed validation and checkpoint rules.
    pub accepted: bool,
    /// Number of newly appended events.
    pub received: usize,
    /// Number of idempotently skipped events.
    pub skipped: usize,
    /// Final source sequence represented by the accepted batch.
    pub last_sequence: u64,
}

impl SyncReceiverState {
    /// Creates receiver state backed by a replication log and checkpoint store.
    pub fn new(
        replication_log: Arc<Mutex<Box<dyn ReplicationLog + Send>>>,
        checkpoint_store: Arc<dyn SyncCheckpointStore>,
    ) -> Self {
        Self {
            replication_log,
            checkpoint_store,
            processed_batches: Arc::new(Mutex::new(ProcessedBatches::default())),
            local_identity: None,
        }
    }

    /// Configures the distributed identity used to validate v1 sync envelopes.
    pub fn with_local_identity(mut self, local_identity: CoreIdentity) -> Self {
        self.local_identity = Some(local_identity);
        self
    }

    /// Returns a shared handle to the receiver's replication log.
    pub fn replication_log(&self) -> Arc<Mutex<Box<dyn ReplicationLog + Send>>> {
        Arc::clone(&self.replication_log)
    }

    /// Validates and idempotently applies one already-authenticated batch.
    pub fn apply_sync_message(&self, message: &SyncMessage) -> SyncResult<SyncReceiveAck> {
        self.validate_message(message)?;

        let peer_id = message.source_node_id.as_str();
        let last_sequence = self.checkpoint_store.get_last_sequence(peer_id)?;

        // Skip completely duplicate old sequence ranges
        if message.sequence_end <= last_sequence {
            return Ok(SyncReceiveAck {
                accepted: true,
                received: 0,
                skipped: message.events.len(),
                last_sequence,
            });
        }

        // New data must cover the next sequence; verified overlap is allowed for
        // recovery when the sender loses its outbound cursor after a successful send.
        let next_sequence = last_sequence
            .checked_add(1)
            .ok_or(SyncError::InvalidSyncMessage(
                "checkpoint sequence is exhausted",
            ))?;
        if message.sequence_start > next_sequence {
            return Err(SyncError::InvalidSequence(message.sequence_start));
        }

        // Verify previous batch hash chain
        if last_sequence > 0 && message.sequence_start == next_sequence {
            if let Some((_, last_hash)) = self.checkpoint_store.get_checkpoint(peer_id)? {
                if !last_hash.is_empty() {
                    let prev_hash = message.previous_batch_hash.as_deref().unwrap_or("");
                    if prev_hash != last_hash {
                        return Err(SyncError::InvalidSyncMessage(
                            "previous batch hash mismatch",
                        ));
                    }
                }
            }
        }

        let (received, skipped) = self.append_events_to_log(message, peer_id, last_sequence)?;
        self.record_processed_batch(message.batch_id.clone());

        Ok(SyncReceiveAck {
            accepted: true,
            received,
            skipped,
            last_sequence: message.sequence_end,
        })
    }

    /// Validates a decoded wire envelope before applying its replication batch.
    pub fn apply_sync_envelope(&self, envelope: &SyncEnvelopeV1) -> SyncResult<SyncReceiveAck> {
        let local_identity = self
            .local_identity
            .as_ref()
            .ok_or(SyncError::InvalidSyncMessage(
                "local sync identity is not configured",
            ))?;
        envelope.validate_for(local_identity)?;
        self.apply_sync_message(&envelope.message)
    }

    fn validate_message(&self, message: &SyncMessage) -> SyncResult<()> {
        if message.sequence_start == 0 {
            return Err(SyncError::InvalidSequence(message.sequence_start));
        }
        if message.events.is_empty() || message.event_count == 0 {
            return Err(SyncError::InvalidSyncMessage("empty batch events"));
        }
        if message.event_count != message.events.len() {
            return Err(SyncError::InvalidSyncMessage("event count mismatch"));
        }
        if message.events.len() > DEFAULT_MAX_EVENTS {
            return Err(SyncError::TooManyEvents {
                count: message.events.len(),
                max: DEFAULT_MAX_EVENTS,
            });
        }
        for event in &message.events {
            validate_record_size(event)?;
        }
        if message.sequence_start > message.sequence_end {
            return Err(SyncError::InvalidSyncMessage("invalid sequence range"));
        }
        let event_count = u64::try_from(message.event_count)
            .map_err(|_| SyncError::InvalidSyncMessage("event count exceeds sequence range"))?;
        let expected_end = message
            .sequence_start
            .checked_add(event_count.saturating_sub(1))
            .ok_or(SyncError::InvalidSyncMessage("sequence range overflow"))?;
        if message.sequence_end != expected_end {
            return Err(SyncError::InvalidSyncMessage("inconsistent sequence range"));
        }
        let computed_hash = crate::sync::types::compute_events_hash(
            &message.batch_id,
            &message.source_node_id,
            message.sequence_start,
            message.sequence_end,
            message.created_at_ms,
            message.previous_batch_hash.as_deref(),
            &message.events,
        );
        if computed_hash != message.events_hash {
            return Err(SyncError::InvalidSyncMessage("invalid events hash"));
        }
        {
            let processed = self.processed_batches.lock();
            if processed.set.contains(&message.batch_id) {
                return Err(SyncError::InvalidSyncMessage("duplicate batch_id"));
            }
        }
        Ok(())
    }

    fn append_events_to_log(
        &self,
        message: &SyncMessage,
        peer_id: &str,
        last_sequence: u64,
    ) -> SyncResult<(usize, usize)> {
        let mut guard = self.replication_log.lock();
        let mut received = 0usize;
        let mut skipped = 0usize;
        for (index, event) in message.events.iter().enumerate() {
            let offset = u64::try_from(index)
                .map_err(|_| SyncError::InvalidSyncMessage("event index exceeds sequence range"))?;
            let seq = message
                .sequence_start
                .checked_add(offset)
                .ok_or(SyncError::InvalidSyncMessage("sequence range overflow"))?;
            match guard.event_at_sequence(seq)? {
                Some(existing) if existing == *event => skipped += 1,
                Some(_) => return Err(SyncError::SequenceConflict(seq)),
                None if seq <= last_sequence => return Err(SyncError::InvalidSequence(seq)),
                None => {
                    received += 1;
                    let _ = guard.append_with_sequence(event.clone(), seq)?;
                }
            }
        }
        self.checkpoint_store.set_checkpoint(
            peer_id,
            message.sequence_end,
            &message.events_hash,
        )?;
        Ok((received, skipped))
    }

    fn record_processed_batch(&self, batch_id: String) {
        let mut processed = self.processed_batches.lock();
        processed.set.insert(batch_id.clone());
        processed.queue.push_back(batch_id);
        if processed.set.len() > 10_000 {
            if let Some(oldest) = processed.queue.pop_front() {
                processed.set.remove(&oldest);
            }
        }
    }
}
