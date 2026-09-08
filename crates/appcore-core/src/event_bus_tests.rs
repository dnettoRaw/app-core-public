// =============================================================================
//        #######
//     ###       ###     F: event_bus_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/09 08:44:50 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{event_retained_bytes, EventBus};
use crate::envelope::EventEnvelope;
use crate::ids::{AppId, EventName, NodeId};
use crate::FileOperationalJournal;
use std::sync::Arc;

fn event(id: &str) -> EventEnvelope {
    let event = EventEnvelope::new(
        EventName::new("RuntimeStarted".to_string()).unwrap(),
        id.to_string(),
        AppId::new("example-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        0,
        vec![],
    );
    match event {
        Ok(event) => event,
        Err(_) => unreachable!(),
    }
}

#[test]
fn new_starts_empty() {
    let bus = EventBus::new();
    assert!(bus.is_empty());
    assert_eq!(bus.len(), 0);
}

#[test]
fn emit_adds_event() {
    let bus = EventBus::new();
    bus.emit(event("evt-1"));
    assert_eq!(bus.len(), 1);
}

#[test]
fn emit_many_adds_multiple_events() {
    let bus = EventBus::new();
    bus.emit_many(vec![event("evt-1"), event("evt-2")]);
    assert_eq!(bus.len(), 2);
}

#[test]
fn clear_removes_events() {
    let bus = EventBus::new();
    bus.emit(event("evt-1"));
    bus.emit(event("evt-2"));
    bus.clear();
    assert!(bus.is_empty());
    assert_eq!(bus.stats().used_bytes, 0);
}

#[test]
fn shared_snapshot_is_stable_and_selects_the_newest_events() {
    let bus = EventBus::new();
    bus.emit_many(vec![event("evt-1"), event("evt-2")]);
    let snapshot = bus.snapshot();
    bus.emit(event("evt-3"));

    assert_eq!(snapshot.len(), 2);
    assert!(!snapshot.is_empty());
    assert_eq!(
        snapshot.recent(1).next().map(|item| item.event_id.as_str()),
        Some("evt-2")
    );
    let decoded: Vec<EventEnvelope> =
        serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0].event_id, "evt-1");
    assert_eq!(bus.len(), 3);
}

#[test]
fn clone_uses_an_independent_copy_on_write_snapshot() {
    let bus = EventBus::new();
    bus.emit(event("evt-1"));
    let cloned = bus.clone();
    cloned.emit(event("evt-2"));

    assert_eq!(bus.len(), 1);
    assert_eq!(cloned.len(), 2);
    assert_eq!(bus.events()[0].event_id, "evt-1");
    assert_eq!(cloned.events()[1].event_id, "evt-2");
}

#[test]
fn aggregate_byte_budget_evicts_and_rejects_without_overshoot() {
    let sample = event("evt-1");
    let one_event_bytes = event_retained_bytes(&sample);
    let bus = EventBus::with_max_bytes(one_event_bytes * 2);
    bus.emit(sample);
    bus.emit(event("evt-2"));
    bus.emit(event("evt-3"));

    let after_eviction = bus.stats();
    assert_eq!(after_eviction.event_count, 2);
    assert_eq!(after_eviction.evictions, 1);
    assert!(after_eviction.used_bytes <= after_eviction.max_bytes);
    assert_eq!(bus.snapshot().recent(2).next().unwrap().event_id, "evt-2");

    let mut oversized = event("evt-oversized");
    oversized.payload = vec![0; after_eviction.max_bytes];
    bus.emit(oversized);
    let after_rejection = bus.stats();
    assert_eq!(after_rejection.event_count, 2);
    assert_eq!(after_rejection.evictions, 1);
    assert_eq!(after_rejection.rejections, 1);
    assert!(after_rejection.used_bytes <= after_rejection.max_bytes);
}

#[test]
fn journal_restore_applies_the_same_byte_budget_to_the_newest_suffix() {
    let root = std::env::temp_dir().join(format!(
        "appcore-event-bus-journal-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let journal =
        Arc::new(FileOperationalJournal::open(root.join("events.jsonl"), 10, 1024 * 1024).unwrap());
    for id in ["evt-1", "evt-2", "evt-3"] {
        journal.append_event(event(id)).unwrap();
    }

    let max_bytes = event_retained_bytes(&event("evt-1")) * 2;
    let bus = EventBus::with_max_bytes(max_bytes);
    bus.attach_journal(Arc::clone(&journal));
    let restored = bus.events();
    assert_eq!(restored.len(), 2);
    assert_eq!(restored[0].event_id, "evt-2");
    assert_eq!(restored[1].event_id, "evt-3");
    assert_eq!(bus.stats().evictions, 1);
    assert!(bus.stats().used_bytes <= max_bytes);

    drop(bus);
    drop(journal);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn journal_and_bus_retain_the_same_immutable_event_record() {
    let root = std::env::temp_dir().join(format!(
        "appcore-event-bus-shared-journal-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let journal =
        Arc::new(FileOperationalJournal::open(root.join("events.jsonl"), 10, 1024 * 1024).unwrap());
    let bus = EventBus::new();
    bus.attach_journal(Arc::clone(&journal));
    bus.emit(event("evt-shared"));

    let journal_records = journal.shared_event_records();
    let snapshot = bus.snapshot();
    assert_eq!(journal_records.len(), 1);
    assert_eq!(snapshot.len(), 1);
    assert!(Arc::ptr_eq(
        journal_records.first().unwrap(),
        snapshot.events.front().unwrap()
    ));
    assert_eq!(bus.events()[0].event_id, "evt-shared");
    assert_eq!(journal.events()[0].event_id, "evt-shared");

    drop(bus);
    drop(journal);
    std::fs::remove_dir_all(root).unwrap();
}
