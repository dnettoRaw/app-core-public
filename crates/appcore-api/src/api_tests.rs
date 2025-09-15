// =============================================================================
//        #######
//     ###       ###     F: api_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/21 19:22:40 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{ApiMethod, ApiRequest, ApiResponse};

#[test]
fn api_request_basico() {
    let req = ApiRequest {
        method: ApiMethod::Query,
        path: "/api/query/runtime.test".to_string(),
        payload: vec![1],
    };
    assert_eq!(req.method, ApiMethod::Query);
    assert_eq!(req.path, "/api/query/runtime.test");
}

#[test]
fn api_response_basico() {
    let resp = ApiResponse {
        status_code: 200,
        payload: vec![2],
    };
    assert_eq!(resp.status_code, 200);
    assert_eq!(resp.payload, vec![2]);
}
