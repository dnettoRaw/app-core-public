// =============================================================================
//        #######
//     ###       ###     F: control_plane_service.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/20 23:03:47 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Control-plane registration, heartbeat, discovery and lease renewal loop.

use crate::bootstrap::{now_ms, BootstrapError};
use crate::server::RuntimeServer;
use appcore_contracts::ServiceId;
use appcore_control_plane::{
    ControlPlaneError, ControlPlaneProvider, CoreRegistration, HeartbeatRequest,
};
use appcore_core::{RuntimeOperationalMode, TraceContext};
use appcore_ops::{ObservationEvent, ObservationKind, ObservationSeverity, ObservationSink};
use appcore_supervisor::{ManagedService, ServiceHealth};
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::thread;
use std::time::Duration;

pub(super) fn control_plane_service_if_enabled(
    server: &RuntimeServer,
) -> Result<Option<Arc<dyn ManagedService>>, BootstrapError> {
    let Some(client) = crate::providers::control_plane_client(&server.app.provider_plan)? else {
        return Ok(None);
    };
    let state = ControlPlaneLoopState {
        manifest: server.app.core_manifest.clone(),
        service_id: server.app.application_manifest.service_id().clone(),
        operation_mode: Arc::clone(&server.app.operation_mode),
        peer_directory: Arc::clone(&server.app.peer_directory),
        leader_lease: Arc::clone(&server.app.leader_lease),
        observations: server.app.observations.clone(),
        interval_ms: control_plane_interval_ms(server),
    };
    let health_mode = Arc::clone(&state.operation_mode);
    let descriptor = crate::runtime_services::service_descriptor(
        crate::runtime_services::CONTROL_PLANE_SERVICE,
        appcore_supervisor::ManagedResource::ControlPlane,
        &[crate::runtime_services::RUNTIME_SERVICE],
    )?;
    Ok(Some(Arc::new(
        appcore_supervisor::ManagedThreadService::new(descriptor, move |shutdown| {
            let client = Arc::clone(&client);
            let state = state.clone();
            thread::Builder::new()
                .name("appcore-control-plane".to_string())
                .spawn(move || {
                    run_control_plane_loop(client, state, shutdown);
                    Ok(())
                })
                .map_err(|error| error.to_string())
        })
        .with_health_probe(move || match *health_mode.lock() {
            RuntimeOperationalMode::Degraded => ServiceHealth::Degraded,
            RuntimeOperationalMode::Isolated => ServiceHealth::Degraded,
            RuntimeOperationalMode::Starting
            | RuntimeOperationalMode::Discovering
            | RuntimeOperationalMode::Syncing => ServiceHealth::Starting,
            RuntimeOperationalMode::ReadOnly | RuntimeOperationalMode::ReadWrite => {
                ServiceHealth::Healthy
            }
        }),
    )))
}

fn control_plane_interval_ms(server: &RuntimeServer) -> u64 {
    server
        .app
        .provider_plan
        .control_plane()
        .and_then(|provider| provider.settings().get("heartbeat_interval_ms"))
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(server.app.config.control_plane_heartbeat_interval_ms)
}

#[derive(Clone)]
struct ControlPlaneLoopState {
    manifest: appcore_core::DistributedCoreManifest,
    service_id: ServiceId,
    operation_mode: Arc<parking_lot::Mutex<RuntimeOperationalMode>>,
    peer_directory: Arc<parking_lot::Mutex<Option<appcore_control_plane::PeerDirectory>>>,
    leader_lease: Arc<parking_lot::Mutex<Option<appcore_control_plane::ServiceLeaderLease>>>,
    observations: appcore_ops::InMemoryObservationSink,
    interval_ms: u64,
}

fn run_control_plane_loop(
    client: Arc<dyn ControlPlaneProvider>,
    state: ControlPlaneLoopState,
    shutdown: Arc<AtomicBool>,
) {
    let _ = run_control_plane_iteration(client.as_ref(), &state, true);
    while !shutdown.load(Ordering::SeqCst) {
        let _ = run_control_plane_iteration(client.as_ref(), &state, false);
        if sleep_or_shutdown(&shutdown, state.interval_ms) {
            break;
        }
    }
    if let Some(lease) = state.leader_lease.lock().take() {
        let trace = control_plane_trace(&state, "release");
        let _ = block_on(client.release_service_lease_traced(lease, trace.as_ref()));
    }
}

fn run_control_plane_iteration<C>(
    client: &C,
    state: &ControlPlaneLoopState,
    register: bool,
) -> Result<(), ControlPlaneError>
where
    C: ControlPlaneProvider + ?Sized,
{
    let iteration_trace = control_plane_trace(state, "iteration");
    let current_mode = *state.operation_mode.lock();
    if register {
        let trace = child_control_plane_trace(&iteration_trace, state, "register");
        block_on(client.register_traced(
            CoreRegistration {
                manifest: state.manifest.clone(),
                registered_at_ms: now_ms(),
                operation_mode: current_mode,
            },
            trace.as_ref(),
        ))
        .map(|_| ())
        .inspect_err(|_| observe_control_plane_failure(state, "register"))?;
    }

    let trace = child_control_plane_trace(&iteration_trace, state, "heartbeat");
    let heartbeat = block_on(client.heartbeat_traced(
        HeartbeatRequest {
            identity: state.manifest.identity.clone(),
            operation_mode: *state.operation_mode.lock(),
            sent_at_ms: now_ms(),
        },
        trace.as_ref(),
    ))
    .inspect_err(|_| observe_control_plane_failure(state, "heartbeat"))?;
    *state.operation_mode.lock() = heartbeat.operation_mode;

    let trace = child_control_plane_trace(&iteration_trace, state, "discover");
    let directory =
        block_on(client.discover_peers_traced(&state.manifest.identity, trace.as_ref()))
            .inspect_err(|_| observe_control_plane_failure(state, "discover"))?;
    *state.peer_directory.lock() = Some(directory);

    if *state.operation_mode.lock() == RuntimeOperationalMode::ReadWrite {
        let ttl_ms = state.interval_ms.saturating_mul(3).max(1_000);
        let trace = child_control_plane_trace(&iteration_trace, state, "lease");
        let lease = block_on(client.acquire_or_renew_service_lease_traced(
            &state.manifest.identity,
            &state.service_id,
            ttl_ms,
            now_ms(),
            trace.as_ref(),
        ))
        .inspect_err(|_| {
            *state.leader_lease.lock() = None;
            observe_control_plane_failure(state, "service_lease");
        })?;
        state.observations.emit(
            ObservationEvent::new(
                ObservationKind::ControlPlane,
                ObservationSeverity::Info,
                "runtime.service_lease.active",
                now_ms(),
            )
            .with_attribute("service_id", state.service_id.as_str())
            .with_attribute("epoch", lease.epoch.to_string()),
        );
        *state.leader_lease.lock() = Some(lease);
    }
    Ok(())
}

fn control_plane_trace(state: &ControlPlaneLoopState, operation: &str) -> Option<TraceContext> {
    // appcore-norm: allow(global-state) reason: atomic sequence prevents process-local trace identifier collisions
    static TRACE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
    let sequence = TRACE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let timestamp = now_ms();
    TraceContext::new(
        format!("cp-{timestamp}-{sequence}"),
        format!("cp-{operation}-{sequence}"),
        state.manifest.identity.core_id.clone(),
        state.manifest.identity.core_id.clone(),
        state.manifest.identity.tenant_id.clone(),
    )
    .ok()
}

fn child_control_plane_trace(
    parent: &Option<TraceContext>,
    state: &ControlPlaneLoopState,
    operation: &str,
) -> Option<TraceContext> {
    // appcore-norm: allow(global-state) reason: atomic sequence prevents process-local span identifier collisions
    static SPAN_SEQUENCE: AtomicU64 = AtomicU64::new(1);
    let sequence = SPAN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    parent.as_ref().and_then(|trace| {
        trace
            .child_span(
                format!("cp-{operation}-{sequence}"),
                state.manifest.identity.core_id.clone(),
            )
            .ok()
    })
}

fn set_degraded(state: &ControlPlaneLoopState) {
    let mut mode = state.operation_mode.lock();
    if matches!(
        *mode,
        RuntimeOperationalMode::ReadWrite | RuntimeOperationalMode::ReadOnly
    ) {
        *mode = RuntimeOperationalMode::Degraded;
    }
}

fn observe_control_plane_failure(state: &ControlPlaneLoopState, operation: &str) {
    set_degraded(state);
    state.observations.emit(
        ObservationEvent::new(
            ObservationKind::ControlPlane,
            ObservationSeverity::Warning,
            "runtime.control_plane.failed",
            now_ms(),
        )
        .with_attribute("operation", operation),
    );
}

fn sleep_or_shutdown(shutdown: &AtomicBool, interval_ms: u64) -> bool {
    let mut slept = 0u64;
    let interval = interval_ms.max(100);
    while slept < interval {
        if shutdown.load(Ordering::SeqCst) {
            return true;
        }
        let step = (interval - slept).min(100);
        thread::sleep(Duration::from_millis(step));
        slept = slept.saturating_add(step);
    }
    shutdown.load(Ordering::SeqCst)
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::yield_now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use appcore_control_plane::{InMemoryControlPlane, OfflineControlPlaneClient};
    use appcore_core::{
        AppFamily, AppId, ClusterId, CoreId, CoreIdentity, CoreKind, DistributedCoreManifest,
        InstanceId, NodeId, ProtocolVersion, RuntimeContractVersion, RuntimeIdentity, SyncGroup,
        TenantId,
    };
    use std::collections::BTreeMap;

    fn state(mode: RuntimeOperationalMode) -> ControlPlaneLoopState {
        ControlPlaneLoopState {
            manifest: DistributedCoreManifest {
                identity: CoreIdentity {
                    tenant_id: TenantId::new("tenant-a").unwrap(),
                    cluster_id: ClusterId::new("cluster-a").unwrap(),
                    core_id: CoreId::new("core-a").unwrap(),
                    instance_id: InstanceId::new("core-a-1").unwrap(),
                    kind: CoreKind::operational(),
                    protocol_version: ProtocolVersion::new(1),
                    runtime: RuntimeIdentity {
                        app_id: AppId::new("app-a").unwrap(),
                        app_family: AppFamily::new("family-a").unwrap(),
                        sync_group: SyncGroup::new("cluster-a").unwrap(),
                        runtime_contract: RuntimeContractVersion::new(1),
                        node_id: NodeId::new("node-a").unwrap(),
                    },
                },
                app_name: "App".to_string(),
                app_version: "0.1.0".to_string(),
                runtime_min_version: "0.6.1".to_string(),
                runtime_max_version: None,
                capabilities: Vec::new(),
                endpoints: Vec::new(),
                metadata: BTreeMap::new(),
            },
            service_id: ServiceId::new("runtime.service").unwrap(),
            operation_mode: Arc::new(parking_lot::Mutex::new(mode)),
            peer_directory: Arc::new(parking_lot::Mutex::new(None)),
            leader_lease: Arc::new(parking_lot::Mutex::new(None)),
            observations: appcore_ops::InMemoryObservationSink::new(16),
            interval_ms: 100,
        }
    }

    #[test]
    fn offline_control_plane_degrades_once_without_loop() {
        let state = state(RuntimeOperationalMode::ReadWrite);
        let result = run_control_plane_iteration(&OfflineControlPlaneClient, &state, true);

        assert!(result.is_err());
        assert_eq!(
            *state.operation_mode.lock(),
            RuntimeOperationalMode::Degraded
        );
    }

    #[test]
    fn in_memory_control_plane_registers_discovers_and_acquires_lease() {
        let state = state(RuntimeOperationalMode::ReadWrite);
        let control = InMemoryControlPlane::default();
        let result = run_control_plane_iteration(&control, &state, true);

        assert!(result.is_ok());
        assert!(state.peer_directory.lock().is_some());
        assert!(state.leader_lease.lock().is_some());
    }
}
