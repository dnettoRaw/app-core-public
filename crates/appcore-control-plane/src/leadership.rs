// =============================================================================
//        #######
//     ###       ###     F: leadership.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

/// In-memory service lease guard for embedded hosts and tests.
#[derive(Debug, Clone, Default)]
pub struct StaticServiceLeadershipGuard {
    leases: Arc<Mutex<BTreeMap<ServiceId, ServiceLeaderLease>>>,
}

impl StaticServiceLeadershipGuard {
    /// Creates a guard from zero or more independently scoped leases.
    pub fn new(leases: impl IntoIterator<Item = ServiceLeaderLease>) -> Self {
        Self {
            leases: Arc::new(Mutex::new(
                leases
                    .into_iter()
                    .map(|lease| (lease.service_id.clone(), lease))
                    .collect(),
            )),
        }
    }

    /// Adds, renews, or removes the lease for one service.
    pub fn set_service_lease(
        &self,
        service_id: ServiceId,
        lease: Option<ServiceLeaderLease>,
    ) -> ControlPlaneResult<()> {
        if lease
            .as_ref()
            .is_some_and(|lease| lease.service_id != service_id)
        {
            return Err(ControlPlaneError::Conflict(
                "service lease identity mismatch".to_string(),
            ));
        }
        let mut leases = self
            .leases
            .lock()
            .map_err(|_| ControlPlaneError::Transport("service lease lock poisoned".to_string()))?;
        if let (Some(existing), Some(replacement)) = (leases.get(&service_id), lease.as_ref()) {
            let same_scope = existing.tenant_id == replacement.tenant_id
                && existing.cluster_id == replacement.cluster_id;
            if same_scope && replacement.epoch < existing.epoch {
                return Err(ControlPlaneError::Conflict("stale lease epoch".to_string()));
            }
            if same_scope
                && replacement.epoch == existing.epoch
                && replacement.holder_core_id != existing.holder_core_id
            {
                return Err(ControlPlaneError::Conflict(
                    "lease epoch holder conflict".to_string(),
                ));
            }
        }
        match lease {
            Some(lease) => {
                leases.insert(service_id, lease);
            }
            None => {
                leases.remove(&service_id);
            }
        }
        Ok(())
    }
}

impl ServiceLeadershipGuard for StaticServiceLeadershipGuard {
    fn current_service_lease(&self, service_id: &ServiceId) -> Option<ServiceLeaderLease> {
        self.leases
            .lock()
            .ok()
            .and_then(|leases| leases.get(service_id).cloned())
    }

    fn check_service_write_permission(
        &self,
        service_id: &ServiceId,
        tenant_id: &TenantId,
        cluster_id: &ClusterId,
        core_id: &CoreId,
        min_epoch: Option<u64>,
        now_ms: u64,
    ) -> LeadershipDecision {
        let Some(lease) = self.current_service_lease(service_id) else {
            return LeadershipDecision::NoLease;
        };
        if lease.expires_at_ms <= now_ms {
            return LeadershipDecision::Expired;
        }
        if lease.service_id != *service_id
            || lease.tenant_id != *tenant_id
            || lease.cluster_id != *cluster_id
            || lease.holder_core_id != *core_id
        {
            return LeadershipDecision::WrongHolder;
        }
        if min_epoch.map(|epoch| lease.epoch < epoch).unwrap_or(false) {
            return LeadershipDecision::StaleEpoch;
        }
        LeadershipDecision::Allowed
    }
}
