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
use crate::metrics::GatewayMetrics;
use crate::tenant_directory::{SharedTenantState, TenantDirectory};
use crate::GatewayResult;
use appcore_peer_rpc::{BoundedReplayStore, PeerNonceStore, ReplayStoreConfig};
use appcore_security::HashTokenProvider;
use appcore_types::TenantId;
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::watch;

/// Central, thread-safe, multi-tenant state repository for the Gateway capability.
///
/// The former public `tenants` map was removed because its single lock made
/// unrelated tenants block each other. Use [`Self::tenant_partition`],
/// [`Self::tenant_partition_or_insert`], [`Self::tenant_count`] and
/// [`Self::connection_count`]. Each returned partition owns an independent lock.
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
        config.validate()?;
        let (shutdown, _) = watch::channel(false);
        Ok(Self {
            config,
            tenants: TenantDirectory::new(),
            connection_admission: Mutex::new(()),
            metrics: GatewayMetrics::new(),
            token_provider,
            connection_replay,
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
