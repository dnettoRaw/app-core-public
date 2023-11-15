// =============================================================================
//        #######
//     ###       ###     F: operational_journal_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use crate::{AppId, AuditCategory, AuditOutcome, EventName, NodeId};

fn temp_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "appcore-operational-journal-{name}-{}-{}",
        std::process::id(),
        crate::operational_journal::JOURNAL_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

fn audit(id: &str) -> AuditEntry {
    AuditEntry::new(
        AuditCategory::Runtime,
        id,
        "runtime.test",
        1,
        2,
        AuditOutcome::Accepted,
    )
}

fn event(id: &str) -> EventEnvelope {
    EventEnvelope::new(
        EventName::new("runtime.test".to_string()).unwrap(),
        id.to_string(),
        AppId::new("app-a".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        1,
        b"opaque".to_vec(),
    )
    .unwrap()
}

#[test]
fn journal_persists_audit_and_events_across_restart() {
    let root = temp_root("restart");
    let path = root.join("journal.jsonl");
    let journal = FileOperationalJournal::open(&path, 10, 1024 * 1024).unwrap();
    journal.append_audit(audit("audit-1")).unwrap();
    journal.append_event(event("event-1")).unwrap();
    drop(journal);

    let reopened = FileOperationalJournal::open(&path, 10, 1024 * 1024).unwrap();
    assert_eq!(reopened.audit_entries(), vec![audit("audit-1")]);
    assert_eq!(reopened.events(), vec![event("event-1")]);
    assert!(reopened.export_audit_jsonl().unwrap().contains("audit-1"));
    drop(reopened);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn journal_rejects_hash_chain_tampering() {
    let root = temp_root("tamper");
    let path = root.join("journal.jsonl");
    let journal = FileOperationalJournal::open(&path, 10, 1024 * 1024).unwrap();
    journal.append_audit(audit("audit-1")).unwrap();
    drop(journal);
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text.replacen("runtime.test", "runtime.fail", 1)).unwrap();

    assert!(FileOperationalJournal::open(&path, 10, 1024 * 1024).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn journal_recovers_partial_tail_and_enforces_retention() {
    let root = temp_root("recovery");
    let path = root.join("journal.jsonl");
    let journal = FileOperationalJournal::open(&path, 2, 1024 * 1024).unwrap();
    journal.append_audit(audit("audit-1")).unwrap();
    journal.append_audit(audit("audit-2")).unwrap();
    journal.append_event(event("event-3")).unwrap();
    drop(journal);
    let mut file = OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(b"{\"sequence\":4").unwrap();
    drop(file);

    let recovered = FileOperationalJournal::open(&path, 2, 1024 * 1024).unwrap();
    assert_eq!(recovered.audit_entries(), vec![audit("audit-2")]);
    assert_eq!(recovered.events(), vec![event("event-3")]);
    assert!(!fs::read_to_string(&path)
        .unwrap()
        .contains("\"sequence\":4"));
    drop(recovered);
    fs::remove_dir_all(root).unwrap();
}
