// =============================================================================
//        #######
//     ###       ###     F: peer_rpc_invoker.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/22 15:41:18 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Adapts capability requests to Peer RPC while preserving caller ownership.
//!
//! Borrowed invocation remains the compatibility path. Owned invocation moves
//! every owned request field into the outbound DTO, translates the copyable
//! mode into the call kind and must not duplicate its opaque payload.

use crate::{
    CapabilityError, CapabilityRequest, CapabilityResponse, CapabilityResult,
    RemoteCapabilityInvoker,
};
use appcore_core::CapabilityMode;
use appcore_distributed_contracts::{
    PeerRecord, PeerRpcCallKind, PeerRpcClientExecutor, PeerRpcOutboundRequest,
};

/// Remote capability invoker backed by the stable peer RPC client contract.
pub struct PeerRpcRemoteCapabilityInvoker<C> {
    client: C,
}

impl<C> PeerRpcRemoteCapabilityInvoker<C> {
    /// Creates an invoker using the supplied peer RPC executor.
    pub fn new(client: C) -> Self {
        Self { client }
    }
}

impl<C> RemoteCapabilityInvoker for PeerRpcRemoteCapabilityInvoker<C>
where
    C: PeerRpcClientExecutor,
{
    fn invoke_remote(
        &self,
        peer: &PeerRecord,
        request: &CapabilityRequest,
    ) -> CapabilityResult<CapabilityResponse> {
        let endpoint_url = peer_rpc_endpoint(peer).ok_or_else(|| {
            CapabilityError::RemoteEndpointUnavailable(request.capability.clone())
        })?;
        let kind = peer_rpc_call_kind(request.mode)?;
        let response = self
            .client
            .call_peer(
                endpoint_url,
                kind,
                PeerRpcOutboundRequest::new(
                    request.request_id.clone(),
                    peer.identity.core_id.clone(),
                    request.capability.clone(),
                    request.payload.clone(),
                    request.idempotency_key.clone(),
                    request.trace.clone(),
                ),
            )
            .map_err(|error| CapabilityError::RemoteInvocationFailed(format!("{error:?}")))?;
        capability_response(peer, response)
    }

    fn invoke_remote_owned(
        &self,
        peer: &PeerRecord,
        request: CapabilityRequest,
    ) -> CapabilityResult<CapabilityResponse> {
        let endpoint_url = peer_rpc_endpoint(peer).ok_or_else(|| {
            CapabilityError::RemoteEndpointUnavailable(request.capability.clone())
        })?;
        let kind = peer_rpc_call_kind(request.mode)?;
        let CapabilityRequest {
            request_id,
            capability,
            mode: _,
            payload,
            idempotency_key,
            trace,
        } = request;
        let response = self
            .client
            .call_peer(
                endpoint_url,
                kind,
                PeerRpcOutboundRequest::new(
                    request_id,
                    peer.identity.core_id.clone(),
                    capability,
                    payload,
                    idempotency_key,
                    trace,
                ),
            )
            .map_err(|error| CapabilityError::RemoteInvocationFailed(format!("{error:?}")))?;
        capability_response(peer, response)
    }
}

fn peer_rpc_call_kind(mode: CapabilityMode) -> CapabilityResult<PeerRpcCallKind> {
    match mode {
        CapabilityMode::Query => Ok(PeerRpcCallKind::Query),
        CapabilityMode::Command => Ok(PeerRpcCallKind::Command),
        CapabilityMode::Stream => Err(CapabilityError::HandlerRejected(
            "stream_remote_invocation_not_supported".to_string(),
        )),
    }
}

fn capability_response(
    peer: &PeerRecord,
    response: appcore_distributed_contracts::PeerRpcResponse,
) -> CapabilityResult<CapabilityResponse> {
    if response.ok {
        return Ok(CapabilityResponse::accepted(
            response.payload,
            Some(peer.identity.core_id.clone()),
        ));
    }
    Ok(CapabilityResponse::rejected(
        response
            .error
            .unwrap_or_else(|| "remote_rejected".to_string()),
    ))
}

fn peer_rpc_endpoint(peer: &PeerRecord) -> Option<&str> {
    peer.endpoints
        .iter()
        .find(|endpoint| {
            endpoint.name == "peer-rpc"
                || endpoint.name == "peer_rpc"
                || endpoint.protocol == "appcore-peer-rpc"
        })
        .map(|endpoint| endpoint.url.as_str())
}
