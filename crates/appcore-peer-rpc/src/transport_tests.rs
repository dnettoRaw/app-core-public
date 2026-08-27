// =============================================================================
//        #######
//     ###       ###     F: transport_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================
// appcore-norm: test

use super::*;
use crate::transport::{compress_request_body, request_content_type};
use crate::v2::{PEER_QUERY_BINARY_PATH_V2, PEER_QUERY_PATH_V2, PEER_RPC_BINARY_CONTENT_TYPE_V2};

fn request(path: &str, body: Vec<u8>) -> PeerRpcHttpRequest {
    PeerRpcHttpRequest {
        method: "POST".to_string(),
        path: path.to_string(),
        body,
        bearer_token: None,
        timeout_ms: 1_000,
        max_response_bytes: 1_048_576,
    }
}

#[test]
fn v2_codecs_keep_exact_authenticated_body_without_http_compression() {
    let body = vec![b'a'; COMPRESSION_THRESHOLD_BYTES * 2];
    for path in [PEER_QUERY_PATH_V2, PEER_QUERY_BINARY_PATH_V2] {
        let (encoded, compressed) = compress_request_body(&request(path, body.clone())).unwrap();
        assert_eq!(encoded, body);
        assert!(!compressed);
    }
    assert_eq!(
        request_content_type(PEER_QUERY_BINARY_PATH_V2),
        PEER_RPC_BINARY_CONTENT_TYPE_V2
    );
    assert_eq!(request_content_type(PEER_QUERY_PATH_V2), "application/json");
}

#[test]
fn v1_keeps_existing_bounded_http_compression() {
    let body = vec![b'a'; COMPRESSION_THRESHOLD_BYTES * 2];
    let (encoded, compressed) =
        compress_request_body(&request(PEER_QUERY_PATH, body.clone())).unwrap();
    assert!(compressed);
    assert!(encoded.len() < body.len());
    assert_eq!(request_content_type(PEER_QUERY_PATH), "application/json");
}
