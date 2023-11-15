// =============================================================================
//        #######
//     ###       ###     F: event_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/09 08:44:50 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::EventRegistry;
use crate::ids::EventName;

#[test]
fn register_event() {
    let mut registry = EventRegistry::new();
    let name = EventName::new("RuntimeStarted".to_string()).unwrap();

    let result = registry.register(name.clone());

    assert!(result.is_ok());
    assert!(registry.contains(&name));
    assert_eq!(registry.list(), &[name]);
}
