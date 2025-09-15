// =============================================================================
//        #######
//     ###       ###     F: query_contract_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 13:45:20 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{QueryRequest, QueryRequestValidationError, QueryResponse};
use serde_json::json;

#[test]
fn query_request_validation_rules() {
    let valid = QueryRequest {
        query_name: "runtime.status".to_string(),
        query_id: "qry-1".to_string(),
        payload: json!({}),
    };
    assert!(valid.validate(1024).is_ok());
    assert_eq!(
        QueryRequest {
            query_name: "".to_string(),
            ..valid.clone()
        }
        .validate(1024),
        Err(QueryRequestValidationError::EmptyQueryName)
    );
    assert_eq!(
        QueryRequest {
            query_id: "".to_string(),
            ..valid.clone()
        }
        .validate(1024),
        Err(QueryRequestValidationError::EmptyQueryId)
    );
    assert_eq!(
        QueryRequest {
            query_name: "runtime status".to_string(),
            ..valid.clone()
        }
        .validate(1024),
        Err(QueryRequestValidationError::InvalidQueryName)
    );
    assert_eq!(
        QueryRequest {
            payload: json!({"x": "123456789"}),
            ..valid
        }
        .validate(8),
        Err(QueryRequestValidationError::PayloadTooLarge)
    );
}

#[test]
fn query_response_helpers() {
    let ok = QueryResponse::ok(json!({"a": 1}));
    assert!(ok.ok);
    let rejected = QueryResponse::rejected("no");
    assert!(!rejected.ok);
}

#[test]
fn query_v1_golden_fixtures_are_stable() {
    let request: QueryRequest =
        serde_json::from_str(include_str!("fixtures/query-request-v1.json")).unwrap();
    let request_round_trip = serde_json::to_value(&request).unwrap();
    let request_golden: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/query-request-v1.json")).unwrap();
    assert_eq!(request_round_trip, request_golden);

    let response: QueryResponse =
        serde_json::from_str(include_str!("fixtures/query-response-v1.json")).unwrap();
    let response_round_trip = serde_json::to_value(&response).unwrap();
    let response_golden: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/query-response-v1.json")).unwrap();
    assert_eq!(response_round_trip, response_golden);
}
