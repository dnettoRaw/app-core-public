// =============================================================================
//        #######
//     ###       ###     F: federation.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Explicit authenticated Gateway-to-Gateway federation wire contract.

use crate::{GatewayError, GatewayRequestFence, GatewayResult, MeshPeerRequest, MeshPeerResponse};
use appcore_peer_rpc::{payload_hash, v2::PeerRpcWireErrorV2};
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};

/// Stable schema marker for Gateway federation messages.
pub const GATEWAY_FEDERATION_SCHEMA_V2: &str = "appcore.gateway.federation.v2";
/// Explicit Gateway-to-Gateway federation endpoint.
pub const GATEWAY_FEDERATION_PATH_V2: &str = "/v2/gateway/federation/mesh";

/// One fenced logical Peer RPC request forwarded to its remote socket owner.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayFederationRequestV2 {
    /// Schema marker. Must be [`GATEWAY_FEDERATION_SCHEMA_V2`].
    pub schema: String,
    /// Exact shared origin/target/worker request fence.
    pub fence: GatewayRequestFence,
    /// Existing logical Peer RPC relay request, including its inner credential.
    pub request: MeshPeerRequest,
}

impl GatewayFederationRequestV2 {
    /// Creates and validates one federation request.
    pub fn new(fence: GatewayRequestFence, request: MeshPeerRequest) -> GatewayResult<Self> {
        let value = Self {
            schema: GATEWAY_FEDERATION_SCHEMA_V2.to_string(),
            fence,
            request,
        };
        value.validate()?;
        Ok(value)
    }

    /// Validates schema, fence and inner routing identity without touching a worker.
    pub fn validate(&self) -> GatewayResult<()> {
        validate_fence(&self.fence)?;
        self.request
            .validate_schema()
            .map_err(|_| protocol_error("federation inner request is invalid"))?;
        if self.schema != GATEWAY_FEDERATION_SCHEMA_V2
            || self.fence.origin_instance_id == self.fence.target_instance_id
            || self.fence.request_id != self.request.request_id
            || self.fence.tenant_id != self.request.target_tenant_id
            || self.fence.target_core_id != self.request.target_core_id
        {
            return Err(protocol_error("federation routing identity is invalid"));
        }
        if let Ok(envelope) = self.request.peer_envelope() {
            if envelope.cluster_id != self.fence.target_cluster_id {
                return Err(protocol_error("federation cluster identity is invalid"));
            }
        }
        Ok(())
    }

    /// Computes the canonical digest bound to the outer one-use credential.
    pub fn body_hash(&self) -> GatewayResult<String> {
        self.validate()?;
        serde_json::to_vec(self)
            .map(|body| payload_hash(&body))
            .map_err(|_| protocol_error("federation request encoding failed"))
    }
}

impl Debug for GatewayFederationRequestV2 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GatewayFederationRequestV2")
            .field("schema", &self.schema)
            .field("fence", &self.fence)
            .field("inner_body_bytes", &self.request.body.len())
            .field("has_inner_credential", &self.request.bearer_token.is_some())
            .finish()
    }
}

/// Fenced success or typed AC-021 rejection returned by a federation target.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayFederationResponseV2 {
    /// Schema marker. Must be [`GATEWAY_FEDERATION_SCHEMA_V2`].
    pub schema: String,
    /// Exact request fence supplied by the origin.
    pub fence: GatewayRequestFence,
    /// Successful inner relay response.
    pub response: Option<MeshPeerResponse>,
    /// Controlled typed rejection, mutually exclusive with `response`.
    pub error: Option<PeerRpcWireErrorV2>,
}

impl GatewayFederationResponseV2 {
    /// Creates one fenced successful response.
    pub fn ok(fence: GatewayRequestFence, response: MeshPeerResponse) -> Self {
        Self {
            schema: GATEWAY_FEDERATION_SCHEMA_V2.to_string(),
            fence,
            response: Some(response),
            error: None,
        }
    }

    /// Creates one fenced controlled rejection.
    pub fn rejected(fence: GatewayRequestFence, error: PeerRpcWireErrorV2) -> Self {
        Self {
            schema: GATEWAY_FEDERATION_SCHEMA_V2.to_string(),
            fence,
            response: None,
            error: Some(error),
        }
    }

    /// Validates response identity, exclusivity, body bound and typed error metadata.
    pub fn validate_for_request(&self, request: &GatewayFederationRequestV2) -> GatewayResult<()> {
        if self.schema != GATEWAY_FEDERATION_SCHEMA_V2 || self.fence != request.fence {
            return Err(protocol_error("federation response identity is invalid"));
        }
        match (&self.response, &self.error) {
            (Some(response), None) => response
                .validate_for_request(
                    &request.request.request_id,
                    request.request.max_response_bytes,
                )
                .map_err(|_| protocol_error("federation response body is invalid")),
            (None, Some(error)) => {
                error
                    .clone()
                    .validated()
                    .map_err(|_| protocol_error("federation rejection metadata is invalid"))?;
                if error.request_id.as_deref() != Some(request.request.request_id.as_str()) {
                    return Err(protocol_error("federation rejection identity is invalid"));
                }
                Ok(())
            }
            _ => Err(protocol_error("federation response shape is invalid")),
        }
    }
}

impl Debug for GatewayFederationResponseV2 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GatewayFederationResponseV2")
            .field("schema", &self.schema)
            .field("fence", &self.fence)
            .field("has_response", &self.response.is_some())
            .field("error", &self.error)
            .finish()
    }
}

fn protocol_error(message: &'static str) -> GatewayError {
    GatewayError::Protocol(message.to_string())
}

fn validate_fence(fence: &GatewayRequestFence) -> GatewayResult<()> {
    if fence.request_id.is_empty()
        || fence.request_id.len() > 128
        || !fence
            .request_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
        || fence.origin_epoch == 0
        || fence.target_epoch == 0
        || fence.worker_generation == 0
        || fence.expires_at_ms == 0
    {
        return Err(protocol_error("federation request fence is invalid"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "federation_tests.rs"]
mod tests;
