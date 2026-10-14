// =============================================================================
//        #######
//     ###       ###     F: protocol.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

// appcore-norm: test

use appcore_ui::{decode_ui_request_v1, UiActionV1, UiError, UiLimits};
fn query_frame() -> serde_json::Value {
    serde_json::json!({ "version":1, "surface_id":"surface-1", "request_id":"request-1", "timeout_ms":1000,
        "action": { "kind":"query", "query": { "query_name":"state.inspect", "query_id":"request-1", "payload":{"opaque":"private-content"} } } })
}
#[test]
fn existing_query_contract_round_trips_without_debug_payload_disclosure() {
    let frame = query_frame();
    let request =
        decode_ui_request_v1(&serde_json::to_vec(&frame).unwrap(), UiLimits::default()).unwrap();
    assert!(matches!(request.action, UiActionV1::Query { .. }));
    assert!(!format!("{request:?}").contains("private-content"));
    assert_eq!(serde_json::to_value(request).unwrap(), frame);
}
#[test]
fn versions_and_unknown_authority_fields_fail_closed() {
    for version in [
        serde_json::Value::Null,
        serde_json::json!(0),
        serde_json::json!(2),
        serde_json::json!("1"),
    ] {
        let mut frame = query_frame();
        frame["version"] = version;
        assert_eq!(
            decode_ui_request_v1(&serde_json::to_vec(&frame).unwrap(), UiLimits::default())
                .unwrap_err(),
            UiError::UnsupportedVersion
        );
    }
    let mut frame = query_frame();
    frame["principal"] = serde_json::json!("forged-admin");
    assert_eq!(
        decode_ui_request_v1(&serde_json::to_vec(&frame).unwrap(), UiLimits::default())
            .unwrap_err(),
        UiError::InvalidFrame
    );
}
#[test]
fn mutation_requires_idempotency_and_correlated_request_identity() {
    let mut frame = query_frame();
    frame["action"] = serde_json::json!({"kind":"command","command":{"command_name":"state.change","command_id":"request-1","payload":"{}","idempotency_key":null}});
    assert_eq!(
        decode_ui_request_v1(&serde_json::to_vec(&frame).unwrap(), UiLimits::default())
            .unwrap_err(),
        UiError::IdempotencyRequired
    );
    frame["action"]["command"]["idempotency_key"] = serde_json::json!("operation-1");
    assert!(
        decode_ui_request_v1(&serde_json::to_vec(&frame).unwrap(), UiLimits::default()).is_ok()
    );
    frame["action"]["command"]["command_id"] = serde_json::json!("another-request");
    assert_eq!(
        decode_ui_request_v1(&serde_json::to_vec(&frame).unwrap(), UiLimits::default())
            .unwrap_err(),
        UiError::InvalidRequest
    );
}
#[test]
fn frame_and_payload_bounds_are_independent_and_enforced() {
    let bytes = serde_json::to_vec(&query_frame()).unwrap();
    let limits = UiLimits {
        max_frame_bytes: bytes.len() - 1,
        max_payload_bytes: 8,
        ..Default::default()
    };
    assert_eq!(
        decode_ui_request_v1(&bytes, limits).unwrap_err(),
        UiError::PayloadTooLarge
    );
    let limits = UiLimits {
        max_payload_bytes: 8,
        ..Default::default()
    };
    assert_eq!(
        decode_ui_request_v1(&bytes, limits).unwrap_err(),
        UiError::PayloadTooLarge
    );
    assert_eq!(
        UiLimits {
            max_inflight: 0,
            ..Default::default()
        }
        .validate(),
        Err(UiError::InvalidRequest)
    );
}
