// =============================================================================
//        #######
//     ###       ###     F: envelope_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{CommandEnvelope, EventEnvelope};
use crate::ids::{AppId, CommandName, EventName, NodeId};

#[test]
fn create_valid_command_envelope() {
    let result = CommandEnvelope::new(
        CommandName::new("runtime.start".to_string()).unwrap(),
        "cmd-1".to_string(),
        AppId::new("example-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        0,
        None,
        vec![1, 2, 3],
    );

    assert!(result.is_ok());
}

#[test]
fn reject_command_envelope_with_empty_command_id() {
    let result = CommandEnvelope::new(
        CommandName::new("runtime.start".to_string()).unwrap(),
        String::new(),
        AppId::new("example-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        0,
        None,
        vec![1],
    );

    assert!(result.is_err());
}

#[test]
fn create_valid_event_envelope() {
    let result = EventEnvelope::new(
        EventName::new("RuntimeStarted".to_string()).unwrap(),
        "evt-1".to_string(),
        AppId::new("example-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        0,
        vec![4, 5],
    );

    assert!(result.is_ok());
}

#[test]
fn reject_event_envelope_with_empty_event_id() {
    let result = EventEnvelope::new(
        EventName::new("RuntimeStarted".to_string()).unwrap(),
        String::new(),
        AppId::new("example-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        0,
        vec![4],
    );

    assert!(result.is_err());
}

#[test]
fn empty_payload_is_accepted() {
    let command = CommandEnvelope::new(
        CommandName::new("runtime.start".to_string()).unwrap(),
        "cmd-1".to_string(),
        AppId::new("example-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        0,
        Some("key-1".to_string()),
        vec![],
    );
    let event = EventEnvelope::new(
        EventName::new("RuntimeStarted".to_string()).unwrap(),
        "evt-1".to_string(),
        AppId::new("example-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        0,
        vec![],
    );

    assert!(command.is_ok());
    assert!(event.is_ok());
}

#[test]
fn reject_command_envelope_with_invalid_command_name() {
    let result = CommandEnvelope::new(
        serde_json::from_str::<CommandName>("\"runtime ping\"").unwrap(),
        "cmd-1".to_string(),
        AppId::new("example-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        0,
        None,
        vec![],
    );

    assert!(result.is_err());
}

#[test]
fn reject_event_envelope_with_invalid_event_name() {
    let result = EventEnvelope::new(
        serde_json::from_str::<EventName>("\"runtime/pong\"").unwrap(),
        "evt-1".to_string(),
        AppId::new("example-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        0,
        vec![],
    );

    assert!(result.is_err());
}
