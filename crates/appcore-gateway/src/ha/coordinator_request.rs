// =============================================================================
//        #######
//     ###       ###     F: coordinator_request.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.6-rc
// =============================================================================

//! Shared claim, completion and cancellation for live routed requests.

use super::{
    GatewayHaCoordinator, GatewayRegistryError, GatewayRegistryResult, GatewayRequestFence,
    GatewayWorkerRecord,
};
use appcore_types::{ClusterId, CoreId, TenantId};

/// Borrowed exact local routing target and request lifetime for one claim.
pub struct GatewayLocalRequestClaim<'a> {
    /// Tenant isolation boundary.
    pub tenant_id: &'a TenantId,
    /// Cluster isolation boundary.
    pub cluster_id: &'a ClusterId,
    /// Selected local Core identity.
    pub core_id: &'a CoreId,
    /// Selected local connection generation.
    pub worker_generation: u64,
    /// Bounded request identity.
    pub request_id: &'a str,
    /// Absolute bounded request expiry.
    pub expires_at_ms: u64,
    /// Current wall-clock epoch milliseconds.
    pub now_ms: u64,
}

impl GatewayHaCoordinator {
    /// Records one fenced remote-owner response accepted by the origin.
    pub(crate) fn record_remote_forward(&self) {
        super::coordinator_support::increment(&self.remote_forwards);
    }

    /// Resolves the shared live owner for one exact tenant/cluster/Core target.
    pub async fn resolve_worker(
        &self,
        tenant_id: &TenantId,
        cluster_id: &ClusterId,
        core_id: &CoreId,
        now_ms: u64,
    ) -> GatewayRegistryResult<Option<GatewayWorkerRecord>> {
        let _operation = self.operation.lock().await;
        self.boundary_lease(tenant_id, cluster_id)?;
        let result = self
            .provider
            .resolve_worker(tenant_id, cluster_id, core_id, now_ms)
            .await;
        match result {
            Ok(Some(worker)) => {
                if worker.validate().is_err()
                    || worker.owner.tenant_id() != tenant_id
                    || worker.owner.cluster_id() != cluster_id
                    || &worker.core_id != core_id
                    || worker.is_expired(now_ms)
                {
                    self.clear_and_fail(GatewayRegistryError::InvalidContract);
                    return Err(GatewayRegistryError::InvalidContract);
                }
                Ok(Some(worker))
            }
            Ok(None) => Ok(None),
            Err(error) => {
                self.clear_and_fail(error);
                Err(error)
            }
        }
    }

    /// Claims one request to an exact locally owned worker generation.
    pub async fn claim_local_request(
        &self,
        claim: GatewayLocalRequestClaim<'_>,
    ) -> GatewayRegistryResult<GatewayRequestFence> {
        if claim.expires_at_ms <= claim.now_ms {
            return Err(GatewayRegistryError::InvalidContract);
        }
        let _operation = self.operation.lock().await;
        let (origin, target) = {
            let ownership = self.ownership.read();
            let origin = ownership
                .leases
                .iter()
                .find(|lease| {
                    lease.tenant_id() == claim.tenant_id && lease.cluster_id() == claim.cluster_id
                })
                .cloned();
            let target = ownership
                .workers
                .iter()
                .find(|worker| {
                    worker.owner.tenant_id() == claim.tenant_id
                        && worker.owner.cluster_id() == claim.cluster_id
                        && &worker.core_id == claim.core_id
                        && worker.generation == claim.worker_generation
                })
                .cloned();
            (origin, target)
        };
        let origin = origin.ok_or(GatewayRegistryError::InvalidContract)?;
        let Some(target) = target else {
            *self.ownership.write() = super::coordinator::CoordinatorOwnership::default();
            self.fail(GatewayRegistryError::StaleOwner);
            return Err(GatewayRegistryError::StaleOwner);
        };
        self.claim_with_target(
            &origin,
            &target,
            claim.request_id,
            claim.expires_at_ms,
            claim.now_ms,
        )
        .await
    }

    /// Claims one request to an exact remotely owned shared worker record.
    pub async fn claim_remote_request(
        &self,
        target: &GatewayWorkerRecord,
        request_id: &str,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryResult<GatewayRequestFence> {
        target.validate()?;
        if expires_at_ms <= now_ms || target.is_expired(now_ms) {
            return Err(GatewayRegistryError::Expired);
        }
        let _operation = self.operation.lock().await;
        let origin = self.boundary_lease(target.owner.tenant_id(), target.owner.cluster_id())?;
        if target.owner.instance_id() == origin.instance_id() {
            return Err(GatewayRegistryError::InvalidContract);
        }
        self.claim_with_target(&origin, target, request_id, expires_at_ms, now_ms)
            .await
    }

    /// Checks an inbound federation claim targets this exact local owner and worker.
    pub async fn check_federated_request(
        &self,
        request: &GatewayRequestFence,
        now_ms: u64,
    ) -> GatewayRegistryResult<()> {
        if request.request_id.is_empty()
            || request.request_id.len() > 128
            || !request.request_id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
            })
            || request.origin_epoch == 0
            || request.target_epoch == 0
            || request.worker_generation == 0
        {
            return Err(GatewayRegistryError::InvalidContract);
        }
        if request.is_expired(now_ms) {
            return Err(GatewayRegistryError::Expired);
        }
        let _operation = self.operation.lock().await;
        let local_lease = self.boundary_lease(&request.tenant_id, &request.target_cluster_id)?;
        if local_lease.instance_id() != &request.target_instance_id
            || local_lease.epoch() != request.target_epoch
            || local_lease.instance_id() == &request.origin_instance_id
        {
            return Err(GatewayRegistryError::StaleOwner);
        }
        let owns_worker = self.ownership.read().workers.iter().any(|worker| {
            worker.owner == local_lease
                && worker.core_id == request.target_core_id
                && worker.generation == request.worker_generation
                && !worker.is_expired(now_ms)
        });
        if !owns_worker {
            return Err(GatewayRegistryError::StaleOwner);
        }
        match self.provider.check_request(request, now_ms).await {
            Ok(()) => Ok(()),
            Err(
                error @ (GatewayRegistryError::Unavailable
                | GatewayRegistryError::UnsupportedSchema),
            ) => {
                self.clear_and_fail(error);
                Err(error)
            }
            Err(error) => Err(error),
        }
    }

    /// Completes one request only while origin, target and worker fences match.
    pub async fn complete_request(
        &self,
        request: &GatewayRequestFence,
        now_ms: u64,
    ) -> GatewayRegistryResult<()> {
        let _operation = self.operation.lock().await;
        let result = self.provider.complete_request(request, now_ms).await;
        self.handle_mutation(result)?;
        super::coordinator_support::increment(&self.request_completions);
        Ok(())
    }

    /// Cancels one request under its exact origin epoch.
    pub async fn cancel_request(&self, request: &GatewayRequestFence) -> GatewayRegistryResult<()> {
        let _operation = self.operation.lock().await;
        let result = self.provider.cancel_request(request).await;
        self.handle_mutation(result)?;
        super::coordinator_support::increment(&self.request_cancellations);
        Ok(())
    }

    async fn claim_with_target(
        &self,
        origin: &super::GatewayInstanceLease,
        target: &GatewayWorkerRecord,
        request_id: &str,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryResult<GatewayRequestFence> {
        let result = self
            .provider
            .claim_request(origin, target, request_id, expires_at_ms, now_ms)
            .await;
        let request = match result {
            Ok(request) => request,
            Err(
                error @ (GatewayRegistryError::Conflict
                | GatewayRegistryError::CapacityExceeded
                | GatewayRegistryError::InvalidContract),
            ) => return Err(error),
            Err(error) => {
                self.clear_and_fail(error);
                return Err(error);
            }
        };
        super::coordinator_support::increment(&self.request_claims);
        Ok(request)
    }

    fn clear_and_fail(&self, error: GatewayRegistryError) {
        *self.ownership.write() = super::coordinator::CoordinatorOwnership::default();
        self.fail(error);
    }
}
