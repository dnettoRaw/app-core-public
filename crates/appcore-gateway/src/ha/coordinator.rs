// =============================================================================
//        #######
//     ###       ###     F: coordinator.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.4-rc
// =============================================================================

//! Bounded tenant-lease coordination for an opt-in HA Gateway instance.

use super::coordinator_support::*;
use super::{
    GatewayFederationUrl, GatewayHaLifecycle, GatewayHaLifecycleSnapshot, GatewayHaMode,
    GatewayHaOwnershipSnapshot, GatewayHaOwnershipSource, GatewayInstanceLease,
    GatewayRegistryError, GatewayRegistryFuture, GatewayRegistryProvider, GatewayRegistryResult,
    GatewaySessionRecord, GatewayWorkerRecord, MAX_GATEWAY_INSTANCE_LEASE_TTL_MS,
    MAX_GATEWAY_REGISTRY_CONCURRENCY,
};
use crate::config::MAX_GATEWAY_TENANTS;
use appcore_types::{ClusterId, InstanceId, TenantId};
use futures_util::{stream, StreamExt};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, Mutex};

/// Minimum ownership TTL accepted by the HA coordinator.
pub const MIN_GATEWAY_HA_LEASE_TTL_MS: u64 = 1_000;

/// One configured tenant and cluster ownership boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayHaTenantBinding {
    /// Tenant whose local connections this instance may own.
    pub tenant_id: TenantId,
    /// Cluster containing those local connections.
    pub cluster_id: ClusterId,
}

/// Validated fixed ownership configuration for one Gateway process.
#[derive(Debug, Clone)]
pub struct GatewayHaCoordinatorConfig {
    /// Stable identity of this Gateway process instance.
    pub instance_id: InstanceId,
    /// Authenticated federation endpoint advertised to peers.
    pub federation_url: GatewayFederationUrl,
    /// Complete bounded tenant ownership set.
    pub tenants: Vec<GatewayHaTenantBinding>,
    /// Shared ownership lease duration.
    pub lease_ttl: Duration,
    /// Interval between exact lease renewals.
    pub renewal_interval: Duration,
}

impl GatewayHaCoordinatorConfig {
    /// Validates tenant uniqueness and renewal safety bounds.
    pub fn validate(&self) -> GatewayRegistryResult<()> {
        let ttl_ms = duration_ms(self.lease_ttl)?;
        let renewal_ms = duration_ms(self.renewal_interval)?;
        if self.tenants.is_empty()
            || self.tenants.len() > MAX_GATEWAY_TENANTS
            || !(MIN_GATEWAY_HA_LEASE_TTL_MS..=MAX_GATEWAY_INSTANCE_LEASE_TTL_MS).contains(&ttl_ms)
            || renewal_ms > ttl_ms / 3
        {
            return Err(GatewayRegistryError::InvalidContract);
        }
        for (index, binding) in self.tenants.iter().enumerate() {
            if self.tenants[index + 1..]
                .iter()
                .any(|candidate| candidate.tenant_id == binding.tenant_id)
            {
                return Err(GatewayRegistryError::InvalidContract);
            }
        }
        Ok(())
    }
}

/// Safe coordinator telemetry without endpoints, credentials, or tenant IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatewayHaCoordinatorSnapshot {
    /// Admission lifecycle and recovery telemetry.
    pub lifecycle: GatewayHaLifecycleSnapshot,
    /// Number of configured tenant ownership boundaries.
    pub configured_tenants: usize,
    /// Number of current locally retained leases.
    pub active_leases: usize,
    /// Number of worker ownership records replayed under the current epochs.
    pub active_workers: usize,
    /// Number of session ownership records replayed under the current epochs.
    pub active_sessions: usize,
    /// Complete acquisition rounds.
    pub acquisitions: u64,
    /// Complete renewal rounds.
    pub renewals: u64,
    /// Provider rounds that failed closed.
    pub failures: u64,
    /// Shared request claims accepted by the provider.
    pub request_claims: u64,
    /// Shared request completions accepted by exact fences.
    pub request_completions: u64,
    /// Shared request cancellations accepted by the origin fence.
    pub request_cancellations: u64,
    /// Successful remote-owner federation responses accepted under a fence.
    pub remote_forwards: u64,
}

/// Serializes acquisition, renewal, recovery, and release for one HA owner.
pub struct GatewayHaCoordinator {
    pub(super) provider: Arc<dyn GatewayRegistryProvider>,
    pub(super) lifecycle: Arc<GatewayHaLifecycle>,
    config: GatewayHaCoordinatorConfig,
    pub(super) ownership: RwLock<CoordinatorOwnership>,
    pub(super) operation: Mutex<()>,
    acquisitions: AtomicU64,
    renewals: AtomicU64,
    failures: AtomicU64,
    pub(super) request_claims: AtomicU64,
    pub(super) request_completions: AtomicU64,
    pub(super) request_cancellations: AtomicU64,
    pub(super) remote_forwards: AtomicU64,
}

#[derive(Default)]
pub(super) struct CoordinatorOwnership {
    pub(super) leases: Vec<GatewayInstanceLease>,
    pub(super) workers: Vec<GatewayWorkerRecord>,
    pub(super) sessions: Vec<GatewaySessionRecord>,
}

pub(super) enum RecoveredOwnership {
    Worker(GatewayWorkerRecord),
    Session(GatewaySessionRecord),
}

impl GatewayHaCoordinator {
    /// Creates a stopped coordinator. No work is admitted before recovery.
    pub fn new(
        provider: Arc<dyn GatewayRegistryProvider>,
        lifecycle: Arc<GatewayHaLifecycle>,
        config: GatewayHaCoordinatorConfig,
    ) -> GatewayRegistryResult<Self> {
        config.validate()?;
        Ok(Self {
            provider,
            lifecycle,
            config,
            ownership: RwLock::new(CoordinatorOwnership::default()),
            operation: Mutex::new(()),
            acquisitions: AtomicU64::new(0),
            renewals: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            request_claims: AtomicU64::new(0),
            request_completions: AtomicU64::new(0),
            request_cancellations: AtomicU64::new(0),
            remote_forwards: AtomicU64::new(0),
        })
    }

    /// Acquires a fresh epoch for every configured tenant before admission.
    pub async fn recover(
        &self,
        source: &dyn GatewayHaOwnershipSource,
        now_ms: u64,
    ) -> GatewayRegistryResult<()> {
        let _operation = self.operation.lock().await;
        self.lifecycle.begin_recovery(now_ms)?;
        *self.ownership.write() = CoordinatorOwnership::default();
        let acquired = match self.acquire_all(now_ms).await {
            Ok(acquired) => acquired,
            Err((error, acquired)) => {
                let _ = self.release_all(&acquired).await;
                self.fail(error);
                return Err(error);
            }
        };
        let snapshot = match source.snapshot(now_ms).and_then(|snapshot| {
            snapshot.validate(&self.config.tenants, now_ms)?;
            Ok(snapshot)
        }) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let _ = self.release_all(&acquired).await;
                self.fail(error);
                return Err(error);
            }
        };
        let ownership = match self.replay(acquired, &snapshot, now_ms).await {
            Ok(ownership) => ownership,
            Err((error, leases)) => {
                let _ = self.release_all(&leases).await;
                self.fail(error);
                return Err(error);
            }
        };
        *self.ownership.write() = ownership;
        increment(&self.acquisitions);
        self.lifecycle.mark_healthy(now_ms)
    }

    /// Renews every exact epoch; any uncertainty clears admission and leases.
    pub async fn renew(&self, now_ms: u64) -> GatewayRegistryResult<()> {
        let _operation = self.operation.lock().await;
        self.lifecycle.admit()?;
        let (current, workers) = {
            let ownership = self.ownership.read();
            (ownership.leases.clone(), ownership.workers.clone())
        };
        if current.len() != self.config.tenants.len() {
            self.fail(GatewayRegistryError::Unavailable);
            return Err(GatewayRegistryError::Unavailable);
        }
        let renewed = match self.renew_all(&current, now_ms).await {
            Ok(renewed) => renewed,
            Err(error) => {
                *self.ownership.write() = CoordinatorOwnership::default();
                self.fail(error);
                return Err(error);
            }
        };
        let renewed_workers = match self.renew_workers(&renewed, &workers, now_ms).await {
            Ok(workers) => workers,
            Err(error) => {
                *self.ownership.write() = CoordinatorOwnership::default();
                self.fail(error);
                return Err(error);
            }
        };
        let mut ownership = self.ownership.write();
        ownership.leases = renewed;
        ownership.workers = renewed_workers;
        increment(&self.renewals);
        Ok(())
    }

    /// Returns the current exact tenant lease only while admission is healthy.
    pub fn lease_for(&self, tenant_id: &TenantId) -> GatewayRegistryResult<GatewayInstanceLease> {
        self.lifecycle.admit()?;
        self.ownership
            .read()
            .leases
            .iter()
            .find(|lease| lease.tenant_id() == tenant_id)
            .cloned()
            .ok_or(GatewayRegistryError::InvalidContract)
    }

    /// Returns the shared admission lifecycle driven by this coordinator.
    pub fn lifecycle(&self) -> Arc<GatewayHaLifecycle> {
        Arc::clone(&self.lifecycle)
    }

    /// Runs renewal and bounded recovery attempts until cooperative shutdown.
    pub async fn run(
        self: Arc<Self>,
        source: Arc<dyn GatewayHaOwnershipSource>,
        mut shutdown: watch::Receiver<bool>,
    ) {
        while !*shutdown.borrow() {
            let now_ms = unix_now_ms();
            let mode = self.lifecycle.snapshot().mode;
            let _ = match mode {
                GatewayHaMode::Healthy => self.renew(now_ms).await,
                GatewayHaMode::Stopped | GatewayHaMode::Isolated => {
                    self.recover(source.as_ref(), now_ms).await
                }
                GatewayHaMode::Recovering => Err(GatewayRegistryError::Unavailable),
            };
            tokio::select! {
                _ = tokio::time::sleep(self.config.renewal_interval) => {}
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
            }
        }
        let _ = self.stop().await;
    }

    /// Stops admission first, then best-effort releases all exact leases.
    pub async fn stop(&self) -> GatewayRegistryResult<()> {
        let _operation = self.operation.lock().await;
        self.lifecycle.stop();
        let leases = std::mem::take(&mut self.ownership.write().leases);
        *self.ownership.write() = CoordinatorOwnership::default();
        self.release_all(&leases).await
    }

    /// Returns bounded, redacted operational telemetry.
    pub fn snapshot(&self) -> GatewayHaCoordinatorSnapshot {
        let ownership = self.ownership.read();
        GatewayHaCoordinatorSnapshot {
            lifecycle: self.lifecycle.snapshot(),
            configured_tenants: self.config.tenants.len(),
            active_leases: ownership.leases.len(),
            active_workers: ownership.workers.len(),
            active_sessions: ownership.sessions.len(),
            acquisitions: self.acquisitions.load(Ordering::Relaxed),
            renewals: self.renewals.load(Ordering::Relaxed),
            failures: self.failures.load(Ordering::Relaxed),
            request_claims: self.request_claims.load(Ordering::Relaxed),
            request_completions: self.request_completions.load(Ordering::Relaxed),
            request_cancellations: self.request_cancellations.load(Ordering::Relaxed),
            remote_forwards: self.remote_forwards.load(Ordering::Relaxed),
        }
    }

    async fn acquire(
        &self,
        binding: &GatewayHaTenantBinding,
        now_ms: u64,
    ) -> GatewayRegistryResult<GatewayInstanceLease> {
        self.provider
            .acquire_instance(
                &binding.tenant_id,
                &binding.cluster_id,
                &self.config.instance_id,
                &self.config.federation_url,
                self.lease_ttl_ms(),
                now_ms,
            )
            .await
    }

    async fn acquire_all(
        &self,
        now_ms: u64,
    ) -> Result<Vec<GatewayInstanceLease>, (GatewayRegistryError, Vec<GatewayInstanceLease>)> {
        let operations = stream::iter(self.config.tenants.clone())
            .map(|binding| async move { self.acquire(&binding, now_ms).await })
            .buffer_unordered(MAX_GATEWAY_REGISTRY_CONCURRENCY)
            .collect::<Vec<_>>();
        let results = match tokio::time::timeout(operation_round_timeout(), operations).await {
            Ok(results) => results,
            Err(_) => return Err((GatewayRegistryError::Unavailable, Vec::new())),
        };
        collect_acquired(results)
    }

    async fn renew_all(
        &self,
        leases: &[GatewayInstanceLease],
        now_ms: u64,
    ) -> GatewayRegistryResult<Vec<GatewayInstanceLease>> {
        let ttl_ms = self.lease_ttl_ms();
        let operations = stream::iter(leases.to_vec())
            .map(|lease| async move { self.provider.renew_instance(&lease, ttl_ms, now_ms).await })
            .buffer_unordered(MAX_GATEWAY_REGISTRY_CONCURRENCY)
            .collect::<Vec<_>>();
        let results = tokio::time::timeout(operation_round_timeout(), operations)
            .await
            .map_err(|_| GatewayRegistryError::Unavailable)?;
        results.into_iter().collect()
    }

    async fn replay(
        &self,
        leases: Vec<GatewayInstanceLease>,
        snapshot: &GatewayHaOwnershipSnapshot,
        now_ms: u64,
    ) -> Result<CoordinatorOwnership, (GatewayRegistryError, Vec<GatewayInstanceLease>)> {
        let mut operations = Vec::with_capacity(
            snapshot
                .workers
                .len()
                .saturating_add(snapshot.sessions.len()),
        );
        for worker in &snapshot.workers {
            let Some(lease) = find_lease(&leases, &worker.tenant_id).cloned() else {
                return Err((GatewayRegistryError::InvalidContract, leases));
            };
            let registration = worker.registration.clone();
            let operation: GatewayRegistryFuture<'_, RecoveredOwnership> = Box::pin(async move {
                self.provider
                    .register_worker(&lease, registration, self.lease_ttl_ms(), now_ms)
                    .await
                    .map(RecoveredOwnership::Worker)
            });
            operations.push(operation);
        }
        for session in &snapshot.sessions {
            let Some(lease) = find_lease(&leases, &session.tenant_id).cloned() else {
                return Err((GatewayRegistryError::InvalidContract, leases));
            };
            let record = match GatewaySessionRecord::new(
                lease.clone(),
                session.session_id.clone(),
                session.expires_at_ms,
            ) {
                Ok(record) => record,
                Err(error) => return Err((error, leases)),
            };
            let operation: GatewayRegistryFuture<'_, RecoveredOwnership> = Box::pin(async move {
                self.provider
                    .register_session(&lease, record, now_ms)
                    .await
                    .map(RecoveredOwnership::Session)
            });
            operations.push(operation);
        }
        let results = replay_operations(operations).await;
        collect_replayed(results, leases)
    }

    async fn renew_workers(
        &self,
        leases: &[GatewayInstanceLease],
        workers: &[GatewayWorkerRecord],
        now_ms: u64,
    ) -> GatewayRegistryResult<Vec<GatewayWorkerRecord>> {
        let ttl_ms = self.lease_ttl_ms();
        let operations = stream::iter(workers.to_vec()).map(|worker| {
            let lease = find_lease(leases, worker.owner.tenant_id()).cloned();
            async move {
                let lease = lease.ok_or(GatewayRegistryError::InvalidContract)?;
                self.provider
                    .renew_worker(&lease, &worker, ttl_ms, now_ms)
                    .await
            }
        });
        let results = operations
            .buffer_unordered(MAX_GATEWAY_REGISTRY_CONCURRENCY)
            .collect::<Vec<_>>();
        tokio::time::timeout(operation_round_timeout(), results)
            .await
            .map_err(|_| GatewayRegistryError::Unavailable)?
            .into_iter()
            .collect()
    }

    async fn release_all(&self, leases: &[GatewayInstanceLease]) -> GatewayRegistryResult<()> {
        let operations = stream::iter(leases.to_vec())
            .map(|lease| async move { self.provider.release_instance(&lease).await })
            .buffer_unordered(MAX_GATEWAY_REGISTRY_CONCURRENCY)
            .collect::<Vec<_>>();
        let results = tokio::time::timeout(operation_round_timeout(), operations)
            .await
            .map_err(|_| GatewayRegistryError::Unavailable)?;
        let mut first_error = None;
        for result in results {
            if let Err(error) = result {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    pub(super) fn fail(&self, error: GatewayRegistryError) {
        increment(&self.failures);
        if matches!(
            error,
            GatewayRegistryError::StaleOwner | GatewayRegistryError::Conflict
        ) {
            self.lifecycle.record_fencing_rejection();
        }
        let _ = self.lifecycle.isolate();
    }

    pub(super) fn lease_ttl_ms(&self) -> u64 {
        u64::try_from(self.config.lease_ttl.as_millis()).unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
#[path = "coordinator_tests.rs"]
pub(crate) mod tests;
