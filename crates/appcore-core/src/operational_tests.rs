// =============================================================================
//        #######
//     ###       ###     F: operational_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/20 23:03:47 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::RuntimeOperationalMode;

#[test]
fn degraded_allows_queries_but_not_writes() {
    assert!(RuntimeOperationalMode::Degraded.allows_local_queries());
    assert!(!RuntimeOperationalMode::Degraded.allows_writes());
}

#[test]
fn read_write_allows_writes() {
    assert!(RuntimeOperationalMode::ReadWrite.allows_writes());
}

#[test]
fn rejects_unversioned_readonly_spelling() {
    assert!(matches!(
        RuntimeOperationalMode::try_from("readonly"),
        Err(appcore_contracts::ContractError::InvalidValue {
            reason: "NO MORE SUPPORTED PLEASE UPDATE",
            ..
        })
    ));
}
