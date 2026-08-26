// =============================================================================
//        #######
//     ###       ###     F: tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use crate::client::now_ms;
use crate::host::{accepts_gzip, decode_peer_envelope};
use crate::stream_host::peer_v2_query_handler;
use crate::transport::{gzip_if_beneficial, parse_http_response, PeerHttpScheme, PeerHttpTarget};
use crate::v2::*;
use appcore_core::{
    AppFamily, AppId, CapabilityName, CoreKind, InstanceId, NodeId, ProtocolVersion,
    RuntimeContractVersion, RuntimeIdentity, SyncGroup,
};
use axum::extract::{Extension, State};
use std::fs;
use std::io::{Cursor, Read};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::Arc;

fn validator() -> PeerRpcValidator {
    PeerRpcValidator::new(PeerRpcValidationConfig {
        local_tenant_id: TenantId::new("tenant-a").unwrap(),
        local_cluster_id: ClusterId::new("cluster-a").unwrap(),
        local_core_id: CoreId::new("core-b").unwrap(),
        max_payload_bytes: 1024,
        nonce_window_ms: 60_000,
    })
}

fn envelope() -> PeerRpcEnvelope {
    PeerRpcEnvelope::new(
        "req-1",
        "trace-1",
        CoreId::new("core-a").unwrap(),
        CoreId::new("core-b").unwrap(),
        TenantId::new("tenant-a").unwrap(),
        ClusterId::new("cluster-a").unwrap(),
        10,
        100,
        "nonce-1",
        CapabilityName::new("runtime.query").unwrap(),
        b"{}".to_vec(),
        None,
        None,
    )
}

fn identity(core_id: &str) -> CoreIdentity {
    CoreIdentity {
        tenant_id: TenantId::new("tenant-a").unwrap(),
        cluster_id: ClusterId::new("cluster-a").unwrap(),
        core_id: CoreId::new(core_id).unwrap(),
        instance_id: InstanceId::new(format!("{core_id}-instance")).unwrap(),
        kind: CoreKind::operational(),
        protocol_version: ProtocolVersion::new(1),
        runtime: RuntimeIdentity {
            app_id: AppId::new("app-a").unwrap(),
            app_family: AppFamily::new("family-a").unwrap(),
            sync_group: SyncGroup::new("cluster-a").unwrap(),
            runtime_contract: RuntimeContractVersion::new(1),
            node_id: NodeId::new(core_id).unwrap(),
        },
    }
}

fn manifest(core_id: &str) -> DistributedCoreManifest {
    DistributedCoreManifest {
        identity: identity(core_id),
        app_name: "App".to_string(),
        app_version: "0.1.0".to_string(),
        runtime_min_version: "0.6.1".to_string(),
        runtime_max_version: None,
        capabilities: Vec::new(),
        endpoints: Vec::new(),
        metadata: Default::default(),
    }
}

#[derive(Debug)]
struct NoopDispatcher;

impl PeerRpcDispatcher for NoopDispatcher {
    fn dispatch_peer_query(
        &self,
        envelope: PeerRpcEnvelope,
    ) -> Result<PeerRpcResponse, PeerRpcError> {
        Ok(PeerRpcResponse::ok(envelope.request_id, envelope.payload))
    }

    fn dispatch_peer_command(
        &self,
        envelope: PeerRpcEnvelope,
    ) -> Result<PeerRpcResponse, PeerRpcError> {
        Ok(PeerRpcResponse::ok(envelope.request_id, envelope.payload))
    }
}

struct EchoStreamDispatcher;

impl PeerRpcStreamDispatcherV2 for EchoStreamDispatcher {
    fn dispatch_peer_stream(
        &self,
        _open: PeerRpcStreamOpenV2,
        mut payload: PeerRpcStreamPayload,
        _cancellation: CancellationToken,
    ) -> Result<PeerRpcStreamResponseSourceV2, PeerRpcStreamErrorV2> {
        let mut response = Vec::new();
        payload
            .read_to_end(&mut response)
            .map_err(|_| PeerRpcStreamErrorV2::Io)?;
        Ok(PeerRpcStreamResponseSourceV2::new(
            response.len() as u64,
            Box::new(Cursor::new(response)),
        ))
    }
}

fn token_claims() -> TokenClaims {
    TokenClaims {
        issuer: "runtime-demo".to_string(),
        audience: "runtime-local".to_string(),
        salt: "peer".to_string(),
        ttl_ms: 60_000,
    }
}

#[derive(Debug, Clone)]
struct RecordingTransport {
    requests: Arc<std::sync::Mutex<Vec<PeerRpcHttpRequest>>>,
}

#[derive(Debug, Clone)]
struct RetryRecordingTransport {
    requests: Arc<std::sync::Mutex<Vec<PeerRpcHttpRequest>>>,
    attempts: Arc<AtomicUsize>,
    always_fail: bool,
    response_request_id: String,
}

impl PeerTransportProvider for RecordingTransport {
    fn send(
        &self,
        _base_url: &str,
        request: PeerRpcHttpRequest,
    ) -> Result<PeerRpcHttpResponse, PeerRpcError> {
        self.requests.lock().unwrap().push(request);
        Ok(PeerRpcHttpResponse {
            status_code: 200,
            body: serde_json::to_vec(&PeerRpcResponse::ok("req-1", b"ok".to_vec())).unwrap(),
        })
    }
}

impl PeerTransportProvider for RetryRecordingTransport {
    fn send(
        &self,
        _base_url: &str,
        request: PeerRpcHttpRequest,
    ) -> Result<PeerRpcHttpResponse, PeerRpcError> {
        self.requests.lock().unwrap().push(request);
        let attempt = self.attempts.fetch_add(1, AtomicOrdering::SeqCst);
        if self.always_fail || attempt == 0 {
            return Err(PeerRpcError::EndpointUnavailable);
        }
        Ok(PeerRpcHttpResponse {
            status_code: 200,
            body: serde_json::to_vec(&PeerRpcResponse::ok(
                self.response_request_id.clone(),
                b"ok".to_vec(),
            ))
            .unwrap(),
        })
    }
}

#[test]
fn validates_compatible_envelope() {
    assert!(validator().validate(&envelope(), 20).is_ok());
}

#[test]
fn rejects_tenant_mismatch() {
    let mut envelope = envelope();
    envelope.tenant_id = TenantId::new("tenant-b").unwrap();
    assert_eq!(
        validator().validate(&envelope, 20),
        Err(PeerRpcError::TenantMismatch)
    );
}

#[test]
fn rejects_cluster_mismatch() {
    let mut envelope = envelope();
    envelope.cluster_id = ClusterId::new("cluster-b").unwrap();
    assert_eq!(
        validator().validate(&envelope, 20),
        Err(PeerRpcError::ClusterMismatch)
    );
}

#[test]
fn rejects_expired_envelope() {
    assert_eq!(
        validator().validate(&envelope(), 100),
        Err(PeerRpcError::Expired)
    );
}

#[test]
fn rejects_nonce_replay() {
    let validator = validator();
    let envelope = envelope();

    assert!(validator.validate(&envelope, 20).is_ok());
    assert_eq!(
        validator.validate(&envelope, 21),
        Err(PeerRpcError::NonceReplay)
    );
}

#[test]
fn nonce_can_be_reused_after_configured_window() {
    let validator = PeerRpcValidator::new(PeerRpcValidationConfig {
        local_tenant_id: TenantId::new("tenant-a").unwrap(),
        local_cluster_id: ClusterId::new("cluster-a").unwrap(),
        local_core_id: CoreId::new("core-b").unwrap(),
        max_payload_bytes: 1024,
        nonce_window_ms: 10,
    });
    let mut envelope = envelope();
    envelope.expires_at_ms = 1_000;

    assert!(validator.validate(&envelope, 20).is_ok());
    envelope.timestamp_ms = 31;
    assert!(validator.validate(&envelope, 31).is_ok());
}

#[test]
fn rejects_body_hash_mismatch() {
    let mut envelope = envelope();
    envelope.body_hash = "bad".to_string();
    assert_eq!(
        validator().validate(&envelope, 20),
        Err(PeerRpcError::InvalidBodyHash)
    );
}

#[test]
fn rejects_incompatible_protocol_version() {
    let validator = validator().with_protocol_version(ProtocolVersion::new(2));

    assert_eq!(
        validator.validate(&envelope(), 20),
        Err(PeerRpcError::ProtocolMismatch)
    );
}

#[test]
fn hash_token_peer_issuer_creates_token_accepted_by_authenticator() {
    let provider =
        HashTokenProvider::from_secret(b"0123456789abcdef0123456789abcdef".to_vec()).unwrap();
    let issuer = HashTokenPeerTokenIssuer::new(provider.clone(), token_claims());
    let authenticator = HashTokenPeerAuthenticator::new(provider, token_claims());

    let token = issuer
        .issue_peer_token("req-1", Some("hash-1"), 10, 1_000)
        .unwrap();

    assert!(authenticator
        .authenticate(Some(&format!("Bearer {token}")), Some("hash-1"), 20)
        .is_ok());
    assert_eq!(
        authenticator.authenticate(Some(&format!("Bearer {token}")), Some("hash-2"), 20),
        Err(PeerRpcError::Forbidden)
    );
}

#[test]
fn signed_envelope_rejects_routing_metadata_tampering() {
    let provider =
        HashTokenProvider::from_secret(b"0123456789abcdef0123456789abcdef".to_vec()).unwrap();
    let issuer = HashTokenPeerTokenIssuer::new(provider.clone(), token_claims());
    let authenticator = HashTokenPeerAuthenticator::new(provider, token_claims());
    let envelope = envelope();
    let original_hash = envelope_signing_hash(&envelope);
    let token = issuer
        .issue_peer_token("req-1", Some(&original_hash), 10, 1_000)
        .unwrap();
    let mut tampered = envelope;
    tampered.target_core_id = CoreId::new("core-c").unwrap();

    assert_eq!(
        authenticator.authenticate(
            Some(&format!("Bearer {token}")),
            Some(&envelope_signing_hash(&tampered)),
            20
        ),
        Err(PeerRpcError::Forbidden)
    );
}

#[test]
fn signed_envelope_binds_protocol_version() {
    let mut envelope = envelope();
    let original_hash = envelope_signing_hash(&envelope);

    envelope.protocol_version = ProtocolVersion::new(2);

    assert_ne!(original_hash, envelope_signing_hash(&envelope));
}

#[test]
fn peer_credentials_are_redacted_from_debug() {
    let issuer = StaticPeerRpcTokenIssuer::new("peer-secret-token");
    let request = PeerRpcHttpRequest {
        method: "POST".to_string(),
        path: PEER_QUERY_PATH.to_string(),
        body: b"secret-body".to_vec(),
        bearer_token: Some("peer-secret-token".to_string()),
        timeout_ms: 10,
        max_response_bytes: 100,
    };

    assert!(!format!("{issuer:?}").contains("peer-secret-token"));
    let debug = format!("{request:?}");
    assert!(!debug.contains("peer-secret-token"));
    assert!(!debug.contains("secret-body"));
}

#[test]
fn peer_http_target_accepts_https_and_bracketed_ipv6() {
    let https = PeerHttpTarget::parse("https://example.com", PEER_QUERY_PATH).unwrap();
    let ipv6 = PeerHttpTarget::parse("http://[::1]:39301/base", PEER_QUERY_PATH).unwrap();

    assert_eq!(https.scheme, PeerHttpScheme::Https);
    assert_eq!(https.port, 443);
    assert_eq!(https.authority(), "example.com");
    assert_eq!(ipv6.host, "::1");
    assert_eq!(ipv6.authority(), "[::1]:39301");
    assert_eq!(ipv6.path, "/base/v1/peer/query");
}

#[test]
fn peer_http_response_decodes_chunked_body_with_limit() {
    let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n7\r\n{\"a\":1}\r\n0\r\n\r\n";
    let response = parse_http_response(raw, 32).unwrap();

    assert_eq!(response.body, br#"{"a":1}"#);
    assert_eq!(
        parse_http_response(raw, 4),
        Err(PeerRpcError::PayloadTooLarge)
    );
}

#[test]
fn peer_http_response_negotiates_bounded_gzip() {
    let body = vec![b'a'; COMPRESSION_THRESHOLD_BYTES * 2];
    let compressed = gzip_if_beneficial(&body).unwrap().unwrap();
    let mut raw = b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\n\r\n".to_vec();
    raw.extend_from_slice(&compressed);

    let response = parse_http_response(&raw, body.len()).unwrap();

    assert_eq!(response.body, body);
    assert_eq!(
        parse_http_response(&raw, body.len() - 1),
        Err(PeerRpcError::PayloadTooLarge)
    );
}

#[test]
fn peer_host_decodes_compressed_envelope_before_validation() {
    let provider = HashTokenProvider::from_secret(b"peer-test-secret-1234567890".to_vec())
        .expect("peer test provider");
    let state = PeerRpcHttpState {
        manifest: manifest("core-b"),
        validator: validator(),
        dispatcher: Arc::new(NoopDispatcher),
        authenticator: Arc::new(HashTokenPeerAuthenticator::new(provider, token_claims())),
    };
    let mut expected = envelope();
    expected.payload = vec![b'a'; COMPRESSION_THRESHOLD_BYTES * 2];
    expected.body_hash = payload_hash(&expected.payload);
    let body = serde_json::to_vec(&expected).unwrap();
    let compressed = gzip_if_beneficial(&body).unwrap().unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));

    let decoded = decode_peer_envelope(&state, &headers, &compressed).unwrap();

    assert_eq!(decoded, expected);
}

#[test]
fn gzip_accept_header_honors_explicit_disable() {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::ACCEPT_ENCODING,
        HeaderValue::from_static("br, gzip"),
    );
    assert!(accepts_gzip(&headers));

    headers.insert(
        header::ACCEPT_ENCODING,
        HeaderValue::from_static("gzip; q=0"),
    );
    assert!(!accepts_gzip(&headers));
}

#[test]
fn client_posts_signed_command_to_peer_command_route() {
    let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let provider =
        HashTokenProvider::from_secret(b"0123456789abcdef0123456789abcdef".to_vec()).unwrap();
    let client = PeerRpcClient::new(
        identity("core-a"),
        PeerRpcClientConfig::default(),
        RecordingTransport {
            requests: Arc::clone(&requests),
        },
        HashTokenPeerTokenIssuer::new(provider, token_claims()),
    );

    let response = client
        .command(
            "http://127.0.0.1:39301",
            PeerRpcOutboundRequest::new(
                "req-1",
                CoreId::new("core-b").unwrap(),
                CapabilityName::new("runtime.ping").unwrap(),
                b"{}".to_vec(),
                Some("idem-1".to_string()),
                None,
            ),
        )
        .unwrap();

    assert!(response.ok);
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, PEER_COMMAND_PATH);
    assert!(requests[0].bearer_token.is_some());
    let envelope = serde_json::from_slice::<PeerRpcEnvelope>(&requests[0].body).unwrap();
    assert_eq!(envelope.source_core_id.as_str(), "core-a");
    assert_eq!(envelope.target_core_id.as_str(), "core-b");
    assert_eq!(envelope.capability.as_str(), "runtime.ping");
    assert_eq!(envelope.idempotency_key.as_deref(), Some("idem-1"));
}

#[test]
fn retry_rebuilds_envelope_with_a_fresh_nonce() {
    let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let attempts = Arc::new(AtomicUsize::new(0));
    let client = PeerRpcClient::new(
        identity("core-a"),
        PeerRpcClientConfig {
            retry_policy: PeerRpcRetryPolicy {
                max_attempts: 2,
                initial_backoff_ms: 0,
                max_backoff_ms: 0,
            },
            ..PeerRpcClientConfig::default()
        },
        RetryRecordingTransport {
            requests: Arc::clone(&requests),
            attempts,
            always_fail: false,
            response_request_id: "req-1".to_string(),
        },
        StaticPeerRpcTokenIssuer::new("token"),
    );

    assert!(client
        .query(
            "http://127.0.0.1:39301",
            PeerRpcOutboundRequest::new(
                "req-1",
                CoreId::new("core-b").unwrap(),
                CapabilityName::new("runtime.query").unwrap(),
                Vec::new(),
                None,
                None,
            )
        )
        .is_ok());
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    let first = serde_json::from_slice::<PeerRpcEnvelope>(&requests[0].body).unwrap();
    let second = serde_json::from_slice::<PeerRpcEnvelope>(&requests[1].body).unwrap();
    assert_ne!(first.nonce, second.nonce);
}

#[test]
fn command_without_idempotency_is_not_retried() {
    let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let client = PeerRpcClient::new(
        identity("core-a"),
        PeerRpcClientConfig {
            retry_policy: PeerRpcRetryPolicy {
                max_attempts: 3,
                initial_backoff_ms: 0,
                max_backoff_ms: 0,
            },
            ..PeerRpcClientConfig::default()
        },
        RetryRecordingTransport {
            requests: Arc::clone(&requests),
            attempts: Arc::new(AtomicUsize::new(0)),
            always_fail: true,
            response_request_id: "req-1".to_string(),
        },
        StaticPeerRpcTokenIssuer::new("token"),
    );

    assert_eq!(
        client.command(
            "http://127.0.0.1:39301",
            PeerRpcOutboundRequest::new(
                "req-1",
                CoreId::new("core-b").unwrap(),
                CapabilityName::new("runtime.command").unwrap(),
                Vec::new(),
                None,
                None,
            )
        ),
        Err(PeerRpcError::EndpointUnavailable)
    );
    assert_eq!(requests.lock().unwrap().len(), 1);
}

#[test]
fn mismatched_response_request_id_is_rejected() {
    let client = PeerRpcClient::new(
        identity("core-a"),
        PeerRpcClientConfig {
            retry_policy: PeerRpcRetryPolicy {
                max_attempts: 2,
                initial_backoff_ms: 0,
                max_backoff_ms: 0,
            },
            ..PeerRpcClientConfig::default()
        },
        RetryRecordingTransport {
            requests: Arc::new(std::sync::Mutex::new(Vec::new())),
            attempts: Arc::new(AtomicUsize::new(1)),
            always_fail: false,
            response_request_id: "wrong-id".to_string(),
        },
        StaticPeerRpcTokenIssuer::new("token"),
    );

    assert!(matches!(
        client.query(
            "http://127.0.0.1:39301",
            PeerRpcOutboundRequest::new(
                "req-1",
                CoreId::new("core-b").unwrap(),
                CapabilityName::new("runtime.query").unwrap(),
                Vec::new(),
                None,
                None,
            )
        ),
        Err(PeerRpcError::InvalidResponse(message))
            if message == "peer response request_id mismatch"
    ));
}

#[test]
fn cancelled_peer_client_does_not_enter_transport() {
    let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let client = PeerRpcClient::new(
        identity("core-a"),
        PeerRpcClientConfig::default(),
        RetryRecordingTransport {
            requests: Arc::clone(&requests),
            attempts: Arc::new(AtomicUsize::new(0)),
            always_fail: false,
            response_request_id: "req-1".to_string(),
        },
        StaticPeerRpcTokenIssuer::new("token"),
    );
    client.cancel();

    assert_eq!(
        client.query(
            "http://127.0.0.1:39301",
            PeerRpcOutboundRequest::new(
                "req-1",
                CoreId::new("core-b").unwrap(),
                CapabilityName::new("runtime.query").unwrap(),
                Vec::new(),
                None,
                None,
            )
        ),
        Err(PeerRpcError::EndpointUnavailable)
    );
    assert!(requests.lock().unwrap().is_empty());
}

struct V2HttpFixture {
    path: std::path::PathBuf,
    registry: Arc<PeerRpcStreamRegistry>,
    issuer: HashTokenPeerTokenIssuer,
    state: PeerRpcHttpState,
    open: PeerRpcStreamOpenV2,
    now: u64,
}

fn v2_http_fixture() -> V2HttpFixture {
    let now = now_ms();
    let path =
        std::env::temp_dir().join(format!("appcore-peer-http-v2-{}-{now}", std::process::id()));
    fs::create_dir(&path).unwrap();
    #[cfg(unix)]
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    let registry = Arc::new(
        PeerRpcStreamRegistry::new(
            PeerRpcStreamRegistryConfig {
                max_sessions: 2,
                max_reserved_payload_bytes: 1_024,
                spool_directory: path.clone(),
                chunk_limits: PeerRpcChunkLimits {
                    max_chunk_bytes: 256,
                    max_encoded_chunk_bytes: 512,
                    max_payload_bytes: 1_024,
                    max_chunks: 4,
                },
            },
            Arc::new(EchoStreamDispatcher),
        )
        .unwrap(),
    );
    let provider =
        HashTokenProvider::from_secret(b"0123456789abcdef0123456789abcdef".to_vec()).unwrap();
    V2HttpFixture {
        path,
        registry,
        issuer: HashTokenPeerTokenIssuer::new(provider.clone(), token_claims()),
        state: PeerRpcHttpState {
            manifest: manifest("core-b"),
            validator: validator(),
            dispatcher: Arc::new(NoopDispatcher),
            authenticator: Arc::new(HashTokenPeerAuthenticator::new(provider, token_claims())),
        },
        open: PeerRpcStreamOpenV2 {
            protocol_version: ProtocolVersion::new(PEER_RPC_PROTOCOL_VERSION_V2),
            request_id: "request-http-1".to_string(),
            stream_id: "stream-http-1".to_string(),
            trace_id: "trace-http-1".to_string(),
            direction: PeerRpcStreamDirectionV2::Request,
            call_kind: PeerRpcCallKind::Query,
            source_core_id: CoreId::new("core-a").unwrap(),
            target_core_id: CoreId::new("core-b").unwrap(),
            tenant_id: TenantId::new("tenant-a").unwrap(),
            cluster_id: ClusterId::new("cluster-a").unwrap(),
            timestamp_ms: now,
            deadline_ms: now.saturating_add(60_000),
            nonce: "nonce-http-1".to_string(),
            capability: CapabilityName::new("runtime.query").unwrap(),
            payload_bytes: 4,
            chunk_bytes: 4,
            chunk_count: 1,
            idempotency_key: None,
            trace: None,
        },
        now,
    }
}

#[test]
fn v2_http_frame_authentication_binds_body_and_query_boundary() {
    let V2HttpFixture {
        path,
        registry,
        issuer,
        state,
        open,
        now,
    } = v2_http_fixture();
    let frame = PeerRpcStreamFrameV2::Open(Box::new(open.clone()));
    let hash = stream_frame_signing_hash(&frame).unwrap();
    let token = issuer
        .issue_peer_token(&open.request_id, Some(&hash), now, 60_000)
        .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    );
    let body = Bytes::from(serde_json::to_vec(&frame).unwrap());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let response = runtime.block_on(peer_v2_query_handler(
        State(state.clone()),
        Extension(Arc::clone(&registry)),
        headers.clone(),
        body,
    ));
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(registry.snapshot().unwrap().active_sessions, 1);
    assert_eq!(
        state.validator.validate_stream_open_v2(&open, now_ms()),
        Err(PeerRpcError::NonceReplay)
    );
    let mut wrong_tenant = open.clone();
    wrong_tenant.nonce = "nonce-http-tenant".to_string();
    wrong_tenant.tenant_id = TenantId::new("tenant-b").unwrap();
    assert_eq!(
        state
            .validator
            .validate_stream_open_v2(&wrong_tenant, now_ms()),
        Err(PeerRpcError::TenantMismatch)
    );

    let mut tampered = open.clone();
    tampered.stream_id = "stream-http-tampered".to_string();
    let response = runtime.block_on(peer_v2_query_handler(
        State(state.clone()),
        Extension(Arc::clone(&registry)),
        headers,
        Bytes::from(serde_json::to_vec(&PeerRpcStreamFrameV2::Open(Box::new(tampered))).unwrap()),
    ));
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let mut wrong_kind = open.clone();
    wrong_kind.request_id = "request-http-2".to_string();
    wrong_kind.stream_id = "stream-http-2".to_string();
    wrong_kind.nonce = "nonce-http-2".to_string();
    wrong_kind.call_kind = PeerRpcCallKind::Command;
    wrong_kind.idempotency_key = Some("idempotency-http-2".to_string());
    let wrong_frame = PeerRpcStreamFrameV2::Open(Box::new(wrong_kind.clone()));
    let wrong_hash = stream_frame_signing_hash(&wrong_frame).unwrap();
    let wrong_token = issuer
        .issue_peer_token(&wrong_kind.request_id, Some(&wrong_hash), now, 60_000)
        .unwrap();
    let mut wrong_headers = HeaderMap::new();
    wrong_headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {wrong_token}")).unwrap(),
    );
    let response = runtime.block_on(peer_v2_query_handler(
        State(state),
        Extension(Arc::clone(&registry)),
        wrong_headers,
        Bytes::from(serde_json::to_vec(&wrong_frame).unwrap()),
    ));
    assert_eq!(response.status(), StatusCode::CONFLICT);

    registry
        .cancel(&PeerRpcStreamCancelV2 {
            protocol_version: ProtocolVersion::new(PEER_RPC_PROTOCOL_VERSION_V2),
            request_id: open.request_id,
            stream_id: open.stream_id,
            reason: PeerRpcStreamCancelReasonV2::Caller,
        })
        .unwrap();
    assert_eq!(registry.snapshot().unwrap().active_sessions, 0);
    fs::remove_dir(path).unwrap();
}
