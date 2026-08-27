// =============================================================================
//        #######
//     ###       ###     F: service_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================
// appcore-norm: test

use super::*;
use crate::{GatewayConfig, GatewayState};
use appcore_security::HashTokenProvider;

fn state() -> GatewayState {
    GatewayState::new(
        GatewayConfig::new(([127, 0, 0, 1], 0).into(), "gateway.test"),
        HashTokenProvider::from_secret(vec![9; 32]).unwrap(),
    )
    .unwrap()
}

fn params(token: Option<&str>) -> ConnectionParams {
    ConnectionParams {
        tenant: None,
        cluster: None,
        installation: None,
        core: None,
        device: None,
        token: token.map(str::to_string),
        capabilities: None,
    }
}

#[test]
fn missing_and_query_credentials_increment_only_redacted_auth_counter() {
    let state = state();
    let headers = HeaderMap::new();

    assert!(matches!(
        authenticate_upgrade(&state, &headers, &params(None), "request-hash"),
        Err(UpgradeError::Missing)
    ));
    assert!(matches!(
        authenticate_upgrade(
            &state,
            &headers,
            &params(Some("must-not-appear")),
            "request-hash"
        ),
        Err(UpgradeError::QueryNotAllowed)
    ));

    let telemetry = state.metrics.telemetry_snapshot();
    assert_eq!(telemetry.authentication_failures, 2);
    assert!(!format!("{telemetry:?}").contains("must-not-appear"));
    assert!(telemetry.capabilities.is_empty());
}
