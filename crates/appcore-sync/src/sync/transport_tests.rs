// =============================================================================
//        #######
//     ###       ###     F: transport_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{decode_sync_message, read_http_request_body, HttpSyncTransport, SyncError};
use crate::sync::types::SyncMessage;
use appcore_core::{
    AppFamily, AppId, ClusterId, CoreId, CoreIdentity, CoreKind, InstanceId, NodeId,
    ProtocolVersion, RuntimeContractVersion, RuntimeIdentity, SyncGroup, TenantId,
};
use std::io::Write;
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

fn test_identity(node: &str) -> CoreIdentity {
    CoreIdentity {
        tenant_id: TenantId::new("tenant-a").unwrap(),
        cluster_id: ClusterId::new("cluster-a").unwrap(),
        core_id: CoreId::new(format!("core-{node}")).unwrap(),
        instance_id: InstanceId::new(format!("instance-{node}")).unwrap(),
        kind: CoreKind::new("replica").unwrap(),
        protocol_version: ProtocolVersion::new(1),
        runtime: RuntimeIdentity {
            app_id: AppId::new("app-a").unwrap(),
            app_family: AppFamily::new("family-a").unwrap(),
            sync_group: SyncGroup::new("cluster-a").unwrap(),
            runtime_contract: RuntimeContractVersion::new(1),
            node_id: NodeId::new(node).unwrap(),
        },
    }
}

fn post_response(
    response: Vec<u8>,
    configure: impl FnOnce(HttpSyncTransport) -> HttpSyncTransport,
) -> Result<(), SyncError> {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let port = listener.local_addr().expect("listener address").port();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept client");
        read_http_request_body(&mut stream).expect("read request body");
        stream.write_all(&response).expect("write response");
    });
    let message = SyncMessage::new_simple(
        NodeId::new("leader".to_string()).unwrap(),
        1,
        vec![b"x".to_vec()],
    );
    let result = configure(HttpSyncTransport::new("127.0.0.1", port))
        .with_source_identity(test_identity("leader"))
        .post_sync_events(&message);
    server.join().expect("join server");
    result
}

#[test]
fn accepts_any_2xx_http_status() {
    let result = post_response(
        b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n".to_vec(),
        |transport| transport,
    );
    assert!(result.is_ok());
}

#[test]
fn rejects_http_error_statuses() {
    for status in [401, 500] {
        let response = format!("HTTP/1.1 {status} Error\r\nContent-Length: 0\r\n\r\n");
        assert_eq!(
            post_response(response.into_bytes(), |transport| transport),
            Err(SyncError::HttpStatus(status))
        );
    }
}

#[test]
fn rejects_empty_http_response() {
    assert_eq!(
        post_response(Vec::new(), |transport| transport),
        Err(SyncError::EmptyHttpResponse)
    );
}

#[test]
fn rejects_http_response_above_limit() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nbody".to_vec();
    assert_eq!(
        post_response(response, |transport| transport.with_max_response_bytes(8)),
        Err(SyncError::ResponseTooLarge { max: 8 })
    );
}

#[test]
fn rejects_request_body_above_limit() {
    let message = SyncMessage::new_simple(
        NodeId::new("leader".to_string()).unwrap(),
        1,
        vec![b"x".to_vec()],
    );
    let result = HttpSyncTransport::new("127.0.0.1", 9)
        .with_source_identity(test_identity("leader"))
        .with_max_request_body_bytes(1)
        .post_sync_events(&message);
    assert!(matches!(
        result,
        Err(SyncError::RequestBodyTooLarge { size, max: 1 }) if size > 1
    ));
}

#[test]
fn transport_debug_output_does_not_expose_auth_token() {
    let transport = HttpSyncTransport::new("127.0.0.1", 9).with_auth_token("sensitive-peer-token");
    let output = format!("{transport:?}");

    assert!(output.contains("auth_configured: true"));
    assert!(!output.contains("sensitive-peer-token"));
}

#[test]
fn applies_configured_read_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let port = listener.local_addr().expect("listener address").port();
    let server = thread::spawn(move || {
        let (_stream, _) = listener.accept().expect("accept client");
        thread::sleep(Duration::from_millis(100));
    });
    let message = SyncMessage::new_simple(
        NodeId::new("leader".to_string()).unwrap(),
        1,
        vec![b"x".to_vec()],
    );
    let result = HttpSyncTransport::new("127.0.0.1", port)
        .with_source_identity(test_identity("leader"))
        .with_timeout_ms(10)
        .post_sync_events(&message);
    server.join().expect("join server");
    assert_eq!(result, Err(SyncError::TransportTimeout("read".to_string())));
}

#[test]
fn cancelled_sync_transport_rejects_request_before_connect() {
    let message = SyncMessage::new_simple(
        NodeId::new("leader".to_string()).unwrap(),
        1,
        vec![b"x".to_vec()],
    );
    let transport =
        HttpSyncTransport::new("127.0.0.1", 9).with_source_identity(test_identity("leader"));
    transport.cancel();

    assert_eq!(
        transport.post_sync_events(&message),
        Err(SyncError::TransportFailed(
            "sync transport cancelled".to_string()
        ))
    );
}

#[test]
fn rejects_empty_sync_message_body() {
    assert_eq!(decode_sync_message(""), Err(SyncError::EmptyRequestBody));
}

#[test]
fn rejects_unversioned_sync_message_without_node_id() {
    assert_eq!(
        decode_sync_message("batch-1\n\n1\n0\n"),
        Err(SyncError::InvalidSyncMessage(
            "NO MORE SUPPORTED PLEASE UPDATE"
        ))
    );
}

#[test]
fn rejects_malformed_sync_message_body() {
    assert!(decode_sync_message("{\"invalid\":true}").is_err());
}

#[test]
fn rejects_unversioned_sync_event_count() {
    assert_eq!(
        decode_sync_message("batch-1\nnode-a\n1\n1\n10001\nhash\n0\n\n"),
        Err(SyncError::InvalidSyncMessage(
            "NO MORE SUPPORTED PLEASE UPDATE"
        ))
    );
}

#[test]
fn rejects_unversioned_signature_format() {
    let payload = "batch-1\nnode-a\n1\n1\n1\nhash\n0\n\nsignature\n6869\n";
    assert_eq!(
        decode_sync_message(payload),
        Err(SyncError::InvalidSyncMessage(
            "NO MORE SUPPORTED PLEASE UPDATE"
        ))
    );
}
