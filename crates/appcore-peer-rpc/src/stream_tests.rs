// =============================================================================
//        #######
//     ###       ###     F: stream_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================
// appcore-norm: test

use super::*;
use crate::v2::*;
use appcore_transport::encode_gzip_if_smaller;
use std::io::{Cursor, Write};

fn open(payload_bytes: u64, chunk_bytes: u32) -> PeerRpcStreamOpenV2 {
    let chunk_count = if payload_bytes == 0 {
        0
    } else {
        ((payload_bytes - 1) / u64::from(chunk_bytes) + 1) as u32
    };
    PeerRpcStreamOpenV2 {
        protocol_version: ProtocolVersion::new(2),
        request_id: "request-1".to_string(),
        stream_id: "stream-1".to_string(),
        trace_id: "trace-1".to_string(),
        direction: PeerRpcStreamDirectionV2::Request,
        call_kind: PeerRpcCallKind::Query,
        source_core_id: CoreId::new("core-a").unwrap(),
        target_core_id: CoreId::new("core-b").unwrap(),
        tenant_id: TenantId::new("tenant-a").unwrap(),
        cluster_id: ClusterId::new("cluster-a").unwrap(),
        timestamp_ms: 10,
        deadline_ms: 1_000,
        nonce: "nonce-1".to_string(),
        capability: appcore_core::CapabilityName::new("runtime.query").unwrap(),
        payload_bytes,
        chunk_bytes,
        chunk_count,
        idempotency_key: None,
        trace: None,
    }
}

#[derive(Default)]
struct CountingSink {
    bytes: u64,
    writes: usize,
    largest_write: usize,
}

impl Write for CountingSink {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.bytes = self.bytes.saturating_add(buffer.len() as u64);
        self.writes = self.writes.saturating_add(1);
        self.largest_write = self.largest_write.max(buffer.len());
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn encoded_frames(payload: Vec<u8>, chunk_bytes: u32) -> Vec<PeerRpcStreamFrameV2> {
    let metadata = open(payload.len() as u64, chunk_bytes);
    let mut encoder = PeerRpcChunkEncoder::new(
        metadata,
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

#[test]
fn large_stream_uses_bounded_chunk_writes_and_commits_integrity() {
    let payload = vec![b'a'; 4 * 1024 * 1024 + 17];
    let frames = encoded_frames(payload.clone(), 64 * 1024);
    let open = match &frames[0] {
        PeerRpcStreamFrameV2::Open(open) => open.as_ref().clone(),
        _ => panic!("first frame must open stream"),
    };
    let mut assembler = Some(
        PeerRpcChunkAssembler::new(
            open,
            CountingSink::default(),
            PeerRpcChunkLimits::default(),
            CancellationToken::new(),
            20,
        )
        .unwrap(),
    );
    let mut completed = None;
    for frame in frames.into_iter().skip(1) {
        match frame {
            PeerRpcStreamFrameV2::Chunk(chunk) => {
                assert!(chunk.payload.len() <= 96 * 1024);
                assembler.as_mut().unwrap().push_chunk(chunk, 20).unwrap();
            }
            PeerRpcStreamFrameV2::Commit(commit) => {
                completed = Some(assembler.take().unwrap().finish(commit, 20).unwrap());
            }
            _ => panic!("unexpected stream frame"),
        }
    }
    let completed = completed.unwrap();
    assert_eq!(completed.bytes, payload.len() as u64);
    assert_eq!(completed.writes, 65);
    assert!(completed.largest_write <= 64 * 1024);
}

#[test]
fn missing_repeated_and_out_of_order_chunks_fail_closed() {
    let frames = encoded_frames(vec![1; 10], 4);
    let open = match &frames[0] {
        PeerRpcStreamFrameV2::Open(open) => open.as_ref().clone(),
        _ => unreachable!(),
    };
    let chunks = frames
        .iter()
        .filter_map(|frame| match frame {
            PeerRpcStreamFrameV2::Chunk(chunk) => Some(chunk.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();

    let mut out_of_order = PeerRpcChunkAssembler::new(
        open.clone(),
        Vec::new(),
        PeerRpcChunkLimits::default(),
        CancellationToken::new(),
        20,
    )
    .unwrap();
    assert_eq!(
        out_of_order.push_chunk(chunks[1].clone(), 20),
        Err(PeerRpcStreamErrorV2::InvalidSequence)
    );
    assert_eq!(
        out_of_order.push_chunk(chunks[0].clone(), 20),
        Err(PeerRpcStreamErrorV2::Closed)
    );

    let mut repeated = PeerRpcChunkAssembler::new(
        open.clone(),
        Vec::new(),
        PeerRpcChunkLimits::default(),
        CancellationToken::new(),
        20,
    )
    .unwrap();
    repeated.push_chunk(chunks[0].clone(), 20).unwrap();
    assert_eq!(
        repeated.push_chunk(chunks[0].clone(), 20),
        Err(PeerRpcStreamErrorV2::InvalidSequence)
    );

    let commit = match frames.last().unwrap() {
        PeerRpcStreamFrameV2::Commit(commit) => commit.clone(),
        _ => unreachable!(),
    };
    let mut missing = PeerRpcChunkAssembler::new(
        open,
        Vec::new(),
        PeerRpcChunkLimits::default(),
        CancellationToken::new(),
        20,
    )
    .unwrap();
    missing.push_chunk(chunks[0].clone(), 20).unwrap();
    assert_eq!(
        missing.finish(commit, 20),
        Err(PeerRpcStreamErrorV2::Incomplete)
    );
}

#[test]
fn corrupted_chunk_and_total_hash_fail_closed() {
    let frames = encoded_frames(b"abcdef".to_vec(), 3);
    let open = match &frames[0] {
        PeerRpcStreamFrameV2::Open(open) => open.as_ref().clone(),
        _ => unreachable!(),
    };
    let mut first = match &frames[1] {
        PeerRpcStreamFrameV2::Chunk(chunk) => chunk.clone(),
        _ => unreachable!(),
    };
    first.payload[0] ^= 1;
    let mut corrupted = PeerRpcChunkAssembler::new(
        open.clone(),
        Vec::new(),
        PeerRpcChunkLimits::default(),
        CancellationToken::new(),
        20,
    )
    .unwrap();
    assert_eq!(
        corrupted.push_chunk(first, 20),
        Err(PeerRpcStreamErrorV2::InvalidChunkHash)
    );

    let mut aggregate = PeerRpcChunkAssembler::new(
        open,
        Vec::new(),
        PeerRpcChunkLimits::default(),
        CancellationToken::new(),
        20,
    )
    .unwrap();
    for frame in &frames[1..frames.len() - 1] {
        let PeerRpcStreamFrameV2::Chunk(chunk) = frame else {
            unreachable!()
        };
        aggregate.push_chunk(chunk.clone(), 20).unwrap();
    }
    let mut commit = match frames.last().unwrap() {
        PeerRpcStreamFrameV2::Commit(commit) => commit.clone(),
        _ => unreachable!(),
    };
    commit.payload_hash = "bad".to_string();
    assert_eq!(
        aggregate.finish(commit, 20),
        Err(PeerRpcStreamErrorV2::InvalidPayloadHash)
    );
}

#[test]
fn decompressed_quota_is_enforced_before_sink_write() {
    let limits = PeerRpcChunkLimits {
        max_chunk_bytes: 4,
        max_encoded_chunk_bytes: 128,
        max_payload_bytes: 4,
        max_chunks: 1,
    };
    let mut assembler = PeerRpcChunkAssembler::new(
        open(4, 4),
        CountingSink::default(),
        limits,
        CancellationToken::new(),
        20,
    )
    .unwrap();
    let compressed = encode_gzip_if_smaller(&vec![b'a'; 1_024]).unwrap().unwrap();
    let chunk = PeerRpcStreamChunkV2 {
        protocol_version: ProtocolVersion::new(2),
        request_id: "request-1".to_string(),
        stream_id: "stream-1".to_string(),
        sequence: 0,
        encoding: PeerRpcChunkEncodingV2::Gzip,
        payload: compressed,
        decoded_bytes: 4,
        chunk_hash: payload_hash(b"aaaa"),
    };
    assert_eq!(
        assembler.push_chunk(chunk, 20),
        Err(PeerRpcStreamErrorV2::ChunkTooLarge)
    );
    assert_eq!(assembler.received_bytes(), 0);
}

#[test]
fn cancellation_deadline_and_declared_size_are_fail_closed() {
    let cancellation = CancellationToken::new();
    let mut assembler = PeerRpcChunkAssembler::new(
        open(1, 1),
        Vec::new(),
        PeerRpcChunkLimits::default(),
        cancellation.clone(),
        20,
    )
    .unwrap();
    cancellation.cancel();
    let chunk = match encoded_frames(vec![1], 1).remove(1) {
        PeerRpcStreamFrameV2::Chunk(chunk) => chunk,
        _ => unreachable!(),
    };
    assert_eq!(
        assembler.push_chunk(chunk.clone(), 20),
        Err(PeerRpcStreamErrorV2::Cancelled)
    );

    let mut expired = PeerRpcChunkAssembler::new(
        open(1, 1),
        Vec::new(),
        PeerRpcChunkLimits::default(),
        CancellationToken::new(),
        20,
    )
    .unwrap();
    assert_eq!(
        expired.push_chunk(chunk, 1_000),
        Err(PeerRpcStreamErrorV2::Expired)
    );

    let mut encoder = PeerRpcChunkEncoder::new(
        open(1, 1),
        Cursor::new(vec![1, 2]),
        PeerRpcChunkLimits::default(),
        CancellationToken::new(),
        20,
    )
    .unwrap();
    assert!(matches!(
        encoder.next_frame(20).unwrap(),
        Some(PeerRpcStreamFrameV2::Open(_))
    ));
    assert!(matches!(
        encoder.next_frame(20).unwrap(),
        Some(PeerRpcStreamFrameV2::Chunk(_))
    ));
    assert_eq!(
        encoder.next_frame(20),
        Err(PeerRpcStreamErrorV2::PayloadTooLarge)
    );
    assert_eq!(encoder.next_frame(20), Err(PeerRpcStreamErrorV2::Closed));
}
