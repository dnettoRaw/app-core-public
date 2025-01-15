// =============================================================================
//        #######
//     ###       ###     F: memory.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

/// Deterministic in-memory control plane for embedded hosts and tests.
#[derive(Debug, Clone, Default)]
pub struct InMemoryControlPlane {
    state: std::sync::Arc<Mutex<InMemoryState>>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InMemoryState {
    registrations: BTreeMap<String, CoreRegistration>,
    service_leases: BTreeMap<String, ServiceLeaseSlot>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct ServiceLeaseSlot {
    lease: Option<ServiceLeaderLease>,
    last_epoch: u64,
}

impl InMemoryControlPlane {
    /// Returns the number of registered runtime instances.
    pub fn registrations_len(&self) -> ControlPlaneResult<usize> {
        Ok(lock_state(&self.state)?.registrations.len())
    }

    pub(crate) fn from_state(state: InMemoryState) -> Self {
        Self {
            state: std::sync::Arc::new(Mutex::new(state)),
        }
    }

    pub(crate) fn snapshot(&self) -> ControlPlaneResult<InMemoryState> {
        Ok(lock_state(&self.state)?.clone())
    }

    pub(crate) fn prune_registrations(&self, cutoff_ms: u64) -> ControlPlaneResult<usize> {
        let mut state = lock_state(&self.state)?;
        let before = state.registrations.len();
        state
            .registrations
            .retain(|_, registration| registration.registered_at_ms >= cutoff_ms);
        Ok(before.saturating_sub(state.registrations.len()))
    }
}

impl ControlPlaneProvider for InMemoryControlPlane {
    fn register<'a>(
        &'a self,
        registration: CoreRegistration,
    ) -> ControlPlaneFuture<'a, CorePresence> {
        Box::pin(async move {
            let presence = CorePresence {
                identity: registration.manifest.identity.clone(),
                operation_mode: registration.operation_mode,
                healthy: is_routable(registration.operation_mode),
                last_seen_ms: registration.registered_at_ms,
            };
            let key = instance_key(&presence.identity);
            lock_state(&self.state)?
                .registrations
                .insert(key, registration);
            Ok(presence)
        })
    }

    fn heartbeat<'a>(
        &'a self,
        request: HeartbeatRequest,
    ) -> ControlPlaneFuture<'a, HeartbeatResponse> {
        Box::pin(async move {
            let mut state = lock_state(&self.state)?;
            let registration_key = instance_key(&request.identity);
            if let Some(registration) = state.registrations.get_mut(&registration_key) {
                registration.registered_at_ms = request.sent_at_ms;
                registration.operation_mode = request.operation_mode;
            }
            Ok(HeartbeatResponse {
                accepted: true,
                server_time_ms: request.sent_at_ms,
                operation_mode: request.operation_mode,
            })
        })
    }

    fn discover_peers<'a>(
        &'a self,
        identity: &'a CoreIdentity,
    ) -> ControlPlaneFuture<'a, PeerDirectory> {
        Box::pin(async move {
            let state = lock_state(&self.state)?;
            let peers = state
                .registrations
                .values()
                .filter(|registration| {
                    registration.manifest.identity.tenant_id == identity.tenant_id
                        && registration.manifest.identity.cluster_id == identity.cluster_id
                        && registration.manifest.identity.instance_id != identity.instance_id
                })
                .map(|registration| PeerRecord {
                    identity: registration.manifest.identity.clone(),
                    endpoints: registration.manifest.endpoints.clone(),
                    capabilities: registration.manifest.capabilities.clone(),
                    healthy: is_routable(registration.operation_mode),
                    last_seen_ms: registration.registered_at_ms,
                    metadata: registration.manifest.metadata.clone(),
                })
                .collect::<Vec<_>>();
            Ok(PeerDirectory {
                tenant_id: identity.tenant_id.clone(),
                cluster_id: Some(identity.cluster_id.clone()),
                peers,
                refreshed_at_ms: 0,
            })
        })
    }

    fn acquire_or_renew_service_lease<'a>(
        &'a self,
        identity: &'a CoreIdentity,
        service_id: &'a ServiceId,
        ttl_ms: u64,
        now_ms: u64,
    ) -> ControlPlaneFuture<'a, ServiceLeaderLease> {
        Box::pin(async move {
            let expires_at_ms = lease_expiration(now_ms, ttl_ms)?;
            let mut state = lock_state(&self.state)?;
            let key = service_lease_key(&identity.tenant_id, &identity.cluster_id, service_id);
            let slot = state.service_leases.entry(key).or_default();
            let lease = match slot.lease.as_ref() {
                Some(current)
                    if current.expires_at_ms > now_ms
                        && current.holder_core_id == identity.core_id =>
                {
                    ServiceLeaderLease {
                        expires_at_ms,
                        ..current.clone()
                    }
                }
                Some(current) if current.expires_at_ms > now_ms => {
                    return Err(ControlPlaneError::LeaseUnavailable);
                }
                _ => {
                    let epoch = slot.last_epoch.checked_add(1).ok_or_else(|| {
                        ControlPlaneError::Conflict(
                            "service lease fencing epoch exhausted".to_string(),
                        )
                    })?;
                    ServiceLeaderLease {
                        service_id: service_id.clone(),
                        tenant_id: identity.tenant_id.clone(),
                        cluster_id: identity.cluster_id.clone(),
                        holder_core_id: identity.core_id.clone(),
                        epoch,
                        acquired_at_ms: now_ms,
                        expires_at_ms,
                    }
                }
            };
            slot.last_epoch = slot.last_epoch.max(lease.epoch);
            slot.lease = Some(lease.clone());
            Ok(lease)
        })
    }

    fn release_service_lease<'a>(
        &'a self,
        lease: ServiceLeaderLease,
    ) -> ControlPlaneFuture<'a, ()> {
        Box::pin(async move {
            let mut state = lock_state(&self.state)?;
            let key = service_lease_key(&lease.tenant_id, &lease.cluster_id, &lease.service_id);
            let Some(slot) = state.service_leases.get_mut(&key) else {
                return Ok(());
            };
            let matches_current = slot.lease.as_ref().is_some_and(|current| {
                current.holder_core_id == lease.holder_core_id && current.epoch == lease.epoch
            });
            if !matches_current {
                return Err(ControlPlaneError::Conflict(
                    "service lease release does not match current epoch and holder".to_string(),
                ));
            }
            slot.lease = None;
            Ok(())
        })
    }
}

fn lease_expiration(now_ms: u64, ttl_ms: u64) -> ControlPlaneResult<u64> {
    if ttl_ms == 0 {
        return Err(ControlPlaneError::Rejected(
            "lease ttl must be greater than zero".to_string(),
        ));
    }
    now_ms.checked_add(ttl_ms).ok_or_else(|| {
        ControlPlaneError::Rejected("lease expiration exceeds the clock range".to_string())
    })
}

fn instance_key(identity: &CoreIdentity) -> String {
    format!(
        "{}:{}:{}",
        identity.tenant_id.as_str(),
        identity.cluster_id.as_str(),
        identity.instance_id.as_str()
    )
}

fn service_lease_key(
    tenant_id: &TenantId,
    cluster_id: &ClusterId,
    service_id: &ServiceId,
) -> String {
    format!(
        "{}:{}:{}",
        tenant_id.as_str(),
        cluster_id.as_str(),
        service_id.as_str()
    )
}

fn lock_state(
    state: &Mutex<InMemoryState>,
) -> ControlPlaneResult<std::sync::MutexGuard<'_, InMemoryState>> {
    state
        .lock()
        .map_err(|_| ControlPlaneError::Transport("control plane state poisoned".to_string()))
}

fn is_routable(mode: RuntimeOperationalMode) -> bool {
    matches!(
        mode,
        RuntimeOperationalMode::ReadWrite
            | RuntimeOperationalMode::ReadOnly
            | RuntimeOperationalMode::Syncing
    )
}
