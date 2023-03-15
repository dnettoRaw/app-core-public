// =============================================================================
//        #######
//     ###       ###     F: supervisor_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 11:51:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use crate::{
    CallbackManagedService, DependencyRequirement, ManagedResource, ManagedThreadService,
    PassiveManagedService, RestartPolicy, ServiceActivationState, ServiceDescriptor,
    ServiceRuntimeState, SupervisorEventKind, WatchdogConfig,
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn passive(
    name: &str,
    dependency: Option<&str>,
    policy: RestartPolicy,
) -> Arc<PassiveManagedService> {
    let mut descriptor =
        ServiceDescriptor::new(name, ManagedResource::Worker, policy).expect("descriptor");
    if let Some(dependency) = dependency {
        descriptor = descriptor.with_dependency(dependency).expect("dependency");
    }
    Arc::new(PassiveManagedService::new(descriptor))
}

fn complete_restart(supervisor: &Supervisor, timestamp_ms: u64) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        supervisor.reconcile(timestamp_ms).expect("reconcile");
        if supervisor.snapshots().iter().all(|snapshot| {
            matches!(
                snapshot.restart_state,
                RestartState::None | RestartState::Failed
            )
        }) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("restart did not complete");
}

#[test]
fn starts_and_stops_in_dependency_order() {
    let supervisor = Supervisor::new();
    supervisor
        .register(passive("runtime", None, RestartPolicy::never()))
        .unwrap();
    supervisor
        .register(passive("peer-rpc", Some("runtime"), RestartPolicy::never()))
        .unwrap();

    supervisor.start_all().unwrap();
    assert!(supervisor
        .snapshots()
        .iter()
        .all(|snapshot| snapshot.health == ServiceHealth::Healthy));
    supervisor.shutdown(10).unwrap();
    assert!(supervisor.events().iter().any(|event| {
        event.service_id == "peer-rpc" && event.kind == SupervisorEventKind::ServiceStopped
    }));
}

#[test]
fn missing_dependency_and_cycle_fail_validation() {
    let missing = Supervisor::new();
    missing
        .register(passive(
            "peer-rpc",
            Some("security"),
            RestartPolicy::never(),
        ))
        .unwrap();
    assert!(matches!(
        missing.validate(),
        Err(SupervisorError::DependencyNotFound { .. })
    ));

    let cycle = Supervisor::new();
    cycle
        .register(passive("a", Some("b"), RestartPolicy::never()))
        .unwrap();
    cycle
        .register(passive("b", Some("a"), RestartPolicy::never()))
        .unwrap();
    assert!(matches!(
        cycle.validate(),
        Err(SupervisorError::DependencyCycle(_))
    ));
}

#[test]
fn adapter_callback_panics_become_controlled_failures() {
    let start = CallbackManagedService::new(
        ServiceDescriptor::new(
            "start-panic",
            ManagedResource::Worker,
            RestartPolicy::never(),
        )
        .unwrap(),
        || panic!("injected start panic"),
        |_| Ok(()),
        || ServiceHealth::Healthy,
    );
    assert!(start.start().is_err());
    assert_eq!(start.runtime_state(), ServiceRuntimeState::Failed);

    let stop = CallbackManagedService::new(
        ServiceDescriptor::new(
            "stop-panic",
            ManagedResource::Worker,
            RestartPolicy::never(),
        )
        .unwrap(),
        || Ok(()),
        |_| panic!("injected stop panic"),
        || panic!("injected health panic"),
    );
    stop.start().unwrap();
    assert_eq!(stop.health(), ServiceHealth::Failed);
    assert!(stop.stop(Duration::from_millis(1)).is_err());
    assert_eq!(stop.runtime_state(), ServiceRuntimeState::Failed);

    let thread = ManagedThreadService::new(
        ServiceDescriptor::new(
            "factory-panic",
            ManagedResource::Worker,
            RestartPolicy::never(),
        )
        .unwrap(),
        |_| panic!("injected factory panic"),
    );
    assert!(thread.start().is_err());
    assert_eq!(thread.runtime_state(), ServiceRuntimeState::Failed);
}

#[test]
fn watchdog_detects_a_stuck_reconcile_independently() {
    let watchdog = SupervisorWatchdog::new(
        WatchdogConfig {
            enabled: true,
            check_interval_ms: 5,
            stall_timeout_ms: 10,
        },
        1,
    )
    .unwrap();
    watchdog.record_reconcile_started(2);

    assert_eq!(
        watchdog.evaluate(20),
        Some((WatchdogState::Starting, WatchdogState::Stalled))
    );
    assert_eq!(watchdog.state(), WatchdogState::Stalled);
    assert_eq!(watchdog.reconcile_sequence(), 0);
}

#[test]
fn restart_does_not_block_observation_of_other_services() {
    let supervisor = Supervisor::new();
    let failed = Arc::new(AtomicBool::new(true));
    let probe_failed = Arc::clone(&failed);
    let slow = CallbackManagedService::new(
        ServiceDescriptor::new(
            "slow",
            ManagedResource::Worker,
            RestartPolicy::bounded(2, Duration::from_secs(60))
                .unwrap()
                .with_backoff(Duration::ZERO, Duration::ZERO),
        )
        .unwrap(),
        || Ok(()),
        |_| {
            std::thread::sleep(Duration::from_millis(150));
            Ok(())
        },
        move || {
            if probe_failed.load(Ordering::Acquire) {
                ServiceHealth::Failed
            } else {
                ServiceHealth::Healthy
            }
        },
    );
    let observed = Arc::new(AtomicU64::new(0));
    let probe_observed = Arc::clone(&observed);
    let peer = CallbackManagedService::new(
        ServiceDescriptor::new("peer", ManagedResource::Worker, RestartPolicy::never()).unwrap(),
        || Ok(()),
        |_| Ok(()),
        move || {
            probe_observed.fetch_add(1, Ordering::AcqRel);
            ServiceHealth::Healthy
        },
    );
    supervisor.register(Arc::new(slow)).unwrap();
    supervisor.register(Arc::new(peer)).unwrap();
    supervisor.start_all().unwrap();

    let started = Instant::now();
    supervisor.reconcile(100).unwrap();
    supervisor.reconcile(101).unwrap();

    assert!(started.elapsed() < Duration::from_millis(100));
    assert!(observed.load(Ordering::Acquire) >= 2);
    failed.store(false, Ordering::Release);
}

#[test]
fn orphaned_thread_cannot_start_a_second_instance_and_is_quarantined() {
    let release = Arc::new(AtomicBool::new(false));
    let thread_release = Arc::clone(&release);
    let descriptor = ServiceDescriptor::new(
        "thread",
        ManagedResource::Worker,
        RestartPolicy::bounded(2, Duration::from_secs(60))
            .unwrap()
            .with_backoff(Duration::ZERO, Duration::ZERO)
            .with_shutdown_timeout(Duration::from_millis(10)),
    )
    .unwrap();
    let service = Arc::new(ManagedThreadService::new(descriptor, move |_shutdown| {
        let release = Arc::clone(&thread_release);
        std::thread::Builder::new()
            .spawn(move || {
                while !release.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(())
            })
            .map_err(|error| error.to_string())
    }));
    let supervisor = Supervisor::new();
    supervisor.register(service.clone()).unwrap();
    supervisor.start_all().unwrap();
    supervisor.restart("thread", 100).unwrap();
    complete_restart(&supervisor, 101);

    let snapshot = supervisor.snapshots().remove(0);
    assert_eq!(snapshot.runtime_state, ServiceRuntimeState::Orphaned);
    assert!(snapshot.quarantined);
    assert_eq!(
        service.start(),
        Err(SupervisorError::ServiceOrphaned("thread".to_string()))
    );
    assert!(supervisor
        .events()
        .iter()
        .any(|event| event.kind == SupervisorEventKind::ServiceOrphaned));
    release.store(true, Ordering::Release);
}

#[test]
fn restart_budget_prevents_a_restart_loop() {
    let supervisor = Supervisor::new();
    let policy = RestartPolicy::bounded(1, Duration::from_secs(600))
        .unwrap()
        .with_backoff(Duration::ZERO, Duration::ZERO);
    supervisor
        .register(passive("worker", None, policy))
        .unwrap();
    supervisor.start_all().unwrap();

    supervisor.restart("worker", 1_000).unwrap();
    complete_restart(&supervisor, 1_001);
    assert_eq!(
        supervisor.restart("worker", 2_000),
        Err(SupervisorError::RestartBudgetExceeded("worker".to_string()))
    );
    let snapshot = supervisor.snapshots().remove(0);
    assert!(snapshot.operator_required);
    assert!(snapshot.quarantined);
}

#[test]
fn healthy_requirement_rejects_a_degraded_dependency() {
    let supervisor = Supervisor::new();
    let degraded = Arc::new(AtomicBool::new(false));
    let probe = Arc::clone(&degraded);
    let dependency = CallbackManagedService::new(
        ServiceDescriptor::new(
            "security",
            ManagedResource::Security,
            RestartPolicy::never(),
        )
        .unwrap(),
        || Ok(()),
        |_| Ok(()),
        move || {
            if probe.load(Ordering::Acquire) {
                ServiceHealth::Degraded
            } else {
                ServiceHealth::Healthy
            }
        },
    );
    let dependent =
        ServiceDescriptor::new("peer-rpc", ManagedResource::PeerRpc, RestartPolicy::never())
            .unwrap()
            .with_dependency_requirement("security", DependencyRequirement::Healthy)
            .unwrap();
    supervisor.register(Arc::new(dependency)).unwrap();
    supervisor
        .register(Arc::new(PassiveManagedService::new(dependent)))
        .unwrap();
    supervisor.start_all().unwrap();

    degraded.store(true, Ordering::Release);
    supervisor.reconcile(100).unwrap();
    let peer = supervisor
        .snapshots()
        .into_iter()
        .find(|snapshot| snapshot.name == "peer-rpc")
        .unwrap();
    assert_eq!(peer.health, ServiceHealth::Degraded);
    assert_eq!(peer.restart_count, 0);
}

#[test]
fn disabled_service_is_not_reported_as_healthy() {
    let descriptor = ServiceDescriptor::new("sync", ManagedResource::Sync, RestartPolicy::never())
        .unwrap()
        .with_activation(ServiceActivationState::Disabled);
    let supervisor = Supervisor::new();
    supervisor
        .register(Arc::new(PassiveManagedService::new(descriptor)))
        .unwrap();
    supervisor.start_all().unwrap();

    let snapshot = supervisor.snapshots().remove(0);
    assert_eq!(snapshot.activation, ServiceActivationState::Disabled);
    assert!(!snapshot.enabled);
    assert!(!snapshot.running);
    assert_eq!(snapshot.health, ServiceHealth::Unknown);
}

#[test]
fn inactive_placeholder_can_be_replaced_but_enabled_service_cannot() {
    let supervisor = Supervisor::new();
    let placeholder = ServiceDescriptor::new(
        "auth-server",
        ManagedResource::AuthServer,
        RestartPolicy::never(),
    )
    .unwrap()
    .with_activation(ServiceActivationState::NotConfigured);
    supervisor
        .register(Arc::new(PassiveManagedService::new(placeholder)))
        .unwrap();

    supervisor
        .register_or_replace_inactive(Arc::new(PassiveManagedService::new(
            ServiceDescriptor::new(
                "auth-server",
                ManagedResource::AuthServer,
                RestartPolicy::never(),
            )
            .unwrap(),
        )))
        .unwrap();
    supervisor.start("auth-server", 1).unwrap();

    assert!(supervisor.snapshots()[0].enabled);
    assert_eq!(
        supervisor.register_or_replace_inactive(Arc::new(PassiveManagedService::new(
            ServiceDescriptor::new(
                "auth-server",
                ManagedResource::AuthServer,
                RestartPolicy::never(),
            )
            .unwrap(),
        ))),
        Err(SupervisorError::ServiceAlreadyRegistered(
            "auth-server".to_string()
        ))
    );
}

#[test]
fn shutdown_stops_watchdog_and_restart_executor() {
    let supervisor = Supervisor::new();
    supervisor
        .register(passive("runtime", None, RestartPolicy::never()))
        .unwrap();
    supervisor.start_all().unwrap();
    supervisor.reconcile(10).unwrap();

    supervisor.shutdown(20).unwrap();
    let diagnosis = supervisor.diagnose();
    assert_eq!(diagnosis.watchdog.state, WatchdogState::Stopping);
    assert!(!diagnosis.restart_executor.healthy);
}
