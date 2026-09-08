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
use std::io::{self, Write};
use std::sync::Arc;

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
fn shared_entry_snapshot_serializes_a_stable_json_array() {
    let log = AuditLog::new();
    log.push_entry(AuditEntry::new(
        AuditCategory::Runtime,
        "first-before-snapshot",
        "runtime.snapshot",
        0,
        1,
        AuditOutcome::Accepted,
    ));
    log.push_entry(AuditEntry::new(
        AuditCategory::Runtime,
        "last-before-snapshot",
        "runtime.snapshot",
        1,
        2,
        AuditOutcome::Accepted,
    ));
    let snapshot = log.entries_snapshot();
    log.push_entry(AuditEntry::new(
        AuditCategory::Runtime,
        "after-snapshot",
        "runtime.snapshot",
        2,
        3,
        AuditOutcome::Accepted,
    ));

    assert_eq!(snapshot.len(), 2);
    assert!(!snapshot.is_empty());
    assert_eq!(
        snapshot
            .recent(1)
            .next()
            .map(|entry| entry.operation_id.as_str()),
        Some("last-before-snapshot")
    );
    let decoded: Vec<AuditEntry> =
        serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0].operation_id, "first-before-snapshot");
    assert_eq!(log.stats().entry_count, 3);
}

#[test]
fn shared_record_snapshot_is_stable_and_selects_the_newest_records() {
    let log = AuditLog::new();
    for index in 0..3 {
        let mut item = record(AuditOutcome::Accepted);
        item.command_id = format!("cmd-{index}");
        log.push(item);
    }
    let snapshot = log.records_snapshot();
    let mut later = record(AuditOutcome::Error);
    later.command_id = "cmd-later".to_string();
    log.push(later);

    assert_eq!(snapshot.len(), 3);
    assert!(!snapshot.is_empty());
    assert_eq!(
        snapshot
            .recent(2)
            .map(|item| item.command_id.as_str())
            .collect::<Vec<_>>(),
        ["cmd-1", "cmd-2"]
    );
    let decoded: Vec<AuditRecord> =
        serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    assert_eq!(decoded.len(), 3);
    assert_eq!(decoded[0].command_id, "cmd-0");
    assert_eq!(log.len(), 4);
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

#[test]
fn aggregate_byte_budget_evicts_and_rejects_without_overshoot() {
    let log = AuditLog::with_max_bytes(1_024);
    for offset in 0..20 {
        log.push_entry(
            AuditEntry::new(
                AuditCategory::Runtime,
                format!("operation-{offset}"),
                "runtime.memory",
                offset,
                offset,
                AuditOutcome::Accepted,
            )
            .with_message(Some("x".repeat(400))),
        );
    }
    let stats = log.stats();
    assert!(stats.used_bytes <= stats.max_bytes);
    assert!(stats.peak_bytes <= stats.max_bytes);
    assert!(stats.evictions > 0);
    assert!(stats.entry_count < 20);

    let tiny = AuditLog::with_max_bytes(1);
    tiny.push_entry(AuditEntry::new(
        AuditCategory::Runtime,
        "operation",
        "runtime.memory",
        0,
        0,
        AuditOutcome::Accepted,
    ));
    assert_eq!(tiny.stats().entry_count, 0);
    assert_eq!(tiny.stats().rejections, 1);
}

#[test]
fn clone_uses_an_independent_copy_on_write_snapshot() {
    let original = AuditLog::new();
    original.push_entry(AuditEntry::new(
        AuditCategory::Runtime,
        "first",
        "runtime.snapshot",
        0,
        0,
        AuditOutcome::Accepted,
    ));
    let cloned = original.clone();
    cloned.push_entry(AuditEntry::new(
        AuditCategory::Runtime,
        "second",
        "runtime.snapshot",
        1,
        1,
        AuditOutcome::Accepted,
    ));
    assert_eq!(original.entries().len(), 1);
    assert_eq!(cloned.entries().len(), 2);
}

#[test]
fn writer_exports_a_snapshot_without_holding_the_log_lock() {
    struct ReentrantWriter<'a> {
        log: &'a AuditLog,
        output: Vec<u8>,
        appended: bool,
    }

    impl Write for ReentrantWriter<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if !self.appended {
                self.appended = true;
                self.log.push_entry(AuditEntry::new(
                    AuditCategory::Runtime,
                    "during-export",
                    "runtime.snapshot",
                    1,
                    1,
                    AuditOutcome::Accepted,
                ));
            }
            self.output.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let log = AuditLog::new();
    log.push_entry(AuditEntry::new(
        AuditCategory::Runtime,
        "before-export",
        "runtime.snapshot",
        0,
        0,
        AuditOutcome::Accepted,
    ));
    let mut writer = ReentrantWriter {
        log: &log,
        output: Vec::new(),
        appended: false,
    };
    log.write_jsonl(&mut writer).unwrap();

    let output = String::from_utf8(writer.output).unwrap();
    assert_eq!(output.lines().count(), 1);
    assert!(output.contains("before-export"));
    assert!(!output.contains("during-export"));
    assert_eq!(log.entries().len(), 2);
}

#[test]
fn journal_and_log_retain_the_same_immutable_audit_record() {
    let root = std::env::temp_dir().join(format!(
        "appcore-audit-log-shared-journal-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let journal = Arc::new(
        crate::FileOperationalJournal::open(root.join("audit.jsonl"), 10, 1024 * 1024).unwrap(),
    );
    let log = AuditLog::new();
    log.attach_journal(Arc::clone(&journal));
    log.push_entry(AuditEntry::new(
        AuditCategory::Runtime,
        "audit-shared",
        "runtime.shared",
        0,
        1,
        AuditOutcome::Accepted,
    ));

    let journal_records = journal.shared_audit_records();
    let snapshot = log.entries_snapshot();
    assert_eq!(journal_records.len(), 1);
    assert_eq!(snapshot.len(), 1);
    assert!(Arc::ptr_eq(
        journal_records.first().unwrap(),
        snapshot.entries.front().unwrap()
    ));
    assert_eq!(log.entries()[0].operation_id, "audit-shared");
    assert_eq!(journal.audit_entries()[0].operation_id, "audit-shared");

    drop(log);
    drop(journal);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn journal_restore_shares_safe_entries() {
    let root = std::env::temp_dir().join(format!(
        "appcore-audit-log-shared-restore-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let journal = Arc::new(
        crate::FileOperationalJournal::open(root.join("audit.jsonl"), 10, 1024 * 1024).unwrap(),
    );
    journal
        .append_audit(AuditEntry::new(
            AuditCategory::Runtime,
            "safe-entry",
            "runtime.restore",
            0,
            1,
            AuditOutcome::Accepted,
        ))
        .unwrap();
    let journal_records = journal.shared_audit_records();
    let log = AuditLog::new();
    log.attach_journal(Arc::clone(&journal));
    let snapshot = log.entries_snapshot();

    assert!(Arc::ptr_eq(
        journal_records.first().unwrap(),
        snapshot.entries.front().unwrap()
    ));
    drop(log);
    drop(journal);
    std::fs::remove_dir_all(root).unwrap();
}
