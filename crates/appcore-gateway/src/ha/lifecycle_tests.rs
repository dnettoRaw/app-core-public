// =============================================================================
//        #######
//     ###       ###     F: lifecycle_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================
// appcore-norm: test

use super::*;

#[test]
fn lifecycle_admits_only_after_complete_recovery() {
    let lifecycle = GatewayHaLifecycle::new();
    assert_eq!(lifecycle.admit(), Err(GatewayRegistryError::Unavailable));
    lifecycle.begin_recovery(1_000).unwrap();
    assert_eq!(lifecycle.admit(), Err(GatewayRegistryError::Unavailable));
    lifecycle.mark_healthy(1_125).unwrap();
    assert_eq!(lifecycle.admit(), Ok(()));
    assert_eq!(
        lifecycle.snapshot(),
        GatewayHaLifecycleSnapshot {
            mode: GatewayHaMode::Healthy,
            transitions: 2,
            recoveries_started: 1,
            isolations: 0,
            fencing_rejections: 0,
            last_recovery_duration_ms: 125,
        }
    );
}

#[test]
fn isolation_is_idempotent_and_requires_recovery() {
    let lifecycle = GatewayHaLifecycle::new();
    lifecycle.begin_recovery(1_000).unwrap();
    lifecycle.mark_healthy(1_010).unwrap();
    lifecycle.isolate().unwrap();
    lifecycle.isolate().unwrap();
    assert_eq!(lifecycle.admit(), Err(GatewayRegistryError::Unavailable));
    lifecycle.begin_recovery(2_000).unwrap();
    lifecycle.record_fencing_rejection();
    lifecycle.mark_healthy(2_020).unwrap();
    let snapshot = lifecycle.snapshot();
    assert_eq!(snapshot.mode, GatewayHaMode::Healthy);
    assert_eq!(snapshot.transitions, 5);
    assert_eq!(snapshot.recoveries_started, 2);
    assert_eq!(snapshot.isolations, 1);
    assert_eq!(snapshot.fencing_rejections, 1);
    assert_eq!(snapshot.last_recovery_duration_ms, 20);
}

#[test]
fn invalid_transitions_fail_closed() {
    let lifecycle = GatewayHaLifecycle::new();
    assert_eq!(
        lifecycle.mark_healthy(1),
        Err(GatewayRegistryError::InvalidContract)
    );
    assert_eq!(
        lifecycle.isolate(),
        Err(GatewayRegistryError::InvalidContract)
    );
    lifecycle.begin_recovery(1).unwrap();
    lifecycle.mark_healthy(2).unwrap();
    assert_eq!(
        lifecycle.begin_recovery(3),
        Err(GatewayRegistryError::InvalidContract)
    );
    lifecycle.stop();
    lifecycle.stop();
    assert_eq!(lifecycle.snapshot().mode, GatewayHaMode::Stopped);
}
