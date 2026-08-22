// =============================================================================
//        #######
//     ###       ###     F: swarm.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AiContributionPolicy, AiError, AiRequest, AiResponse, AiResult, ArtifactDigest,
    ArtifactLocation, BackendFuture, BackendHealth, BackendId, CancellationToken, CapabilityId,
    DeviceId, DeviceKind, DeviceMemoryKind, DeviceSnapshot, ModelDescriptor, PeerId,
    PlacementMetrics, ResourceBudget, ResourceEstimate,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::Duration;

const MAX_DIRECTORY_PEERS: usize = 4_096;
const MAX_ADVERTISED_DEVICES: usize = 64;
const MAX_ADVERTISED_ARTIFACTS: usize = 1_024;
const MAX_ADVERTISEMENT_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_ADVERTISED_TRANSFERS: usize = 1_024;

/// One compute device in an authenticated, expiring peer advertisement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdvertisedCompute {
    /// Backend offered by the remote Runtime.
    pub backend: BackendId,
    /// Peer-owned device.
    pub device: DeviceId,
    /// Backend-neutral device kind.
    pub kind: DeviceKind,
    /// Current bounded observations, never total physical capacity.
    pub metrics: PlacementMetrics,
}

impl AdvertisedCompute {
    /// Clamps one physical snapshot to the budget the local node chose to donate.
    #[must_use]
    pub fn from_device(
        backend: BackendId,
        device: &DeviceSnapshot,
        budget: ResourceBudget,
    ) -> Self {
        let unified = device.capabilities.memory_kind == DeviceMemoryKind::Unified;
        Self {
            backend,
            device: device.id.clone(),
            kind: device.kind,
            metrics: PlacementMetrics {
                load_percent: device.utilization_percent,
                available_memory_bytes: unified.then(|| {
                    device
                        .available_memory_bytes
                        .unwrap_or(budget.memory_bytes.unwrap_or_default())
                        .min(budget.memory_bytes.unwrap_or_default())
                }),
                available_vram_bytes: (!unified).then(|| {
                    device
                        .available_memory_bytes
                        .unwrap_or(budget.vram_bytes.unwrap_or_default())
                        .min(budget.vram_bytes.unwrap_or_default())
                }),
                ..PlacementMetrics::default()
            },
        }
    }
}

/// Donated artifact-storage budget independent from compute contribution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdvertisedStorage {
    /// Maximum bytes exposed by current contribution policy.
    pub available_bytes: u64,
    /// Maximum simultaneous artifact transfers.
    pub max_transfers: usize,
}

/// Signed-by-infrastructure peer resource view consumed after authorization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiNodeCapabilities {
    /// Advertising peer.
    pub peer: PeerId,
    /// Authorized tenant scope.
    pub tenant: CapabilityId,
    /// Capture timestamp in the control-plane monotonic time domain.
    pub captured_at_ms: u64,
    /// Expiration from the control-plane monotonic time domain.
    pub expires_at_ms: u64,
    /// Independently donated compute devices.
    pub compute: Vec<AdvertisedCompute>,
    /// Independently donated artifact storage.
    pub storage: Option<AdvertisedStorage>,
    /// Bounded exact artifact digests currently advertised.
    pub model_artifacts: BTreeSet<ArtifactDigest>,
    /// Current donated budget after local contribution policy.
    pub current_budget: ResourceBudget,
}

impl AiNodeCapabilities {
    /// Creates a bounded advertisement and enforces local contribution ceilings.
    #[allow(clippy::too_many_arguments)]
    pub fn from_contribution(
        peer: PeerId,
        tenant: CapabilityId,
        captured_at_ms: u64,
        expires_at_ms: u64,
        compute: Vec<AdvertisedCompute>,
        storage: Option<AdvertisedStorage>,
        model_artifacts: BTreeSet<ArtifactDigest>,
        current_budget: ResourceBudget,
        policy: AiContributionPolicy,
    ) -> AiResult<Self> {
        if (!policy.contribute_compute && !compute.is_empty())
            || (!policy.contribute_storage && (storage.is_some() || !model_artifacts.is_empty()))
            || current_budget.cpu_percent > policy.max_cpu_percent
            || current_budget.gpu_percent > policy.max_gpu_percent
            || current_budget.memory_bytes.unwrap_or_default() > policy.max_memory_bytes
            || current_budget.vram_bytes.unwrap_or_default() > policy.max_vram_bytes
            || current_budget.storage_bytes > policy.max_storage_bytes
            || current_budget.workers > policy.max_workers
            || current_budget.concurrent_jobs > policy.max_concurrent_jobs
        {
            return Err(AiError::Unauthorized);
        }
        let capabilities = Self {
            peer,
            tenant,
            captured_at_ms,
            expires_at_ms,
            compute,
            storage,
            model_artifacts,
            current_budget,
        };
        capabilities.validate(64, 1_024)?;
        Ok(capabilities)
    }

    fn validate(&self, max_devices: usize, max_artifacts: usize) -> AiResult<()> {
        let mut devices = BTreeSet::new();
        if self.expires_at_ms <= self.captured_at_ms
            || self.compute.len() > max_devices
            || self.model_artifacts.len() > max_artifacts
            || self.current_budget.cpu_percent > 100
            || self.current_budget.gpu_percent > 100
            || (!self.compute.is_empty()
                && (self.current_budget.workers == 0 || self.current_budget.concurrent_jobs == 0))
            || self.compute.iter().any(|compute| {
                compute.metrics.load_percent.is_some_and(|load| load > 100)
                    || compute
                        .metrics
                        .available_memory_bytes
                        .is_some_and(|available| {
                            available > self.current_budget.memory_bytes.unwrap_or_default()
                        })
                    || compute
                        .metrics
                        .available_vram_bytes
                        .is_some_and(|available| {
                            available > self.current_budget.vram_bytes.unwrap_or_default()
                        })
                    || !devices.insert((&compute.backend, &compute.device))
            })
            || self.storage.is_some_and(|storage| {
                storage.available_bytes == 0
                    || storage.available_bytes > self.current_budget.storage_bytes
                    || storage.max_transfers == 0
                    || storage.max_transfers > MAX_ADVERTISED_TRANSFERS
            })
            || (!self.model_artifacts.is_empty() && self.storage.is_none())
        {
            return Err(AiError::InvalidInput("AI peer advertisement"));
        }
        Ok(())
    }
}

/// Authorization result produced by an AppCore security adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeerAuthorization {
    /// Authentication over the advertisement succeeded.
    pub authenticated: bool,
    /// Authorized tenant scopes.
    pub tenants: BTreeSet<CapabilityId>,
    /// Whether remote AI compute is granted.
    pub allow_compute: bool,
    /// Whether peer artifact storage is granted.
    pub allow_storage: bool,
}

/// Boundary implemented with existing AppCore security and replay contracts.
pub trait PeerCapabilityAuthorizer: Send + Sync {
    /// Authenticates and authorizes one complete advertisement.
    fn authorize(&self, capabilities: &AiNodeCapabilities) -> AiResult<PeerAuthorization>;
}

/// Bounds for an expiring in-memory peer view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PeerDirectoryConfig {
    /// Maximum tracked peers.
    pub max_peers: usize,
    /// Maximum devices in one advertisement.
    pub max_devices_per_peer: usize,
    /// Maximum artifact digests in one advertisement.
    pub max_artifacts_per_peer: usize,
    /// Maximum accepted advertisement lifetime.
    pub max_ttl: Duration,
}

/// Low-cardinality authenticated peer-directory counters and gauges.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PeerDirectoryMetrics {
    /// Current unexpired peers after the most recent prune.
    pub current_peers: usize,
    /// Currently advertised compute devices.
    pub compute_devices: usize,
    /// Current peers donating artifact storage.
    pub storage_peers: usize,
    /// Current donated worker slots.
    pub donated_workers: usize,
    /// Current donated storage bytes.
    pub donated_storage_bytes: u64,
    /// Authenticated advertisements accepted.
    pub accepted_updates: u64,
    /// Invalid or unauthorized advertisements rejected.
    pub rejected_updates: u64,
    /// Entries pruned after expiration.
    pub expired_peers: u64,
}

impl Default for PeerDirectoryConfig {
    fn default() -> Self {
        Self {
            max_peers: 64,
            max_devices_per_peer: 16,
            max_artifacts_per_peer: 256,
            max_ttl: Duration::from_secs(30),
        }
    }
}

#[derive(Clone, Debug)]
struct AuthorizedCapabilities {
    capabilities: AiNodeCapabilities,
    authorization: PeerAuthorization,
}

/// Expiring capability cache; it is not a discovery service or control plane.
#[derive(Debug)]
pub struct PeerCapabilityDirectory {
    config: PeerDirectoryConfig,
    peers: RwLock<BTreeMap<PeerId, AuthorizedCapabilities>>,
    accepted_updates: AtomicU64,
    rejected_updates: AtomicU64,
    expired_peers: AtomicU64,
}

impl PeerCapabilityDirectory {
    /// Creates an empty bounded peer cache.
    pub fn new(config: PeerDirectoryConfig) -> AiResult<Self> {
        if config.max_peers == 0
            || config.max_peers > MAX_DIRECTORY_PEERS
            || config.max_devices_per_peer == 0
            || config.max_devices_per_peer > MAX_ADVERTISED_DEVICES
            || config.max_artifacts_per_peer == 0
            || config.max_artifacts_per_peer > MAX_ADVERTISED_ARTIFACTS
            || config.max_ttl.is_zero()
            || config.max_ttl > MAX_ADVERTISEMENT_TTL
        {
            return Err(AiError::InvalidInput("AI peer directory"));
        }
        Ok(Self {
            config,
            peers: RwLock::new(BTreeMap::new()),
            accepted_updates: AtomicU64::new(0),
            rejected_updates: AtomicU64::new(0),
            expired_peers: AtomicU64::new(0),
        })
    }

    /// Replaces one peer view only after AppCore authentication and authorization.
    pub fn update(
        &self,
        capabilities: AiNodeCapabilities,
        authorizer: &dyn PeerCapabilityAuthorizer,
        now_ms: u64,
    ) -> AiResult<()> {
        let result = self.update_inner(capabilities, authorizer, now_ms);
        if result.is_ok() {
            self.accepted_updates.fetch_add(1, Ordering::Relaxed);
        } else {
            self.rejected_updates.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    fn update_inner(
        &self,
        capabilities: AiNodeCapabilities,
        authorizer: &dyn PeerCapabilityAuthorizer,
        now_ms: u64,
    ) -> AiResult<()> {
        capabilities.validate(
            self.config.max_devices_per_peer,
            self.config.max_artifacts_per_peer,
        )?;
        let max_expiry = now_ms.saturating_add(millis(self.config.max_ttl));
        if capabilities.captured_at_ms > now_ms
            || capabilities.expires_at_ms <= now_ms
            || capabilities.expires_at_ms > max_expiry
        {
            return Err(AiError::Unauthorized);
        }
        let authorization = authorizer.authorize(&capabilities)?;
        if !authorization.authenticated
            || !authorization.tenants.contains(&capabilities.tenant)
            || (!capabilities.compute.is_empty() && !authorization.allow_compute)
            || ((capabilities.storage.is_some() || !capabilities.model_artifacts.is_empty())
                && !authorization.allow_storage)
        {
            return Err(AiError::Unauthorized);
        }
        let mut peers = self.peers.write().map_err(|_| AiError::InternalState)?;
        if !peers.contains_key(&capabilities.peer) && peers.len() >= self.config.max_peers {
            return Err(AiError::Capacity("AI peer directory"));
        }
        if peers
            .get(&capabilities.peer)
            .is_some_and(|current| capabilities.expires_at_ms <= current.capabilities.expires_at_ms)
        {
            return Err(AiError::Conflict("stale AI peer advertisement"));
        }
        peers.insert(
            capabilities.peer.clone(),
            AuthorizedCapabilities {
                capabilities,
                authorization,
            },
        );
        Ok(())
    }

    /// Returns stable authorized, live tenant views and prunes expired entries.
    pub fn live(
        &self,
        tenant: &CapabilityId,
        now_ms: u64,
        limit: usize,
    ) -> AiResult<Vec<AiNodeCapabilities>> {
        if limit == 0 || limit > self.config.max_peers {
            return Err(AiError::InvalidInput("AI peer query limit"));
        }
        let mut peers = self.peers.write().map_err(|_| AiError::InternalState)?;
        let before = peers.len();
        peers.retain(|_, peer| peer.capabilities.expires_at_ms > now_ms);
        self.expired_peers.fetch_add(
            u64::try_from(before.saturating_sub(peers.len())).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        Ok(peers
            .values()
            .filter(|peer| {
                peer.authorization.tenants.contains(tenant) && &peer.capabilities.tenant == tenant
            })
            .take(limit)
            .map(|peer| peer.capabilities.clone())
            .collect())
    }

    /// Returns aggregate peer availability and contribution metrics.
    pub fn metrics(&self) -> AiResult<PeerDirectoryMetrics> {
        let peers = self.peers.read().map_err(|_| AiError::InternalState)?;
        Ok(PeerDirectoryMetrics {
            current_peers: peers.len(),
            compute_devices: peers
                .values()
                .map(|peer| peer.capabilities.compute.len())
                .sum(),
            storage_peers: peers
                .values()
                .filter(|peer| peer.capabilities.storage.is_some())
                .count(),
            donated_workers: peers
                .values()
                .map(|peer| peer.capabilities.current_budget.workers)
                .sum(),
            donated_storage_bytes: peers.values().fold(0u64, |sum, peer| {
                sum.saturating_add(peer.capabilities.current_budget.storage_bytes)
            }),
            accepted_updates: self.accepted_updates.load(Ordering::Relaxed),
            rejected_updates: self.rejected_updates.load(Ordering::Relaxed),
            expired_peers: self.expired_peers.load(Ordering::Relaxed),
        })
    }
}

/// One authorized remote compute placement returned to the shared scheduler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SwarmRoute {
    /// Internal peer identity.
    pub peer: PeerId,
    /// Low-cardinality class safe for returned diagnostics.
    pub peer_class: String,
    /// Authorized tenant.
    pub tenant: CapabilityId,
    /// Backend offered by the peer.
    pub backend: BackendId,
    /// Remote device.
    pub device: DeviceId,
    /// Remote device kind.
    pub kind: DeviceKind,
    /// Remote backend health.
    pub health: BackendHealth,
    /// Current remote observations.
    pub metrics: PlacementMetrics,
    /// Peak resource estimate already checked against advertised budget.
    pub resources: ResourceEstimate,
    /// Artifact source selected independently from compute.
    pub artifact_source: Option<ArtifactLocation>,
    /// Whether the model is already hot on this compute target.
    pub model_resident: bool,
    /// Cold activation estimate.
    pub load_time_ms: u64,
    /// Artifact-transfer cost.
    pub transfer_cost_units: u64,
    /// Inference cost.
    pub inference_cost_units: u64,
    /// Recent RTT.
    pub rtt_ms: u64,
    /// Recent bandwidth.
    pub bandwidth_bytes_per_second: Option<u64>,
    /// Failover cost.
    pub failover_cost_units: u64,
    /// Remaining authenticated route lease.
    pub lease_remaining: Duration,
}

impl SwarmRoute {
    /// Validates bounded, policy-neutral route claims before scheduler use.
    pub fn validate(&self, model: &ModelDescriptor) -> AiResult<()> {
        if self.peer_class.is_empty()
            || self.peer_class.len() > 32
            || !self
                .peer_class
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
            || self.lease_remaining.is_zero()
            || self.rtt_ms == 0
            || !model.supports_route(&self.backend, self.kind)
        {
            return Err(AiError::InvalidInput("AI swarm route"));
        }
        Ok(())
    }
}

/// Authenticated transport adapter implemented with AppCore Peer RPC and artifact transfer.
pub trait SwarmBridge: Send + Sync {
    /// Returns bounded authorized routes; no generic RPC may carry giant model bytes.
    fn routes(
        &self,
        request: &AiRequest,
        model: &ModelDescriptor,
        max_peers: usize,
    ) -> AiResult<Vec<SwarmRoute>>;

    /// Executes with transport timeout, replay protection and tenant checks in the adapter.
    fn execute<'a>(
        &'a self,
        route: &'a SwarmRoute,
        request: &'a AiRequest,
        cancellation: &'a CancellationToken,
    ) -> BackendFuture<'a, AiResponse>;
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
