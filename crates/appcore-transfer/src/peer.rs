// =============================================================================
//        #######
//     ###       ###     F: peer.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/06 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/06 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Explicit transfer-block capability over existing authenticated Peer RPC V2 streams.
//! Host/deployment must install the dispatcher behind the authenticated HTTP host,
//! declare capability limits, and authorize the descriptor/source for each identity.
use crate::{
    BlockSource, ObjectDescriptor, TransferControl, TransferError as E, TransferLimits,
    TransferResult,
};
use appcore_core::{CapabilityName, CoreId};
use appcore_peer_rpc::v2::{PeerRpcStreamErrorV2, PeerRpcStreamOpenV2};
use appcore_peer_rpc::{
    CancellationToken, PeerRpcCallKind, PeerRpcChunkLimits, PeerRpcClient,
    PeerRpcStreamDispatcherV2, PeerRpcStreamPayload, PeerRpcStreamRequestV2,
    PeerRpcStreamResponseSourceV2, PeerRpcTokenIssuer, PeerTransportProvider,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{Cursor, Read, Seek, SeekFrom, Write},
    sync::atomic::{AtomicU64, Ordering},
};
/// Exact query capability; register an application/deployment capability descriptor.
pub const TRANSFER_BLOCK_CAPABILITY: &str = "transfer.block.read";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BlockRequest {
    format_version: u16,
    object_hash: String,
    index: usize,
}
/// A deployment-selected peer. Official client owns signed identity, credentials,
/// codec and bounded I/O timeouts. Its configured timeout/TTL must fit the transfer
/// deadline; stream sink checkpoints cancellation between response writes.
/// This adapter neither discovers peers nor falls back between protocol versions.
pub struct PeerBlockSource<'a, T, I> {
    client: &'a PeerRpcClient<T, I>,
    endpoint: String,
    target: CoreId,
}
impl<'a, T: PeerTransportProvider, I: PeerRpcTokenIssuer> PeerBlockSource<'a, T, I> {
    /// Selects a preauthorized endpoint/target and an authenticated client.
    pub fn new(
        client: &'a PeerRpcClient<T, I>,
        endpoint: impl Into<String>,
        target: CoreId,
    ) -> TransferResult<Self> {
        let endpoint = endpoint.into();
        if endpoint.len() > 2048
            || !(endpoint.starts_with("https://") || endpoint.starts_with("http://"))
            || endpoint.chars().any(char::is_control)
        {
            return Err(E::InvalidContract);
        }
        Ok(Self {
            client,
            endpoint,
            target,
        })
    }
}
impl<T: PeerTransportProvider, I: PeerRpcTokenIssuer> BlockSource for PeerBlockSource<'_, T, I> {
    fn read_block(
        &self,
        descriptor: &ObjectDescriptor,
        index: usize,
        control: &TransferControl,
        sink: &mut dyn Write,
    ) -> TransferResult<()> {
        control.checkpoint()?;
        let length = descriptor.block_len(index)?;
        let payload = serde_json::to_vec(&BlockRequest {
            format_version: 1,
            object_hash: descriptor.object_hash().into(),
            index,
        })
        .map_err(|_| E::InvalidContract)?;
        // appcore-norm: allow(global-state) reason: process-local request IDs must be unique across concurrent bounded workers
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let mut request = PeerRpcStreamRequestV2::new(
            format!(
                "transfer-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ),
            self.target.clone(),
            CapabilityName::new(TRANSFER_BLOCK_CAPABILITY).map_err(|_| E::InvalidContract)?,
            payload.len() as u64,
            None,
            None,
        );
        request.limits = PeerRpcChunkLimits {
            max_payload_bytes: length.max(512) as u64,
            max_chunks: 16,
            max_chunk_bytes: 64 * 1024,
            max_encoded_chunk_bytes: 96 * 1024,
        };
        self.client
            .query_stream_v2(&self.endpoint, request, Cursor::new(payload), sink)
            .map_err(|_| E::Source)?;
        control.checkpoint()
    }
}
/// Seekable opaque object reader returned by a repository after authorization.
pub trait ObjectReader: Read + Seek + Send {}
impl<T: Read + Seek + Send> ObjectReader for T {}
/// Deployment-owned authorization and immutable object storage boundary.
pub trait TransferRepository: Send + Sync {
    /// Authorize tenant, cluster, origin, target, capability and this object identity.
    /// Return the trusted descriptor plus a bounded immutable reader. The HTTP host
    /// has already verified signatures/replay; repository must not skip object policy.
    fn open_authorized(
        &self,
        open: &PeerRpcStreamOpenV2,
        object_hash: &str,
    ) -> TransferResult<(ObjectDescriptor, Box<dyn ObjectReader>)>;
}
/// Host dispatcher for one bounded read capability; no command/update dispatch.
pub struct TransferDispatcher<R> {
    repository: R,
    limits: TransferLimits,
}
impl<R: TransferRepository> TransferDispatcher<R> {
    /// Creates a dispatcher; deployment installs it behind authenticated Peer RPC V2.
    pub fn new(repository: R, limits: TransferLimits) -> TransferResult<Self> {
        limits.validate()?;
        Ok(Self { repository, limits })
    }
}
impl<R: TransferRepository> PeerRpcStreamDispatcherV2 for TransferDispatcher<R> {
    fn dispatch_peer_stream(
        &self,
        open: PeerRpcStreamOpenV2,
        mut payload: PeerRpcStreamPayload,
        cancellation: CancellationToken,
    ) -> Result<PeerRpcStreamResponseSourceV2, PeerRpcStreamErrorV2> {
        if cancellation.is_cancelled() {
            return Err(PeerRpcStreamErrorV2::Cancelled);
        }
        if open.call_kind != PeerRpcCallKind::Query
            || open.capability.as_str() != TRANSFER_BLOCK_CAPABILITY
        {
            return Err(PeerRpcStreamErrorV2::CallKindMismatch);
        }
        if payload.len() > 512 {
            return Err(PeerRpcStreamErrorV2::PayloadTooLarge);
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut payload)
            .take(513)
            .read_to_end(&mut bytes)
            .map_err(|_| PeerRpcStreamErrorV2::InvalidConfig)?;
        let request: BlockRequest =
            serde_json::from_slice(&bytes).map_err(|_| PeerRpcStreamErrorV2::InvalidConfig)?;
        if request.format_version != 1 {
            return Err(PeerRpcStreamErrorV2::ProtocolMismatch);
        }
        let (descriptor, mut reader) = self
            .repository
            .open_authorized(&open, &request.object_hash)
            .map_err(|_| PeerRpcStreamErrorV2::IdentityMismatch)?;
        descriptor
            .validate(self.limits)
            .map_err(|_| PeerRpcStreamErrorV2::InvalidConfig)?;
        if descriptor.object_hash() != request.object_hash {
            return Err(PeerRpcStreamErrorV2::IdentityMismatch);
        }
        let length = descriptor
            .block_len(request.index)
            .map_err(|_| PeerRpcStreamErrorV2::InvalidConfig)?;
        reader
            .seek(SeekFrom::Start(
                descriptor
                    .block_offset(request.index)
                    .map_err(|_| PeerRpcStreamErrorV2::InvalidConfig)?,
            ))
            .map_err(|_| PeerRpcStreamErrorV2::InvalidConfig)?;
        let mut block = vec![0; length];
        reader
            .read_exact(&mut block)
            .map_err(|_| PeerRpcStreamErrorV2::Incomplete)?;
        if cancellation.is_cancelled() {
            return Err(PeerRpcStreamErrorV2::Cancelled);
        }
        descriptor
            .verify_block(request.index, &block)
            .map_err(|_| PeerRpcStreamErrorV2::InvalidChunkHash)?;
        Ok(PeerRpcStreamResponseSourceV2::new(
            length as u64,
            Box::new(Cursor::new(block)),
        ))
    }
}
