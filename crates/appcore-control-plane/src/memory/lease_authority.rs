// =============================================================================
//        #######
//     ###       ###     F: lease_authority.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:42:56 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 1.0.4-rc.3
// =============================================================================

use super::*;

fn current_checked<'a>(
    store: &'a MemoryStore,
    key: &str,
) -> ControlPlaneResult<Option<&'a ServiceLeaderLease>> {
    let Some(slot) = store.persisted.service_leases.get(key) else {
        return Ok(None);
    };
    let invalid = || ControlPlaneError::InvalidResponse("invalid persisted authority scope".into());
    if slot.last_epoch == 0 {
        return Err(invalid());
    }
    if let Some(lease) = &slot.lease {
        if lease.tenant_id.validate().is_err()
            || lease.cluster_id.validate().is_err()
            || lease.holder_core_id.validate().is_err()
            || ServiceId::new(lease.service_id.as_str()).is_err()
            || lease.epoch != slot.last_epoch
            || lease.expires_at_ms <= lease.acquired_at_ms
            || service_lease_key(&lease.tenant_id, &lease.cluster_id, &lease.service_id) != key
        {
            return Err(invalid());
        }
    }
    Ok(slot.lease.as_ref())
}
impl InMemoryControlPlane {
    pub(crate) fn inspect_exact(
        &self,
        identity: &CoreIdentity,
        service: &ServiceId,
    ) -> ControlPlaneResult<Option<ServiceLeaderLease>> {
        let store = lock_state(&self.state)?;
        Ok(current_checked(
            &store,
            &service_lease_key(&identity.tenant_id, &identity.cluster_id, service),
        )?
        .cloned())
    }
    pub(crate) fn check_exact(
        &self,
        lease: &ServiceLeaderLease,
        now: u64,
    ) -> ControlPlaneResult<()> {
        let store = lock_state(&self.state)?;
        let current = current_checked(
            &store,
            &service_lease_key(&lease.tenant_id, &lease.cluster_id, &lease.service_id),
        )?;
        if current != Some(lease)
            || lease.expires_at_ms <= now
            || lease.acquired_at_ms > now
            || lease.expires_at_ms.saturating_sub(now) > 30_000
        {
            return Err(ControlPlaneError::Conflict(
                "service lease is stale or expired".into(),
            ));
        }
        Ok(())
    }
    pub(crate) fn renew_exact(
        &self,
        lease: &ServiceLeaderLease,
        ttl: u64,
        now: u64,
    ) -> ControlPlaneResult<ServiceLeaderLease> {
        self.check_exact(lease, now)?;
        let expires_at_ms = lease_expiration(now, ttl)?.max(lease.expires_at_ms);
        let mut store = lock_state(&self.state)?;
        let slot = store
            .persisted
            .service_leases
            .get_mut(&service_lease_key(
                &lease.tenant_id,
                &lease.cluster_id,
                &lease.service_id,
            ))
            .ok_or(ControlPlaneError::LeaseUnavailable)?;
        if slot.lease.as_ref() != Some(lease) {
            return Err(ControlPlaneError::LeaseUnavailable);
        }
        let renewed = ServiceLeaderLease {
            expires_at_ms,
            ..lease.clone()
        };
        slot.lease = Some(renewed.clone());
        Ok(renewed)
    }
}
