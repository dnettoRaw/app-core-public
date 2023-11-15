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

use super::EventBus;
use crate::envelope::EventEnvelope;
use crate::ids::{AppId, EventName, NodeId};

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
}
