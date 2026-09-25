// =============================================================================
//        #######
//     ###       ###     F: reload_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================
// appcore-norm: test

use super::{
    HttpApiConfig, HttpReloadPhase, HttpReloadPolicy, ReloadableRuntimeHttpHost, RuntimeHttpHost,
    RuntimeHttpReloadError, RuntimeHttpStateParts, RuntimeStaticInfo,
};
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tower::ServiceExt;

fn enabled_config(port: u16) -> HttpApiConfig {
    HttpApiConfig {
        host: "127.0.0.1".to_string(),
        port,
        enabled: true,
        max_payload_bytes: 65_536,
    }
}

fn static_info(security_ok: bool) -> RuntimeStaticInfo {
    RuntimeStaticInfo {
        app_id: "reload-test".to_string(),
        node_id: "node-reload".to_string(),
        tenant_id: "tenant-reload".to_string(),
        cluster_id: "cluster-reload".to_string(),
        core_id: "core-reload".to_string(),
        operation_mode: "read_write".to_string(),
        storage_status: "Online".to_string(),
        security_ok,
        api_enabled: true,
        sync_enabled: false,
        sync_role: "follower".to_string(),
        sync_log_len: 0,
        sync_log_path: None,
        sync_checkpoint_path: None,
        sync_peers: Vec::new(),
        sync_dns_enabled: false,
        sync_dns_seeds: Vec::new(),
        sync_dns_default_port: 39_201,
        idempotency_ttl_ms: 60_000,
        idempotency_path: None,
    }
}

fn policy() -> HttpReloadPolicy {
    HttpReloadPolicy::new(Duration::from_millis(100), Duration::from_millis(200)).unwrap()
}

fn healthy_router(label: &'static str) -> Router {
    Router::new()
        .route("/v1/health", get(|| async { StatusCode::OK }))
        .route("/work", get(move || async move { label }))
}

fn blocking_router(
    label: &'static str,
    started: Arc<AtomicBool>,
    release: Arc<AtomicBool>,
    retained_marker: Option<Arc<Vec<u8>>>,
) -> Router {
    Router::new()
        .route("/v1/health", get(|| async { StatusCode::OK }))
        .route(
            "/work",
            get(move || {
                let started = Arc::clone(&started);
                let release = Arc::clone(&release);
                let retained_marker = retained_marker.clone();
                async move {
                    started.store(true, Ordering::Release);
                    while !release.load(Ordering::Acquire) {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                    if let Some(marker) = retained_marker {
                        std::hint::black_box(marker.len());
                    }
                    label
                }
            }),
        )
}

async fn response_text(response: axum::response::Response) -> String {
    let bytes = to_bytes(response.into_body(), 1024).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[test]
fn construction_and_prepare_reject_unsafe_boundaries() {
    let disabled = RuntimeHttpHost::new(HttpApiConfig::default(), static_info(true));
    assert!(matches!(
        ReloadableRuntimeHttpHost::new(1, disabled),
        Err(RuntimeHttpReloadError::ListenerDisabled)
    ));

    let initial = RuntimeHttpHost::new(enabled_config(39001), static_info(true));
    let host = ReloadableRuntimeHttpHost::new(1, initial).unwrap();
    let stale = RuntimeHttpHost::new(enabled_config(39001), static_info(true));
    assert!(matches!(
        host.prepare(1, stale),
        Err(RuntimeHttpReloadError::StaleGeneration)
    ));
    let moved = RuntimeHttpHost::new(enabled_config(39002), static_info(true));
    assert!(matches!(
        host.prepare(2, moved),
        Err(RuntimeHttpReloadError::ListenerAddressChanged)
    ));
    let unauthenticated = RuntimeHttpHost::with_state_parts(
        enabled_config(39001),
        static_info(true),
        RuntimeHttpStateParts {
            auth: super::HttpCommandAuth::insecure_local_for_testing(),
            ..RuntimeHttpStateParts::default()
        },
    );
    assert!(matches!(
        host.prepare(2, unauthenticated),
        Err(RuntimeHttpReloadError::AuthenticationPolicyChanged)
    ));

    assert_eq!(
        HttpReloadPolicy::new(Duration::ZERO, Duration::from_secs(1)),
        Err(RuntimeHttpReloadError::InvalidPolicy)
    );
    assert_eq!(
        HttpReloadPolicy::new(Duration::from_secs(61), Duration::from_secs(1)),
        Err(RuntimeHttpReloadError::InvalidPolicy)
    );
}

#[test]
fn unhealthy_candidate_never_changes_active_generation() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let initial = RuntimeHttpHost::new(enabled_config(39003), static_info(true));
        let host = ReloadableRuntimeHttpHost::new(1, initial).unwrap();
        let candidate = RuntimeHttpHost::new(enabled_config(39003), static_info(false));
        let prepared = host.prepare(2, candidate).unwrap();

        assert_eq!(
            host.reload(prepared, policy()).await,
            Err(RuntimeHttpReloadError::HealthGateFailed(
                HttpReloadPhase::Prepare
            ))
        );
        let snapshot = host.snapshot();
        assert_eq!(snapshot.active_generation, 1);
        assert_eq!(snapshot.failed_reloads, 1);
        assert_eq!(snapshot.rollbacks, 0);
    });
}

#[test]
fn post_switch_health_failure_restores_previous_generation() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let host = ReloadableRuntimeHttpHost::new_for_test(
            1,
            enabled_config(39004),
            healthy_router("old"),
        );
        let checks = Arc::new(AtomicUsize::new(0));
        let check_state = Arc::clone(&checks);
        let candidate = Router::new().route(
            "/v1/health",
            get(move || {
                let check_state = Arc::clone(&check_state);
                async move {
                    if check_state.fetch_add(1, Ordering::AcqRel) == 0 {
                        StatusCode::OK
                    } else {
                        StatusCode::SERVICE_UNAVAILABLE
                    }
                }
            }),
        );
        let prepared = host.prepare_router_for_test(2, candidate);

        assert_eq!(
            host.reload(prepared, policy()).await,
            Err(RuntimeHttpReloadError::HealthGateFailed(
                HttpReloadPhase::Switch
            ))
        );
        let snapshot = host.snapshot();
        assert_eq!(snapshot.active_generation, 1);
        assert_eq!(snapshot.failed_reloads, 1);
        assert_eq!(snapshot.rollbacks, 1);
    });
}

#[test]
fn accepted_request_finishes_while_new_requests_use_next_generation() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let started = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let old_started = Arc::clone(&started);
        let old_release = Arc::clone(&release);
        let initial = Router::new()
            .route("/v1/health", get(|| async { StatusCode::OK }))
            .route(
                "/work",
                get(move || {
                    let started = Arc::clone(&old_started);
                    let release = Arc::clone(&old_release);
                    async move {
                        started.store(true, Ordering::Release);
                        while !release.load(Ordering::Acquire) {
                            tokio::time::sleep(Duration::from_millis(1)).await;
                        }
                        "old"
                    }
                }),
            );
        let host = Arc::new(ReloadableRuntimeHttpHost::new_for_test(
            1,
            enabled_config(39005),
            initial,
        ));
        let old_router = host.router();
        let old_request = tokio::spawn(async move {
            old_router
                .oneshot(Request::get("/work").body(Body::empty()).unwrap())
                .await
                .unwrap()
        });
        while !started.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        let prepared = host.prepare_router_for_test(2, healthy_router("new"));
        let reload_host = Arc::clone(&host);
        let reload = tokio::spawn(async move { reload_host.reload(prepared, policy()).await });
        while host.snapshot().active_generation != 2 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        let generations = host.generation_snapshot();
        assert_eq!(generations.active.generation, 2);
        assert_eq!(generations.active.inflight, 0);
        assert_eq!(generations.retained_generations, 2);
        assert_eq!(generations.max_retained_generations, 2);
        let retiring = generations.retiring.unwrap();
        assert_eq!(retiring.generation, 1);
        assert!(!retiring.accepting);
        assert_eq!(retiring.inflight, 1);
        let next = host
            .router()
            .oneshot(Request::get("/work").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response_text(next).await, "new");
        release.store(true, Ordering::Release);

        assert_eq!(response_text(old_request.await.unwrap()).await, "old");
        assert_eq!(reload.await.unwrap(), Ok(()));
        let snapshot = host.snapshot();
        assert_eq!(snapshot.active_generation, 2);
        assert_eq!(snapshot.successful_reloads, 1);
        assert_eq!(snapshot.active_inflight, 0);
        let generations = host.generation_snapshot();
        assert_eq!(generations.retained_generations, 1);
        assert!(generations.retiring.is_none());
    });
}

#[test]
fn failed_generation_is_bounded_until_its_request_releases_it() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let old_started = Arc::new(AtomicBool::new(false));
        let release_old = Arc::new(AtomicBool::new(false));
        let initial = blocking_router(
            "old",
            Arc::clone(&old_started),
            Arc::clone(&release_old),
            None,
        );
        let host = Arc::new(ReloadableRuntimeHttpHost::new_for_test(
            1,
            enabled_config(39007),
            initial,
        ));
        let old_router = host.router();
        let old_request = tokio::spawn(async move {
            old_router
                .oneshot(Request::get("/work").body(Body::empty()).unwrap())
                .await
                .unwrap()
        });
        while !old_started.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        let candidate_started = Arc::new(AtomicBool::new(false));
        let release_candidate = Arc::new(AtomicBool::new(false));
        let retained_marker = Arc::new(vec![0x5a_u8; 1024 * 1024]);
        let weak_marker = Arc::downgrade(&retained_marker);
        let candidate = blocking_router(
            "candidate",
            Arc::clone(&candidate_started),
            Arc::clone(&release_candidate),
            Some(retained_marker),
        );
        let prepared = host.prepare_router_for_test(2, candidate);
        let reload_host = Arc::clone(&host);
        let short =
            HttpReloadPolicy::new(Duration::from_millis(50), Duration::from_millis(30)).unwrap();
        let reload = tokio::spawn(async move { reload_host.reload(prepared, short).await });
        while host.snapshot().active_generation != 2 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        let candidate_router = host.router();
        let candidate_request = tokio::spawn(async move {
            candidate_router
                .oneshot(Request::get("/work").body(Body::empty()).unwrap())
                .await
                .unwrap()
        });
        while !candidate_started.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        assert_eq!(
            reload.await.unwrap(),
            Err(RuntimeHttpReloadError::RollbackDrainTimedOut)
        );
        let generations = host.generation_snapshot();
        assert_eq!(generations.active.generation, 1);
        assert_eq!(generations.retained_generations, 2);
        let retiring = generations.retiring.unwrap();
        assert_eq!(retiring.generation, 2);
        assert!(!retiring.accepting);
        assert_eq!(retiring.inflight, 1);
        assert!(weak_marker.upgrade().is_some());

        let blocked = host.prepare_router_for_test(3, healthy_router("blocked"));
        assert_eq!(
            host.reload(blocked, policy()).await,
            Err(RuntimeHttpReloadError::RetiringGenerationBusy)
        );
        assert_eq!(host.generation_snapshot().retained_generations, 2);

        release_candidate.store(true, Ordering::Release);
        assert_eq!(
            response_text(candidate_request.await.unwrap()).await,
            "candidate"
        );
        assert!(host.generation_snapshot().retiring.is_none());
        assert!(weak_marker.upgrade().is_none());

        release_old.store(true, Ordering::Release);
        assert_eq!(response_text(old_request.await.unwrap()).await, "old");
    });
}

#[test]
fn cancelled_reload_restores_previous_generation_and_releases_candidate() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let retained_marker = Arc::new(vec![0x36_u8; 1024 * 1024]);
        let weak_marker = Arc::downgrade(&retained_marker);
        let old_marker = Arc::clone(&retained_marker);
        drop(retained_marker);
        let initial = Router::new()
            .route("/v1/health", get(|| async { StatusCode::OK }))
            .route(
                "/marker",
                get(move || {
                    let marker = Arc::clone(&old_marker);
                    async move { marker.len().to_string() }
                }),
            );
        let host = Arc::new(ReloadableRuntimeHttpHost::new_for_test(
            1,
            enabled_config(39008),
            initial,
        ));

        let health_checks = Arc::new(AtomicUsize::new(0));
        let second_health_started = Arc::new(AtomicBool::new(false));
        let health_checks_route = Arc::clone(&health_checks);
        let second_health_route = Arc::clone(&second_health_started);
        let candidate = Router::new().route(
            "/v1/health",
            get(move || {
                let checks = Arc::clone(&health_checks_route);
                let second_started = Arc::clone(&second_health_route);
                async move {
                    if checks.fetch_add(1, Ordering::AcqRel) > 0 {
                        second_started.store(true, Ordering::Release);
                        std::future::pending::<()>().await;
                    }
                    StatusCode::OK
                }
            }),
        );
        let prepared = host.prepare_router_for_test(2, candidate);
        let reload_host = Arc::clone(&host);
        let reload = tokio::spawn(async move { reload_host.reload(prepared, policy()).await });
        while !second_health_started.load(Ordering::Acquire) {
            tokio::task::yield_now().await;
        }

        let generations = host.generation_snapshot();
        assert_eq!(generations.active.generation, 2);
        assert_eq!(generations.retiring.unwrap().generation, 1);
        assert!(host.snapshot().reload_in_progress);
        reload.abort();
        assert!(reload.await.unwrap_err().is_cancelled());

        let state = host.snapshot();
        assert_eq!(state.active_generation, 1);
        assert_eq!(state.failed_reloads, 1);
        assert_eq!(state.rollbacks, 1);
        assert!(!state.reload_in_progress);
        assert!(host.generation_snapshot().retiring.is_none());
        assert!(weak_marker.upgrade().is_some());

        let next = host.prepare_router_for_test(3, healthy_router("next"));
        assert_eq!(host.reload(next, policy()).await, Ok(()));
        assert!(weak_marker.upgrade().is_none());
    });
}

#[test]
fn drain_timeout_rolls_back_and_stale_generation_cannot_take_over() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let started = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let old_started = Arc::clone(&started);
        let old_release = Arc::clone(&release);
        let initial = Router::new()
            .route("/v1/health", get(|| async { StatusCode::OK }))
            .route(
                "/work",
                get(move || {
                    let started = Arc::clone(&old_started);
                    let release = Arc::clone(&old_release);
                    async move {
                        started.store(true, Ordering::Release);
                        while !release.load(Ordering::Acquire) {
                            tokio::time::sleep(Duration::from_millis(1)).await;
                        }
                        "old"
                    }
                }),
            );
        let host = ReloadableRuntimeHttpHost::new_for_test(4, enabled_config(39006), initial);
        let old_request = host
            .router()
            .oneshot(Request::get("/work").body(Body::empty()).unwrap());
        let old_request = tokio::spawn(old_request);
        while !started.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        let prepared = host.prepare_router_for_test(5, healthy_router("new"));
        let short =
            HttpReloadPolicy::new(Duration::from_millis(50), Duration::from_millis(10)).unwrap();
        assert_eq!(
            host.reload(prepared, short).await,
            Err(RuntimeHttpReloadError::DrainTimedOut)
        );
        assert_eq!(host.snapshot().active_generation, 4);
        let stale = host.prepare_router_for_test(4, healthy_router("stale"));
        assert_eq!(
            host.reload(stale, policy()).await,
            Err(RuntimeHttpReloadError::StaleGeneration)
        );

        release.store(true, Ordering::Release);
        assert_eq!(
            response_text(old_request.await.unwrap().unwrap()).await,
            "old"
        );
    });
}

#[test]
fn bound_listener_keeps_accepted_request_during_routing_switch() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let started = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let old_started = Arc::clone(&started);
    let old_release = Arc::clone(&release);
    let initial = Router::new()
        .route("/v1/health", get(|| async { StatusCode::OK }))
        .route(
            "/work",
            get(move || {
                let started = Arc::clone(&old_started);
                let release = Arc::clone(&old_release);
                async move {
                    started.store(true, Ordering::Release);
                    while !release.load(Ordering::Acquire) {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                    "old"
                }
            }),
        );
    let host = Arc::new(ReloadableRuntimeHttpHost::new_for_test(
        1,
        enabled_config(address.port()),
        initial,
    ));
    let shutdown = Arc::new(AtomicBool::new(false));
    let server_host = Arc::clone(&host);
    let server_shutdown = Arc::clone(&shutdown);
    let server = thread::spawn(move || {
        server_host.run_on_listener_until_shutdown(listener, server_shutdown)
    });
    wait_for_listener(address);

    let old_request = thread::spawn(move || http_get(address, "/work"));
    while !started.load(Ordering::Acquire) {
        thread::sleep(Duration::from_millis(1));
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let prepared = host.prepare_router_for_test(2, healthy_router("new"));
    let reload_host = Arc::clone(&host);
    let release_after_switch = Arc::clone(&release);
    let reload = thread::spawn(move || {
        runtime.block_on(async move { reload_host.reload(prepared, policy()).await })
    });
    while host.snapshot().active_generation != 2 {
        thread::sleep(Duration::from_millis(1));
    }

    assert!(http_get(address, "/work").ends_with("new"));
    release_after_switch.store(true, Ordering::Release);
    assert!(old_request.join().unwrap().ends_with("old"));
    assert_eq!(reload.join().unwrap(), Ok(()));
    shutdown.store(true, Ordering::Release);
    assert!(server.join().unwrap().is_ok());
}

fn wait_for_listener(address: SocketAddr) {
    for _ in 0..200 {
        if TcpStream::connect_timeout(&address, Duration::from_millis(10)).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(1));
    }
    panic!("listener did not become ready");
}

fn http_get(address: SocketAddr, path: &str) -> String {
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(1)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}
