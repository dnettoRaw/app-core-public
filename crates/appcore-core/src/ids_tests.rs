// =============================================================================
//        #######
//     ###       ###     F: ids_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/09 08:35:21 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{AppId, CommandName, EventName, NodeId};

fn long_value() -> String {
    "a".repeat(129)
}

#[test]
fn valid_names_are_accepted() {
    assert!(AppId::new("app.core-1").is_ok());
    assert!(CommandName::new("runtime.ping").is_ok());
    assert!(EventName::new("runtime.pong").is_ok());
    assert!(NodeId::new("node-1").is_ok());
}

#[test]
fn empty_is_rejected() {
    assert!(AppId::new("").is_err());
}

#[test]
fn spaces_are_rejected() {
    assert!(CommandName::new("runtime ping").is_err());
}

#[test]
fn traversal_is_rejected() {
    assert!(NodeId::new("../node").is_err());
}

#[test]
fn slash_is_rejected() {
    assert!(EventName::new("runtime/pong").is_err());
}

#[test]
fn too_long_is_rejected() {
    assert!(AppId::new(long_value()).is_err());
}

#[test]
fn control_characters_are_rejected() {
    assert!(AppId::new("app\ncore").is_err());
    assert!(AppId::new("app\tcore").is_err());
    assert!(AppId::new("app\0core").is_err());
}
