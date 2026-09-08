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

use super::{next_state, RuntimeLifecycle, RuntimeLifecycleEvent, RuntimeLifecycleState};

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
    assert_eq!(lifecycle.current(), RuntimeLifecycleState::Booting);
}

#[test]
fn transition_function_has_exactly_the_twelve_stable_edges() {
    let states = [
        RuntimeLifecycleState::Booting,
        RuntimeLifecycleState::LoadingConfig,
        RuntimeLifecycleState::CheckingSecurity,
        RuntimeLifecycleState::OpeningStorage,
        RuntimeLifecycleState::StartingApi,
        RuntimeLifecycleState::Running,
        RuntimeLifecycleState::Degraded,
        RuntimeLifecycleState::Restricted,
        RuntimeLifecycleState::ShuttingDown,
        RuntimeLifecycleState::Stopped,
    ];
    let events = [
        RuntimeLifecycleEvent::ConfigLoaded,
        RuntimeLifecycleEvent::SecurityChecked,
        RuntimeLifecycleEvent::StorageOpened,
        RuntimeLifecycleEvent::ApiStarted,
        RuntimeLifecycleEvent::DegradedDetected,
        RuntimeLifecycleEvent::RestrictedDetected,
        RuntimeLifecycleEvent::ShutdownRequested,
        RuntimeLifecycleEvent::ShutdownCompleted,
        RuntimeLifecycleEvent::RecoveryCompleted,
    ];

    let accepted = states
        .into_iter()
        .flat_map(|state| events.into_iter().map(move |event| (state, event)))
        .filter(|(state, event)| next_state(*state, *event).is_some())
        .count();

    assert_eq!(accepted, 12);
    assert_eq!(
        next_state(
            RuntimeLifecycleState::Degraded,
            RuntimeLifecycleEvent::ShutdownRequested,
        ),
        Some(RuntimeLifecycleState::ShuttingDown)
    );
    assert_eq!(
        next_state(
            RuntimeLifecycleState::Restricted,
            RuntimeLifecycleEvent::ShutdownRequested,
        ),
        Some(RuntimeLifecycleState::ShuttingDown)
    );
}

#[test]
fn clone_copies_only_the_current_lifecycle_state() {
    let lifecycle = RuntimeLifecycle::new();
    lifecycle
        .apply(RuntimeLifecycleEvent::ConfigLoaded)
        .unwrap();

    let cloned = lifecycle.clone();

    assert_eq!(cloned.current(), RuntimeLifecycleState::CheckingSecurity);
    cloned
        .apply(RuntimeLifecycleEvent::SecurityChecked)
        .unwrap();
    assert_eq!(lifecycle.current(), RuntimeLifecycleState::CheckingSecurity);
}
