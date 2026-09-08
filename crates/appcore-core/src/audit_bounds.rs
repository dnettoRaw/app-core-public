// =============================================================================
//        #######
//     ###       ###     F: audit_bounds.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/03 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/03 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Audit text bounds and retained-memory accounting.

use crate::audit::{AuditEntry, AuditRecord};
use crate::redaction::is_text_redacted_and_bounded;
use crate::{redact_text_with_limit, TraceContext, MAX_OPERATIONAL_TEXT_BYTES};

pub(super) const MAX_AUDIT_ID_BYTES: usize = 256;
pub(super) const MAX_AUDIT_SCOPE_BYTES: usize = 128;

pub(super) fn bound_audit_entry(mut entry: AuditEntry) -> AuditEntry {
    entry.operation_id = bound_audit_text(&entry.operation_id, MAX_AUDIT_ID_BYTES);
    entry.operation_name = bound_audit_text(&entry.operation_name, MAX_AUDIT_ID_BYTES);
    entry.app_id = entry
        .app_id
        .map(|value| bound_audit_text(&value, MAX_AUDIT_SCOPE_BYTES));
    entry.node_id = entry
        .node_id
        .map(|value| bound_audit_text(&value, MAX_AUDIT_SCOPE_BYTES));
    entry.message = entry
        .message
        .map(|value| bound_audit_text(&value, MAX_OPERATIONAL_TEXT_BYTES));
    bound_audit_trace(&mut entry.trace);
    entry
}

pub(super) fn audit_entry_is_bounded_and_redacted(entry: &AuditEntry) -> bool {
    is_text_redacted_and_bounded(&entry.operation_id, MAX_AUDIT_ID_BYTES)
        && is_text_redacted_and_bounded(&entry.operation_name, MAX_AUDIT_ID_BYTES)
        && entry
            .app_id
            .as_deref()
            .is_none_or(|value| is_text_redacted_and_bounded(value, MAX_AUDIT_SCOPE_BYTES))
        && entry
            .node_id
            .as_deref()
            .is_none_or(|value| is_text_redacted_and_bounded(value, MAX_AUDIT_SCOPE_BYTES))
        && entry
            .message
            .as_deref()
            .is_none_or(|value| is_text_redacted_and_bounded(value, MAX_OPERATIONAL_TEXT_BYTES))
        && entry
            .trace
            .as_ref()
            .is_none_or(trace_is_bounded_and_redacted)
}

pub(super) fn bound_audit_text(value: &str, limit: usize) -> String {
    let mut bounded = redact_text_with_limit(value, limit);
    bounded.shrink_to_fit();
    bounded
}

pub(super) fn bound_audit_trace(trace: &mut Option<TraceContext>) {
    let Some(trace) = trace else {
        return;
    };
    trace.trace_id = bound_audit_text(&trace.trace_id, MAX_AUDIT_ID_BYTES);
    trace.span_id = bound_audit_text(&trace.span_id, MAX_AUDIT_ID_BYTES);
    trace.parent_span_id = trace
        .parent_span_id
        .take()
        .map(|value| bound_audit_text(&value, MAX_AUDIT_ID_BYTES));
    trace.command_id = trace
        .command_id
        .take()
        .map(|value| bound_audit_text(&value, MAX_AUDIT_ID_BYTES));
}

fn trace_is_bounded_and_redacted(trace: &TraceContext) -> bool {
    is_text_redacted_and_bounded(&trace.trace_id, MAX_AUDIT_ID_BYTES)
        && is_text_redacted_and_bounded(&trace.span_id, MAX_AUDIT_ID_BYTES)
        && trace
            .parent_span_id
            .as_deref()
            .is_none_or(|value| is_text_redacted_and_bounded(value, MAX_AUDIT_ID_BYTES))
        && trace
            .command_id
            .as_deref()
            .is_none_or(|value| is_text_redacted_and_bounded(value, MAX_AUDIT_ID_BYTES))
}

pub(super) fn audit_record_retained_bytes(record: &AuditRecord) -> usize {
    std::mem::size_of::<AuditRecord>()
        .saturating_add(record.command_id.len())
        .saturating_add(record.command_name.as_str().len())
        .saturating_add(record.app_id.as_str().len())
        .saturating_add(record.node_id.as_str().len())
        .saturating_add(record.message.as_ref().map_or(0, String::len))
        .saturating_add(trace_retained_bytes(record.trace.as_ref()))
}

pub(super) fn audit_entry_retained_bytes(entry: &AuditEntry) -> usize {
    std::mem::size_of::<AuditEntry>()
        .saturating_add(entry.operation_id.len())
        .saturating_add(entry.operation_name.len())
        .saturating_add(entry.app_id.as_ref().map_or(0, String::len))
        .saturating_add(entry.node_id.as_ref().map_or(0, String::len))
        .saturating_add(entry.message.as_ref().map_or(0, String::len))
        .saturating_add(trace_retained_bytes(entry.trace.as_ref()))
}

fn trace_retained_bytes(trace: Option<&TraceContext>) -> usize {
    let Some(trace) = trace else {
        return 0;
    };
    std::mem::size_of::<TraceContext>()
        .saturating_add(trace.trace_id.len())
        .saturating_add(trace.span_id.len())
        .saturating_add(trace.parent_span_id.as_ref().map_or(0, String::len))
        .saturating_add(trace.originating_core_id.as_str().len())
        .saturating_add(trace.current_core_id.as_str().len())
        .saturating_add(trace.tenant_id.as_str().len())
        .saturating_add(trace.command_id.as_ref().map_or(0, String::len))
}
