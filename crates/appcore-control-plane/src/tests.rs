// =============================================================================
//        #######
//     ###       ###     F: tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use crate::transport::{control_plane_trace_headers, parse_http_response, HttpScheme, HttpTarget};
use crate::worker::ControlPlaneWorker;
use crate::{BearerHttpTransport, SecretString};
use appcore_core::{
    AppFamily, AppId, CapabilityDescriptor, CapabilityMode, CapabilityName, CapabilityVisibility,
    Clock, CoreKind, DistributedCoreManifest, InstanceId, NodeId, PeerEndpoint, ProtocolVersion,
    RuntimeContractVersion, RuntimeIdentity, SyncGroup,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = std::task::Waker::noop();
    let mut context = std::task::Context::from_waker(waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => return value,
            std::task::Poll::Pending => thread::yield_now(),
        }
    }
}

fn identity(core_id: &str) -> CoreIdentity {
    CoreIdentity {
        tenant_id: TenantId::new("tenant-a").unwrap(),
        cluster_id: ClusterId::new("cluster-a").unwrap(),
        core_id: CoreId::new(core_id).unwrap(),
        instance_id: InstanceId::new(format!("{core_id}-instance")).unwrap(),
        kind: CoreKind::operational(),
        protocol_version: ProtocolVersion::new(1),
        runtime: RuntimeIdentity {
            app_id: AppId::new("app-a").unwrap(),
            app_family: AppFamily::new("family-a").unwrap(),
            sync_group: SyncGroup::new("cluster-a").unwrap(),
            runtime_contract: RuntimeContractVersion::new(1),
            node_id: NodeId::new(core_id).unwrap(),
        },
    }
}

fn manifest(core_id: &str) -> DistributedCoreManifest {
    DistributedCoreManifest {
        identity: identity(core_id),
        app_name: "App".to_string(),
        app_version: "0.1.0".to_string(),
        runtime_min_version: "0.6.1".to_string(),
        runtime_max_version: None,
        capabilities: vec![CapabilityDescriptor::new(
            CapabilityName::new("runtime.query").unwrap(),
            "1",
            CapabilityMode::Query,
            CapabilityVisibility::Cluster,
        )],
        endpoints: Vec::new(),
        metadata: BTreeMap::new(),
    }
}

#[test]
fn fake_control_plane_registers_and_discovers_peers() {
    let control = InMemoryControlPlane::default();
    let first = CoreRegistration {
        manifest: manifest("core-a"),
        registered_at_ms: 10,
        operation_mode: RuntimeOperationalMode::ReadWrite,
    };
    let second = CoreRegistration {
        manifest: manifest("core-b"),
        registered_at_ms: 20,
        operation_mode: RuntimeOperationalMode::ReadOnly,
    };

    assert!(block_on(control.register(first)).is_ok());
    assert!(block_on(control.register(second)).is_ok());
    let directory = block_on(control.discover_peers(&identity("core-a"))).unwrap();

    assert_eq!(control.registrations_len().unwrap(), 2);
    assert_eq!(directory.peers.len(), 1);
    assert_eq!(directory.peers[0].identity.core_id.as_str(), "core-b");
}

#[test]
fn offline_control_plane_can_degrade_once_without_loop() {
    let coordinator =
        ControlPlaneCoordinator::new(OfflineControlPlaneClient, HeartbeatPolicy::default());
    let mode = block_on(coordinator.heartbeat_once(HeartbeatRequest {
        identity: identity("core-a"),
        operation_mode: RuntimeOperationalMode::ReadWrite,
        sent_at_ms: 100,
    }))
    .unwrap();

    assert_eq!(mode, RuntimeOperationalMode::Degraded);
}

#[test]
fn http_control_plane_future_does_not_block_the_polling_thread() {
    #[derive(Clone)]
    struct GatedTransport {
        gate: Arc<std::sync::Barrier>,
        response: HttpControlPlaneResponse,
    }

    impl HttpTransport for GatedTransport {
        fn send_json(
            &self,
            _base_url: &str,
            _request: HttpControlPlaneRequest,
        ) -> ControlPlaneResult<HttpControlPlaneResponse> {
            self.gate.wait();
            Ok(self.response.clone())
        }
    }

    let identity = identity("core-a");
    let presence = CorePresence {
        identity: identity.clone(),
        operation_mode: RuntimeOperationalMode::ReadWrite,
        healthy: true,
        last_seen_ms: 10,
    };
    let gate = Arc::new(std::sync::Barrier::new(2));
    let client = HttpControlPlaneClient::new(
        ControlPlaneHttpConfig {
            base_url: "http://127.0.0.1:1".to_string(),
            timeout_ms: 100,
            retry_policy: RetryPolicy {
                max_attempts: 1,
                ..RetryPolicy::default()
            },
        },
        GatedTransport {
            gate: Arc::clone(&gate),
            response: HttpControlPlaneResponse {
                status_code: 200,
                body: serde_json::to_vec(&presence).unwrap(),
            },
        },
    );
    let mut future = client.register(CoreRegistration {
        manifest: manifest("core-a"),
        registered_at_ms: 10,
        operation_mode: RuntimeOperationalMode::ReadWrite,
    });
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);

    assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
    gate.wait();
    assert_eq!(block_on(future).unwrap(), presence);
}

#[test]
fn control_plane_worker_applies_bounded_backpressure() {
    let worker = ControlPlaneWorker::new();
    let (started_sender, started_receiver) = mpsc::sync_channel(0);
    let release = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    let worker_release = Arc::clone(&release);
    let first = worker.enqueue(move || {
        started_sender.send(()).unwrap();
        let (released, wake) = &*worker_release;
        let mut released = released.lock().unwrap();
        while !*released {
            released = wake.wait(released).unwrap();
        }
        Ok(())
    });
    started_receiver
        .recv_timeout(Duration::from_secs(1))
        .unwrap();

    let mut pending = Vec::with_capacity(MAX_CONTROL_PLANE_WORK_ITEMS);
    for _ in 0..MAX_CONTROL_PLANE_WORK_ITEMS {
        pending.push(worker.enqueue(|| Ok::<(), ControlPlaneError>(())))
    }
    let overflow = worker.enqueue(|| Ok::<(), ControlPlaneError>(()));
    assert_eq!(
        block_on(overflow),
        Err(ControlPlaneError::Transport(
            "control-plane worker queue is full".to_string()
        ))
    );

    let (released, wake) = &*release;
    *released.lock().unwrap() = true;
    wake.notify_one();
    assert_eq!(block_on(first), Ok(()));
    drop(pending);
}

#[test]
fn control_plane_worker_drop_drains_and_joins_accepted_work() {
    let worker = ControlPlaneWorker::new();
    let completed = Arc::new(AtomicBool::new(false));
    let worker_completed = Arc::clone(&completed);
    let future = worker.enqueue(move || {
        thread::sleep(Duration::from_millis(20));
        worker_completed.store(true, Ordering::SeqCst);
        Ok(())
    });

    drop(worker);

    assert!(completed.load(Ordering::SeqCst));
    assert_eq!(block_on(future), Ok(()));
}

#[test]
fn cancelled_control_plane_client_does_not_enter_transport() {
    #[derive(Clone)]
    struct CountingTransport(Arc<AtomicU64>);

    impl HttpTransport for CountingTransport {
        fn send_json(
            &self,
            _base_url: &str,
            _request: HttpControlPlaneRequest,
        ) -> ControlPlaneResult<HttpControlPlaneResponse> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(ControlPlaneError::Offline)
        }
    }

    let attempts = Arc::new(AtomicU64::new(0));
    let client = HttpControlPlaneClient::new(
        ControlPlaneHttpConfig {
            base_url: "http://127.0.0.1:1".to_string(),
            timeout_ms: 100,
            retry_policy: RetryPolicy::default(),
        },
        CountingTransport(Arc::clone(&attempts)),
    );
    client.cancel();
    let result = block_on(client.register(CoreRegistration {
        manifest: manifest("core-a"),
        registered_at_ms: 10,
        operation_mode: RuntimeOperationalMode::ReadWrite,
    }));

    assert!(matches!(
        result,
        Err(ControlPlaneError::Transport(message))
            if message == "control-plane request cancelled"
    ));
    assert_eq!(attempts.load(Ordering::SeqCst), 0);
}

#[test]
fn service_lease_guard_rejects_missing_expired_and_stale_leases() {
    let guard = StaticServiceLeadershipGuard::default();
    let core = identity("core-a");
    let service = ServiceId::new("runtime.query").unwrap();

    assert_eq!(
        guard.check_service_write_permission(
            &service,
            &core.tenant_id,
            &core.cluster_id,
            &core.core_id,
            None,
            10
        ),
        LeadershipDecision::NoLease
    );

    guard
        .set_service_lease(
            service.clone(),
            Some(ServiceLeaderLease {
                service_id: service.clone(),
                tenant_id: core.tenant_id.clone(),
                cluster_id: core.cluster_id.clone(),
                holder_core_id: core.core_id.clone(),
                epoch: 1,
                acquired_at_ms: 0,
                expires_at_ms: 5,
            }),
        )
        .unwrap();

    assert_eq!(
        guard.check_service_write_permission(
            &service,
            &core.tenant_id,
            &core.cluster_id,
            &core.core_id,
            None,
            10
        ),
        LeadershipDecision::Expired
    );

    guard
        .set_service_lease(
            service.clone(),
            Some(ServiceLeaderLease {
                service_id: service.clone(),
                tenant_id: core.tenant_id.clone(),
                cluster_id: core.cluster_id.clone(),
                holder_core_id: core.core_id.clone(),
                epoch: 1,
                acquired_at_ms: 0,
                expires_at_ms: 100,
            }),
        )
        .unwrap();

    assert_eq!(
        guard.check_service_write_permission(
            &service,
            &core.tenant_id,
            &core.cluster_id,
            &core.core_id,
            Some(2),
            10
        ),
        LeadershipDecision::StaleEpoch
    );
}

#[test]
fn in_memory_lease_renewal_keeps_epoch() {
    let control = InMemoryControlPlane::default();
    let core = identity("core-a");
    let service = ServiceId::new("runtime.query").unwrap();
    let first = block_on(control.acquire_or_renew_service_lease(&core, &service, 10, 1)).unwrap();
    let second = block_on(control.acquire_or_renew_service_lease(&core, &service, 10, 2)).unwrap();

    assert_eq!(first.epoch, 1);
    assert_eq!(second.epoch, 1);
    assert!(second.expires_at_ms > first.expires_at_ms);
}

#[test]
fn service_lease_rejects_clock_and_fencing_overflow() {
    let core = identity("core-a");
    let service = ServiceId::new("runtime.query").unwrap();
    let control = InMemoryControlPlane::default();
    assert!(matches!(
        block_on(control.acquire_or_renew_service_lease(&core, &service, 10, u64::MAX - 5)),
        Err(ControlPlaneError::Rejected(_))
    ));

    let state: crate::memory::InMemoryState = serde_json::from_value(serde_json::json!({
        "registrations": {},
        "service_leases": {
            "tenant-a:cluster-a:runtime.query": {
                "lease": null,
                "last_epoch": u64::MAX
            }
        }
    }))
    .unwrap();
    let exhausted = InMemoryControlPlane::from_state(state);
    assert!(matches!(
        block_on(exhausted.acquire_or_renew_service_lease(&core, &service, 10, 1)),
        Err(ControlPlaneError::Conflict(_))
    ));
}

#[test]
fn leases_are_isolated_by_tenant_and_cluster() {
    let control = InMemoryControlPlane::default();
    let first = identity("core-a");
    let mut second = identity("core-b");
    second.tenant_id = TenantId::new("tenant-b").unwrap();
    second.cluster_id = ClusterId::new("cluster-b").unwrap();

    let service = ServiceId::new("runtime.query").unwrap();
    let first_lease =
        block_on(control.acquire_or_renew_service_lease(&first, &service, 100, 1)).unwrap();
    let second_lease =
        block_on(control.acquire_or_renew_service_lease(&second, &service, 100, 1)).unwrap();

    assert_eq!(first_lease.epoch, 1);
    assert_eq!(second_lease.epoch, 1);
    assert_eq!(first_lease.holder_core_id.as_str(), "core-a");
    assert_eq!(second_lease.holder_core_id.as_str(), "core-b");
    assert_eq!(first_lease.service_id, service);
    assert_eq!(second_lease.service_id, service);
}

#[test]
fn service_leases_are_independent_inside_the_same_cluster() {
    let control = InMemoryControlPlane::default();
    let first = identity("core-a");
    let second = identity("core-b");
    let document = ServiceId::new("document.extract").unwrap();
    let storage = ServiceId::new("storage.query").unwrap();

    let document_lease =
        block_on(control.acquire_or_renew_service_lease(&first, &document, 100, 1)).unwrap();
    let storage_lease =
        block_on(control.acquire_or_renew_service_lease(&second, &storage, 100, 1)).unwrap();

    assert_eq!(document_lease.service_id, document);
    assert_eq!(storage_lease.service_id, storage);
    assert_eq!(document_lease.holder_core_id.as_str(), "core-a");
    assert_eq!(storage_lease.holder_core_id.as_str(), "core-b");
}

#[test]
fn service_lease_conflicts_only_within_the_same_service() {
    let control = InMemoryControlPlane::default();
    let first = identity("core-a");
    let second = identity("core-b");
    let service = ServiceId::new("document.extract").unwrap();

    block_on(control.acquire_or_renew_service_lease(&first, &service, 100, 1)).unwrap();
    assert_eq!(
        block_on(control.acquire_or_renew_service_lease(&second, &service, 100, 2)),
        Err(ControlPlaneError::LeaseUnavailable)
    );
}

#[test]
fn http_target_accepts_https_and_uses_default_port() {
    let target = HttpTarget::parse("https://example.com", "/v1").unwrap();

    assert_eq!(target.scheme, HttpScheme::Https);
    assert_eq!(target.port, 443);
    assert_eq!(target.path, "/v1");
}

#[test]
fn remote_control_plane_endpoint_requires_https() {
    assert!(crate::require_secure_remote_endpoint("http://control.example.test").is_err());
    assert!(crate::require_secure_remote_endpoint("https://control.example.test").is_ok());
    assert!(crate::require_secure_remote_endpoint("http://127.0.0.1:8080").is_ok());
}

#[test]
fn stale_release_cannot_clear_newer_lease() {
    let control = InMemoryControlPlane::default();
    let core = identity("core-a");
    let service = ServiceId::new("runtime.query").unwrap();
    let first = block_on(control.acquire_or_renew_service_lease(&core, &service, 10, 1)).unwrap();
    let second = block_on(control.acquire_or_renew_service_lease(&core, &service, 10, 20)).unwrap();

    assert_eq!(second.epoch, first.epoch + 1);
    assert!(matches!(
        block_on(control.release_service_lease(first)),
        Err(ControlPlaneError::Conflict(_))
    ));
    let contender = identity("core-b");
    assert_eq!(
        block_on(control.acquire_or_renew_service_lease(&contender, &service, 10, 21)),
        Err(ControlPlaneError::LeaseUnavailable)
    );
    assert_eq!(second.holder_core_id, core.core_id);
}

#[test]
fn chunked_http_response_is_decoded_with_limit() {
    let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n7\r\n{\"a\":1}\r\n0\r\n\r\n";
    let response = parse_http_response(raw, 32).unwrap();

    assert_eq!(response.body, br#"{"a":1}"#);
}

#[test]
fn control_plane_http_trace_headers_preserve_full_context() {
    let core = identity("core-a");
    let root = TraceContext::new(
        "trace-control-1",
        "span-root-1",
        core.core_id.clone(),
        core.core_id.clone(),
        core.tenant_id.clone(),
    )
    .unwrap();
    let trace = root
        .child_span("span-control-1", core.core_id.clone())
        .unwrap()
        .with_command_id("command-1")
        .unwrap();

    let headers = control_plane_trace_headers(Some(&trace)).unwrap();

    assert!(headers.contains("X-AppCore-Trace-Id: trace-control-1\r\n"));
    assert!(headers.contains("X-AppCore-Span-Id: span-control-1\r\n"));
    assert!(headers.contains("X-AppCore-Parent-Span-Id: span-root-1\r\n"));
    assert!(headers.contains("X-AppCore-Origin-Core-Id: core-a\r\n"));
    assert!(headers.contains("X-AppCore-Current-Core-Id: core-a\r\n"));
    assert!(headers.contains("X-AppCore-Tenant-Id: tenant-a\r\n"));
    assert!(headers.contains("X-AppCore-Command-Id: command-1\r\n"));
}

#[derive(Debug)]
struct TestClock {
    now_ms: AtomicU64,
}

impl TestClock {
    fn new(now_ms: u64) -> Self {
        Self {
            now_ms: AtomicU64::new(now_ms),
        }
    }

    fn set(&self, now_ms: u64) {
        self.now_ms.store(now_ms, Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now_ms(&self) -> u64 {
        self.now_ms.load(Ordering::SeqCst)
    }
}

fn file_control_plane(clock: Arc<TestClock>, retention_ms: u64) -> (FileControlPlane, PathBuf) {
    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);
    let path = std::env::temp_dir().join(format!(
        "appcore-file-control-plane-{}-{}",
        std::process::id(),
        NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    let control = FileControlPlane::with_clock(&path, retention_ms, clock).unwrap();
    (control, path)
}

#[test]
fn file_control_plane_survives_restart_and_ignores_client_clock_skew() {
    let clock = Arc::new(TestClock::new(1_000));
    let (control, path) = file_control_plane(Arc::clone(&clock), 10_000);
    block_on(control.register(CoreRegistration {
        manifest: manifest("core-a"),
        registered_at_ms: u64::MAX,
        operation_mode: RuntimeOperationalMode::ReadWrite,
    }))
    .unwrap();
    let service = ServiceId::new("runtime.query").unwrap();
    let lease = block_on(control.acquire_or_renew_service_lease(
        &identity("core-a"),
        &service,
        100,
        u64::MAX,
    ))
    .unwrap();
    assert_eq!(lease.acquired_at_ms, 1_000);

    clock.set(1_050);
    let restarted = FileControlPlane::with_clock(&path, 10_000, clock).unwrap();
    let renewed =
        block_on(restarted.acquire_or_renew_service_lease(&identity("core-a"), &service, 100, 0))
            .unwrap();
    assert_eq!(renewed.epoch, lease.epoch);
    assert_eq!(renewed.expires_at_ms, 1_150);
    assert_eq!(restarted.state_path(), control.state_path());
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn file_control_plane_serializes_competing_service_leases_and_fences_expiry() {
    let clock = Arc::new(TestClock::new(100));
    let (control, path) = file_control_plane(Arc::clone(&clock), 10_000);
    let service = ServiceId::new("storage.query").unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let contenders = ["core-a", "core-b"]
        .into_iter()
        .map(|core_id| {
            let control = control.clone();
            let service = service.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                block_on(control.acquire_or_renew_service_lease(
                    &identity(core_id),
                    &service,
                    50,
                    0,
                ))
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let results = contenders
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);

    let first = results.into_iter().find_map(Result::ok).unwrap();
    clock.set(151);
    let second =
        block_on(control.acquire_or_renew_service_lease(&identity("core-c"), &service, 50, 0))
            .unwrap();
    assert_eq!(second.epoch, first.epoch + 1);
    assert!(matches!(
        block_on(control.release_service_lease(first)),
        Err(ControlPlaneError::Conflict(_))
    ));
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn discovery_tracks_address_health_and_retention_changes() {
    let clock = Arc::new(TestClock::new(100));
    let (control, path) = file_control_plane(Arc::clone(&clock), 50);
    let mut peer = manifest("core-b");
    peer.endpoints.push(PeerEndpoint {
        name: "peer-rpc".to_string(),
        url: "https://127.0.0.1:9001".to_string(),
        protocol: "appcore.peer-rpc.v1".to_string(),
        metadata: BTreeMap::new(),
    });
    block_on(control.register(CoreRegistration {
        manifest: peer.clone(),
        registered_at_ms: 0,
        operation_mode: RuntimeOperationalMode::ReadWrite,
    }))
    .unwrap();
    let first = block_on(control.discover_peers(&identity("core-a"))).unwrap();
    assert_eq!(first.peers[0].endpoints[0].url, "https://127.0.0.1:9001");
    assert!(first.peers[0].healthy);

    clock.set(120);
    peer.endpoints[0].url = "https://127.0.0.1:9002".to_string();
    block_on(control.register(CoreRegistration {
        manifest: peer,
        registered_at_ms: 0,
        operation_mode: RuntimeOperationalMode::Degraded,
    }))
    .unwrap();
    let changed = block_on(control.discover_peers(&identity("core-a"))).unwrap();
    assert_eq!(changed.peers[0].endpoints[0].url, "https://127.0.0.1:9002");
    assert!(!changed.peers[0].healthy);

    clock.set(171);
    assert!(block_on(control.discover_peers(&identity("core-a")))
        .unwrap()
        .peers
        .is_empty());
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn file_control_plane_backup_restore_is_validated() {
    let clock = Arc::new(TestClock::new(100));
    let (control, path) = file_control_plane(clock, 1_000);
    block_on(control.register(CoreRegistration {
        manifest: manifest("core-b"),
        registered_at_ms: 0,
        operation_mode: RuntimeOperationalMode::ReadOnly,
    }))
    .unwrap();
    let backup = path.join("control-plane.backup");
    control.backup_to(&backup).unwrap();
    std::fs::write(control.state_path(), b"truncated").unwrap();
    assert!(block_on(control.discover_peers(&identity("core-a"))).is_err());
    control.restore_from(&backup).unwrap();
    assert_eq!(
        block_on(control.discover_peers(&identity("core-a")))
            .unwrap()
            .peers
            .len(),
        1
    );
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn file_control_plane_rejects_state_larger_than_limit() {
    let clock = Arc::new(TestClock::new(100));
    let (control, path) = file_control_plane(clock, 1_000);
    let state = std::fs::OpenOptions::new()
        .write(true)
        .open(control.state_path())
        .unwrap();
    state.set_len(16 * 1024 * 1024 + 1).unwrap();

    assert!(matches!(
        block_on(control.discover_peers(&identity("core-a"))),
        Err(ControlPlaneError::Rejected(message)) if message.contains("configured limit")
    ));
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn control_plane_retry_storm_is_bounded_by_policy() {
    #[derive(Clone)]
    struct FailingTransport {
        attempts: Arc<AtomicU64>,
    }

    impl HttpTransport for FailingTransport {
        fn send_json(
            &self,
            _base_url: &str,
            _request: HttpControlPlaneRequest,
        ) -> ControlPlaneResult<HttpControlPlaneResponse> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            Err(ControlPlaneError::Timeout)
        }
    }

    let attempts = Arc::new(AtomicU64::new(0));
    let client = HttpControlPlaneClient::new(
        ControlPlaneHttpConfig {
            base_url: "https://control.invalid".to_string(),
            timeout_ms: 1,
            retry_policy: RetryPolicy {
                max_attempts: 3,
                initial_backoff_ms: 1,
                max_backoff_ms: 1,
            },
        },
        FailingTransport {
            attempts: Arc::clone(&attempts),
        },
    );
    let result = block_on(client.heartbeat(HeartbeatRequest {
        identity: identity("core-a"),
        operation_mode: RuntimeOperationalMode::ReadWrite,
        sent_at_ms: 1,
    }));

    assert_eq!(result, Err(ControlPlaneError::Timeout));
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}
#[test]
fn bearer_transport_debug_never_exposes_secret() {
    let secret = "control-plane-secret-value";
    let transport = BearerHttpTransport::from_secret(SecretString::new(secret));
    let output = format!("{transport:?}");
    assert!(!output.contains(secret));
    assert!(output.contains("REDACTED"));
}
