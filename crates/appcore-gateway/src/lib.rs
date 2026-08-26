// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/26 08:53:09 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:48:56 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Multi-tenant Gateway capability for the AppCore Runtime.

#![deny(missing_docs)]

pub mod authorization;
pub mod capability;
pub mod config;
pub mod connection;
pub mod error;
pub mod heartbeat;
pub mod mesh;
pub mod metrics;
pub mod registry;
pub mod resolver;
pub mod router;
pub mod runtime;
pub mod service;
pub mod session;
pub mod socket;
pub mod state;
pub mod tenant;
mod tenant_directory;

pub use appcore_distributed_contracts::{
    OpaqueContentEnvelopeV1, OpaqueEnvelopeDecision, OpaqueEnvelopeDeduplicator,
    OpaqueEnvelopePolicy, OPAQUE_CONTENT_ENVELOPE_SCHEMA_V1,
};
pub use authorization::{
    client_connection_hash, gateway_token_claims, worker_connection_hash,
    GATEWAY_CONNECTION_TOKEN_TTL_MS,
};
pub use capability::{
    gateway_capability_descriptor, GatewayCapability, GATEWAY_RUNTIME_CAPABILITY,
};
pub use config::{GatewayConfig, GATEWAY_ADAPTER_NAME, GATEWAY_PROVIDER_ID};
pub use connection::{ClientConnection, WorkerConnection, WorkerConnectionKey};
pub use error::{GatewayError, GatewayResult};
pub use heartbeat::spawn_heartbeat_pruner;
pub use mesh::{
    MeshPeerRequest, MeshPeerResponse, MeshPeerTransport, MESH_HTTP_SCHEMA_V1, MESH_PEER_RELAY_PATH,
};
pub use metrics::GatewayMetrics;
pub use registry::CapabilityRegistry;
pub use resolver::CapabilityResolver;
pub use router::EnvelopeRouter;
pub use runtime::{GatewayRuntime, GatewayRuntimeSnapshot, GatewayRuntimeState};
pub use service::make_gateway_router;
pub use session::GatewaySession;
pub use state::GatewayState;
pub use tenant::TenantState;
pub use tenant_directory::SharedTenantState;

#[cfg(test)]
mod tests;
