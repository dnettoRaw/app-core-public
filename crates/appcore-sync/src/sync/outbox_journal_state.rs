// =============================================================================
//        #######
//     ###       ###     F: outbox_journal_state.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

//! Compact operations emitted while scanning the durable outbox journal.

use crate::sync::outbox_format::HASH_BYTES;
use std::sync::Arc;

pub(super) enum JournalOperation {
    Enqueue {
        batch_id: Arc<str>,
        ordinal: u64,
        data_offset: u64,
        data_digest: [u8; HASH_BYTES],
        encoded_bytes: usize,
        frame_bytes: u64,
    },
    Acknowledge {
        count: usize,
    },
    Attempt {
        attempts: u32,
        next_ready_at_ms: u64,
        frame_bytes: u64,
    },
}

pub(super) struct ScanPending {
    pub(super) batch_id: Arc<str>,
    pub(super) attempts: u32,
}

impl ScanPending {
    pub(super) fn new(batch_id: Arc<str>, attempts: u32) -> Self {
        Self { batch_id, attempts }
    }
}

pub(super) struct ScanResult {
    pub(super) operations: Vec<JournalOperation>,
    pub(super) scanned_bytes: u64,
    pub(super) record_count: u64,
    pub(super) chain_head: [u8; HASH_BYTES],
    pub(super) recovered_tail: bool,
}
