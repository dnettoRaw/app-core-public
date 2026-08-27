// =============================================================================
//        #######
//     ###       ###     F: stream_client_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================
// appcore-norm: test

use super::*;
use crate::v2::*;
use appcore_core::{
    AppFamily, AppId, CapabilityName, CoreKind, InstanceId, NodeId, RuntimeContractVersion,
    RuntimeIdentity, SyncGroup,
};
use std::fs;
use std::io::{Cursor, Read};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::Mutex;

struct SpoolEchoDispatcher;

impl PeerRpcStreamDispatcherV2 for SpoolEchoDispatcher {
    fn dispatch_peer_stream(
        &self,
        _open: PeerRpcStreamOpenV2,
        payload: PeerRpcStreamPayload,
        _cancellation: CancellationToken,
    ) -> Result<PeerRpcStreamResponseSourceV2, PeerRpcStreamErrorV2> {
        Ok(PeerRpcStreamResponseSourceV2::new(
            payload.len(),
            Box::new(payload),
        ))
    }
}

#[derive(Clone)]
struct LoopbackStreamTransport {
    registry: Arc<PeerRpcStreamRegistry>,
    largest_body: Arc<AtomicUsize>,
    requests: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct RejectingStreamTransport {
    error: PeerRpcWireErrorV2,
    requests: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct UnsupportedBinaryTransport {
    paths: Arc<Mutex<Vec<String>>>,
}

impl PeerTransportProvider for LoopbackStreamTransport {
    fn send(
        &self,
        _base_url: &str,
        request: PeerRpcHttpRequest,
    ) -> Result<PeerRpcHttpResponse, PeerRpcError> {
        self.largest_body
            .fetch_max(request.body.len(), AtomicOrdering::Relaxed);
        self.requests.fetch_add(1, AtomicOrdering::Relaxed);
        let (kind, codec) = match request.path.as_str() {
            PEER_QUERY_PATH_V2 => (PeerRpcCallKind::Query, PeerRpcStreamCodecV2::Json),
            PEER_COMMAND_PATH_V2 => (PeerRpcCallKind::Command, PeerRpcStreamCodecV2::Json),
            PEER_QUERY_BINARY_PATH_V2 => (PeerRpcCallKind::Query, PeerRpcStreamCodecV2::Binary),
            PEER_COMMAND_BINARY_PATH_V2 => (PeerRpcCallKind::Command, PeerRpcStreamCodecV2::Binary),
            _ => return Err(PeerRpcError::EndpointUnavailable),
        };
        if request.bearer_token.is_none() {
            return Err(PeerRpcError::Unauthorized);
        }
        let frame = match codec {
            PeerRpcStreamCodecV2::Json => serde_json::from_slice(&request.body)
                .map_err(|_| PeerRpcError::InvalidEnvelope("invalid_v2_frame".to_string()))?,
            PeerRpcStreamCodecV2::Binary => {
                decode_binary_frame_v2(&request.body, MAX_PEER_RPC_BINARY_FRAME_BYTES_V2)
                    .map_err(|_| PeerRpcError::InvalidEnvelope("invalid_v2_frame".to_string()))?
            }
        };
        let reply = self
            .registry
            .exchange(kind, frame, crate::client::now_ms())
            .map_err(|_| PeerRpcError::InvalidResponse("v2_frame_rejected".to_string()))?;
        let body = match codec {
            PeerRpcStreamCodecV2::Json => serde_json::to_vec(&reply)
                .map_err(|_| PeerRpcError::InvalidResponse("invalid_v2_reply".to_string()))?,
            PeerRpcStreamCodecV2::Binary => {
                encode_binary_reply_v2(&reply, MAX_PEER_RPC_BINARY_FRAME_BYTES_V2)
                    .map_err(|_| PeerRpcError::InvalidResponse("invalid_v2_reply".to_string()))?
            }
        };
        Ok(PeerRpcHttpResponse {
            status_code: 200,
            body,
        })
    }
}

impl PeerTransportProvider for RejectingStreamTransport {
    fn send(
        &self,
        _base_url: &str,
        _request: PeerRpcHttpRequest,
    ) -> Result<PeerRpcHttpResponse, PeerRpcError> {
        self.requests.fetch_add(1, AtomicOrdering::Relaxed);
        Ok(PeerRpcHttpResponse {
            status_code: 503,
            body: serde_json::to_vec(&self.error)
                .map_err(|error| PeerRpcError::InvalidResponse(error.to_string()))?,
        })
    }
}

impl PeerTransportProvider for UnsupportedBinaryTransport {
    fn send(
        &self,
        _base_url: &str,
        request: PeerRpcHttpRequest,
    ) -> Result<PeerRpcHttpResponse, PeerRpcError> {
        self.paths.lock().unwrap().push(request.path);
        Ok(PeerRpcHttpResponse {
            status_code: 404,
            body: Vec::new(),
        })
    }
}

struct CountingReader {
    cursor: Cursor<Vec<u8>>,
    largest_read: Arc<AtomicUsize>,
}

impl Read for CountingReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.largest_read
            .fetch_max(buffer.len(), AtomicOrdering::Relaxed);
        self.cursor.read(buffer)
    }
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "appcore-peer-client-v2-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, AtomicOrdering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        #[cfg(unix)]
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.0);
    }
}

fn source_identity() -> CoreIdentity {
    CoreIdentity {
        tenant_id: TenantId::new("tenant-a").unwrap(),
        cluster_id: ClusterId::new("cluster-a").unwrap(),
        core_id: CoreId::new("core-a").unwrap(),
        instance_id: InstanceId::new("core-a-instance").unwrap(),
        kind: CoreKind::operational(),
        protocol_version: ProtocolVersion::new(1),
        runtime: RuntimeIdentity {
            app_id: AppId::new("app-a").unwrap(),
            app_family: AppFamily::new("family-a").unwrap(),
            sync_group: SyncGroup::new("cluster-a").unwrap(),
            runtime_contract: RuntimeContractVersion::new(1),
            node_id: NodeId::new("node-a").unwrap(),
        },
    }
}

#[test]
fn client_streams_large_query_request_and_response_with_bounded_frames() {
    let directory = TestDirectory::create();
    let registry = Arc::new(
        PeerRpcStreamRegistry::new(
            PeerRpcStreamRegistryConfig {
                max_sessions: 2,
                max_reserved_payload_bytes: 8 * 1024 * 1024,
                spool_directory: directory.path().to_path_buf(),
                chunk_limits: PeerRpcChunkLimits::default(),
            },
            Arc::new(SpoolEchoDispatcher),
        )
        .unwrap(),
    );
    let largest_body = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(AtomicUsize::new(0));
    let client = PeerRpcClient::new(
        source_identity(),
        PeerRpcClientConfig::default(),
        LoopbackStreamTransport {
            registry: Arc::clone(&registry),
            largest_body: Arc::clone(&largest_body),
            requests: Arc::clone(&requests),
        },
        StaticPeerRpcTokenIssuer::new("test-token"),
    );
    let payload = vec![b'a'; 2 * 1024 * 1024 + 17];
    let largest_read = Arc::new(AtomicUsize::new(0));
    let source = CountingReader {
        cursor: Cursor::new(payload.clone()),
        largest_read: Arc::clone(&largest_read),
    };
    let output = client
        .query_stream_v2(
            "http://peer.invalid",
            PeerRpcStreamRequestV2::new(
                "request-large-1",
                CoreId::new("core-b").unwrap(),
                CapabilityName::new("runtime.query").unwrap(),
                payload.len() as u64,
                None,
                None,
            ),
            source,
            Vec::new(),
        )
        .unwrap();

    assert_eq!(output, payload);
    assert!(largest_read.load(AtomicOrdering::Relaxed) <= 64 * 1024);
    assert!(largest_body.load(AtomicOrdering::Relaxed) < 160 * 1024);
    assert!(requests.load(AtomicOrdering::Relaxed) > 64);
    let snapshot = registry.snapshot().unwrap();
    assert_eq!(snapshot.active_sessions, 0);
    assert_eq!(snapshot.reserved_payload_bytes, 0);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn binary_client_streams_native_bytes_through_only_binary_routes() {
    let directory = TestDirectory::create();
    let registry = Arc::new(
        PeerRpcStreamRegistry::new(
            PeerRpcStreamRegistryConfig {
                max_sessions: 2,
                max_reserved_payload_bytes: 8 * 1024 * 1024,
                spool_directory: directory.path().to_path_buf(),
                chunk_limits: PeerRpcChunkLimits::default(),
            },
            Arc::new(SpoolEchoDispatcher),
        )
        .unwrap(),
    );
    let largest_body = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(AtomicUsize::new(0));
    let client = PeerRpcClient::new(
        source_identity(),
        PeerRpcClientConfig::default(),
        LoopbackStreamTransport {
            registry: Arc::clone(&registry),
            largest_body: Arc::clone(&largest_body),
            requests: Arc::clone(&requests),
        },
        StaticPeerRpcTokenIssuer::new("test-token"),
    )
    .with_stream_codec_v2(PeerRpcStreamCodecV2::Binary);
    let payload = pseudo_random_payload(2 * 1024 * 1024 + 17);
    let output = client
        .query_stream_v2(
            "http://peer.invalid",
            PeerRpcStreamRequestV2::new(
                "request-binary-1",
                CoreId::new("core-b").unwrap(),
                CapabilityName::new("runtime.query").unwrap(),
                payload.len() as u64,
                None,
                None,
            ),
            Cursor::new(payload.clone()),
            Vec::new(),
        )
        .unwrap();

    assert_eq!(output, payload);
    assert!(largest_body.load(AtomicOrdering::Relaxed) < 72 * 1024);
    assert!(requests.load(AtomicOrdering::Relaxed) > 64);
    assert_eq!(registry.snapshot().unwrap().active_sessions, 0);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn unavailable_binary_codec_never_falls_back_to_json() {
    let paths = Arc::new(Mutex::new(Vec::new()));
    let client = PeerRpcClient::new(
        source_identity(),
        PeerRpcClientConfig::default(),
        UnsupportedBinaryTransport {
            paths: Arc::clone(&paths),
        },
        StaticPeerRpcTokenIssuer::new("test-token"),
    )
    .with_stream_codec_v2(PeerRpcStreamCodecV2::Binary);
    let result = client.query_stream_v2(
        "http://peer.invalid",
        PeerRpcStreamRequestV2::new(
            "request-no-downgrade-1",
            CoreId::new("core-b").unwrap(),
            CapabilityName::new("runtime.query").unwrap(),
            0,
            None,
            None,
        ),
        Cursor::new(Vec::new()),
        Vec::new(),
    );

    assert!(matches!(
        result,
        Err(PeerRpcStreamClientErrorV2::Transport(_))
    ));
    let paths = paths.lock().unwrap();
    assert_eq!(paths.len(), 2);
    assert!(paths.iter().all(|path| path == PEER_QUERY_BINARY_PATH_V2));
}

fn pseudo_random_payload(bytes: usize) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    (0..bytes)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

#[test]
fn v2_command_requires_idempotency_before_transport() {
    let directory = TestDirectory::create();
    let registry = Arc::new(
        PeerRpcStreamRegistry::new(
            PeerRpcStreamRegistryConfig {
                max_sessions: 1,
                max_reserved_payload_bytes: 1_024,
                spool_directory: directory.path().to_path_buf(),
                chunk_limits: PeerRpcChunkLimits::default(),
            },
            Arc::new(SpoolEchoDispatcher),
        )
        .unwrap(),
    );
    let requests = Arc::new(AtomicUsize::new(0));
    let client = PeerRpcClient::new(
        source_identity(),
        PeerRpcClientConfig::default(),
        LoopbackStreamTransport {
            registry,
            largest_body: Arc::new(AtomicUsize::new(0)),
            requests: Arc::clone(&requests),
        },
        StaticPeerRpcTokenIssuer::new("test-token"),
    );
    let result = client.command_stream_v2(
        "http://peer.invalid",
        PeerRpcStreamRequestV2::new(
            "request-command-1",
            CoreId::new("core-b").unwrap(),
            CapabilityName::new("runtime.command").unwrap(),
            0,
            None,
            None,
        ),
        Cursor::new(Vec::new()),
        Vec::new(),
    );
    assert!(matches!(
        result,
        Err(PeerRpcStreamClientErrorV2::Stream(
            PeerRpcStreamErrorV2::InvalidConfig
        ))
    ));
    assert_eq!(requests.load(AtomicOrdering::Relaxed), 0);
}

#[test]
fn v2_client_returns_validated_typed_remote_error_without_frame_retry() {
    let requests = Arc::new(AtomicUsize::new(0));
    let error = PeerRpcWireErrorV2::controlled(
        Some("request-capacity-1".to_string()),
        None,
        PeerRpcWireErrorCodeV2::CapacityExceeded,
    );
    let client = PeerRpcClient::new(
        source_identity(),
        PeerRpcClientConfig::default(),
        RejectingStreamTransport {
            error,
            requests: Arc::clone(&requests),
        },
        StaticPeerRpcTokenIssuer::new("test-token"),
    );
    let result = client.query_stream_v2(
        "http://peer.invalid",
        PeerRpcStreamRequestV2::new(
            "request-capacity-1",
            CoreId::new("core-b").unwrap(),
            CapabilityName::new("runtime.query").unwrap(),
            0,
            None,
            None,
        ),
        Cursor::new(Vec::new()),
        Vec::new(),
    );

    assert!(matches!(
        result,
        Err(PeerRpcStreamClientErrorV2::Remote(error))
            if error.code == PeerRpcWireErrorCodeV2::CapacityExceeded
                && error.retryable
                && error.phase == PeerRpcWireErrorPhaseV2::Admission
                && error.retry_after_ms == Some(100)
    ));
    assert_eq!(requests.load(AtomicOrdering::Relaxed), 2);
}

#[test]
fn v2_client_rejects_contradictory_remote_retry_metadata() {
    let requests = Arc::new(AtomicUsize::new(0));
    let mut error = PeerRpcWireErrorV2::controlled(
        Some("request-forbidden-1".to_string()),
        None,
        PeerRpcWireErrorCodeV2::Forbidden,
    );
    error.retryable = true;
    let client = PeerRpcClient::new(
        source_identity(),
        PeerRpcClientConfig::default(),
        RejectingStreamTransport {
            error,
            requests: Arc::clone(&requests),
        },
        StaticPeerRpcTokenIssuer::new("test-token"),
    );
    let result = client.query_stream_v2(
        "http://peer.invalid",
        PeerRpcStreamRequestV2::new(
            "request-forbidden-1",
            CoreId::new("core-b").unwrap(),
            CapabilityName::new("runtime.query").unwrap(),
            0,
            None,
            None,
        ),
        Cursor::new(Vec::new()),
        Vec::new(),
    );

    assert!(matches!(
        result,
        Err(PeerRpcStreamClientErrorV2::InvalidResponse)
    ));
    assert_eq!(requests.load(AtomicOrdering::Relaxed), 2);
}
