// =============================================================================
//        #######
//     ###       ###     F: coordinator_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.4-rc
// =============================================================================
// appcore-norm: test

use super::*;
use crate::ha::{
    GatewayHaSessionSnapshot, GatewayHaWorkerSnapshot, GatewayLocalRequestClaim,
    GatewayRegistryFuture, GatewayRequestFence, GatewaySessionRecord, GatewayWorkerRecord,
    GatewayWorkerRegistration,
};
use appcore_contracts::InstallationId;
use appcore_types::{CapabilityName, CoreId};
use parking_lot::Mutex as ParkingMutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Default)]
struct ProviderState {
    epochs: HashMap<String, u64>,
    released: Vec<(String, u64)>,
    workers: Vec<GatewayWorkerRecord>,
    requests: HashMap<String, GatewayRequestFence>,
}

#[derive(Default)]
pub(crate) struct TestProvider {
    state: ParkingMutex<ProviderState>,
    unavailable_tenant: ParkingMutex<Option<String>>,
    stale_renewal: AtomicBool,
    claims: AtomicU64,
    completions: AtomicU64,
    cancellations: AtomicU64,
}

impl TestProvider {
    fn fail_acquisition_for(&self, tenant: Option<&str>) {
        *self.unavailable_tenant.lock() = tenant.map(str::to_string);
    }

    fn released(&self) -> Vec<(String, u64)> {
        self.state.lock().released.clone()
    }

    pub(crate) fn request_counts(&self) -> (u64, u64, u64) {
        (
            self.claims.load(Ordering::Relaxed),
            self.completions.load(Ordering::Relaxed),
            self.cancellations.load(Ordering::Relaxed),
        )
    }
}

impl GatewayRegistryProvider for TestProvider {
    fn acquire_instance<'a>(
        &'a self,
        tenant_id: &'a TenantId,
        cluster_id: &'a ClusterId,
        instance_id: &'a InstanceId,
        federation_url: &'a GatewayFederationUrl,
        ttl_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, GatewayInstanceLease> {
        Box::pin(async move {
            if self
                .unavailable_tenant
                .lock()
                .as_deref()
                .is_some_and(|tenant| tenant == tenant_id.as_str())
            {
                return Err(GatewayRegistryError::Unavailable);
            }
            let epoch = {
                let mut state = self.state.lock();
                let epoch = state
                    .epochs
                    .entry(tenant_id.as_str().to_string())
                    .or_default();
                *epoch = epoch.saturating_add(1);
                *epoch
            };
            GatewayInstanceLease::new(
                tenant_id.clone(),
                cluster_id.clone(),
                instance_id.clone(),
                federation_url.clone(),
                epoch,
                now_ms.saturating_add(ttl_ms),
            )
        })
    }

    fn renew_instance<'a>(
        &'a self,
        lease: &'a GatewayInstanceLease,
        ttl_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, GatewayInstanceLease> {
        Box::pin(async move {
            if self.stale_renewal.load(Ordering::Acquire) {
                return Err(GatewayRegistryError::StaleOwner);
            }
            GatewayInstanceLease::new(
                lease.tenant_id().clone(),
                lease.cluster_id().clone(),
                lease.instance_id().clone(),
                lease.federation_url().clone(),
                lease.epoch(),
                now_ms.saturating_add(ttl_ms),
            )
        })
    }

    fn release_instance<'a>(
        &'a self,
        lease: &'a GatewayInstanceLease,
    ) -> GatewayRegistryFuture<'a, ()> {
        Box::pin(async move {
            self.state
                .lock()
                .released
                .push((lease.tenant_id().as_str().to_string(), lease.epoch()));
            Ok(())
        })
    }

    fn check_instance<'a>(
        &'a self,
        _lease: &'a GatewayInstanceLease,
        _now_ms: u64,
    ) -> GatewayRegistryFuture<'a, ()> {
        unsupported()
    }

    fn register_worker<'a>(
        &'a self,
        lease: &'a GatewayInstanceLease,
        registration: GatewayWorkerRegistration,
        ttl_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, GatewayWorkerRecord> {
        Box::pin(async move {
            let worker = GatewayWorkerRecord::new(
                lease.clone(),
                registration,
                now_ms.saturating_add(ttl_ms),
            )?;
            let mut state = self.state.lock();
            state.workers.retain(|current| {
                current.owner.tenant_id() != worker.owner.tenant_id()
                    || current.owner.cluster_id() != worker.owner.cluster_id()
                    || current.core_id != worker.core_id
            });
            state.workers.push(worker.clone());
            Ok(worker)
        })
    }

    fn renew_worker<'a>(
        &'a self,
        lease: &'a GatewayInstanceLease,
        worker: &'a GatewayWorkerRecord,
        ttl_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, GatewayWorkerRecord> {
        Box::pin(async move {
            let renewed = GatewayWorkerRecord::new(
                lease.clone(),
                GatewayWorkerRegistration::new(
                    worker.installation_id.clone(),
                    worker.core_id.clone(),
                    worker.generation,
                    worker.capabilities.clone(),
                )?,
                now_ms.saturating_add(ttl_ms),
            )?;
            let mut state = self.state.lock();
            state.workers.retain(|current| current != worker);
            state.workers.push(renewed.clone());
            Ok(renewed)
        })
    }

    fn remove_worker<'a>(
        &'a self,
        _lease: &'a GatewayInstanceLease,
        worker: &'a GatewayWorkerRecord,
    ) -> GatewayRegistryFuture<'a, ()> {
        Box::pin(async move {
            self.state
                .lock()
                .workers
                .retain(|current| current != worker);
            Ok(())
        })
    }

    fn resolve_worker<'a>(
        &'a self,
        tenant_id: &'a TenantId,
        cluster_id: &'a ClusterId,
        core_id: &'a CoreId,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, Option<GatewayWorkerRecord>> {
        Box::pin(async move {
            Ok(self
                .state
                .lock()
                .workers
                .iter()
                .find(|worker| {
                    worker.owner.tenant_id() == tenant_id
                        && worker.owner.cluster_id() == cluster_id
                        && &worker.core_id == core_id
                        && !worker.is_expired(now_ms)
                })
                .cloned())
        })
    }

    fn resolve_capability<'a>(
        &'a self,
        tenant_id: &'a TenantId,
        capability: &'a CapabilityName,
        limit: usize,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, Vec<GatewayWorkerRecord>> {
        Box::pin(async move {
            Ok(self
                .state
                .lock()
                .workers
                .iter()
                .filter(|worker| {
                    worker.owner.tenant_id() == tenant_id
                        && worker.capabilities.contains(capability)
                        && !worker.is_expired(now_ms)
                })
                .take(limit)
                .cloned()
                .collect())
        })
    }

    fn register_session<'a>(
        &'a self,
        _lease: &'a GatewayInstanceLease,
        session: GatewaySessionRecord,
        _now_ms: u64,
    ) -> GatewayRegistryFuture<'a, GatewaySessionRecord> {
        Box::pin(async move { Ok(session) })
    }

    fn remove_session<'a>(
        &'a self,
        _lease: &'a GatewayInstanceLease,
        _session: &'a GatewaySessionRecord,
    ) -> GatewayRegistryFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }

    fn claim_request<'a>(
        &'a self,
        origin: &'a GatewayInstanceLease,
        target: &'a GatewayWorkerRecord,
        request_id: &'a str,
        expires_at_ms: u64,
        _now_ms: u64,
    ) -> GatewayRegistryFuture<'a, GatewayRequestFence> {
        Box::pin(async move {
            self.claims.fetch_add(1, Ordering::Relaxed);
            let request = GatewayRequestFence::new(origin, target, request_id, expires_at_ms)?;
            let mut state = self.state.lock();
            if state.requests.contains_key(request_id) {
                return Err(GatewayRegistryError::Conflict);
            }
            state
                .requests
                .insert(request_id.to_string(), request.clone());
            Ok(request)
        })
    }

    fn complete_request<'a>(
        &'a self,
        request: &'a GatewayRequestFence,
        _now_ms: u64,
    ) -> GatewayRegistryFuture<'a, ()> {
        Box::pin(async move {
            self.completions.fetch_add(1, Ordering::Relaxed);
            match self.state.lock().requests.remove(&request.request_id) {
                Some(current) if current == *request => Ok(()),
                _ => Err(GatewayRegistryError::StaleOwner),
            }
        })
    }

    fn check_request<'a>(
        &'a self,
        request: &'a GatewayRequestFence,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, ()> {
        Box::pin(async move {
            if request.is_expired(now_ms) {
                return Err(GatewayRegistryError::Expired);
            }
            self.state
                .lock()
                .requests
                .get(&request.request_id)
                .filter(|current| *current == request)
                .map(|_| ())
                .ok_or(GatewayRegistryError::StaleOwner)
        })
    }

    fn cancel_request<'a>(
        &'a self,
        request: &'a GatewayRequestFence,
    ) -> GatewayRegistryFuture<'a, ()> {
        Box::pin(async move {
            self.cancellations.fetch_add(1, Ordering::Relaxed);
            self.state.lock().requests.remove(&request.request_id);
            Ok(())
        })
    }
}

fn unsupported<'a, T: 'a>() -> GatewayRegistryFuture<'a, T> {
    Box::pin(async { Err(GatewayRegistryError::InvalidContract) })
}

#[test]
fn config_rejects_unsafe_or_ambiguous_ownership() {
    let mut unsafe_config = config(&["tenant-a"]);
    unsafe_config.renewal_interval = Duration::from_millis(20_001);
    assert_eq!(
        unsafe_config.validate(),
        Err(GatewayRegistryError::InvalidContract)
    );

    let mut duplicate = config(&["tenant-a", "tenant-a"]);
    duplicate.renewal_interval = Duration::from_secs(10);
    assert_eq!(
        duplicate.validate(),
        Err(GatewayRegistryError::InvalidContract)
    );
}

#[tokio::test]
async fn recovery_acquires_every_tenant_before_admission() {
    let provider = Arc::new(TestProvider::default());
    let coordinator = coordinator(provider, &["tenant-a", "tenant-b"]);

    assert_eq!(
        coordinator.lease_for(&tenant("tenant-a")),
        Err(GatewayRegistryError::Unavailable)
    );
    coordinator.recover(&empty(), 1_000).await.unwrap();

    assert_eq!(
        coordinator.lease_for(&tenant("tenant-a")).unwrap().epoch(),
        1
    );
    assert_eq!(
        coordinator.lease_for(&tenant("tenant-b")).unwrap().epoch(),
        1
    );
    assert_eq!(
        coordinator.snapshot().lifecycle.mode,
        GatewayHaMode::Healthy
    );
    assert_eq!(coordinator.snapshot().active_leases, 2);
    assert_eq!(coordinator.snapshot().acquisitions, 1);
}

#[tokio::test]
async fn recovery_replays_complete_worker_and_session_snapshot_before_healthy() {
    let provider = Arc::new(TestProvider::default());
    let coordinator = coordinator(provider, &["tenant-a"]);
    let source = GatewayHaOwnershipSnapshot {
        workers: vec![GatewayHaWorkerSnapshot {
            tenant_id: tenant("tenant-a"),
            cluster_id: ClusterId::new("cluster-a").unwrap(),
            registration: GatewayWorkerRegistration::new(
                InstallationId::new("install-a").unwrap(),
                CoreId::new("core-a").unwrap(),
                7,
                vec![CapabilityName::new("runtime.query").unwrap()],
            )
            .unwrap(),
        }],
        sessions: vec![GatewayHaSessionSnapshot {
            tenant_id: tenant("tenant-a"),
            session_id: "session-a".to_string(),
            expires_at_ms: 50_000,
        }],
    };

    coordinator.recover(&source, 1_000).await.unwrap();

    let snapshot = coordinator.snapshot();
    assert_eq!(snapshot.lifecycle.mode, GatewayHaMode::Healthy);
    assert_eq!(snapshot.active_workers, 1);
    assert_eq!(snapshot.active_sessions, 1);
    coordinator.renew(10_000).await.unwrap();
    assert_eq!(coordinator.snapshot().active_workers, 1);
}

#[tokio::test]
async fn partial_recovery_rolls_back_and_isolates() {
    let provider = Arc::new(TestProvider::default());
    provider.fail_acquisition_for(Some("tenant-b"));
    let coordinator = coordinator(Arc::clone(&provider), &["tenant-a", "tenant-b"]);

    assert_eq!(
        coordinator.recover(&empty(), 1_000).await,
        Err(GatewayRegistryError::Unavailable)
    );

    let snapshot = coordinator.snapshot();
    assert_eq!(snapshot.lifecycle.mode, GatewayHaMode::Isolated);
    assert_eq!(snapshot.active_leases, 0);
    assert_eq!(snapshot.failures, 1);
    assert_eq!(provider.released(), vec![("tenant-a".to_string(), 1)]);
}

#[tokio::test]
async fn stale_renewal_drops_all_local_leases_and_requires_new_epochs() {
    let provider = Arc::new(TestProvider::default());
    let coordinator = coordinator(Arc::clone(&provider), &["tenant-a", "tenant-b"]);
    coordinator.recover(&empty(), 1_000).await.unwrap();
    provider.stale_renewal.store(true, Ordering::Release);

    assert_eq!(
        coordinator.renew(10_000).await,
        Err(GatewayRegistryError::StaleOwner)
    );
    let snapshot = coordinator.snapshot();
    assert_eq!(snapshot.lifecycle.mode, GatewayHaMode::Isolated);
    assert_eq!(snapshot.lifecycle.fencing_rejections, 1);
    assert_eq!(snapshot.active_leases, 0);

    provider.stale_renewal.store(false, Ordering::Release);
    coordinator.recover(&empty(), 11_000).await.unwrap();
    assert_eq!(
        coordinator.lease_for(&tenant("tenant-a")).unwrap().epoch(),
        2
    );
}

#[tokio::test]
async fn stop_blocks_admission_before_releasing_current_leases() {
    let provider = Arc::new(TestProvider::default());
    let coordinator = coordinator(Arc::clone(&provider), &["tenant-a", "tenant-b"]);
    coordinator.recover(&empty(), 1_000).await.unwrap();
    coordinator.stop().await.unwrap();

    assert_eq!(
        coordinator.snapshot().lifecycle.mode,
        GatewayHaMode::Stopped
    );
    assert_eq!(coordinator.snapshot().active_leases, 0);
    assert_eq!(provider.released().len(), 2);
}

#[tokio::test]
async fn live_worker_and_session_mutations_track_exact_local_ownership() {
    let provider = Arc::new(TestProvider::default());
    let coordinator = coordinator(provider, &["tenant-a"]);
    coordinator.recover(&empty(), 1_000).await.unwrap();
    let tenant_id = tenant("tenant-a");
    let cluster_id = ClusterId::new("cluster-a").unwrap();
    let installation_id = InstallationId::new("install-a").unwrap();
    let core_id = CoreId::new("core-a").unwrap();

    coordinator
        .register_worker(
            &tenant_id,
            &cluster_id,
            GatewayWorkerRegistration::new(installation_id.clone(), core_id.clone(), 9, Vec::new())
                .unwrap(),
            2_000,
        )
        .await
        .unwrap();
    coordinator
        .register_session(&tenant_id, &cluster_id, "session-a", 50_000, 2_000)
        .await
        .unwrap();
    assert_eq!(coordinator.snapshot().active_workers, 1);
    assert_eq!(coordinator.snapshot().active_sessions, 1);

    coordinator
        .remove_worker(&tenant_id, &installation_id, &core_id, 9)
        .await
        .unwrap();
    coordinator
        .remove_session(&tenant_id, "session-a")
        .await
        .unwrap();
    assert_eq!(coordinator.snapshot().active_workers, 0);
    assert_eq!(coordinator.snapshot().active_sessions, 0);
}

#[tokio::test]
async fn live_request_claim_completion_and_cancellation_use_exact_worker_generation() {
    let provider = Arc::new(TestProvider::default());
    let coordinator = coordinator(provider, &["tenant-a"]);
    coordinator.recover(&empty(), 1_000).await.unwrap();
    let tenant_id = tenant("tenant-a");
    let cluster_id = ClusterId::new("cluster-a").unwrap();
    let core_id = CoreId::new("core-a").unwrap();
    coordinator
        .register_worker(
            &tenant_id,
            &cluster_id,
            GatewayWorkerRegistration::new(
                InstallationId::new("install-a").unwrap(),
                core_id.clone(),
                11,
                Vec::new(),
            )
            .unwrap(),
            2_000,
        )
        .await
        .unwrap();

    let completed = coordinator
        .claim_local_request(GatewayLocalRequestClaim {
            tenant_id: &tenant_id,
            cluster_id: &cluster_id,
            core_id: &core_id,
            worker_generation: 11,
            request_id: "request-complete",
            expires_at_ms: 20_000,
            now_ms: 2_000,
        })
        .await
        .unwrap();
    coordinator
        .complete_request(&completed, 3_000)
        .await
        .unwrap();
    let cancelled = coordinator
        .claim_local_request(GatewayLocalRequestClaim {
            tenant_id: &tenant_id,
            cluster_id: &cluster_id,
            core_id: &core_id,
            worker_generation: 11,
            request_id: "request-cancel",
            expires_at_ms: 20_000,
            now_ms: 3_000,
        })
        .await
        .unwrap();
    coordinator.cancel_request(&cancelled).await.unwrap();
    let request_snapshot = coordinator.snapshot();
    assert_eq!(request_snapshot.request_claims, 2);
    assert_eq!(request_snapshot.request_completions, 1);
    assert_eq!(request_snapshot.request_cancellations, 1);
    assert_eq!(
        coordinator
            .claim_local_request(GatewayLocalRequestClaim {
                tenant_id: &tenant_id,
                cluster_id: &cluster_id,
                core_id: &core_id,
                worker_generation: 10,
                request_id: "request-stale",
                expires_at_ms: 20_000,
                now_ms: 4_000,
            })
            .await,
        Err(GatewayRegistryError::StaleOwner)
    );
    assert_eq!(
        coordinator.snapshot().lifecycle.mode,
        GatewayHaMode::Isolated
    );
}

#[tokio::test]
async fn two_coordinators_resolve_claim_and_validate_one_remote_worker() {
    let provider = Arc::new(TestProvider::default());
    let origin = coordinator_named(Arc::clone(&provider), "gateway-origin");
    let target = coordinator_named(Arc::clone(&provider), "gateway-target");
    let tenant_id = tenant("tenant-a");
    let cluster_id = ClusterId::new("cluster-a").unwrap();
    let core_id = CoreId::new("core-remote").unwrap();
    origin.recover(&empty(), 1_000).await.unwrap();
    target.recover(&empty(), 1_000).await.unwrap();
    target
        .register_worker(
            &tenant_id,
            &cluster_id,
            GatewayWorkerRegistration::new(
                InstallationId::new("install-remote").unwrap(),
                core_id.clone(),
                17,
                vec![CapabilityName::new("runtime.query").unwrap()],
            )
            .unwrap(),
            2_000,
        )
        .await
        .unwrap();

    let worker = origin
        .resolve_worker(&tenant_id, &cluster_id, &core_id, 2_000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(worker.owner.instance_id().as_str(), "gateway-target");
    let request = origin
        .claim_remote_request(&worker, "request-remote", 20_000, 2_000)
        .await
        .unwrap();
    target
        .check_federated_request(&request, 2_001)
        .await
        .unwrap();

    let mut stale = request.clone();
    stale.worker_generation += 1;
    assert_eq!(
        target.check_federated_request(&stale, 2_001).await,
        Err(GatewayRegistryError::StaleOwner)
    );
    origin.complete_request(&request, 2_002).await.unwrap();
    assert_eq!(provider.request_counts(), (1, 1, 0));
}

pub(crate) fn coordinator(provider: Arc<TestProvider>, tenants: &[&str]) -> GatewayHaCoordinator {
    GatewayHaCoordinator::new(
        provider,
        Arc::new(GatewayHaLifecycle::new()),
        config(tenants),
    )
    .unwrap()
}

fn coordinator_named(provider: Arc<TestProvider>, instance_id: &str) -> GatewayHaCoordinator {
    let mut config = config(&["tenant-a"]);
    config.instance_id = InstanceId::new(instance_id).unwrap();
    config.federation_url =
        GatewayFederationUrl::new(format!("https://{instance_id}.example.test")).unwrap();
    GatewayHaCoordinator::new(provider, Arc::new(GatewayHaLifecycle::new()), config).unwrap()
}

fn config(tenants: &[&str]) -> GatewayHaCoordinatorConfig {
    GatewayHaCoordinatorConfig {
        instance_id: InstanceId::new("gateway-a").unwrap(),
        federation_url: GatewayFederationUrl::new("https://gateway-a.example.test").unwrap(),
        tenants: tenants
            .iter()
            .map(|value| GatewayHaTenantBinding {
                tenant_id: tenant(value),
                cluster_id: ClusterId::new("cluster-a").unwrap(),
            })
            .collect(),
        lease_ttl: Duration::from_secs(60),
        renewal_interval: Duration::from_secs(10),
    }
}

fn tenant(value: &str) -> TenantId {
    TenantId::new(value).unwrap()
}

fn empty() -> GatewayHaOwnershipSnapshot {
    GatewayHaOwnershipSnapshot::default()
}
