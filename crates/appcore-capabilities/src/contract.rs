// =============================================================================
//        #######
//     ###       ###     F: contract.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/22 15:41:18 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use appcore_core::{CapabilityDescriptor, CapabilityMode, CapabilityName, CoreId, TraceContext};
use appcore_distributed_contracts::PeerRecord;

/// Result returned by capability registry, resolution, and invocation operations.
pub type CapabilityResult<T> = Result<T, CapabilityError>;

/// Failures produced while registering, resolving, or invoking a capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityError {
    /// A descriptor is already present in the composed capability catalog.
    DescriptorAlreadyRegistered(CapabilityName),
    /// The composed host did not declare the requested capability.
    CapabilityNotDeclared(CapabilityName),
    /// A local handler is already registered for the capability.
    HandlerAlreadyRegistered(CapabilityName),
    /// The selected local handler is no longer present in the registry.
    HandlerNotFound(CapabilityName),
    /// No healthy compatible provider can serve the capability.
    ProviderUnavailable(CapabilityName),
    /// The capability requires a valid leadership lease.
    RequiresLeader(CapabilityName),
    /// The leadership lease has expired.
    LeaseExpired(CapabilityName),
    /// The leadership epoch is older than the active lease epoch.
    StaleEpoch(CapabilityName),
    /// The host's current operational mode does not permit writes.
    WritesDisabled(CapabilityName),
    /// A remote provider was selected but has no usable peer endpoint.
    RemoteEndpointUnavailable(CapabilityName),
    /// The peer RPC transport failed to invoke a remote provider.
    RemoteInvocationFailed(String),
    /// A handler rejected the request before or during execution.
    HandlerRejected(String),
}

/// Transport-neutral request for a named runtime capability.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CapabilityRequest {
    /// Caller-assigned identifier used for tracing and response correlation.
    pub request_id: String,
    /// Capability to resolve and invoke.
    pub capability: CapabilityName,
    /// Invocation mode required by the caller.
    pub mode: CapabilityMode,
    /// Opaque application-owned payload.
    pub payload: Vec<u8>,
    /// Optional key used to deduplicate mutating requests.
    pub idempotency_key: Option<String>,
    /// Optional distributed trace context propagated to the provider.
    pub trace: Option<TraceContext>,
}

/// Transport-neutral result of a capability invocation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CapabilityResponse {
    /// Whether the provider accepted and completed the request.
    pub accepted: bool,
    /// Opaque application-owned response payload.
    pub payload: Vec<u8>,
    /// Core that handled the request, when known.
    pub provider_core_id: Option<CoreId>,
    /// Controlled rejection or informational message.
    pub message: Option<String>,
}

impl CapabilityResponse {
    /// Creates an accepted response with an optional provider identity.
    pub fn accepted(payload: Vec<u8>, provider_core_id: Option<CoreId>) -> Self {
        Self {
            accepted: true,
            payload,
            provider_core_id,
            message: None,
        }
    }

    /// Creates a rejected response without a payload.
    pub fn rejected(message: impl Into<String>) -> Self {
        Self {
            accepted: false,
            payload: Vec::new(),
            provider_core_id: None,
            message: Some(message.into()),
        }
    }
}

/// Application-provided implementation of a capability hosted in this process.
pub trait LocalCapabilityHandler: Send + Sync {
    /// Describes the capability exposed by this handler.
    fn descriptor(&self) -> CapabilityDescriptor;

    /// Reports whether this handler may currently receive traffic.
    fn is_healthy(&self) -> bool {
        true
    }

    /// Handles one validated capability request.
    fn handle(&self, request: &CapabilityRequest) -> CapabilityResult<CapabilityResponse>;
}

/// Adapter used to invoke a capability hosted by a remote runtime peer.
pub trait RemoteCapabilityInvoker: Send + Sync {
    /// Sends one request to the selected peer and returns its response.
    fn invoke_remote(
        &self,
        peer: &PeerRecord,
        request: &CapabilityRequest,
    ) -> CapabilityResult<CapabilityResponse>;
}
