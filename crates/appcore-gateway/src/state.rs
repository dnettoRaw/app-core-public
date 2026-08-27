// =============================================================================
//        #######
//     ###       ###     F: state.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/26 08:53:09 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/26 08:53:09 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Shared central Gateway state.

use crate::config::GatewayConfig;
use crate::federation_transport::GatewayFederationTransport;
use crate::ha::{
    GatewayHaCoordinator, GatewayHaLifecycle, GatewayHaLifecycleSnapshot,
    GatewayHaOwnershipSnapshot, GatewayHaOwnershipSource, GatewayHaSessionSnapshot,
    GatewayHaWorkerSnapshot, GatewayRegistryError, GatewayRegistryResult,
};
use crate::metrics::GatewayMetrics;
use crate::tenant_directory::{SharedTenantState, TenantDirectory};
use crate::GatewayResult;
use appcore_peer_rpc::{BoundedReplayStore, PeerNonceStore, ReplayStoreConfig};
use appcore_security::HashTokenProvider;
use appcore_types::{ClusterId, TenantId};
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::watch;

/// Central, thread-safe, multi-tenant state repository for the Gateway capability.
///
/// The former public `tenants` map was removed because its single lock made
/// unrelated tenants block each other. Use [`Self::tenant_partition`],
/// [`Self::tenant_partition_or_insert`], [`Self::tenant_count`] and
/// [`Self::connection_count`]. Code accessing the removed field intentionally
/// fails to compile; no compatibility alias or mirror map is provided. Each
/// returned partition owns an independent lock.
pub struct GatewayState {
    /// Service configuration parameters.
    config: GatewayConfig,

    tenants: TenantDirectory,
    connection_admission: Mutex<()>,

    /// Live telemetry and performance counters.
    pub metrics: Arc<GatewayMetrics>,

    /// Token provider for cryptographic authentication checks.
    pub token_provider: HashTokenProvider,

    connection_replay: Arc<dyn PeerNonceStore>,
    ha_lifecycle: Option<Arc<GatewayHaLifecycle>>,
    ha_coordinator: Option<Arc<GatewayHaCoordinator>>,
    federation_transport: GatewayFederationTransport,
    shutdown: watch::Sender<bool>,
}

impl GatewayState {
    /// Validates configuration and instantiates the central Gateway state.
    pub fn new(config: GatewayConfig, token_provider: HashTokenProvider) -> GatewayResult<Self> {
        Self::with_replay_store(
            config,
            token_provider,
            Arc::new(BoundedReplayStore::new(ReplayStoreConfig::default())),
        )
    }

    /// Creates state with an explicit replay store shared by every accepted
    /// connection for this Gateway instance.
    pub fn with_replay_store(
        config: GatewayConfig,
        token_provider: HashTokenProvider,
        connection_replay: Arc<dyn PeerNonceStore>,
    ) -> GatewayResult<Self> {
        Self::build(config, token_provider, connection_replay, None, None)
    }

    /// Creates state with an explicit fail-closed HA admission lifecycle.
    pub fn with_ha_lifecycle(
        config: GatewayConfig,
        token_provider: HashTokenProvider,
        connection_replay: Arc<dyn PeerNonceStore>,
        ha_lifecycle: Arc<GatewayHaLifecycle>,
    ) -> GatewayResult<Self> {
        Self::build(
            config,
            token_provider,
            connection_replay,
            Some(ha_lifecycle),
            None,
        )
    }

    /// Creates state driven by an explicit shared-registry coordinator.
    pub fn with_ha_coordinator(
        config: GatewayConfig,
        token_provider: HashTokenProvider,
        connection_replay: Arc<dyn PeerNonceStore>,
        coordinator: Arc<GatewayHaCoordinator>,
    ) -> GatewayResult<Self> {
        let lifecycle = coordinator.lifecycle();
        Self::build(
            config,
            token_provider,
            connection_replay,
            Some(lifecycle),
            Some(coordinator),
        )
    }

    fn build(
        config: GatewayConfig,
        token_provider: HashTokenProvider,
        connection_replay: Arc<dyn PeerNonceStore>,
        ha_lifecycle: Option<Arc<GatewayHaLifecycle>>,
        ha_coordinator: Option<Arc<GatewayHaCoordinator>>,
    ) -> GatewayResult<Self> {
        config.validate()?;
        let (shutdown, _) = watch::channel(false);
        Ok(Self {
            config,
            tenants: TenantDirectory::new(),
            connection_admission: Mutex::new(()),
            metrics: GatewayMetrics::new(),
            token_provider,
            connection_replay,
            ha_lifecycle,
            ha_coordinator,
            federation_transport: GatewayFederationTransport::default(),
            shutdown,
        })
    }

    /// Returns the validated immutable service configuration.
    pub fn config(&self) -> &GatewayConfig {
        &self.config
    }

    /// Returns the number of active tenant partitions.
    pub fn tenant_count(&self) -> usize {
        self.tenants.len()
    }

    /// Returns the current bounded worker and client connection count.
    pub fn connection_count(&self) -> usize {
        self.tenants.connection_count()
    }

    /// Returns the opt-in HA lifecycle snapshot, when configured.
    pub fn ha_lifecycle_snapshot(&self) -> Option<GatewayHaLifecycleSnapshot> {
        self.ha_lifecycle
            .as_ref()
            .map(|lifecycle| lifecycle.snapshot())
    }

    pub(crate) fn admit_ha_work(&self) -> GatewayRegistryResult<()> {
        self.ha_lifecycle
            .as_ref()
            .map_or(Ok(()), |lifecycle| lifecycle.admit())
    }

    pub(crate) fn admit_ha_tenant(&self, tenant_id: &TenantId) -> GatewayRegistryResult<()> {
        match &self.ha_coordinator {
            Some(coordinator) => coordinator.lease_for(tenant_id).map(|_| ()),
            None => self.admit_ha_work(),
        }
    }

    pub(crate) fn admit_ha_boundary(
        &self,
        tenant_id: &TenantId,
        cluster_id: &ClusterId,
    ) -> GatewayRegistryResult<()> {
        match &self.ha_coordinator {
            Some(coordinator) => coordinator.lease_for(tenant_id).and_then(|lease| {
                (lease.cluster_id() == cluster_id)
                    .then_some(())
                    .ok_or(GatewayRegistryError::InvalidContract)
            }),
            None => self.admit_ha_work(),
        }
    }

    pub(crate) fn ha_coordinator(&self) -> Option<Arc<GatewayHaCoordinator>> {
        self.ha_coordinator.as_ref().map(Arc::clone)
    }

    pub(crate) fn federation_transport(&self) -> GatewayFederationTransport {
        self.federation_transport.clone()
    }

    /// Returns the independently synchronized state partition for one tenant.
    pub fn tenant_partition(&self, tenant_id: &TenantId) -> Option<SharedTenantState> {
        self.tenants.get(tenant_id)
    }

    /// Returns or creates one bounded, independently synchronized tenant partition.
    pub fn tenant_partition_or_insert(
        &self,
        tenant_id: &TenantId,
    ) -> GatewayResult<SharedTenantState> {
        self.tenants.get_or_insert(tenant_id)
    }

    pub(crate) fn tenant_entries(&self) -> Vec<(TenantId, SharedTenantState)> {
        self.tenants.entries()
    }

    pub(crate) fn lock_connection_admission(&self) -> parking_lot::MutexGuard<'_, ()> {
        self.connection_admission.lock()
    }

    /// Requests cooperative termination of all Gateway-owned background work
    /// and active connection loops.
    pub fn request_shutdown(&self) {
        if let Some(lifecycle) = &self.ha_lifecycle {
            lifecycle.stop();
        }
        self.shutdown.send_replace(true);
    }

    /// Reports whether cooperative shutdown has been requested.
    pub fn is_shutting_down(&self) -> bool {
        *self.shutdown.borrow()
    }

    pub(crate) fn subscribe_shutdown(&self) -> watch::Receiver<bool> {
        self.shutdown.subscribe()
    }

    pub(crate) fn connection_replay(&self) -> &dyn PeerNonceStore {
        self.connection_replay.as_ref()
    }

    pub(crate) async fn wait_for_shutdown(&self) {
        let mut shutdown = self.subscribe_shutdown();
        while !*shutdown.borrow() {
            if shutdown.changed().await.is_err() {
                break;
            }
        }
    }
}

impl GatewayHaOwnershipSource for GatewayState {
    fn snapshot(&self, now_ms: u64) -> GatewayRegistryResult<GatewayHaOwnershipSnapshot> {
        let mut snapshot = GatewayHaOwnershipSnapshot::default();
        for (tenant_id, tenant) in self.tenant_entries() {
            let tenant = tenant.read();
            for worker in tenant.workers.values() {
                let cluster_id = worker
                    .cluster_id()
                    .cloned()
                    .ok_or(GatewayRegistryError::InvalidContract)?;
                snapshot.workers.push(GatewayHaWorkerSnapshot {
                    tenant_id: tenant_id.clone(),
                    cluster_id,
                    registration: crate::GatewayWorkerRegistration::new(
                        worker.key.installation_id.clone(),
                        worker.key.core_id.clone(),
                        worker.generation(),
                        tenant.registry.capabilities_for(&worker.key),
                    )?,
                });
            }
            snapshot.sessions.extend(
                tenant
                    .sessions
                    .values()
                    .filter(|session| !session.is_expired(now_ms))
                    .map(|session| GatewayHaSessionSnapshot {
                        tenant_id: tenant_id.clone(),
                        session_id: session.session_id.clone(),
                        expires_at_ms: session.expires_at_ms,
                    }),
            );
        }
        Ok(snapshot)
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
