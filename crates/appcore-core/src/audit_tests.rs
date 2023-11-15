// =============================================================================
//        #######
//     ###       ###     F: audit_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{AuditCategory, AuditEntry, AuditLog, AuditOutcome, AuditRecord, MAX_AUDIT_RECORDS};
use crate::ids::{AppId, CommandName, NodeId};

fn record(outcome: AuditOutcome) -> AuditRecord {
    AuditRecord {
        command_id: "cmd-1".to_string(),
        command_name: CommandName::new("runtime.start".to_string()).unwrap(),
        app_id: AppId::new("example-app".to_string()).unwrap(),
        node_id: NodeId::new("node-a".to_string()).unwrap(),
        timestamp_ms: 0,
        outcome,
        message: None,
        trace: None,
    }
}

#[test]
fn new_starts_empty() {
    let log = AuditLog::new();
    assert!(log.is_empty());
    assert_eq!(log.len(), 0);
}

#[test]
fn push_adds_record() {
    let log = AuditLog::new();
    log.push(record(AuditOutcome::Accepted));
    assert_eq!(log.len(), 1);
    assert_eq!(log.entries().len(), 1);
}

#[test]
fn export_jsonl_redacts_messages() {
    let log = AuditLog::new();
    let mut item = record(AuditOutcome::Error);
    item.message = Some("request token=top-secret failed".to_string());
    log.push(item);

    let exported = log.export_jsonl().unwrap();
    assert!(exported.contains("[REDACTED]"));
    assert!(!exported.contains("top-secret"));
    assert_eq!(exported.lines().count(), 1);
}

#[test]
fn bounded_entries_discard_the_oldest_record() {
    let log = AuditLog::new();
    for offset in 0..=MAX_AUDIT_RECORDS {
        log.push_entry(AuditEntry::new(
            AuditCategory::Runtime,
            format!("operation-{offset}"),
            "runtime.test",
            offset as u64,
            offset as u64,
            AuditOutcome::Accepted,
        ));
    }

    let entries = log.entries();
    assert_eq!(entries.len(), MAX_AUDIT_RECORDS);
    assert_eq!(entries.first().unwrap().operation_id, "operation-1");
    assert_eq!(
        entries.last().unwrap().operation_id,
        format!("operation-{MAX_AUDIT_RECORDS}")
    );
}
