// =============================================================================
//        #######
//     ###       ###     F: coordinator_live.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Live worker and session ownership mutations under current tenant fences.

use super::{
    GatewayHaCoordinator, GatewayRegistryError, GatewayRegistryResult, GatewaySessionRecord,
    GatewayWorkerRecord, GatewayWorkerRegistration,
};
use appcore_contracts::InstallationId;
use appcore_types::{ClusterId, CoreId, TenantId};

impl GatewayHaCoordinator {
    /// Registers one authenticated local worker under the current tenant epoch.
    pub async fn register_worker(
        &self,
        tenant_id: &TenantId,
        cluster_id: &ClusterId,
        registration: GatewayWorkerRegistration,
        now_ms: u64,
    ) -> GatewayRegistryResult<GatewayWorkerRecord> {
        registration.validate()?;
        let _operation = self.operation.lock().await;
        let lease = self.boundary_lease(tenant_id, cluster_id)?;
        let ttl_ms = lease
            .expires_at_ms()
            .checked_sub(now_ms)
            .filter(|ttl| *ttl > 0)
            .ok_or(GatewayRegistryError::Expired)?;
        let result = self
            .provider
            .register_worker(&lease, registration, ttl_ms, now_ms)
            .await;
        let record = self.handle_mutation(result)?;
        let mut ownership = self.ownership.write();
        ownership.workers.retain(|current| {
            current.owner.tenant_id() != tenant_id
                || current.installation_id != record.installation_id
                || current.core_id != record.core_id
        });
        ownership.workers.push(record.clone());
        Ok(record)
    }

    /// Removes one exact local worker generation from shared ownership.
    pub async fn remove_worker(
        &self,
        tenant_id: &TenantId,
        installation_id: &InstallationId,
        core_id: &CoreId,
        generation: u64,
    ) -> GatewayRegistryResult<()> {
        let _operation = self.operation.lock().await;
        let record = self
            .ownership
            .read()
            .workers
            .iter()
            .find(|worker| {
                worker.owner.tenant_id() == tenant_id
                    && &worker.installation_id == installation_id
                    && &worker.core_id == core_id
                    && worker.generation == generation
            })
            .cloned();
        let Some(record) = record else {
            return Ok(());
        };
        let lease = self.lease_for(tenant_id)?;
        let result = self.provider.remove_worker(&lease, &record).await;
        self.handle_mutation(result)?;
        self.ownership
            .write()
            .workers
            .retain(|worker| worker != &record);
        Ok(())
    }

    /// Registers one authenticated local session under the current tenant epoch.
    pub async fn register_session(
        &self,
        tenant_id: &TenantId,
        cluster_id: &ClusterId,
        session_id: impl Into<String>,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryResult<GatewaySessionRecord> {
        if expires_at_ms <= now_ms {
            return Err(GatewayRegistryError::InvalidContract);
        }
        let _operation = self.operation.lock().await;
        let lease = self.boundary_lease(tenant_id, cluster_id)?;
        let record = GatewaySessionRecord::new(lease.clone(), session_id, expires_at_ms)?;
        let result = self.provider.register_session(&lease, record, now_ms).await;
        let record = self.handle_mutation(result)?;
        let mut ownership = self.ownership.write();
        ownership.sessions.retain(|current| {
            current.owner.tenant_id() != tenant_id || current.session_id != record.session_id
        });
        ownership.sessions.push(record.clone());
        Ok(record)
    }

    /// Removes one exact local session from shared ownership.
    pub async fn remove_session(
        &self,
        tenant_id: &TenantId,
        session_id: &str,
    ) -> GatewayRegistryResult<()> {
        let _operation = self.operation.lock().await;
        let record = self
            .ownership
            .read()
            .sessions
            .iter()
            .find(|session| {
                session.owner.tenant_id() == tenant_id && session.session_id == session_id
            })
            .cloned();
        let Some(record) = record else {
            return Ok(());
        };
        let lease = self.lease_for(tenant_id)?;
        let result = self.provider.remove_session(&lease, &record).await;
        self.handle_mutation(result)?;
        self.ownership
            .write()
            .sessions
            .retain(|session| session != &record);
        Ok(())
    }

    pub(super) fn boundary_lease(
        &self,
        tenant_id: &TenantId,
        cluster_id: &ClusterId,
    ) -> GatewayRegistryResult<super::GatewayInstanceLease> {
        let lease = self.lease_for(tenant_id)?;
        if lease.cluster_id() != cluster_id {
            return Err(GatewayRegistryError::InvalidContract);
        }
        Ok(lease)
    }

    pub(super) fn handle_mutation<T>(
        &self,
        result: GatewayRegistryResult<T>,
    ) -> GatewayRegistryResult<T> {
        result.inspect_err(|&error| {
            if !matches!(
                error,
                GatewayRegistryError::InvalidContract | GatewayRegistryError::CapacityExceeded
            ) {
                *self.ownership.write() = super::coordinator::CoordinatorOwnership::default();
                self.fail(error);
            }
        })
    }
}
