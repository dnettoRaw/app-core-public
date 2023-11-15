// =============================================================================
//        #######
//     ###       ###     F: command_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/09 08:44:50 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::CommandRegistry;
use crate::ids::CommandName;

#[test]
fn register_command() {
    let mut registry = CommandRegistry::new();
    let name = CommandName::new("runtime.start".to_string()).unwrap();

    let result = registry.register(name.clone());

    assert!(result.is_ok());
    assert!(registry.contains(&name));
    assert_eq!(registry.len(), 1);
    assert!(!registry.is_empty());
}

#[test]
fn reject_duplicate_command() {
    let mut registry = CommandRegistry::new();
    let name = CommandName::new("runtime.start".to_string()).unwrap();

    let first = registry.register(name.clone());
    let second = registry.register(name);

    assert!(first.is_ok());
    assert!(second.is_err());
    assert_eq!(registry.len(), 1);
}

#[test]
fn list_preserves_registration_order() {
    let mut registry = CommandRegistry::new();
    let first = CommandName::new("runtime.start".to_string()).unwrap();
    let second = CommandName::new("runtime.shutdown".to_string()).unwrap();

    assert!(registry.register(first.clone()).is_ok());
    assert!(registry.register(second.clone()).is_ok());

    assert_eq!(registry.list(), &[first, second]);
}
