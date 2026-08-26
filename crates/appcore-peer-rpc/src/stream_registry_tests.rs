// =============================================================================
//        #######
//     ###       ###     F: stream_registry_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================
// appcore-norm: test

use super::*;
use crate::v2::*;
use std::fs;
use std::io::{Cursor, Read};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct EchoDispatcher;

impl PeerRpcStreamDispatcherV2 for EchoDispatcher {
    fn dispatch_peer_stream(
        &self,
        _open: PeerRpcStreamOpenV2,
        mut payload: PeerRpcStreamPayload,
        cancellation: CancellationToken,
    ) -> Result<PeerRpcStreamResponseSourceV2, PeerRpcStreamErrorV2> {
        if cancellation.is_cancelled() {
            return Err(PeerRpcStreamErrorV2::Cancelled);
        }
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

struct OversizedResponseDispatcher;

impl PeerRpcStreamDispatcherV2 for OversizedResponseDispatcher {
    fn dispatch_peer_stream(
        &self,
        _open: PeerRpcStreamOpenV2,
        _payload: PeerRpcStreamPayload,
        _cancellation: CancellationToken,
    ) -> Result<PeerRpcStreamResponseSourceV2, PeerRpcStreamErrorV2> {
        Ok(PeerRpcStreamResponseSourceV2::new(
            2_000,
            Box::new(Cursor::new(Vec::new())),
        ))
    }
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "appcore-peer-stream-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        #[cfg(unix)]
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn entry_count(&self) -> usize {
        fs::read_dir(&self.0).unwrap().count()
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.0);
    }
}

fn request_open(
    request_id: &str,
    stream_id: &str,
    payload_bytes: u64,
    chunk_bytes: u32,
) -> PeerRpcStreamOpenV2 {
    let chunk_count = if payload_bytes == 0 {
        0
    } else {
        ((payload_bytes - 1) / u64::from(chunk_bytes) + 1) as u32
    };
    PeerRpcStreamOpenV2 {
        protocol_version: ProtocolVersion::new(PEER_RPC_PROTOCOL_VERSION_V2),
        request_id: request_id.to_string(),
        stream_id: stream_id.to_string(),
        trace_id: format!("trace-{request_id}"),
        direction: PeerRpcStreamDirectionV2::Request,
        call_kind: PeerRpcCallKind::Query,
        source_core_id: CoreId::new("core-a").unwrap(),
        target_core_id: CoreId::new("core-b").unwrap(),
        tenant_id: TenantId::new("tenant-a").unwrap(),
        cluster_id: ClusterId::new("cluster-a").unwrap(),
        timestamp_ms: 10,
        deadline_ms: 1_000,
        nonce: format!("nonce-{request_id}"),
        capability: appcore_core::CapabilityName::new("runtime.query").unwrap(),
        payload_bytes,
        chunk_bytes,
        chunk_count,
        idempotency_key: None,
        trace: None,
    }
}

fn frames(open: PeerRpcStreamOpenV2, payload: Vec<u8>) -> Vec<PeerRpcStreamFrameV2> {
    let mut encoder = PeerRpcChunkEncoder::new(
        open,
        Cursor::new(payload),
        PeerRpcChunkLimits::default(),
        CancellationToken::new(),
        20,
    )
    .unwrap();
    let mut frames = Vec::new();
    while let Some(frame) = encoder.next_frame(20).unwrap() {
        frames.push(frame);
    }
    frames
}

fn registry(
    directory: &TestDirectory,
    dispatcher: Arc<dyn PeerRpcStreamDispatcherV2>,
    max_sessions: usize,
    max_reserved_payload_bytes: u64,
) -> PeerRpcStreamRegistry {
    PeerRpcStreamRegistry::new(
        PeerRpcStreamRegistryConfig {
            max_sessions,
            max_reserved_payload_bytes,
            spool_directory: directory.path().to_path_buf(),
            chunk_limits: PeerRpcChunkLimits {
                max_chunk_bytes: 256,
                max_encoded_chunk_bytes: 512,
                max_payload_bytes: 1_024,
                max_chunks: 16,
            },
        },
        dispatcher,
    )
    .unwrap()
}

fn submit(
    registry: &PeerRpcStreamRegistry,
    frames: &[PeerRpcStreamFrameV2],
) -> PeerRpcStreamReplyV2 {
    let PeerRpcStreamFrameV2::Open(open) = &frames[0] else {
        unreachable!()
    };
    registry.open(open.as_ref().clone(), 20).unwrap();
    for frame in &frames[1..frames.len() - 1] {
        let PeerRpcStreamFrameV2::Chunk(chunk) = frame else {
            unreachable!()
        };
        registry.push_chunk(chunk.clone(), 20).unwrap();
    }
    let PeerRpcStreamFrameV2::Commit(commit) = frames.last().unwrap() else {
        unreachable!()
    };
    registry.commit(commit.clone(), 20).unwrap()
}

#[cfg(unix)]
#[test]
fn registry_rejects_spool_directory_accessible_by_other_users() {
    let directory = TestDirectory::create();
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let result = PeerRpcStreamRegistry::new(
        PeerRpcStreamRegistryConfig {
            max_sessions: 1,
            max_reserved_payload_bytes: 1_024,
            spool_directory: directory.path().to_path_buf(),
            chunk_limits: PeerRpcChunkLimits::default(),
        },
        Arc::new(EchoDispatcher),
    );
    assert!(matches!(result, Err(PeerRpcStreamErrorV2::InvalidConfig)));
    assert_eq!(directory.entry_count(), 0);
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn registry_spools_request_and_response_with_exact_cleanup() {
    let directory = TestDirectory::create();
    let registry = registry(&directory, Arc::new(EchoDispatcher), 2, 2_048);
    let payload = vec![b'a'; 777];
    let request_frames = frames(
        request_open("request-1", "stream-1", 777, 128),
        payload.clone(),
    );
    let reply = submit(&registry, &request_frames);
    let response_open = match reply.response_frame.unwrap().as_ref() {
        PeerRpcStreamFrameV2::Open(open) => open.as_ref().clone(),
        _ => panic!("commit must return the response open frame"),
    };
    let response_stream_id = response_open.stream_id.clone();
    let mut assembler = Some(
        PeerRpcChunkAssembler::new(
            response_open,
            Vec::new(),
            PeerRpcChunkLimits::default(),
            CancellationToken::new(),
            20,
        )
        .unwrap(),
    );
    let mut completed = None;
    loop {
        let reply = registry
            .pull(
                PeerRpcStreamPullV2 {
                    protocol_version: ProtocolVersion::new(PEER_RPC_PROTOCOL_VERSION_V2),
                    request_id: "request-1".to_string(),
                    stream_id: response_stream_id.clone(),
                },
                20,
            )
            .unwrap();
        match reply.response_frame.map(|frame| *frame) {
            Some(PeerRpcStreamFrameV2::Chunk(chunk)) => {
                assembler.as_mut().unwrap().push_chunk(chunk, 20).unwrap();
            }
            Some(PeerRpcStreamFrameV2::Commit(commit)) => {
                completed = Some(assembler.take().unwrap().finish(commit, 20).unwrap());
            }
            None if reply.complete => break,
            _ => panic!("unexpected response frame"),
        }
        if reply.complete {
            break;
        }
    }
    assert_eq!(completed.unwrap(), payload);
    assert_eq!(registry.snapshot().unwrap().active_sessions, 0);
    assert_eq!(registry.snapshot().unwrap().reserved_payload_bytes, 0);
    assert_eq!(directory.entry_count(), 0);
}

#[test]
fn admission_limits_report_saturation_and_cancel_releases_reservation() {
    let directory = TestDirectory::create();
    let registry = registry(&directory, Arc::new(EchoDispatcher), 1, 100);
    let first = request_open("request-1", "stream-1", 80, 20);
    registry.open(first.clone(), 20).unwrap();
    let second = request_open("request-2", "stream-2", 20, 20);
    assert_eq!(
        registry.open(second, 20),
        Err(PeerRpcStreamErrorV2::CapacityExceeded)
    );
    assert_eq!(registry.snapshot().unwrap().saturation_count, 1);
    assert_eq!(registry.snapshot().unwrap().reserved_payload_bytes, 80);
    assert!(registry
        .cancel(&PeerRpcStreamCancelV2 {
            protocol_version: ProtocolVersion::new(PEER_RPC_PROTOCOL_VERSION_V2),
            request_id: first.request_id,
            stream_id: first.stream_id,
            reason: PeerRpcStreamCancelReasonV2::Caller,
        })
        .unwrap());
    let snapshot = registry.snapshot().unwrap();
    assert_eq!(snapshot.active_sessions, 0);
    assert_eq!(snapshot.reserved_payload_bytes, 0);
    assert_eq!(snapshot.cleanup_count, 1);
    assert_eq!(directory.entry_count(), 0);
}

#[test]
fn request_registry_rejects_response_direction_before_creating_state() {
    let directory = TestDirectory::create();
    let registry = registry(&directory, Arc::new(EchoDispatcher), 1, 100);
    let mut response = request_open("request-1", "stream-1", 20, 20);
    response.direction = PeerRpcStreamDirectionV2::Response;
    assert_eq!(
        registry.open(response, 20),
        Err(PeerRpcStreamErrorV2::DirectionMismatch)
    );
    assert_eq!(registry.snapshot().unwrap().active_sessions, 0);
    assert_eq!(directory.entry_count(), 0);
}

#[test]
fn invalid_chunk_and_expiry_remove_partial_spools() {
    let directory = TestDirectory::create();
    let registry = registry(&directory, Arc::new(EchoDispatcher), 2, 2_048);
    let request_frames = frames(
        request_open("request-1", "stream-1", 4, 4),
        b"data".to_vec(),
    );
    let PeerRpcStreamFrameV2::Open(open) = &request_frames[0] else {
        unreachable!()
    };
    registry.open(open.as_ref().clone(), 20).unwrap();
    let PeerRpcStreamFrameV2::Chunk(mut chunk) = request_frames[1].clone() else {
        unreachable!()
    };
    chunk.payload[0] ^= 1;
    assert_eq!(
        registry.push_chunk(chunk, 20),
        Err(PeerRpcStreamErrorV2::InvalidChunkHash)
    );
    assert_eq!(registry.snapshot().unwrap().active_sessions, 0);
    assert_eq!(directory.entry_count(), 0);

    let mut expiring = request_open("request-2", "stream-2", 4, 4);
    expiring.deadline_ms = 30;
    registry.open(expiring, 20).unwrap();
    assert_eq!(registry.cleanup_expired(30), 1);
    assert_eq!(registry.snapshot().unwrap().reserved_payload_bytes, 0);
    assert_eq!(directory.entry_count(), 0);
}

#[test]
fn oversized_dispatch_response_removes_request_state() {
    let directory = TestDirectory::create();
    let registry = registry(&directory, Arc::new(OversizedResponseDispatcher), 1, 2_048);
    let request_frames = frames(
        request_open("request-1", "stream-1", 4, 4),
        b"data".to_vec(),
    );
    let PeerRpcStreamFrameV2::Open(open) = &request_frames[0] else {
        unreachable!()
    };
    registry.open(open.as_ref().clone(), 20).unwrap();
    let PeerRpcStreamFrameV2::Chunk(chunk) = &request_frames[1] else {
        unreachable!()
    };
    registry.push_chunk(chunk.clone(), 20).unwrap();
    let PeerRpcStreamFrameV2::Commit(commit) = request_frames.last().unwrap() else {
        unreachable!()
    };
    assert_eq!(
        registry.commit(commit.clone(), 20),
        Err(PeerRpcStreamErrorV2::PayloadTooLarge)
    );
    let snapshot = registry.snapshot().unwrap();
    assert_eq!(snapshot.active_sessions, 0);
    assert_eq!(snapshot.reserved_payload_bytes, 0);
    assert_eq!(directory.entry_count(), 0);
}
