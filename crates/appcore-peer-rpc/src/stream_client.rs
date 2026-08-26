// =============================================================================
//        #######
//     ###       ###     F: stream_client.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Incremental signed V2 client built over the bounded peer transport provider.

use super::*;
use crate::client::now_ms;
use crate::transport::http_status_error;
use crate::v2::{
    PeerRpcStreamCancelReasonV2, PeerRpcStreamCancelV2, PeerRpcStreamErrorV2, PeerRpcStreamFrameV2,
    PeerRpcStreamHttpErrorCodeV2, PeerRpcStreamHttpErrorV2, PeerRpcStreamOpenV2,
    PeerRpcStreamPullV2, PeerRpcStreamReplyV2, PEER_COMMAND_PATH_V2, PEER_QUERY_PATH_V2,
    PEER_RPC_PROTOCOL_VERSION_V2,
};
use appcore_core::{CapabilityName, TraceContext};
use std::io::{Read, Write};

/// Metadata and limits for one incremental V2 request payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerRpcStreamRequestV2 {
    /// Stable RPC request identity.
    pub request_id: String,
    /// Target peer Core identity.
    pub target_core_id: CoreId,
    /// Generic capability invoked at the peer.
    pub capability: CapabilityName,
    /// Exact decoded request payload size.
    pub payload_bytes: u64,
    /// Optional idempotency identity; required for commands.
    pub idempotency_key: Option<String>,
    /// Optional distributed trace context.
    pub trace: Option<TraceContext>,
    /// Aggregate, chunk, encoded and count limits used in both directions.
    pub limits: PeerRpcChunkLimits,
}

impl PeerRpcStreamRequestV2 {
    /// Creates request metadata with the default 64 MiB aggregate limits.
    pub fn new(
        request_id: impl Into<String>,
        target_core_id: CoreId,
        capability: CapabilityName,
        payload_bytes: u64,
        idempotency_key: Option<String>,
        trace: Option<TraceContext>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            target_core_id,
            capability,
            payload_bytes,
            idempotency_key,
            trace,
            limits: PeerRpcChunkLimits::default(),
        }
    }
}

/// Typed client failure for signed V2 frame exchange.
#[derive(Debug, thiserror::Error)]
pub enum PeerRpcStreamClientErrorV2 {
    /// Local codec, integrity, quota, deadline, or cancellation failure.
    #[error(transparent)]
    Stream(#[from] PeerRpcStreamErrorV2),
    /// Controlled failure returned by the V2 host.
    #[error("peer RPC V2 host rejected the frame: {0:?}")]
    Remote(PeerRpcStreamHttpErrorCodeV2),
    /// Existing bounded HTTP transport failed.
    #[error("peer RPC V2 HTTP transport failed")]
    Transport(#[source] PeerRpcError),
    /// Host reply identity or frame lifecycle was incoherent.
    #[error("peer RPC V2 host response is invalid")]
    InvalidResponse,
}

impl<T, I> PeerRpcClient<T, I>
where
    T: PeerTransportProvider,
    I: PeerRpcTokenIssuer,
{
    /// Streams a V2 query request and response under the request's explicit limits.
    pub fn query_stream_v2<R, W>(
        &self,
        endpoint_url: &str,
        request: PeerRpcStreamRequestV2,
        source: R,
        sink: W,
    ) -> Result<W, PeerRpcStreamClientErrorV2>
    where
        R: Read,
        W: Write,
    {
        self.call_stream_v2(endpoint_url, PeerRpcCallKind::Query, request, source, sink)
    }

    /// Streams a V2 command request and response under the request's explicit limits.
    pub fn command_stream_v2<R, W>(
        &self,
        endpoint_url: &str,
        request: PeerRpcStreamRequestV2,
        source: R,
        sink: W,
    ) -> Result<W, PeerRpcStreamClientErrorV2>
    where
        R: Read,
        W: Write,
    {
        if request.idempotency_key.is_none() {
            return Err(PeerRpcStreamErrorV2::InvalidConfig.into());
        }
        self.call_stream_v2(
            endpoint_url,
            PeerRpcCallKind::Command,
            request,
            source,
            sink,
        )
    }

    fn call_stream_v2<R, W>(
        &self,
        endpoint_url: &str,
        kind: PeerRpcCallKind,
        request: PeerRpcStreamRequestV2,
        source: R,
        sink: W,
    ) -> Result<W, PeerRpcStreamClientErrorV2>
    where
        R: Read,
        W: Write,
    {
        let open = self.build_stream_open(kind, &request);
        let mut current_stream_id = open.stream_id.clone();
        let result = self.execute_stream_v2(
            endpoint_url,
            kind,
            &request.limits,
            open,
            source,
            sink,
            &mut current_stream_id,
        );
        if result.is_err() {
            let reason = if self.cancellation.is_cancelled() {
                PeerRpcStreamCancelReasonV2::Caller
            } else {
                PeerRpcStreamCancelReasonV2::Transport
            };
            self.cancel_stream_best_effort(
                endpoint_url,
                kind,
                &request.request_id,
                &current_stream_id,
                reason,
            );
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_stream_v2<R, W>(
        &self,
        endpoint_url: &str,
        kind: PeerRpcCallKind,
        limits: &PeerRpcChunkLimits,
        open: PeerRpcStreamOpenV2,
        source: R,
        sink: W,
        current_stream_id: &mut String,
    ) -> Result<W, PeerRpcStreamClientErrorV2>
    where
        R: Read,
        W: Write,
    {
        let mut encoder = PeerRpcChunkEncoder::new(
            open.clone(),
            source,
            limits.clone(),
            self.cancellation.clone(),
            now_ms(),
        )?;
        let mut commit_reply = None;
        while let Some(frame) = encoder.next_frame(now_ms())? {
            let is_commit = matches!(frame, PeerRpcStreamFrameV2::Commit(_));
            let reply = self.exchange_stream_frame(endpoint_url, kind, frame)?;
            if reply.request_id != open.request_id {
                return Err(PeerRpcStreamClientErrorV2::InvalidResponse);
            }
            if is_commit {
                if reply.complete {
                    return Err(PeerRpcStreamClientErrorV2::InvalidResponse);
                }
                *current_stream_id = reply.stream_id.clone();
                commit_reply = Some(reply);
            } else if reply.stream_id != open.stream_id
                || reply.response_frame.is_some()
                || reply.complete
            {
                return Err(PeerRpcStreamClientErrorV2::InvalidResponse);
            }
        }
        let reply = commit_reply.ok_or(PeerRpcStreamClientErrorV2::InvalidResponse)?;
        let response_open = match reply.response_frame.map(|frame| *frame) {
            Some(PeerRpcStreamFrameV2::Open(open)) => *open,
            _ => return Err(PeerRpcStreamClientErrorV2::InvalidResponse),
        };
        if reply.stream_id != response_open.stream_id {
            return Err(PeerRpcStreamClientErrorV2::InvalidResponse);
        }
        validate_response_open(&open, &response_open)?;
        *current_stream_id = response_open.stream_id.clone();
        self.receive_stream_response(endpoint_url, kind, limits, response_open, sink)
    }

    fn receive_stream_response<W>(
        &self,
        endpoint_url: &str,
        kind: PeerRpcCallKind,
        limits: &PeerRpcChunkLimits,
        response_open: PeerRpcStreamOpenV2,
        sink: W,
    ) -> Result<W, PeerRpcStreamClientErrorV2>
    where
        W: Write,
    {
        let mut assembler = Some(PeerRpcChunkAssembler::new(
            response_open.clone(),
            sink,
            limits.clone(),
            self.cancellation.clone(),
            now_ms(),
        )?);
        loop {
            let reply = self.exchange_stream_frame(
                endpoint_url,
                kind,
                PeerRpcStreamFrameV2::Pull(PeerRpcStreamPullV2 {
                    protocol_version: ProtocolVersion::new(PEER_RPC_PROTOCOL_VERSION_V2),
                    request_id: response_open.request_id.clone(),
                    stream_id: response_open.stream_id.clone(),
                }),
            )?;
            if reply.request_id != response_open.request_id
                || reply.stream_id != response_open.stream_id
            {
                return Err(PeerRpcStreamClientErrorV2::InvalidResponse);
            }
            match reply.response_frame.map(|frame| *frame) {
                Some(PeerRpcStreamFrameV2::Chunk(chunk)) if !reply.complete => {
                    assembler
                        .as_mut()
                        .ok_or(PeerRpcStreamClientErrorV2::InvalidResponse)?
                        .push_chunk(chunk, now_ms())?;
                }
                Some(PeerRpcStreamFrameV2::Commit(commit)) if reply.complete => {
                    return assembler
                        .take()
                        .ok_or(PeerRpcStreamClientErrorV2::InvalidResponse)?
                        .finish(commit, now_ms())
                        .map_err(Into::into);
                }
                _ => return Err(PeerRpcStreamClientErrorV2::InvalidResponse),
            }
        }
    }

    fn exchange_stream_frame(
        &self,
        endpoint_url: &str,
        kind: PeerRpcCallKind,
        frame: PeerRpcStreamFrameV2,
    ) -> Result<PeerRpcStreamReplyV2, PeerRpcStreamClientErrorV2> {
        let request_id = frame.request_id().to_string();
        let stream_id = frame.stream_id().to_string();
        let body = serde_json::to_vec(&frame).map_err(|_| PeerRpcStreamErrorV2::InvalidConfig)?;
        let hash = payload_hash(&body);
        let token = self
            .token_issuer
            .issue_peer_token(
                frame.request_id(),
                Some(&hash),
                now_ms(),
                self.config.envelope_ttl_ms,
            )
            .map_err(PeerRpcStreamClientErrorV2::Transport)?;
        let response = self
            .transport
            .send_cancellable(
                endpoint_url,
                PeerRpcHttpRequest {
                    method: "POST".to_string(),
                    path: stream_path(kind).to_string(),
                    body,
                    bearer_token: Some(token),
                    timeout_ms: self.config.request_timeout_ms,
                    max_response_bytes: self.config.max_response_bytes,
                },
                &self.cancellation,
            )
            .map_err(PeerRpcStreamClientErrorV2::Transport)?;
        if !(200..300).contains(&response.status_code) {
            if let Ok(error) = serde_json::from_slice::<PeerRpcStreamHttpErrorV2>(&response.body) {
                if error
                    .request_id
                    .as_deref()
                    .is_some_and(|id| id != request_id)
                    || error.stream_id.as_deref().is_some_and(|id| id != stream_id)
                {
                    return Err(PeerRpcStreamClientErrorV2::InvalidResponse);
                }
                return Err(PeerRpcStreamClientErrorV2::Remote(error.code));
            }
            return Err(PeerRpcStreamClientErrorV2::Transport(http_status_error(
                response.status_code,
                response.body,
            )));
        }
        serde_json::from_slice(&response.body)
            .map_err(|_| PeerRpcStreamClientErrorV2::InvalidResponse)
    }

    fn build_stream_open(
        &self,
        kind: PeerRpcCallKind,
        request: &PeerRpcStreamRequestV2,
    ) -> PeerRpcStreamOpenV2 {
        let now = now_ms();
        let (stream_id, nonce) = next_stream_identity(now);
        let chunk_bytes = request.limits.max_chunk_bytes.min(u32::MAX as usize) as u32;
        let chunk_count = if request.payload_bytes == 0 || chunk_bytes == 0 {
            0
        } else {
            ((request.payload_bytes - 1) / u64::from(chunk_bytes) + 1).min(u64::from(u32::MAX))
                as u32
        };
        PeerRpcStreamOpenV2 {
            protocol_version: ProtocolVersion::new(PEER_RPC_PROTOCOL_VERSION_V2),
            request_id: request.request_id.clone(),
            stream_id,
            trace_id: request
                .trace
                .as_ref()
                .map(|trace| trace.trace_id.clone())
                .unwrap_or_else(|| request.request_id.clone()),
            direction: crate::v2::PeerRpcStreamDirectionV2::Request,
            call_kind: kind,
            source_core_id: self.source_identity.core_id.clone(),
            target_core_id: request.target_core_id.clone(),
            tenant_id: self.source_identity.tenant_id.clone(),
            cluster_id: self.source_identity.cluster_id.clone(),
            timestamp_ms: now,
            deadline_ms: now.saturating_add(self.config.envelope_ttl_ms.max(1)),
            nonce,
            capability: request.capability.clone(),
            payload_bytes: request.payload_bytes,
            chunk_bytes,
            chunk_count,
            idempotency_key: request.idempotency_key.clone(),
            trace: request.trace.clone(),
        }
    }

    fn cancel_stream_best_effort(
        &self,
        endpoint_url: &str,
        kind: PeerRpcCallKind,
        request_id: &str,
        stream_id: &str,
        reason: PeerRpcStreamCancelReasonV2,
    ) {
        let _ = self.exchange_stream_frame(
            endpoint_url,
            kind,
            PeerRpcStreamFrameV2::Cancel(PeerRpcStreamCancelV2 {
                protocol_version: ProtocolVersion::new(PEER_RPC_PROTOCOL_VERSION_V2),
                request_id: request_id.to_string(),
                stream_id: stream_id.to_string(),
                reason,
            }),
        );
    }
}

fn validate_response_open(
    request: &PeerRpcStreamOpenV2,
    response: &PeerRpcStreamOpenV2,
) -> Result<(), PeerRpcStreamClientErrorV2> {
    if response.protocol_version.as_u16() != PEER_RPC_PROTOCOL_VERSION_V2
        || response.direction != crate::v2::PeerRpcStreamDirectionV2::Response
        || response.request_id != request.request_id
        || response.trace_id != request.trace_id
        || response.call_kind != request.call_kind
        || response.source_core_id != request.target_core_id
        || response.target_core_id != request.source_core_id
        || response.tenant_id != request.tenant_id
        || response.cluster_id != request.cluster_id
        || response.capability != request.capability
        || response.deadline_ms != request.deadline_ms
        || response.idempotency_key.is_some()
        || response.trace != request.trace
    {
        return Err(PeerRpcStreamClientErrorV2::InvalidResponse);
    }
    Ok(())
}

fn stream_path(kind: PeerRpcCallKind) -> &'static str {
    match kind {
        PeerRpcCallKind::Query => PEER_QUERY_PATH_V2,
        PeerRpcCallKind::Command => PEER_COMMAND_PATH_V2,
    }
}

fn next_stream_identity(now_ms: u64) -> (String, String) {
    // appcore-norm: allow(global-state) reason: atomic sequence prevents process-local stream and nonce reuse
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    (
        format!("stream-{}-{sequence}", std::process::id()),
        format!("nonce-{}-{now_ms}-{sequence}", std::process::id()),
    )
}
