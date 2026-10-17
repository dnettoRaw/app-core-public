// =============================================================================
//        #######
//     ###       ###     F: format.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 10:22:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 2.0.0-alpha.6
// =============================================================================

use super::{
    invalid, validate_identifier, CanonicalBatchV1 as Batch, CanonicalHeadV1, CanonicalStreamV1,
    MAX_CANONICAL_BATCHES,
};
use crate::sync::{compute_events_hash, SyncMessage, SyncResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct History {
    pub format_version: u16,
    pub stream: CanonicalStreamV1,
    pub batches: Vec<Batch>,
}

impl History {
    pub fn head(&self) -> CanonicalHeadV1 {
        CanonicalHeadV1 {
            stream: self.stream.clone(),
            sequence: self
                .batches
                .last()
                .map_or(0, |batch| batch.message.sequence_end),
            batch_hash: self
                .batches
                .last()
                .map(|batch| batch.message.events_hash.clone()),
            history_hash: self.batches.last().map(|batch| batch.digest.clone()),
        }
    }

    pub fn validate(&self, scope: &CanonicalStreamV1) -> SyncResult<()> {
        if self.format_version != 1 {
            return Err(invalid("NO MORE SUPPORTED PLEASE UPDATE"));
        }
        self.stream.validate()?;
        if &self.stream != scope || self.batches.len() > MAX_CANONICAL_BATCHES {
            return Err(invalid("canonical scope or capacity mismatch"));
        }
        let mut previous: Option<&Batch> = None;
        let mut event_count = 0usize;
        let mut payload_bytes = 0usize;
        let mut ids = std::collections::BTreeSet::new();
        for batch in &self.batches {
            validate_identifier(&batch.writer)?;
            validate_message(&batch.message)?;
            event_count = event_count.saturating_add(batch.message.events.len());
            for event in &batch.message.events {
                payload_bytes = payload_bytes.saturating_add(event.len());
            }
            if event_count > 16_384
                || payload_bytes > 4 * 1024 * 1024
                || !ids.insert(&batch.message.batch_id)
                || batch.epoch == 0
            {
                return Err(invalid("canonical history admission failed"));
            }
            validate_successor(previous, batch)?;
            if digest(&self.stream, previous, batch)? != batch.digest {
                return Err(invalid("canonical history digest mismatch"));
            }
            previous = Some(batch);
        }
        Ok(())
    }
}

pub(super) fn digest(
    scope: &CanonicalStreamV1,
    previous: Option<&Batch>,
    batch: &Batch,
) -> SyncResult<String> {
    let bytes = serde_json::to_vec(&(
        "appcore-canonical-history-v1",
        scope,
        previous.map(|batch| batch.digest.as_str()),
        batch.epoch,
        &batch.writer,
        &batch.message.events_hash,
    ))
    .map_err(|_| invalid("canonical digest encoding failed"))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn validate_successor(previous: Option<&Batch>, batch: &Batch) -> SyncResult<()> {
    let expected = match previous {
        Some(old) => old.message.sequence_end.checked_add(1),
        None => Some(1),
    };
    if expected != Some(batch.message.sequence_start)
        || batch.message.previous_batch_hash.as_deref()
            != previous.map(|old| old.message.events_hash.as_str())
        || previous.is_some_and(|old| {
            batch.epoch < old.epoch || (batch.epoch == old.epoch && batch.writer != old.writer)
        })
    {
        return Err(invalid("canonical sequence, hash or epoch discontinuity"));
    }
    Ok(())
}

fn validate_message(message: &SyncMessage) -> SyncResult<()> {
    if !crate::sync::types::is_valid_sync_batch_id(&message.batch_id)
        || message.events.is_empty()
        || message.events.len() > 10_000
        || message.event_count != message.events.len()
        || message.sequence_start == 0
        || message
            .sequence_start
            .checked_add(message.events.len() as u64 - 1)
            != Some(message.sequence_end)
    {
        return Err(invalid("invalid canonical batch structure"));
    }
    validate_identifier(message.source_node_id.as_str())?;
    for event in &message.events {
        crate::sync::log::validate_record_size(event)?;
    }
    if message.events_hash
        != compute_events_hash(
            &message.batch_id,
            &message.source_node_id,
            message.sequence_start,
            message.sequence_end,
            message.created_at_ms,
            message.previous_batch_hash.as_deref(),
            &message.events,
        )
    {
        return Err(invalid("invalid canonical batch hash"));
    }
    Ok(())
}
