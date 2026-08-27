// =============================================================================
//        #######
//     ###       ###     F: redis_provider_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================
// appcore-norm: test

use super::*;

#[test]
fn redis_statuses_map_without_remote_text() {
    assert_eq!(status_result(STATUS_OK), Ok(()));
    assert_eq!(
        status_result(STATUS_CONFLICT),
        Err(GatewayRegistryError::Conflict)
    );
    assert_eq!(
        status_result(STATUS_STALE),
        Err(GatewayRegistryError::StaleOwner)
    );
    assert_eq!(
        status_result(STATUS_EXPIRED),
        Err(GatewayRegistryError::Expired)
    );
    assert_eq!(
        status_result(STATUS_UNSUPPORTED_SCHEMA),
        Err(GatewayRegistryError::UnsupportedSchema)
    );
    assert_eq!(
        status_result(STATUS_CAPACITY),
        Err(GatewayRegistryError::CapacityExceeded)
    );
    assert_eq!(
        status_result(STATUS_INVALID),
        Err(GatewayRegistryError::InvalidContract)
    );
    assert_eq!(status_result(99), Err(GatewayRegistryError::Unavailable));
}
