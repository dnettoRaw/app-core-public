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
use crate::transport::{request_content_type, take_transport_body};
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
        let mut request = request(path, body.clone());
        let body_pointer = request.body.as_ptr();
        let (encoded, compressed) = take_transport_body(&mut request).unwrap();
        assert_eq!(encoded, body);
        assert_eq!(encoded.as_ptr(), body_pointer);
        assert!(!compressed);
        assert!(request.body.is_empty());
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
    let mut request = request(PEER_QUERY_PATH, body.clone());
    let (encoded, compressed) = take_transport_body(&mut request).unwrap();
    assert!(compressed);
    assert!(encoded.len() < body.len());
    assert!(request.body.is_empty());
    assert_eq!(request_content_type(PEER_QUERY_PATH), "application/json");
}

#[test]
fn v1_uncompressed_body_transfers_its_allocation() {
    let mut request = request(PEER_QUERY_PATH, b"small-body".to_vec());
    let body_pointer = request.body.as_ptr();

    let (encoded, compressed) = take_transport_body(&mut request).unwrap();

    assert_eq!(encoded, b"small-body");
    assert_eq!(encoded.as_ptr(), body_pointer);
    assert!(!compressed);
    assert!(request.body.is_empty());
}
