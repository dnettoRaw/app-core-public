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
fn journal_bounds_and_redacts_audit_entries_before_persistence() {
    let root = temp_root("redacted-audit");
    let path = root.join("journal.jsonl");
    let journal = FileOperationalJournal::open(&path, 10, 1024 * 1024).unwrap();
    let mut entry = audit("unsafe-audit");
    entry.message = Some("token=do-not-persist".to_string());
    journal.append_audit(entry).unwrap();

    let retained = journal.audit_entries();
    assert_eq!(retained[0].message.as_deref(), Some("token=[REDACTED]"));
    let encoded = fs::read_to_string(&path).unwrap();
    assert!(encoded.contains("token=[REDACTED]"));
    assert!(!encoded.contains("do-not-persist"));

    drop(journal);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn journal_sanitizes_and_rewrites_an_unsafe_v1_record_during_load() {
    let root = temp_root("sanitize-load");
    let path = root.join("journal.jsonl");
    let journal = FileOperationalJournal::open(&path, 10, 1024 * 1024).unwrap();
    let mut entry = audit("unsafe-loaded-audit");
    entry.message = Some("token=do-not-retain".to_string());
    journal
        .append(OperationalJournalRecord::Audit(entry))
        .unwrap();
    drop(journal);
    assert!(fs::read_to_string(&path).unwrap().contains("do-not-retain"));

    let reopened = FileOperationalJournal::open(&path, 10, 1024 * 1024).unwrap();
    assert_eq!(
        reopened.audit_entries()[0].message.as_deref(),
        Some("token=[REDACTED]")
    );
    assert!(!fs::read_to_string(&path).unwrap().contains("do-not-retain"));

    drop(reopened);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn journal_rejects_an_unsafe_shared_audit_record() {
    let root = temp_root("reject-shared-audit");
    let path = root.join("journal.jsonl");
    let journal = FileOperationalJournal::open(&path, 10, 1024 * 1024).unwrap();
    let mut entry = audit("unsafe-shared-audit");
    entry.message = Some("secret=do-not-persist".to_string());

    let error = journal
        .append_shared_audit(Arc::new(OperationalJournalRecord::Audit(entry)))
        .unwrap_err();

    assert!(matches!(
        error,
        RuntimeError::OperationalJournalIo {
            operation: "append_audit",
            ..
        }
    ));
    assert!(journal.audit_entries().is_empty());
    assert!(!fs::read_to_string(&path)
        .unwrap()
        .contains("do-not-persist"));

    drop(journal);
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

#[test]
fn journal_streams_audit_export_without_including_events() {
    let root = temp_root("stream-export");
    let path = root.join("journal.jsonl");
    let journal = FileOperationalJournal::open(&path, 10, 1024 * 1024).unwrap();
    journal.append_audit(audit("audit-1")).unwrap();
    journal.append_event(event("event-1")).unwrap();
    journal.append_audit(audit("audit-2")).unwrap();

    let mut output = Vec::new();
    journal.write_audit_jsonl(&mut output).unwrap();
    let lines = std::str::from_utf8(&output)
        .unwrap()
        .lines()
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("audit-1"));
    assert!(lines[1].contains("audit-2"));
    assert!(!output
        .windows("event-1".len())
        .any(|item| item == b"event-1"));
    assert_eq!(journal.export_audit_jsonl().unwrap().as_bytes(), output);

    drop(journal);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn audit_export_releases_state_lock_before_writer_callbacks() {
    let root = temp_root("export-callback");
    let path = root.join("journal.jsonl");
    let journal = FileOperationalJournal::open(&path, 10, 1024 * 1024).unwrap();
    journal.append_audit(audit("audit-before")).unwrap();
    let mut writer = AppendingWriter {
        journal: &journal,
        output: Vec::new(),
        appended: false,
    };

    journal.write_audit_jsonl(&mut writer).unwrap();

    assert!(writer.appended);
    assert!(std::str::from_utf8(&writer.output)
        .unwrap()
        .contains("audit-before"));
    assert!(!writer
        .output
        .windows("audit-during".len())
        .any(|item| item == b"audit-during"));
    assert_eq!(journal.audit_entries().len(), 2);

    drop(writer);
    drop(journal);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn journal_rejects_an_oversized_record_before_append() {
    let root = temp_root("oversized-record");
    let path = root.join("journal.jsonl");
    let journal = FileOperationalJournal::open(&path, 10, 2 * 1024 * 1024).unwrap();
    let oversized = EventEnvelope::new(
        EventName::new("runtime.test".to_string()).unwrap(),
        "event-large".to_string(),
        AppId::new("app-a".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        1,
        vec![255; MAX_JOURNAL_RECORD_BYTES],
    )
    .unwrap();

    let error = journal.append_event(oversized).unwrap_err();
    assert!(matches!(
        error,
        RuntimeError::OperationalJournalIo {
            operation: "validate_record",
            ..
        }
    ));
    assert!(journal.events().is_empty());
    assert_eq!(
        fs::read(&path).unwrap(),
        format!("{OPERATIONAL_JOURNAL_FORMAT_V1}\n").as_bytes()
    );

    drop(journal);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn journal_rejects_an_oversized_complete_line_during_scan() {
    let root = temp_root("oversized-line");
    let path = root.join("journal.jsonl");
    fs::create_dir_all(&root).unwrap();
    let mut file = File::create(&path).unwrap();
    writeln!(file, "{OPERATIONAL_JOURNAL_FORMAT_V1}").unwrap();
    file.write_all(&vec![b' '; MAX_JOURNAL_ENVELOPE_BYTES + 1])
        .unwrap();
    file.write_all(b"\n").unwrap();
    drop(file);

    let error = FileOperationalJournal::open(&path, 10, 2 * 1024 * 1024).unwrap_err();
    assert!(matches!(
        error,
        RuntimeError::OperationalJournalIo {
            operation: "validate_record",
            ..
        }
    ));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn byte_retention_keeps_the_largest_suffix_that_fits() {
    let records = (1..=12)
        .map(|index| {
            Arc::new(OperationalJournalRecord::Audit(audit(&format!(
                "audit-{index}"
            ))))
        })
        .collect::<VecDeque<_>>();
    for expected in 1..=records.len() {
        let maximum = encoded_records_bytes(
            records
                .iter()
                .skip(records.len() - expected)
                .map(AsRef::as_ref),
        )
        .unwrap();
        let mut retained = records.clone();

        retain_within_bytes(&mut retained, maximum).unwrap();

        assert_eq!(retained.len(), expected);
        assert_eq!(
            retained,
            records
                .iter()
                .skip(records.len() - expected)
                .cloned()
                .collect::<VecDeque<_>>()
        );
        assert!(encoded_records_bytes(retained.iter().map(AsRef::as_ref)).unwrap() <= maximum);
    }
}

struct AppendingWriter<'a> {
    journal: &'a FileOperationalJournal,
    output: Vec<u8>,
    appended: bool,
}

impl Write for AppendingWriter<'_> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if !self.appended {
            self.journal
                .append_audit(audit("audit-during"))
                .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
            self.appended = true;
        }
        self.output.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
