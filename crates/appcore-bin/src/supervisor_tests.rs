// =============================================================================
//        #######
//     ###       ###     F: supervisor_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/04 11:57:41 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{should_check_health_tick, should_restart, should_restart_for_health};

#[test]
fn restart_policy_still_works() {
    assert!(should_restart(0, 1));
    assert!(!should_restart(1, 1));
}

#[test]
fn health_tick_policy_works() {
    assert!(!should_check_health_tick(0, 2));
    assert!(!should_check_health_tick(1, 2));
    assert!(should_check_health_tick(2, 2));
}

#[test]
fn health_fail_limit_policy_works() {
    assert!(!should_restart_for_health(1, 0));
    assert!(!should_restart_for_health(1, 2));
    assert!(should_restart_for_health(2, 2));
}
