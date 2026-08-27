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
mod federated_route;
pub mod federation;
mod federation_auth;
mod federation_transport;
pub mod ha;
pub mod heartbeat;
pub mod mesh;
mod mesh_route;
pub mod metrics;
mod peer_route;
pub mod registry;
pub mod resolver;
mod route_admission;
mod route_fencing;
pub mod router;
pub mod runtime;
pub mod service;
pub mod session;
pub mod socket;
mod socket_ownership;
pub mod state;
mod telemetry;
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
pub use config::{
    GatewayConfig, GATEWAY_ADAPTER_NAME, GATEWAY_PROVIDER_ID, MAX_GATEWAY_AFFINITY_KEY_BYTES,
    MAX_GATEWAY_WORKER_INFLIGHT,
};
pub use connection::{ClientConnection, WorkerConnection, WorkerConnectionKey};
pub use error::{GatewayError, GatewayResult};
pub use federation::{
    GatewayFederationRequestV2, GatewayFederationResponseV2, GATEWAY_FEDERATION_PATH_V2,
    GATEWAY_FEDERATION_SCHEMA_V2,
};
pub use federation_auth::{gateway_federation_token_claims, GATEWAY_FEDERATION_TOKEN_TTL_MS};
pub use ha::{
    GatewayFederationUrl, GatewayHaCoordinator, GatewayHaCoordinatorConfig,
    GatewayHaCoordinatorSnapshot, GatewayHaLifecycle, GatewayHaLifecycleSnapshot, GatewayHaMode,
    GatewayHaOwnershipSnapshot, GatewayHaOwnershipSource, GatewayHaSessionSnapshot,
    GatewayHaTenantBinding, GatewayHaWorkerSnapshot, GatewayInstanceLease,
    GatewayLocalRequestClaim, GatewayRegistryError, GatewayRegistryFuture, GatewayRegistryProvider,
    GatewayRegistryResult, GatewayRequestFence, GatewaySessionRecord, GatewayWorkerRecord,
    GatewayWorkerRegistration, RedisGatewayCredential, RedisGatewayRegistryConfig,
    RedisGatewayRegistryProvider, MAX_GATEWAY_INSTANCE_LEASE_TTL_MS,
    MAX_GATEWAY_REDIS_NAMESPACE_BYTES, MAX_GATEWAY_REGISTRY_CONCURRENCY,
    MAX_GATEWAY_RESOLVE_CANDIDATES,
};
pub use heartbeat::spawn_heartbeat_pruner;
pub use mesh::{
    MeshPeerRequest, MeshPeerResponse, MeshPeerTransport, MESH_HTTP_SCHEMA_V1, MESH_PEER_RELAY_PATH,
};
pub use metrics::GatewayMetrics;
pub use registry::CapabilityRegistry;
pub use resolver::{
    CapabilityResolver, SelectionPolicy, WorkerSelectionError, WorkerSelectionInput,
};
pub use router::EnvelopeRouter;
pub use runtime::{GatewayRuntime, GatewayRuntimeSnapshot, GatewayRuntimeState};
pub use service::make_gateway_router;
pub use session::GatewaySession;
pub use state::GatewayState;
pub use telemetry::{
    GatewayCapabilityTelemetrySnapshot, GatewayTelemetryExportError, GatewayTelemetryExporter,
    GatewayTelemetrySnapshot, MAX_GATEWAY_TELEMETRY_CAPABILITIES,
};
pub use tenant::TenantState;
pub use tenant_directory::SharedTenantState;

#[cfg(test)]
mod tests;
