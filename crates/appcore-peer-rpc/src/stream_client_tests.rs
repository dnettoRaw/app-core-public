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

impl PeerTransportProvider for LoopbackStreamTransport {
    fn send(
        &self,
        _base_url: &str,
        request: PeerRpcHttpRequest,
    ) -> Result<PeerRpcHttpResponse, PeerRpcError> {
        self.largest_body
            .fetch_max(request.body.len(), AtomicOrdering::Relaxed);
        self.requests.fetch_add(1, AtomicOrdering::Relaxed);
        let kind = match request.path.as_str() {
            PEER_QUERY_PATH_V2 => PeerRpcCallKind::Query,
            PEER_COMMAND_PATH_V2 => PeerRpcCallKind::Command,
            _ => return Err(PeerRpcError::EndpointUnavailable),
        };
        if request.bearer_token.is_none() {
            return Err(PeerRpcError::Unauthorized);
        }
        let frame = serde_json::from_slice(&request.body)
            .map_err(|_| PeerRpcError::InvalidEnvelope("invalid_v2_frame".to_string()))?;
        let reply = self
            .registry
            .exchange(kind, frame, crate::client::now_ms())
            .map_err(|_| PeerRpcError::InvalidResponse("v2_frame_rejected".to_string()))?;
        Ok(PeerRpcHttpResponse {
            status_code: 200,
            body: serde_json::to_vec(&reply)
                .map_err(|_| PeerRpcError::InvalidResponse("invalid_v2_reply".to_string()))?,
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
