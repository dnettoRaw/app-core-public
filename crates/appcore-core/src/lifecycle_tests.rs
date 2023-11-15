// =============================================================================
//        #######
//     ###       ###     F: lifecycle_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/09 08:35:21 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{RuntimeLifecycle, RuntimeLifecycleEvent, RuntimeLifecycleState};

#[test]
fn new_starts_in_booting() {
    let lifecycle = RuntimeLifecycle::new();
    assert_eq!(lifecycle.current(), RuntimeLifecycleState::Booting);
}

#[test]
fn happy_path_reaches_running() {
    let lifecycle = RuntimeLifecycle::new();
    assert!(lifecycle.apply(RuntimeLifecycleEvent::ConfigLoaded).is_ok());
    assert!(lifecycle
        .apply(RuntimeLifecycleEvent::SecurityChecked)
        .is_ok());
    assert!(lifecycle
        .apply(RuntimeLifecycleEvent::StorageOpened)
        .is_ok());
    let state = lifecycle.apply(RuntimeLifecycleEvent::ApiStarted);
    assert!(state.is_ok());
    assert_eq!(lifecycle.current(), RuntimeLifecycleState::Running);
    assert!(lifecycle.is_running());
}

#[test]
fn running_can_become_degraded() {
    let lifecycle = RuntimeLifecycle::new();
    assert!(lifecycle.apply(RuntimeLifecycleEvent::ConfigLoaded).is_ok());
    assert!(lifecycle
        .apply(RuntimeLifecycleEvent::SecurityChecked)
        .is_ok());
    assert!(lifecycle
        .apply(RuntimeLifecycleEvent::StorageOpened)
        .is_ok());
    assert!(lifecycle.apply(RuntimeLifecycleEvent::ApiStarted).is_ok());
    let state = lifecycle.apply(RuntimeLifecycleEvent::DegradedDetected);
    assert!(state.is_ok());
    assert_eq!(lifecycle.current(), RuntimeLifecycleState::Degraded);
}

#[test]
fn degraded_recovers_to_running() {
    let lifecycle = RuntimeLifecycle::new();
    assert!(lifecycle.apply(RuntimeLifecycleEvent::ConfigLoaded).is_ok());
    assert!(lifecycle
        .apply(RuntimeLifecycleEvent::SecurityChecked)
        .is_ok());
    assert!(lifecycle
        .apply(RuntimeLifecycleEvent::StorageOpened)
        .is_ok());
    assert!(lifecycle.apply(RuntimeLifecycleEvent::ApiStarted).is_ok());
    assert!(lifecycle
        .apply(RuntimeLifecycleEvent::DegradedDetected)
        .is_ok());
    let state = lifecycle.apply(RuntimeLifecycleEvent::RecoveryCompleted);
    assert!(state.is_ok());
    assert_eq!(lifecycle.current(), RuntimeLifecycleState::Running);
}

#[test]
fn running_can_become_restricted() {
    let lifecycle = RuntimeLifecycle::new();
    assert!(lifecycle.apply(RuntimeLifecycleEvent::ConfigLoaded).is_ok());
    assert!(lifecycle
        .apply(RuntimeLifecycleEvent::SecurityChecked)
        .is_ok());
    assert!(lifecycle
        .apply(RuntimeLifecycleEvent::StorageOpened)
        .is_ok());
    assert!(lifecycle.apply(RuntimeLifecycleEvent::ApiStarted).is_ok());
    let state = lifecycle.apply(RuntimeLifecycleEvent::RestrictedDetected);
    assert!(state.is_ok());
    assert_eq!(lifecycle.current(), RuntimeLifecycleState::Restricted);
    assert!(lifecycle.is_restricted());
}

#[test]
fn restricted_recovers_to_running() {
    let lifecycle = RuntimeLifecycle::new();
    assert!(lifecycle.apply(RuntimeLifecycleEvent::ConfigLoaded).is_ok());
    assert!(lifecycle
        .apply(RuntimeLifecycleEvent::SecurityChecked)
        .is_ok());
    assert!(lifecycle
        .apply(RuntimeLifecycleEvent::StorageOpened)
        .is_ok());
    assert!(lifecycle.apply(RuntimeLifecycleEvent::ApiStarted).is_ok());
    assert!(lifecycle
        .apply(RuntimeLifecycleEvent::RestrictedDetected)
        .is_ok());
    let state = lifecycle.apply(RuntimeLifecycleEvent::RecoveryCompleted);
    assert!(state.is_ok());
    assert_eq!(lifecycle.current(), RuntimeLifecycleState::Running);
}

#[test]
fn shutdown_leads_to_stopped() {
    let lifecycle = RuntimeLifecycle::new();
    assert!(lifecycle.apply(RuntimeLifecycleEvent::ConfigLoaded).is_ok());
    assert!(lifecycle
        .apply(RuntimeLifecycleEvent::SecurityChecked)
        .is_ok());
    assert!(lifecycle
        .apply(RuntimeLifecycleEvent::StorageOpened)
        .is_ok());
    assert!(lifecycle.apply(RuntimeLifecycleEvent::ApiStarted).is_ok());
    assert!(lifecycle
        .apply(RuntimeLifecycleEvent::ShutdownRequested)
        .is_ok());
    let state = lifecycle.apply(RuntimeLifecycleEvent::ShutdownCompleted);
    assert!(state.is_ok());
    assert_eq!(lifecycle.current(), RuntimeLifecycleState::Stopped);
    assert!(lifecycle.is_stopped());
}

#[test]
fn invalid_transition_returns_error() {
    let lifecycle = RuntimeLifecycle::new();
    let result = lifecycle.apply(RuntimeLifecycleEvent::ApiStarted);
    assert!(result.is_err());
}
