// =============================================================================
//        #######
//     ###       ###     F: patch_log.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Retains bounded undo and redo snapshots without duplicate document clones.

use std::collections::VecDeque;
use std::mem;
use std::sync::Arc;

use crate::{DocumentIr, ErrorCode, FileMakerError, Patch, PatchTransaction, Result};

/// Bounded in-memory operation log providing explicit undo and redo.
pub struct OperationLog {
    max_entries: usize,
    max_bytes: usize,
    used_bytes: usize,
    undo: VecDeque<HistoryEntry>,
    redo: VecDeque<HistoryEntry>,
}

struct HistoryEntry {
    document: Arc<DocumentIr>,
    bytes: usize,
}

impl OperationLog {
    /// Default aggregate serialized snapshot budget for undo and redo.
    pub const DEFAULT_MAX_BYTES: usize = 64 * 1024 * 1024;

    /// Creates a log with a non-zero entry bound and default byte budget.
    pub fn new(max_entries: usize) -> Result<Self> {
        Self::new_bounded(max_entries, Self::DEFAULT_MAX_BYTES)
    }

    /// Creates a log bounded by both entries and aggregate snapshot bytes.
    pub fn new_bounded(max_entries: usize, max_bytes: usize) -> Result<Self> {
        if max_entries == 0 || max_bytes == 0 {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                "operation log entry and byte bounds must be non-zero",
            ));
        }
        Ok(Self {
            max_entries,
            max_bytes,
            used_bytes: 0,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
        })
    }

    /// Applies one atomic patch and records the previous document on success.
    pub fn apply(
        &mut self,
        document: &mut DocumentIr,
        patch: &Patch,
        max_operations: usize,
    ) -> Result<()> {
        let bytes = crate::memory::serialized_size(document)?;
        self.ensure_snapshot_fits(bytes)?;
        let previous =
            PatchTransaction::new(document, max_operations).apply_with_rollback_snapshot(patch)?;
        self.clear_redo();
        self.push_undo(HistoryEntry {
            document: Arc::new(previous),
            bytes,
        });
        Ok(())
    }

    /// Restores the previous successful document state.
    pub fn undo(&mut self, document: &mut DocumentIr) -> Result<()> {
        let current_bytes = crate::memory::serialized_size(document)?;
        self.ensure_snapshot_fits(current_bytes)?;
        let previous = self.undo.pop_back().ok_or_else(|| {
            FileMakerError::new(ErrorCode::PatchInvalid, "operation log has no undo entry")
        })?;
        self.used_bytes = self.used_bytes.saturating_sub(previous.bytes);
        let current = mem::replace(document, unwrap_snapshot(previous.document));
        self.push_redo(HistoryEntry {
            document: Arc::new(current),
            bytes: current_bytes,
        });
        Ok(())
    }

    /// Reapplies the most recently undone document state.
    pub fn redo(&mut self, document: &mut DocumentIr) -> Result<()> {
        let current_bytes = crate::memory::serialized_size(document)?;
        self.ensure_snapshot_fits(current_bytes)?;
        let next = self.redo.pop_back().ok_or_else(|| {
            FileMakerError::new(ErrorCode::PatchInvalid, "operation log has no redo entry")
        })?;
        self.used_bytes = self.used_bytes.saturating_sub(next.bytes);
        let current = mem::replace(document, unwrap_snapshot(next.document));
        self.push_undo(HistoryEntry {
            document: Arc::new(current),
            bytes: current_bytes,
        });
        Ok(())
    }

    /// Number of available undo entries.
    #[must_use]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// Number of available redo entries.
    #[must_use]
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// Aggregate serialized bytes retained by undo and redo snapshots.
    #[must_use]
    pub const fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    /// Configured aggregate serialized snapshot byte budget.
    #[must_use]
    pub const fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    fn ensure_snapshot_fits(&self, bytes: usize) -> Result<()> {
        if bytes > self.max_bytes {
            return Err(FileMakerError::new(
                ErrorCode::LimitExceeded,
                "document snapshot exceeds the operation log byte budget",
            ));
        }
        Ok(())
    }

    fn clear_redo(&mut self) {
        while let Some(entry) = self.redo.pop_front() {
            self.used_bytes = self.used_bytes.saturating_sub(entry.bytes);
        }
    }

    fn push_undo(&mut self, entry: HistoryEntry) {
        make_room(
            &mut self.undo,
            &mut self.used_bytes,
            self.max_entries,
            self.max_bytes,
            entry.bytes,
        );
        evict_bytes(
            &mut self.redo,
            &mut self.used_bytes,
            self.max_bytes,
            entry.bytes,
        );
        self.used_bytes += entry.bytes;
        self.undo.push_back(entry);
    }

    fn push_redo(&mut self, entry: HistoryEntry) {
        make_room(
            &mut self.redo,
            &mut self.used_bytes,
            self.max_entries,
            self.max_bytes,
            entry.bytes,
        );
        evict_bytes(
            &mut self.undo,
            &mut self.used_bytes,
            self.max_bytes,
            entry.bytes,
        );
        self.used_bytes += entry.bytes;
        self.redo.push_back(entry);
    }
}

fn make_room(
    entries: &mut VecDeque<HistoryEntry>,
    used_bytes: &mut usize,
    max_entries: usize,
    max_bytes: usize,
    incoming_bytes: usize,
) {
    while entries.len() >= max_entries || used_bytes.saturating_add(incoming_bytes) > max_bytes {
        let Some(entry) = entries.pop_front() else {
            break;
        };
        *used_bytes = used_bytes.saturating_sub(entry.bytes);
    }
}

fn evict_bytes(
    entries: &mut VecDeque<HistoryEntry>,
    used_bytes: &mut usize,
    max_bytes: usize,
    incoming_bytes: usize,
) {
    while used_bytes.saturating_add(incoming_bytes) > max_bytes {
        let Some(entry) = entries.pop_front() else {
            break;
        };
        *used_bytes = used_bytes.saturating_sub(entry.bytes);
    }
}

fn unwrap_snapshot(snapshot: Arc<DocumentIr>) -> DocumentIr {
    Arc::try_unwrap(snapshot).unwrap_or_else(|snapshot| (*snapshot).clone())
}
