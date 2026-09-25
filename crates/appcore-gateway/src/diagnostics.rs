// =============================================================================
//        #######
//     ###       ###     F: diagnostics.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/25 00:00:00 by dnettoRaw
// =============================================================================

//! Bounded, payload-free Gateway peer and capability introspection.

use crate::config::MAX_GATEWAY_WORKERS_PER_TENANT;
use crate::{GatewayResult, TenantState};
use appcore_types::{CapabilityName, ClusterId, CoreId, TenantId};
use serde::{Deserialize, Serialize};

/// Bounded query for one tenant's Gateway peer diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayDiagnosticsQuery {
    /// Tenant partition to inspect.
    pub tenant_id: TenantId,
    /// Optional capability filter.
    pub capability: Option<CapabilityName>,
    /// Optional cluster filter.
    pub cluster_id: Option<ClusterId>,
}

/// Safe public snapshot of one connected Gateway worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayPeerSnapshot {
    /// Worker installation identity.
    pub installation_id: String,
    /// Worker Core identity.
    pub core_id: CoreId,
    /// Optional authenticated cluster identity.
    pub cluster_id: Option<ClusterId>,
    /// Last heartbeat timestamp in milliseconds.
    pub last_heartbeat_ms: u64,
    /// Whether the worker is currently healthy and connected.
    pub healthy: bool,
    /// Requests currently admitted to this worker.
    pub inflight: u64,
    /// Capabilities advertised by this worker.
    pub capabilities: Vec<CapabilityName>,
}

/// Safe public snapshot of one advertised capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayCapabilitySnapshot {
    /// Capability name.
    pub name: CapabilityName,
    /// Number of connected workers advertising the capability.
    pub worker_count: usize,
    /// Number of healthy workers advertising the capability.
    pub healthy_worker_count: usize,
}

/// Bounded, payload-free Gateway diagnostics response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayDiagnosticsSnapshot {
    /// Inspected tenant identity.
    pub tenant_id: TenantId,
    /// Connected workers matching the query.
    pub peers: Vec<GatewayPeerSnapshot>,
    /// Capabilities visible in the matching peer set.
    pub capabilities: Vec<GatewayCapabilitySnapshot>,
}

/// Builds a redacted snapshot from one tenant partition.
pub(crate) fn snapshot_tenant(
    tenant: &TenantState,
    query: &GatewayDiagnosticsQuery,
    now_ms: u64,
    heartbeat_timeout_ms: u64,
) -> GatewayResult<GatewayDiagnosticsSnapshot> {
    let mut peers = tenant
        .workers
        .values()
        .filter(|worker| {
            query
                .cluster_id
                .as_ref()
                .is_none_or(|cluster| worker.cluster_id() == Some(cluster))
        })
        .filter(|worker| {
            query.capability.as_ref().is_none_or(|capability| {
                tenant
                    .registry
                    .capabilities_for(&worker.key)
                    .iter()
                    .any(|advertised| advertised == capability)
            })
        })
        .map(|worker| GatewayPeerSnapshot {
            installation_id: worker.key.installation_id.as_str().to_string(),
            core_id: worker.key.core_id.clone(),
            cluster_id: worker.cluster_id().cloned(),
            last_heartbeat_ms: worker.last_heartbeat(),
            healthy: now_ms.saturating_sub(worker.last_heartbeat()) <= heartbeat_timeout_ms,
            inflight: worker.inflight(),
            capabilities: tenant.registry.capabilities_for(&worker.key),
        })
        .collect::<Vec<_>>();
    peers.sort_by(|left, right| {
        left.core_id
            .as_str()
            .cmp(right.core_id.as_str())
            .then_with(|| left.installation_id.cmp(&right.installation_id))
    });
    peers.truncate(MAX_GATEWAY_WORKERS_PER_TENANT);

    let mut capabilities = peers
        .iter()
        .flat_map(|peer| peer.capabilities.iter())
        .cloned()
        .collect::<Vec<_>>();
    capabilities.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    capabilities.dedup_by(|left, right| left == right);
    let capabilities = capabilities
        .into_iter()
        .map(|name| {
            let matching = peers
                .iter()
                .filter(|peer| peer.capabilities.iter().any(|item| item == &name));
            let mut worker_count = 0;
            let mut healthy_worker_count = 0;
            for peer in matching {
                worker_count += 1;
                if peer.healthy {
                    healthy_worker_count += 1;
                }
            }
            GatewayCapabilitySnapshot {
                name,
                worker_count,
                healthy_worker_count,
            }
        })
        .collect();

    Ok(GatewayDiagnosticsSnapshot {
        tenant_id: query.tenant_id.clone(),
        peers,
        capabilities,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WorkerConnection, WorkerConnectionKey};
    use appcore_contracts::InstallationId;
    use tokio::sync::mpsc;

    #[test]
    fn snapshot_filters_by_capability_and_redacts_transport_state() {
        let tenant_id = TenantId::new("tenant-diagnostics").unwrap();
        let mut tenant = TenantState::new(tenant_id.clone());
        let (sender, _receiver) = mpsc::channel(4);
        let worker = WorkerConnection::new_in_cluster(
            WorkerConnectionKey {
                tenant_id: tenant_id.clone(),
                installation_id: InstallationId::new("install-1").unwrap(),
                core_id: CoreId::new("core-1").unwrap(),
            },
            ClusterId::new("cluster-1").unwrap(),
            sender,
            1_000,
        );
        tenant
            .add_worker(
                worker,
                vec![
                    CapabilityName::new("appcore.update.chunk").unwrap(),
                    CapabilityName::new("runtime.query").unwrap(),
                ],
            )
            .unwrap();
        let query = GatewayDiagnosticsQuery {
            tenant_id,
            capability: Some(CapabilityName::new("appcore.update.chunk").unwrap()),
            cluster_id: Some(ClusterId::new("cluster-1").unwrap()),
        };
        let snapshot = snapshot_tenant(&tenant, &query, 1_001, 90_000).unwrap();
        assert_eq!(snapshot.peers.len(), 1);
        assert_eq!(snapshot.capabilities.len(), 2);
        assert_eq!(snapshot.peers[0].inflight, 0);
        assert!(!serde_json::to_string(&snapshot).unwrap().contains("sender"));
    }
}
