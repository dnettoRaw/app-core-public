// =============================================================================
//        #######
//     ###       ###     F: audit.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Bounded in-memory audit log for command dispatch outcomes.

use crate::audit_bounds::{
    audit_entry_is_bounded_and_redacted, audit_entry_retained_bytes, audit_record_retained_bytes,
    bound_audit_entry, bound_audit_text, bound_audit_trace, MAX_AUDIT_ID_BYTES,
};
use crate::ids::{AppId, CommandName, NodeId};
use crate::operational_journal::{FileOperationalJournal, OperationalJournalRecord};
use crate::trace::TraceContext;
use crate::{redact_text, MAX_OPERATIONAL_TEXT_BYTES};
use parking_lot::Mutex;
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use std::collections::VecDeque;
use std::io::{self, Write};
use std::sync::Arc;

const MAX_AUDIT_RECORDS: usize = 10_000;
/// Default aggregate memory budget for process-local audit records and entries.
pub const DEFAULT_AUDIT_LOG_MAX_BYTES: usize = 16 * 1024 * 1024;

/// Controlled outcome recorded for an audited operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    /// Operation completed successfully.
    Accepted,
    /// Operation was rejected by policy or validation.
    Rejected,
    /// Operation failed during execution.
    Error,
}

/// Command-specific audit record.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AuditRecord {
    /// Command identity.
    pub command_id: String,
    /// Command name.
    pub command_name: CommandName,
    /// Application scope.
    pub app_id: AppId,
    /// Runtime node scope.
    pub node_id: NodeId,
    /// Command start timestamp in Unix milliseconds.
    pub timestamp_ms: u64,
    /// Recorded command outcome.
    pub outcome: AuditOutcome,
    /// Optional redacted detail.
    pub message: Option<String>,
    /// Optional distributed trace context.
    pub trace: Option<TraceContext>,
}

/// Generic operational category associated with an audit entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditCategory {
    /// Command dispatch.
    Command,
    /// Query dispatch.
    Query,
    /// Event processing.
    Event,
    /// Scheduler execution.
    Scheduler,
    /// Control-plane operation.
    ControlPlane,
    /// Direct peer RPC operation.
    PeerRpc,
    /// Runtime lifecycle or infrastructure operation.
    Runtime,
}

/// Transport-neutral append-only operational audit entry.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AuditEntry {
    /// Generic operation category.
    pub category: AuditCategory,
    /// Stable operation identity.
    pub operation_id: String,
    /// Stable operation name.
    pub operation_name: String,
    /// Optional application scope.
    pub app_id: Option<String>,
    /// Optional node scope.
    pub node_id: Option<String>,
    /// Start timestamp in Unix milliseconds.
    pub started_at_ms: u64,
    /// Completion timestamp in Unix milliseconds.
    pub completed_at_ms: u64,
    /// Saturating elapsed time in milliseconds.
    pub latency_ms: u64,
    /// Recorded operation outcome.
    pub outcome: AuditOutcome,
    /// Optional redacted detail.
    pub message: Option<String>,
    /// Optional distributed trace context.
    pub trace: Option<TraceContext>,
}

impl AuditEntry {
    /// Creates an unscoped operational audit entry.
    pub fn new(
        category: AuditCategory,
        operation_id: impl Into<String>,
        operation_name: impl Into<String>,
        started_at_ms: u64,
        completed_at_ms: u64,
        outcome: AuditOutcome,
    ) -> Self {
        Self {
            category,
            operation_id: operation_id.into(),
            operation_name: operation_name.into(),
            app_id: None,
            node_id: None,
            started_at_ms,
            completed_at_ms,
            latency_ms: completed_at_ms.saturating_sub(started_at_ms),
            outcome,
            message: None,
            trace: None,
        }
    }

    /// Adds application and node scope.
    pub fn with_runtime_scope(mut self, app_id: &AppId, node_id: &NodeId) -> Self {
        self.app_id = Some(app_id.as_str().to_string());
        self.node_id = Some(node_id.as_str().to_string());
        self
    }

    /// Adds a redacted optional message.
    pub fn with_message(mut self, message: Option<String>) -> Self {
        self.message = message.map(|value| redact_text(&value));
        self
    }

    /// Adds distributed trace context.
    pub fn with_trace(mut self, trace: Option<TraceContext>) -> Self {
        self.trace = trace;
        self
    }

    pub(crate) fn into_bounded(self) -> Self {
        bound_audit_entry(self)
    }

    pub(crate) fn is_bounded_and_redacted(&self) -> bool {
        audit_entry_is_bounded_and_redacted(self)
    }
}

/// Point-in-time pressure metrics for a process-local audit log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditLogStats {
    /// Retained command records.
    pub record_count: usize,
    /// Retained generic audit entries.
    pub entry_count: usize,
    /// Estimated bytes retained by both snapshots.
    pub used_bytes: usize,
    /// Highest retained byte count observed by this log.
    pub peak_bytes: usize,
    /// Records or entries evicted to maintain count or byte limits.
    pub evictions: u64,
    /// Individual records or entries too large for the configured budget.
    pub rejections: u64,
    /// Aggregate configured byte budget.
    pub max_bytes: usize,
}

/// Shared immutable point-in-time view of command audit records.
#[derive(Clone, Debug)]
pub struct AuditRecordsSnapshot {
    records: Arc<VecDeque<Arc<AuditRecord>>>,
}

impl AuditRecordsSnapshot {
    /// Returns the number of records captured by this snapshot.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Reports whether this snapshot contains no records.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Iterates over at most the newest `limit` records without cloning them.
    pub fn recent(&self, limit: usize) -> impl Iterator<Item = &AuditRecord> {
        let start = self.records.len().saturating_sub(limit);
        self.records.iter().skip(start).map(Arc::as_ref)
    }
}

impl Serialize for AuditRecordsSnapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(Some(self.records.len()))?;
        for record in self.records.iter() {
            sequence.serialize_element(record.as_ref())?;
        }
        sequence.end()
    }
}

/// Shared immutable point-in-time view of generic audit entries.
#[derive(Clone, Debug)]
pub struct AuditEntriesSnapshot {
    entries: Arc<VecDeque<Arc<OperationalJournalRecord>>>,
}

impl AuditEntriesSnapshot {
    /// Returns the number of entries captured by this snapshot.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Reports whether this snapshot contains no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterates over at most the newest `limit` entries without cloning them.
    pub fn recent(&self, limit: usize) -> impl Iterator<Item = &AuditEntry> {
        let start = self.entries.len().saturating_sub(limit);
        self.entries.iter().skip(start).filter_map(audit_entry)
    }
}

impl Serialize for AuditEntriesSnapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(Some(self.entries.len()))?;
        for entry in self.entries.iter().filter_map(audit_entry) {
            sequence.serialize_element(entry)?;
        }
        sequence.end()
    }
}

#[derive(Debug, Clone)]
struct AuditState {
    records: Arc<VecDeque<Arc<AuditRecord>>>,
    entries: Arc<VecDeque<Arc<OperationalJournalRecord>>>,
    used_bytes: usize,
    peak_bytes: usize,
    evictions: u64,
    rejections: u64,
    max_bytes: usize,
}

impl AuditState {
    fn new(max_bytes: usize) -> Self {
        Self {
            records: Arc::default(),
            entries: Arc::default(),
            used_bytes: 0,
            peak_bytes: 0,
            evictions: 0,
            rejections: 0,
            max_bytes: max_bytes.max(1),
        }
    }

    fn push_record(&mut self, record: AuditRecord) {
        let bytes = audit_record_retained_bytes(&record);
        while self.records.len() >= MAX_AUDIT_RECORDS {
            self.pop_record();
        }
        if !self.make_room(bytes) {
            self.rejections = self.rejections.saturating_add(1);
            return;
        }
        Arc::make_mut(&mut self.records).push_back(Arc::new(record));
        self.add_bytes(bytes);
    }

    fn push_shared_entry(&mut self, record: Arc<OperationalJournalRecord>) {
        let Some(entry) = audit_entry(&record) else {
            self.rejections = self.rejections.saturating_add(1);
            return;
        };
        let bytes = audit_entry_retained_bytes(entry);
        while self.entries.len() >= MAX_AUDIT_RECORDS {
            self.pop_entry();
        }
        if !self.make_room(bytes) {
            self.rejections = self.rejections.saturating_add(1);
            return;
        }
        Arc::make_mut(&mut self.entries).push_back(record);
        self.add_bytes(bytes);
    }

    fn replace_entries(&mut self, entries: Vec<Arc<OperationalJournalRecord>>) {
        while !self.entries.is_empty() {
            self.pop_entry();
        }
        for entry in entries {
            self.push_shared_entry(entry);
        }
    }

    fn make_room(&mut self, incoming: usize) -> bool {
        if incoming > self.max_bytes {
            return false;
        }
        while self.used_bytes.saturating_add(incoming) > self.max_bytes {
            if !self.pop_oldest() {
                return false;
            }
        }
        true
    }

    fn pop_oldest(&mut self) -> bool {
        let record_time = self.records.front().map(|record| record.timestamp_ms);
        let entry_time = self
            .entries
            .front()
            .and_then(audit_entry)
            .map(|entry| entry.started_at_ms);
        match (record_time, entry_time) {
            (Some(record), Some(entry)) if record <= entry => self.pop_record(),
            (Some(_), Some(_)) | (None, Some(_)) => self.pop_entry(),
            (Some(_), None) => self.pop_record(),
            (None, None) => return false,
        }
        true
    }

    fn pop_record(&mut self) {
        if let Some(record) = Arc::make_mut(&mut self.records).pop_front() {
            self.used_bytes = self
                .used_bytes
                .saturating_sub(audit_record_retained_bytes(&record));
            self.evictions = self.evictions.saturating_add(1);
        }
    }

    fn pop_entry(&mut self) {
        if let Some(entry) = Arc::make_mut(&mut self.entries).pop_front() {
            if let Some(entry) = audit_entry(&entry) {
                self.used_bytes = self
                    .used_bytes
                    .saturating_sub(audit_entry_retained_bytes(entry));
            }
            self.evictions = self.evictions.saturating_add(1);
        }
    }

    fn add_bytes(&mut self, bytes: usize) {
        self.used_bytes = self.used_bytes.saturating_add(bytes);
        self.peak_bytes = self.peak_bytes.max(self.used_bytes);
    }

    fn stats(&self) -> AuditLogStats {
        AuditLogStats {
            record_count: self.records.len(),
            entry_count: self.entries.len(),
            used_bytes: self.used_bytes,
            peak_bytes: self.peak_bytes,
            evictions: self.evictions,
            rejections: self.rejections,
            max_bytes: self.max_bytes,
        }
    }
}

/// Bounded process-local audit log.
#[derive(Debug)]
pub struct AuditLog {
    state: Mutex<AuditState>,
    journal: Mutex<Option<Arc<FileOperationalJournal>>>,
    journal_error: Mutex<Option<String>>,
}

impl Default for AuditLog {
    fn default() -> Self {
        Self::with_max_bytes(DEFAULT_AUDIT_LOG_MAX_BYTES)
    }
}

impl Clone for AuditLog {
    fn clone(&self) -> Self {
        Self {
            state: Mutex::new(self.state.lock().clone()),
            journal: Mutex::new(self.journal.lock().clone()),
            journal_error: Mutex::new(self.journal_error.lock().clone()),
        }
    }
}

impl AuditLog {
    /// Creates an empty audit log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an empty audit log with an aggregate retained-byte budget.
    pub fn with_max_bytes(max_bytes: usize) -> Self {
        Self {
            state: Mutex::new(AuditState::new(max_bytes)),
            journal: Mutex::new(None),
            journal_error: Mutex::new(None),
        }
    }

    /// Attaches a journal, sharing safe entries and sanitizing any unsafe record.
    pub fn attach_journal(&self, journal: Arc<FileOperationalJournal>) {
        let mut entries = journal.shared_audit_records();
        if entries.len() > MAX_AUDIT_RECORDS {
            entries.drain(..entries.len() - MAX_AUDIT_RECORDS);
        }
        self.state.lock().replace_entries(entries);
        *self.journal.lock() = Some(journal);
        *self.journal_error.lock() = None;
    }

    /// Returns the last durable journal failure, when persistence degraded.
    pub fn durability_error(&self) -> Option<String> {
        self.journal_error.lock().clone()
    }

    /// Appends a command record and its generic audit projection.
    pub fn push(&self, mut record: AuditRecord) {
        record.command_id = bound_audit_text(&record.command_id, MAX_AUDIT_ID_BYTES);
        record.message = record
            .message
            .map(|message| bound_audit_text(&message, MAX_OPERATIONAL_TEXT_BYTES));
        bound_audit_trace(&mut record.trace);
        let completed_at_ms = now_ms();
        let entry = AuditEntry::new(
            AuditCategory::Command,
            record.command_id.clone(),
            record.command_name.as_str(),
            record.timestamp_ms,
            completed_at_ms,
            record.outcome,
        )
        .with_runtime_scope(&record.app_id, &record.node_id)
        .with_message(record.message.clone())
        .with_trace(record.trace.clone());
        let entry = Arc::new(OperationalJournalRecord::Audit(entry));
        self.persist_entry(Arc::clone(&entry));
        let mut state = self.state.lock();
        state.push_record(record);
        state.push_shared_entry(entry);
    }

    /// Appends one generic audit entry after redaction.
    pub fn push_entry(&self, entry: AuditEntry) {
        let entry = entry.into_bounded();
        let entry = Arc::new(OperationalJournalRecord::Audit(entry));
        self.persist_entry(Arc::clone(&entry));
        self.state.lock().push_shared_entry(entry);
    }

    fn persist_entry(&self, entry: Arc<OperationalJournalRecord>) {
        if let Some(journal) = self.journal.lock().clone() {
            if let Err(error) = journal.append_shared_audit(entry) {
                *self.journal_error.lock() = Some(redact_text(&format!("{error:?}")));
            }
        }
    }

    /// Returns the number of command records.
    pub fn len(&self) -> usize {
        self.state.lock().records.len()
    }

    /// Reports whether no command records exist.
    pub fn is_empty(&self) -> bool {
        self.state.lock().records.is_empty()
    }

    /// Returns a point-in-time copy of command records.
    pub fn records(&self) -> Vec<AuditRecord> {
        self.records_snapshot()
            .recent(usize::MAX)
            .cloned()
            .collect()
    }

    /// Returns a point-in-time copy of generic audit entries.
    pub fn entries(&self) -> Vec<AuditEntry> {
        self.entries_snapshot()
            .recent(usize::MAX)
            .cloned()
            .collect()
    }

    /// Returns a shared immutable snapshot without cloning record fields.
    pub fn records_snapshot(&self) -> AuditRecordsSnapshot {
        AuditRecordsSnapshot {
            records: Arc::clone(&self.state.lock().records),
        }
    }

    /// Returns a shared immutable snapshot without cloning entry fields.
    pub fn entries_snapshot(&self) -> AuditEntriesSnapshot {
        AuditEntriesSnapshot {
            entries: Arc::clone(&self.state.lock().entries),
        }
    }

    /// Returns current count, byte-pressure, eviction, and rejection metrics.
    pub fn stats(&self) -> AuditLogStats {
        self.state.lock().stats()
    }

    /// Writes generic entries as JSONL from a shared immutable snapshot.
    pub fn write_jsonl(&self, writer: &mut impl Write) -> io::Result<()> {
        let snapshot = self.entries_snapshot();
        for entry in snapshot.entries.iter().filter_map(audit_entry) {
            serde_json::to_writer(&mut *writer, entry).map_err(io::Error::other)?;
            writer.write_all(b"\n")?;
        }
        Ok(())
    }

    /// Exports generic entries as newline-delimited JSON.
    pub fn export_jsonl(&self) -> Result<String, serde_json::Error> {
        let mut output = Vec::new();
        self.write_jsonl(&mut output)
            .map_err(serde_json::Error::io)?;
        String::from_utf8(output).map_err(|error| {
            serde_json::Error::io(io::Error::new(io::ErrorKind::InvalidData, error))
        })
    }
}

fn audit_entry(record: &Arc<OperationalJournalRecord>) -> Option<&AuditEntry> {
    match record.as_ref() {
        OperationalJournalRecord::Audit(entry) => Some(entry),
        OperationalJournalRecord::Event(_) => None,
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "audit_tests.rs"]
mod tests;
