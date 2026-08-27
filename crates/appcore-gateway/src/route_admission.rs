// =============================================================================
//        #######
//     ###       ###     F: route_admission.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.5.0-alpha.1
// =============================================================================

//! Synchronous worker admission before Gateway route dispatch.

use crate::connection::{WorkerAdmissionFailure, WorkerRoutePermit};
use crate::mesh::{MeshPeerRequest, MeshPeerResponse};
use crate::metrics::RouteObservation;
use crate::state::GatewayState;
use crate::telemetry::RouteOutcome;
use crate::WorkerConnection;
use appcore_distributed_contracts::{PeerRpcEnvelope, PeerRpcResponse};
use appcore_types::CapabilityName;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;

pub(crate) struct PeerRouteAdmission {
    pub(crate) receiver: oneshot::Receiver<PeerRpcResponse>,
    pub(crate) worker: WorkerConnection,
    pub(crate) permit: WorkerRoutePermit,
}

pub(crate) struct MeshRouteAdmission {
    pub(crate) receiver: oneshot::Receiver<MeshPeerResponse>,
    pub(crate) worker: WorkerConnection,
    pub(crate) permit: WorkerRoutePermit,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum RouteAdmissionRejection {
    TenantLimit,
    TenantUnavailable,
    WorkerUnavailable,
    WorkerUnhealthy,
    WorkerAtCapacity,
    PendingSaturation,
}

impl RouteAdmissionRejection {
    pub(crate) fn outcome(self) -> RouteOutcome {
        match self {
            Self::TenantLimit | Self::TenantUnavailable | Self::WorkerUnavailable => {
                RouteOutcome::WorkerUnavailable
            }
            Self::WorkerUnhealthy => RouteOutcome::WorkerUnhealthy,
            Self::WorkerAtCapacity => RouteOutcome::WorkerAtCapacity,
            Self::PendingSaturation => RouteOutcome::PendingSaturation,
        }
    }

    pub(crate) fn peer_error(self, capability: &CapabilityName) -> String {
        match self {
            Self::TenantLimit => "tenant_limit_reached".to_string(),
            Self::WorkerUnavailable => {
                format!("compatible_worker_unavailable: {}", capability.as_str())
            }
            Self::WorkerUnhealthy => "worker_unhealthy".to_string(),
            Self::WorkerAtCapacity => "worker_at_capacity".to_string(),
            Self::PendingSaturation => "pending_request_rejected".to_string(),
            Self::TenantUnavailable => "tenant_unavailable".to_string(),
        }
    }

    pub(crate) fn mesh_error(self) -> &'static str {
        match self {
            Self::TenantUnavailable | Self::TenantLimit => "tenant_unavailable",
            Self::WorkerUnavailable => "worker_offline",
            Self::WorkerUnhealthy => "worker_unhealthy",
            Self::WorkerAtCapacity => "worker_at_capacity",
            Self::PendingSaturation => "pending_request_rejected",
        }
    }
}

pub(crate) fn admit_peer_route(
    state: &GatewayState,
    envelope: &PeerRpcEnvelope,
    deadline: Instant,
    telemetry: &mut RouteObservation,
) -> Result<PeerRouteAdmission, RouteAdmissionRejection> {
    let tenant = state
        .tenant_partition_or_insert(&envelope.tenant_id)
        .map_err(|_| RouteAdmissionRejection::TenantLimit)?;
    let lock_started = Instant::now();
    let mut tenant = tenant.write();
    telemetry.record_lock_wait(lock_started.elapsed());
    let worker = tenant
        .get_worker_in_cluster(&envelope.cluster_id, &envelope.target_core_id)
        .filter(|worker| {
            tenant
                .registry
                .resolve(&envelope.capability)
                .is_some_and(|workers| workers.contains(&worker.key))
        })
        .cloned()
        .ok_or(RouteAdmissionRejection::WorkerUnavailable)?;
    let permit = admit_worker(state, &worker)?;
    let receiver = tenant
        .register_pending_request(envelope.request_id.clone(), worker.generation(), deadline)
        .map_err(|_| RouteAdmissionRejection::PendingSaturation)?;
    Ok(PeerRouteAdmission {
        receiver,
        worker,
        permit,
    })
}

pub(crate) fn admit_mesh_route(
    state: &GatewayState,
    request: &MeshPeerRequest,
    deadline: Instant,
    telemetry: &mut RouteObservation,
) -> Result<MeshRouteAdmission, RouteAdmissionRejection> {
    let tenant = state
        .tenant_partition(&request.target_tenant_id)
        .ok_or(RouteAdmissionRejection::TenantUnavailable)?;
    let lock_started = Instant::now();
    let mut tenant = tenant.write();
    telemetry.record_lock_wait(lock_started.elapsed());
    let peer_envelope = request.peer_envelope().ok();
    let worker = match peer_envelope.as_ref() {
        Some(envelope) => {
            tenant.get_worker_in_cluster(&envelope.cluster_id, &request.target_core_id)
        }
        None => tenant.get_worker_by_core(&request.target_core_id),
    }
    .filter(|worker| {
        peer_envelope.as_ref().is_none_or(|envelope| {
            tenant
                .registry
                .resolve(&envelope.capability)
                .is_some_and(|workers| workers.contains(&worker.key))
        })
    })
    .cloned()
    .ok_or(RouteAdmissionRejection::WorkerUnavailable)?;
    let permit = admit_worker(state, &worker)?;
    let receiver = tenant
        .register_pending_mesh_request(
            request.request_id.clone(),
            worker.generation(),
            deadline,
            request.max_response_bytes,
        )
        .map_err(|_| RouteAdmissionRejection::PendingSaturation)?;
    Ok(MeshRouteAdmission {
        receiver,
        worker,
        permit,
    })
}

fn admit_worker(
    state: &GatewayState,
    worker: &WorkerConnection,
) -> Result<WorkerRoutePermit, RouteAdmissionRejection> {
    if !worker.is_open_and_healthy(now_ms(), state.config().heartbeat_timeout) {
        return Err(RouteAdmissionRejection::WorkerUnhealthy);
    }
    let permit = worker
        .try_admit_route(crate::config::MAX_GATEWAY_WORKER_INFLIGHT)
        .map_err(|error| match error {
            WorkerAdmissionFailure::Closed => RouteAdmissionRejection::WorkerUnhealthy,
            WorkerAdmissionFailure::AtCapacity => RouteAdmissionRejection::WorkerAtCapacity,
        })?;
    state.metrics.worker_route_admitted(permit.admitted());
    Ok(permit)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
