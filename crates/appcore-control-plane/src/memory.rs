// =============================================================================
//        #######
//     ###       ###     F: memory.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Defines bounded memory contracts and behavior for this crate.

use super::*;

mod accounting;

use accounting::{lease_slot_retained_bytes, registration_retained_bytes, state_retained_bytes};

/// Deterministic in-memory control plane for embedded hosts and tests.
#[derive(Debug, Clone)]
pub struct InMemoryControlPlane {
    state: std::sync::Arc<Mutex<MemoryStore>>,
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

/// Point-in-time pressure metrics for an in-memory control-plane provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlPlaneMemoryStats {
    /// Retained core registrations.
    pub registrations: usize,
    /// Retained service lease slots, including fencing history without an active lease.
    pub service_lease_slots: usize,
    /// Estimated bytes retained by registrations, lease slots, and their map entries.
    pub used_bytes: usize,
    /// Highest estimated retained-byte count observed by this provider.
    pub peak_bytes: usize,
    /// Operations rejected by the configured record or byte budget.
    pub rejections: u64,
    /// Aggregate registration and lease-slot limit.
    pub max_records: usize,
    /// Aggregate retained-byte budget.
    pub max_bytes: usize,
}

#[derive(Debug)]
struct MemoryStore {
    persisted: InMemoryState,
    used_bytes: usize,
    peak_bytes: usize,
    rejections: u64,
    max_records: usize,
    max_bytes: usize,
}

impl MemoryStore {
    fn new(
        persisted: InMemoryState,
        max_records: usize,
        max_bytes: usize,
    ) -> ControlPlaneResult<Self> {
        let max_records = max_records.max(1);
        let max_bytes = max_bytes.max(1);
        let record_count = persisted
            .registrations
            .len()
            .saturating_add(persisted.service_leases.len());
        let used_bytes = state_retained_bytes(&persisted);
        if record_count > max_records || used_bytes > max_bytes {
            return Err(resident_limit_error());
        }
        Ok(Self {
            persisted,
            used_bytes,
            peak_bytes: used_bytes,
            rejections: 0,
            max_records,
            max_bytes,
        })
    }

    fn record_count(&self) -> usize {
        self.persisted
            .registrations
            .len()
            .saturating_add(self.persisted.service_leases.len())
    }

    fn admit_replacement(&mut self, previous_bytes: usize, incoming_bytes: usize) -> bool {
        let candidate = self
            .used_bytes
            .saturating_sub(previous_bytes)
            .saturating_add(incoming_bytes);
        if incoming_bytes > self.max_bytes || candidate > self.max_bytes {
            self.rejections = self.rejections.saturating_add(1);
            return false;
        }
        self.used_bytes = candidate;
        self.peak_bytes = self.peak_bytes.max(candidate);
        true
    }

    fn admit_new_record(&mut self, incoming_bytes: usize) -> bool {
        if self.record_count() >= self.max_records {
            self.rejections = self.rejections.saturating_add(1);
            return false;
        }
        self.admit_replacement(0, incoming_bytes)
    }

    fn stats(&self) -> ControlPlaneMemoryStats {
        ControlPlaneMemoryStats {
            registrations: self.persisted.registrations.len(),
            service_lease_slots: self.persisted.service_leases.len(),
            used_bytes: self.used_bytes,
            peak_bytes: self.peak_bytes,
            rejections: self.rejections,
            max_records: self.max_records,
            max_bytes: self.max_bytes,
        }
    }

    fn recalculate_used_bytes(&mut self) {
        self.used_bytes = state_retained_bytes(&self.persisted);
    }
}

impl Default for InMemoryControlPlane {
    fn default() -> Self {
        Self::with_limits(
            DEFAULT_CONTROL_PLANE_MAX_RECORDS,
            DEFAULT_CONTROL_PLANE_MAX_BYTES,
        )
    }
}

impl InMemoryControlPlane {
    /// Creates an empty provider with aggregate record and retained-byte limits.
    pub fn with_limits(max_records: usize, max_bytes: usize) -> Self {
        let max_records = max_records.max(1);
        let max_bytes = max_bytes.max(1);
        let store = MemoryStore {
            persisted: InMemoryState::default(),
            used_bytes: 0,
            peak_bytes: 0,
            rejections: 0,
            max_records,
            max_bytes,
        };
        Self {
            state: std::sync::Arc::new(Mutex::new(store)),
        }
    }

    /// Returns the number of registered runtime instances.
    pub fn registrations_len(&self) -> ControlPlaneResult<usize> {
        Ok(lock_state(&self.state)?.persisted.registrations.len())
    }

    /// Returns current retained-state pressure and configured limits.
    pub fn stats(&self) -> ControlPlaneResult<ControlPlaneMemoryStats> {
        Ok(lock_state(&self.state)?.stats())
    }

    pub(crate) fn from_state_with_limits(
        state: InMemoryState,
        max_records: usize,
        max_bytes: usize,
    ) -> ControlPlaneResult<Self> {
        Ok(Self {
            state: std::sync::Arc::new(Mutex::new(MemoryStore::new(
                state,
                max_records,
                max_bytes,
            )?)),
        })
    }

    pub(crate) fn into_state(self) -> ControlPlaneResult<InMemoryState> {
        Arc::try_unwrap(self.state)
            .map_err(|_| {
                ControlPlaneError::Transport(
                    "file control plane state still has shared owners".to_string(),
                )
            })?
            .into_inner()
            .map(|store| store.persisted)
            .map_err(|_| ControlPlaneError::Transport("control plane state poisoned".to_string()))
    }

    pub(crate) fn prune_registrations(&self, cutoff_ms: u64) -> ControlPlaneResult<usize> {
        let mut store = lock_state(&self.state)?;
        let before = store.persisted.registrations.len();
        store
            .persisted
            .registrations
            .retain(|_, registration| registration.registered_at_ms >= cutoff_ms);
        let removed = before.saturating_sub(store.persisted.registrations.len());
        if removed > 0 {
            store.recalculate_used_bytes();
        }
        Ok(removed)
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
            let incoming_bytes = registration_retained_bytes(&key, &registration);
            let mut store = lock_state(&self.state)?;
            let previous_bytes = store
                .persisted
                .registrations
                .get(&key)
                .map_or(0, |previous| registration_retained_bytes(&key, previous));
            let admitted = if previous_bytes == 0 {
                store.admit_new_record(incoming_bytes)
            } else {
                store.admit_replacement(previous_bytes, incoming_bytes)
            };
            if !admitted {
                return Err(resident_limit_error());
            }
            store.persisted.registrations.insert(key, registration);
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
            if let Some(registration) = state.persisted.registrations.get_mut(&registration_key) {
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
                .persisted
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
            let mut store = lock_state(&self.state)?;
            let key = service_lease_key(&identity.tenant_id, &identity.cluster_id, service_id);
            let current = store.persisted.service_leases.get(&key);
            let lease = match current.and_then(|slot| slot.lease.as_ref()) {
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
                    let epoch = current
                        .map_or(0, |slot| slot.last_epoch)
                        .checked_add(1)
                        .ok_or_else(|| {
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
            let replacement = ServiceLeaseSlot {
                last_epoch: current.map_or(lease.epoch, |slot| slot.last_epoch.max(lease.epoch)),
                lease: Some(lease.clone()),
            };
            let previous_bytes = current.map_or(0, |slot| lease_slot_retained_bytes(&key, slot));
            let incoming_bytes = lease_slot_retained_bytes(&key, &replacement);
            let admitted = if current.is_none() {
                store.admit_new_record(incoming_bytes)
            } else {
                store.admit_replacement(previous_bytes, incoming_bytes)
            };
            if !admitted {
                return Err(resident_limit_error());
            }
            store.persisted.service_leases.insert(key, replacement);
            Ok(lease)
        })
    }

    fn release_service_lease<'a>(
        &'a self,
        lease: ServiceLeaderLease,
    ) -> ControlPlaneFuture<'a, ()> {
        Box::pin(async move {
            let mut store = lock_state(&self.state)?;
            let key = service_lease_key(&lease.tenant_id, &lease.cluster_id, &lease.service_id);
            let Some(slot) = store.persisted.service_leases.get_mut(&key) else {
                return Ok(());
            };
            let previous_bytes = lease_slot_retained_bytes(&key, slot);
            let matches_current = slot.lease.as_ref().is_some_and(|current| {
                current.holder_core_id == lease.holder_core_id && current.epoch == lease.epoch
            });
            if !matches_current {
                return Err(ControlPlaneError::Conflict(
                    "service lease release does not match current epoch and holder".to_string(),
                ));
            }
            slot.lease = None;
            let retained_bytes = lease_slot_retained_bytes(&key, slot);
            store.used_bytes = store
                .used_bytes
                .saturating_sub(previous_bytes)
                .saturating_add(retained_bytes);
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
    state: &Mutex<MemoryStore>,
) -> ControlPlaneResult<std::sync::MutexGuard<'_, MemoryStore>> {
    state
        .lock()
        .map_err(|_| ControlPlaneError::Transport("control plane state poisoned".to_string()))
}

fn resident_limit_error() -> ControlPlaneError {
    ControlPlaneError::Rejected("control-plane resident state exceeds configured limit".to_string())
}

fn is_routable(mode: RuntimeOperationalMode) -> bool {
    matches!(
        mode,
        RuntimeOperationalMode::ReadWrite
            | RuntimeOperationalMode::ReadOnly
            | RuntimeOperationalMode::Syncing
    )
}
