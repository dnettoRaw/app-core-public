// =============================================================================
//        #######
//     ###       ###     F: outbox_journal_view.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

//! Bounded payload views over file outbox journal state.

use crate::sync::error::{SyncError, SyncResult};
use crate::sync::outbox::{SyncOutboxReceipt, SyncOutboxStats};
use crate::sync::outbox_journal::PendingMessage;
use crate::sync::types::SyncMessage;
use std::collections::VecDeque;

pub(super) fn page(
    messages: &VecDeque<PendingMessage>,
    limit: usize,
    max_bytes: usize,
    ready_at_ms: Option<u64>,
) -> Vec<SyncMessage> {
    let mut page = Vec::new();
    let mut bytes = 0usize;
    for pending in messages.iter().take(limit) {
        if ready_at_ms.is_some_and(|now| pending.next_ready_at_ms > now)
            || bytes
                .checked_add(pending.encoded_bytes)
                .is_none_or(|total| total > max_bytes)
        {
            break;
        }
        bytes += pending.encoded_bytes;
        page.push(pending.message.clone());
    }
    page
}

pub(super) fn stats(messages: &VecDeque<PendingMessage>) -> SyncResult<SyncOutboxStats> {
    let pending_bytes = messages.iter().try_fold(0usize, |total, pending| {
        total
            .checked_add(pending.encoded_bytes)
            .ok_or(SyncError::InvalidSyncMessage("outbox byte overflow"))
    })?;
    let total_attempts = messages.iter().try_fold(0u64, |total, pending| {
        total
            .checked_add(u64::from(pending.attempts))
            .ok_or(SyncError::InvalidSyncMessage("outbox attempt overflow"))
    })?;
    Ok(SyncOutboxStats {
        pending_messages: messages.len(),
        pending_bytes: Some(pending_bytes),
        attempted_messages: Some(
            messages
                .iter()
                .filter(|pending| pending.attempts > 0)
                .count(),
        ),
        total_attempts: Some(total_attempts),
        next_ready_at_ms: messages.front().map(|pending| pending.next_ready_at_ms),
    })
}

pub(super) fn validate_receipt_prefix(
    messages: &VecDeque<PendingMessage>,
    receipt: &SyncOutboxReceipt,
) -> SyncResult<()> {
    if messages.len() < receipt.batch_ids().len()
        || messages
            .iter()
            .zip(receipt.batch_ids())
            .any(|(pending, batch_id)| pending.message.batch_id != *batch_id)
    {
        return Err(SyncError::InvalidSyncMessage(
            "outbox acknowledgement mismatch",
        ));
    }
    Ok(())
}
