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

use crate::ids::{AppId, CommandName, NodeId};
use crate::operational_journal::FileOperationalJournal;
use crate::redact_text;
use crate::trace::TraceContext;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

const MAX_AUDIT_RECORDS: usize = 10_000;

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
}

/// Bounded process-local audit log.
#[derive(Debug, Default)]
pub struct AuditLog {
    records: Mutex<VecDeque<AuditRecord>>,
    entries: Mutex<VecDeque<AuditEntry>>,
    journal: Mutex<Option<Arc<FileOperationalJournal>>>,
    journal_error: Mutex<Option<String>>,
}

impl Clone for AuditLog {
    fn clone(&self) -> Self {
        let guard = self.records.lock();
        Self {
            records: Mutex::new(guard.clone()),
            entries: Mutex::new(self.entries.lock().clone()),
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

    /// Attaches a durable journal and loads its retained audit entries.
    pub fn attach_journal(&self, journal: Arc<FileOperationalJournal>) {
        let mut entries = journal.audit_entries();
        if entries.len() > MAX_AUDIT_RECORDS {
            entries.drain(..entries.len() - MAX_AUDIT_RECORDS);
        }
        *self.entries.lock() = entries.into();
        *self.journal.lock() = Some(journal);
        *self.journal_error.lock() = None;
    }

    /// Returns the last durable journal failure, when persistence degraded.
    pub fn durability_error(&self) -> Option<String> {
        self.journal_error.lock().clone()
    }

    /// Appends a command record and its generic audit projection.
    pub fn push(&self, mut record: AuditRecord) {
        record.message = record.message.map(|message| redact_text(&message));
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
        let mut guard = self.records.lock();
        while guard.len() >= MAX_AUDIT_RECORDS {
            guard.pop_front();
        }
        guard.push_back(record);
        drop(guard);
        self.push_entry(entry);
    }

    /// Appends one generic audit entry after redaction.
    pub fn push_entry(&self, mut entry: AuditEntry) {
        entry.message = entry.message.map(|message| redact_text(&message));
        if let Some(journal) = self.journal.lock().clone() {
            if let Err(error) = journal.append_audit(entry.clone()) {
                *self.journal_error.lock() = Some(redact_text(&format!("{error:?}")));
            }
        }
        let mut entries = self.entries.lock();
        while entries.len() >= MAX_AUDIT_RECORDS {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    /// Returns the number of command records.
    pub fn len(&self) -> usize {
        self.records.lock().len()
    }

    /// Reports whether no command records exist.
    pub fn is_empty(&self) -> bool {
        self.records.lock().is_empty()
    }

    /// Returns a point-in-time copy of command records.
    pub fn records(&self) -> Vec<AuditRecord> {
        self.records.lock().iter().cloned().collect()
    }

    /// Returns a point-in-time copy of generic audit entries.
    pub fn entries(&self) -> Vec<AuditEntry> {
        self.entries.lock().iter().cloned().collect()
    }

    /// Exports generic entries as newline-delimited JSON.
    pub fn export_jsonl(&self) -> Result<String, serde_json::Error> {
        let entries = self.entries.lock();
        let mut output = String::new();
        for entry in entries.iter() {
            output.push_str(&serde_json::to_string(entry)?);
            output.push('\n');
        }
        Ok(output)
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
