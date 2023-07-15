// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 13:21:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 00:04:12 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Versioned contracts shared by distributed runtime implementations.
//!
//! This crate owns wire data and provider-facing traits. It deliberately has no
//! HTTP, TLS, database, queue or filesystem implementation.

#![deny(missing_docs)]

/// Control-plane coordination contracts.
pub mod control_plane;
/// Opaque content-envelope transport contracts.
pub mod opaque;
/// Direct peer RPC wire contracts.
pub mod peer_rpc;

pub use control_plane::v1::{
    ControlPlaneError, ControlPlaneFuture, ControlPlaneProvider, ControlPlaneResult, CorePresence,
    CoreRegistration, DiscoveryProvider, HeartbeatRequest, HeartbeatResponse, LeadershipDecision,
    PeerDirectory, PeerRecord, ServiceLeaderLease, ServiceLeadershipGuard,
};
pub use opaque::{
    OpaqueContentEnvelopeV1, OpaqueEnvelopeDecision, OpaqueEnvelopeDeduplicator,
    OpaqueEnvelopePolicy, OPAQUE_CONTENT_ENVELOPE_SCHEMA_V1,
};
pub use peer_rpc::v1::{
    PeerAdvertisementV1, PeerCapabilityV1, PeerEndpointV1, PeerHealthResponse, PeerIdentityV1,
    PeerManifestResponse, PeerRpcCallKind, PeerRpcClientExecutor, PeerRpcEnvelope, PeerRpcError,
    PeerRpcOutboundRequest, PeerRpcResponse, PEER_COMMAND_PATH, PEER_HEALTH_PATH,
    PEER_MANIFEST_PATH, PEER_QUERY_PATH,
};
