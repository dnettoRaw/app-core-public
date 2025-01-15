// =============================================================================
//        #######
//     ###       ###     F: offline.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

/// Client that explicitly reports the control plane as unavailable.
#[derive(Debug, Clone, Copy, Default)]
pub struct OfflineControlPlaneClient;

impl ControlPlaneProvider for OfflineControlPlaneClient {
    fn register<'a>(
        &'a self,
        _registration: CoreRegistration,
    ) -> ControlPlaneFuture<'a, CorePresence> {
        Box::pin(async { Err(ControlPlaneError::Offline) })
    }

    fn heartbeat<'a>(
        &'a self,
        _request: HeartbeatRequest,
    ) -> ControlPlaneFuture<'a, HeartbeatResponse> {
        Box::pin(async { Err(ControlPlaneError::Offline) })
    }

    fn discover_peers<'a>(
        &'a self,
        _identity: &'a CoreIdentity,
    ) -> ControlPlaneFuture<'a, PeerDirectory> {
        Box::pin(async { Err(ControlPlaneError::Offline) })
    }

    fn acquire_or_renew_service_lease<'a>(
        &'a self,
        _identity: &'a CoreIdentity,
        _service_id: &'a ServiceId,
        _ttl_ms: u64,
        _now_ms: u64,
    ) -> ControlPlaneFuture<'a, ServiceLeaderLease> {
        Box::pin(async { Err(ControlPlaneError::Offline) })
    }

    fn release_service_lease<'a>(
        &'a self,
        _lease: ServiceLeaderLease,
    ) -> ControlPlaneFuture<'a, ()> {
        Box::pin(async { Err(ControlPlaneError::Offline) })
    }
}
