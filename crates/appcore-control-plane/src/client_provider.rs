// =============================================================================
//        #######
//     ###       ###     F: client_provider.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/07 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/07 00:00:00 by dnettoRaw
//      ###########      S: 1.0.3-rc
// =============================================================================

//! Provider entry points capture request budgets before bounded worker admission.

use super::*;

impl<T> ControlPlaneProvider for HttpControlPlaneClient<T>
where
    T: HttpTransport + 'static,
{
    fn register<'a>(
        &'a self,
        registration: CoreRegistration,
    ) -> ControlPlaneFuture<'a, CorePresence> {
        let client = self.for_request();
        self.worker
            .enqueue(move || client.post(CONTROL_REGISTER_PATH, &registration))
    }

    fn heartbeat<'a>(
        &'a self,
        request: HeartbeatRequest,
    ) -> ControlPlaneFuture<'a, HeartbeatResponse> {
        let client = self.for_request();
        self.worker
            .enqueue(move || client.post(CONTROL_HEARTBEAT_PATH, &request))
    }

    fn discover_peers<'a>(
        &'a self,
        identity: &'a CoreIdentity,
    ) -> ControlPlaneFuture<'a, PeerDirectory> {
        let client = self.for_request();
        let identity = identity.clone();
        self.worker
            .enqueue(move || client.post(CONTROL_PEERS_PATH, &identity))
    }

    fn acquire_or_renew_service_lease<'a>(
        &'a self,
        identity: &'a CoreIdentity,
        service_id: &'a ServiceId,
        ttl_ms: u64,
        now_ms: u64,
    ) -> ControlPlaneFuture<'a, ServiceLeaderLease> {
        let client = self.for_request();
        let request = ServiceLeaseRequest {
            identity: identity.clone(),
            service_id: service_id.clone(),
            ttl_ms,
            now_ms,
        };
        self.worker
            .enqueue(move || client.post(CONTROL_SERVICE_LEASE_PATH, &request))
    }

    fn release_service_lease<'a>(
        &'a self,
        lease: ServiceLeaderLease,
    ) -> ControlPlaneFuture<'a, ()> {
        let client = self.for_request();
        self.worker.enqueue(move || {
            let _: EmptyResponse = client.post(CONTROL_SERVICE_LEASE_RELEASE_PATH, &lease)?;
            Ok(())
        })
    }

    fn register_traced<'a>(
        &'a self,
        registration: CoreRegistration,
        trace: Option<&'a TraceContext>,
    ) -> ControlPlaneFuture<'a, CorePresence> {
        let client = self.for_request();
        let trace = trace.cloned();
        self.worker.enqueue(move || {
            client.post_traced(CONTROL_REGISTER_PATH, &registration, trace.as_ref())
        })
    }

    fn heartbeat_traced<'a>(
        &'a self,
        request: HeartbeatRequest,
        trace: Option<&'a TraceContext>,
    ) -> ControlPlaneFuture<'a, HeartbeatResponse> {
        let client = self.for_request();
        let trace = trace.cloned();
        self.worker
            .enqueue(move || client.post_traced(CONTROL_HEARTBEAT_PATH, &request, trace.as_ref()))
    }

    fn discover_peers_traced<'a>(
        &'a self,
        identity: &'a CoreIdentity,
        trace: Option<&'a TraceContext>,
    ) -> ControlPlaneFuture<'a, PeerDirectory> {
        let client = self.for_request();
        let identity = identity.clone();
        let trace = trace.cloned();
        self.worker
            .enqueue(move || client.post_traced(CONTROL_PEERS_PATH, &identity, trace.as_ref()))
    }

    fn acquire_or_renew_service_lease_traced<'a>(
        &'a self,
        identity: &'a CoreIdentity,
        service_id: &'a ServiceId,
        ttl_ms: u64,
        now_ms: u64,
        trace: Option<&'a TraceContext>,
    ) -> ControlPlaneFuture<'a, ServiceLeaderLease> {
        let client = self.for_request();
        let trace = trace.cloned();
        let request = ServiceLeaseRequest {
            identity: identity.clone(),
            service_id: service_id.clone(),
            ttl_ms,
            now_ms,
        };
        self.worker.enqueue(move || {
            client.post_traced(CONTROL_SERVICE_LEASE_PATH, &request, trace.as_ref())
        })
    }

    fn release_service_lease_traced<'a>(
        &'a self,
        lease: ServiceLeaderLease,
        trace: Option<&'a TraceContext>,
    ) -> ControlPlaneFuture<'a, ()> {
        let client = self.for_request();
        let trace = trace.cloned();
        self.worker.enqueue(move || {
            let _: EmptyResponse =
                client.post_traced(CONTROL_SERVICE_LEASE_RELEASE_PATH, &lease, trace.as_ref())?;
            Ok(())
        })
    }
}
