// =============================================================================
//        #######
//     ###       ###     F: v1.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 13:21:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Control-plane protocol version 1.

use appcore_contracts::ServiceId;
use appcore_types::{
    CapabilityDescriptor, ClusterId, CoreId, CoreIdentity, DistributedCoreManifest, PeerEndpoint,
    RuntimeOperationalMode, TenantId, TraceContext,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

/// Version number of this control-plane wire contract.
pub const CONTROL_PLANE_PROTOCOL_VERSION: u16 = 1;
/// Registration endpoint path.
pub const CONTROL_REGISTER_PATH: &str = "/v1/control/register";
/// Heartbeat endpoint path.
pub const CONTROL_HEARTBEAT_PATH: &str = "/v1/control/heartbeat";
/// Peer-discovery endpoint path.
pub const CONTROL_PEERS_PATH: &str = "/v1/control/peers";
/// Service-scoped lease endpoint path.
pub const CONTROL_SERVICE_LEASE_PATH: &str = "/v1/control/service-lease";
/// Service-scoped lease release endpoint path.
pub const CONTROL_SERVICE_LEASE_RELEASE_PATH: &str = "/v1/control/service-lease/release";

/// Result returned by control-plane contracts.
pub type ControlPlaneResult<T> = Result<T, ControlPlaneError>;
/// Sendable future returned by a control-plane provider.
pub type ControlPlaneFuture<'a, T> =
    Pin<Box<dyn Future<Output = ControlPlaneResult<T>> + Send + 'a>>;

/// Provider-independent control-plane failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ControlPlaneError {
    /// No provider is available.
    #[error("control plane is offline")]
    Offline,
    /// A provider request exceeded its deadline.
    #[error("control plane request timed out")]
    Timeout,
    /// The provider rejected the operation.
    #[error("control plane rejected operation: {0}")]
    Rejected(String),
    /// The operation conflicts with current provider state.
    #[error("control plane state conflict: {0}")]
    Conflict(String),
    /// The provider returned a malformed or incompatible response.
    #[error("invalid control plane response: {0}")]
    InvalidResponse(String),
    /// Transport execution failed before a valid response was received.
    #[error("control plane transport failed: {0}")]
    Transport(String),
    /// Leadership could not be acquired or renewed.
    #[error("control plane lease is unavailable")]
    LeaseUnavailable,
}

/// Registration submitted by one running core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreRegistration {
    /// Public core manifest.
    pub manifest: DistributedCoreManifest,
    /// Client timestamp in milliseconds.
    pub registered_at_ms: u64,
    /// Current operational mode.
    pub operation_mode: RuntimeOperationalMode,
}

/// Presence record acknowledged by a control-plane provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorePresence {
    /// Distributed core identity.
    pub identity: CoreIdentity,
    /// Last reported operational mode.
    pub operation_mode: RuntimeOperationalMode,
    /// Whether the provider considers the core healthy.
    pub healthy: bool,
    /// Provider timestamp of the last accepted report.
    pub last_seen_ms: u64,
}

/// Heartbeat submitted by one running core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    /// Distributed core identity.
    pub identity: CoreIdentity,
    /// Current operational mode.
    pub operation_mode: RuntimeOperationalMode,
    /// Client timestamp in milliseconds.
    pub sent_at_ms: u64,
}

/// Heartbeat acknowledgement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatResponse {
    /// Whether the heartbeat was accepted.
    pub accepted: bool,
    /// Provider timestamp in milliseconds.
    pub server_time_ms: u64,
    /// Operational mode requested by coordination policy.
    pub operation_mode: RuntimeOperationalMode,
}

/// Compatible peers discovered for a tenant and cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerDirectory {
    /// Tenant boundary used for discovery.
    pub tenant_id: TenantId,
    /// Optional cluster boundary used for discovery.
    pub cluster_id: Option<ClusterId>,
    /// Discovered peer records.
    pub peers: Vec<PeerRecord>,
    /// Provider timestamp of the snapshot.
    pub refreshed_at_ms: u64,
}

/// Generic routing record for one distributed core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerRecord {
    /// Distributed peer identity.
    pub identity: CoreIdentity,
    /// Public endpoints advertised by the peer.
    pub endpoints: Vec<PeerEndpoint>,
    /// Generic capabilities advertised by the peer.
    pub capabilities: Vec<CapabilityDescriptor>,
    /// Whether the peer is eligible for routing.
    pub healthy: bool,
    /// Last accepted presence timestamp.
    pub last_seen_ms: u64,
    /// Non-sensitive routing metadata.
    pub metadata: BTreeMap<String, String>,
}

/// Leadership lease scoped to one independently coordinated service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceLeaderLease {
    /// Service governed by this lease.
    pub service_id: ServiceId,
    /// Tenant boundary.
    pub tenant_id: TenantId,
    /// Cluster boundary.
    pub cluster_id: ClusterId,
    /// Core currently holding leadership.
    pub holder_core_id: CoreId,
    /// Monotonic fencing epoch.
    pub epoch: u64,
    /// Acquisition timestamp in milliseconds.
    pub acquired_at_ms: u64,
    /// Expiration timestamp in milliseconds.
    pub expires_at_ms: u64,
}

/// Wire request for a service-scoped lease.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceLeaseRequest {
    /// Requesting core identity.
    pub identity: CoreIdentity,
    /// Service whose leadership is requested.
    pub service_id: ServiceId,
    /// Requested lease duration in milliseconds.
    pub ttl_ms: u64,
    /// Client timestamp in milliseconds.
    pub now_ms: u64,
}

/// Empty successful response used by release endpoints.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmptyResponse {}

/// Provider contract for presence, discovery and leadership coordination.
pub trait ControlPlaneProvider: Send + Sync {
    /// Registers a running core.
    fn register<'a>(
        &'a self,
        registration: CoreRegistration,
    ) -> ControlPlaneFuture<'a, CorePresence>;

    /// Reports liveness and current mode.
    fn heartbeat<'a>(
        &'a self,
        request: HeartbeatRequest,
    ) -> ControlPlaneFuture<'a, HeartbeatResponse>;

    /// Discovers compatible peers.
    fn discover_peers<'a>(
        &'a self,
        identity: &'a CoreIdentity,
    ) -> ControlPlaneFuture<'a, PeerDirectory>;

    /// Acquires or renews leadership independently for one service.
    fn acquire_or_renew_service_lease<'a>(
        &'a self,
        identity: &'a CoreIdentity,
        service_id: &'a ServiceId,
        ttl_ms: u64,
        now_ms: u64,
    ) -> ControlPlaneFuture<'a, ServiceLeaderLease>;

    /// Releases leadership for one service.
    fn release_service_lease<'a>(&'a self, lease: ServiceLeaderLease)
        -> ControlPlaneFuture<'a, ()>;

    /// Registers a core while propagating trace context when supported.
    fn register_traced<'a>(
        &'a self,
        registration: CoreRegistration,
        _trace: Option<&'a TraceContext>,
    ) -> ControlPlaneFuture<'a, CorePresence> {
        self.register(registration)
    }

    /// Sends a heartbeat while propagating trace context when supported.
    fn heartbeat_traced<'a>(
        &'a self,
        request: HeartbeatRequest,
        _trace: Option<&'a TraceContext>,
    ) -> ControlPlaneFuture<'a, HeartbeatResponse> {
        self.heartbeat(request)
    }

    /// Discovers peers while propagating trace context when supported.
    fn discover_peers_traced<'a>(
        &'a self,
        identity: &'a CoreIdentity,
        _trace: Option<&'a TraceContext>,
    ) -> ControlPlaneFuture<'a, PeerDirectory> {
        self.discover_peers(identity)
    }

    /// Acquires a service lease while propagating trace context when supported.
    fn acquire_or_renew_service_lease_traced<'a>(
        &'a self,
        identity: &'a CoreIdentity,
        service_id: &'a ServiceId,
        ttl_ms: u64,
        now_ms: u64,
        _trace: Option<&'a TraceContext>,
    ) -> ControlPlaneFuture<'a, ServiceLeaderLease> {
        self.acquire_or_renew_service_lease(identity, service_id, ttl_ms, now_ms)
    }

    /// Releases a service lease while propagating trace context when supported.
    fn release_service_lease_traced<'a>(
        &'a self,
        lease: ServiceLeaderLease,
        _trace: Option<&'a TraceContext>,
    ) -> ControlPlaneFuture<'a, ()> {
        self.release_service_lease(lease)
    }
}

/// Provider contract limited to compatible peer discovery.
pub trait DiscoveryProvider: Send + Sync {
    /// Discovers peers compatible with the supplied runtime identity.
    fn discover<'a>(&'a self, identity: &'a CoreIdentity) -> ControlPlaneFuture<'a, PeerDirectory>;

    /// Discovers peers while propagating trace context when supported.
    fn discover_traced<'a>(
        &'a self,
        identity: &'a CoreIdentity,
        trace: Option<&'a TraceContext>,
    ) -> ControlPlaneFuture<'a, PeerDirectory>;
}

impl<T> DiscoveryProvider for T
where
    T: ControlPlaneProvider + ?Sized,
{
    fn discover<'a>(&'a self, identity: &'a CoreIdentity) -> ControlPlaneFuture<'a, PeerDirectory> {
        self.discover_peers(identity)
    }

    fn discover_traced<'a>(
        &'a self,
        identity: &'a CoreIdentity,
        trace: Option<&'a TraceContext>,
    ) -> ControlPlaneFuture<'a, PeerDirectory> {
        self.discover_peers_traced(identity, trace)
    }
}

/// Checks leadership for an independently coordinated service.
pub trait ServiceLeadershipGuard: Send + Sync {
    /// Returns the current lease for `service_id`, when one is known.
    fn current_service_lease(&self, service_id: &ServiceId) -> Option<ServiceLeaderLease>;

    /// Checks whether `core_id` may write for `service_id` at `now_ms`.
    fn check_service_write_permission(
        &self,
        service_id: &ServiceId,
        tenant_id: &TenantId,
        cluster_id: &ClusterId,
        core_id: &CoreId,
        min_epoch: Option<u64>,
        now_ms: u64,
    ) -> LeadershipDecision;
}

/// Result of a leadership fencing check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeadershipDecision {
    /// The write is permitted.
    Allowed,
    /// No applicable lease exists.
    NoLease,
    /// The applicable lease has expired.
    Expired,
    /// The caller supplied an epoch newer than the known lease.
    StaleEpoch,
    /// Another core holds the applicable lease.
    WrongHolder,
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
