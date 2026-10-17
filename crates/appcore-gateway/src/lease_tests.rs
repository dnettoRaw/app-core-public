// =============================================================================
//        #######
//     ###       ###     F: lease_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:42:56 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 2.0.0-alpha.6
// =============================================================================

use crate::*;
use appcore_contracts::ServiceId;
use appcore_security::{CommandTokenFactory, HashTokenProvider};
use appcore_types::{
    AppFamily, AppId, CoreIdentity, NodeId, RuntimeContractVersion, RuntimeIdentity, SyncGroup,
};
use std::sync::Arc;
fn state() -> Arc<GatewayState> {
    let mut secret = vec![0; 32];
    getrandom::fill(&mut secret).unwrap();
    Arc::new(
        GatewayState::new(
            GatewayConfig::new(([127, 0, 0, 1], 0).into(), "qa.invalid"),
            HashTokenProvider::from_secret(secret).unwrap(),
        )
        .unwrap(),
    )
}
fn request() -> ServiceLeaseRequestV1 {
    ServiceLeaseRequestV1 {
        version: 1,
        identity: CoreIdentity::from_runtime_defaults(RuntimeIdentity {
            app_id: AppId::new("audit").unwrap(),
            app_family: AppFamily::new("audit").unwrap(),
            sync_group: SyncGroup::new("isolated").unwrap(),
            runtime_contract: RuntimeContractVersion::new(1),
            node_id: NodeId::new("core-a").unwrap(),
        })
        .unwrap(),
        service_id: ServiceId::new("sync.writer").unwrap(),
        action: ServiceLeaseOperationV1::Acquire { ttl_ms: 30_000 },
    }
}
fn token(state: &GatewayState, request: &ServiceLeaseRequestV1, jti: &str, now: u64) -> String {
    CommandTokenFactory::new(&state.token_provider, service_lease_token_claims())
        .create_v1_with_jti_and_hash(
            request.token_purpose(),
            Some(request.token_name()),
            None,
            Some(request.identity.core_id.as_str()),
            now,
            10_000,
            Some(jti.into()),
            Some(request.request_hash().unwrap()),
        )
        .unwrap()
}
#[test]
fn service_credentials_bind_every_scope_action_and_reject_replay() {
    let state = state();
    let replay = appcore_peer_rpc::InMemoryPeerNonceStore::default();
    let original = request();
    let now = 10_000;
    let signed = token(&state, &original, "scope-bound", now);
    let mut altered = original.clone();
    altered.service_id = ServiceId::new("update.coordinator").unwrap();
    assert!(crate::lease_service::authorize(&state, &replay, &signed, &altered, now).is_err());
    altered = original.clone();
    altered.identity.cluster_id = appcore_types::ClusterId::new("other").unwrap();
    assert!(crate::lease_service::authorize(&state, &replay, &signed, &altered, now).is_err());
    altered = original.clone();
    altered.identity.tenant_id = appcore_types::TenantId::new("other").unwrap();
    assert!(crate::lease_service::authorize(&state, &replay, &signed, &altered, now).is_err());
    altered = original.clone();
    altered.action = ServiceLeaseOperationV1::Inspect;
    assert!(crate::lease_service::authorize(&state, &replay, &signed, &altered, now).is_err());
    assert!(crate::lease_service::authorize(&state, &replay, &signed, &original, now).is_ok());
    assert!(crate::lease_service::authorize(&state, &replay, &signed, &original, now).is_err());
    let short = ServiceLeaseRequestV1 {
        action: ServiceLeaseOperationV1::Acquire { ttl_ms: 1 },
        ..original.clone()
    };
    let signed = token(&state, &short, "too-long", now);
    assert!(crate::lease_service::authorize(&state, &replay, &signed, &short, now).is_err());
}
fn http(
    address: std::net::SocketAddr,
    request: &ServiceLeaseRequestV1,
    token: Option<&str>,
) -> String {
    use std::io::{Read, Write};
    let body = serde_json::to_string(request).unwrap();
    let mut socket = std::net::TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(3)))
        .unwrap();
    write!(socket,"POST {} HTTP/1.1\r\nHost: qa.invalid\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",GATEWAY_SERVICE_LEASE_PATH,body.len()).unwrap();
    if let Some(token) = token {
        write!(socket, "Authorization: Bearer {token}\r\n").unwrap();
    }
    write!(socket, "\r\n{body}").unwrap();
    let mut response = String::new();
    socket.read_to_string(&mut response).unwrap();
    response
}
async fn start_lease_server(
    state: Arc<GatewayState>,
    authority: Arc<appcore_control_plane::FileControlPlane>,
    replay_path: &std::path::Path,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let replay = Arc::new(appcore_peer_rpc::FilePeerNonceStore::open(replay_path).unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = make_service_lease_router(state, authority, replay);
    (
        address,
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() }),
    )
}
async fn call(
    state: &GatewayState,
    address: std::net::SocketAddr,
    request: ServiceLeaseRequestV1,
    jti: &str,
) -> (
    u16,
    Option<appcore_distributed_contracts::control_plane::v1::ServiceLeaderLease>,
) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let signed = token(state, &request, jti, now);
    let response = tokio::task::spawn_blocking(move || http(address, &request, Some(&signed)))
        .await
        .unwrap();
    let status = response.split_whitespace().nth(1).unwrap().parse().unwrap();
    let lease = if status == 200 {
        serde_json::from_str(response.split("\r\n\r\n").nth(1).unwrap()).unwrap()
    } else {
        None
    };
    (status, lease)
}
#[test]
fn real_http_adapter_authenticates_recovers_lost_responses_and_fails_closed() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let state = state();
            let request = request();
            let mut nonce = [0u8; 16];
            getrandom::fill(&mut nonce).unwrap();
            let root = std::env::temp_dir().join(format!("appcore-lease-http-{nonce:x?}"));
            let authority =
                Arc::new(appcore_control_plane::FileControlPlane::open(&root, 60_000).unwrap());
            let replay_path = root.join("lease-replay.json");
            let (address, server) =
                start_lease_server(state.clone(), authority.clone(), &replay_path).await;
            let unsigned = request.clone();
            let response = tokio::task::spawn_blocking(move || http(address, &unsigned, None))
                .await
                .unwrap();
            assert!(response.starts_with("HTTP/1.1 401"));
            let (status, lease) = call(&state, address, request.clone(), "http-acquire").await;
            assert_eq!(status, 200);
            let lease = lease.unwrap();
            assert_eq!(lease.epoch, 1);
            let inspect = ServiceLeaseRequestV1 {
                action: ServiceLeaseOperationV1::Inspect,
                ..request.clone()
            };
            let (_, observed) =
                call(&state, address, inspect.clone(), "inspect-lost-acquire").await;
            assert_eq!(observed, Some(lease.clone()));
            let renew = ServiceLeaseRequestV1 {
                action: ServiceLeaseOperationV1::Renew {
                    lease: lease.clone(),
                    ttl_ms: 30_000,
                },
                ..request.clone()
            };
            let (status, renewed) = call(&state, address, renew, "http-renew").await;
            assert_eq!(status, 200);
            let renewed = renewed.unwrap();
            assert_eq!(renewed.epoch, lease.epoch);
            authority
                .with_fenced_service_write(&renewed, || Ok(()))
                .unwrap();
            server.abort();
            let _ = server.await;
            let restarted = Arc::new(
                GatewayState::new(
                    GatewayConfig::new(([127, 0, 0, 1], 0).into(), "qa.invalid"),
                    state.token_provider.clone(),
                )
                .unwrap(),
            );
            let (address, server) =
                start_lease_server(restarted.clone(), authority.clone(), &replay_path).await;
            let (_, observed) = call(
                &restarted,
                address,
                inspect.clone(),
                "inspect-after-restart",
            )
            .await;
            assert_eq!(observed, Some(renewed.clone()));
            let check = ServiceLeaseRequestV1 {
                action: ServiceLeaseOperationV1::Check {
                    lease: renewed.clone(),
                },
                ..request.clone()
            };
            let parked = root.join("state.parked");
            std::fs::rename(authority.state_path(), &parked).unwrap();
            assert_eq!(
                call(&restarted, address, check.clone(), "authority-offline")
                    .await
                    .0,
                503
            );
            std::fs::rename(&parked, authority.state_path()).unwrap();
            assert_eq!(
                call(&restarted, address, check, "authority-back").await.0,
                200
            );
            let release = ServiceLeaseRequestV1 {
                action: ServiceLeaseOperationV1::Release { lease: renewed },
                ..request
            };
            assert_eq!(call(&restarted, address, release, "release").await.0, 200);
            assert_eq!(
                call(&restarted, address, inspect, "inspect-lost-release").await,
                (200, None)
            );
            server.abort();
            let _ = server.await;
            drop(authority);
            std::fs::remove_dir_all(root).unwrap();
        });
}
#[test]
fn durable_replay_survives_a_new_store_and_gateway_instance() {
    let state = state();
    let request = request();
    let now = 10_000;
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).unwrap();
    let root = std::env::temp_dir().join(format!("appcore-lease-replay-{nonce:x?}"));
    let path = root.join("replay.json");
    let signed = token(&state, &request, "durable-jti", now);
    let store = appcore_peer_rpc::FilePeerNonceStore::open(&path).unwrap();
    crate::lease_service::authorize(&state, &store, &signed, &request, now).unwrap();
    drop(store);
    let restarted = GatewayState::new(
        GatewayConfig::new(([127, 0, 0, 1], 0).into(), "qa.invalid"),
        state.token_provider.clone(),
    )
    .unwrap();
    let store = appcore_peer_rpc::FilePeerNonceStore::open(path).unwrap();
    assert!(
        crate::lease_service::authorize(&restarted, &store, &signed, &request, now + 1).is_err()
    );
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn service_wire_fixture_revalidates_scope_and_rejects_unknown_versions() {
    let source = include_str!("../fixtures/service-lease-request-v1.json");
    let request: ServiceLeaseRequestV1 = serde_json::from_str(source).unwrap();
    request.validate().unwrap();
    let round: ServiceLeaseRequestV1 =
        serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
    assert_eq!(
        round.request_hash().unwrap(),
        request.request_hash().unwrap()
    );
    let mut value: serde_json::Value = serde_json::from_str(source).unwrap();
    value["identity"]["tenant_id"] = "tenant-a:cluster-a".into();
    assert!(serde_json::from_value::<ServiceLeaseRequestV1>(value)
        .unwrap()
        .validate()
        .is_err());
    let mut request = request;
    request.version = 2;
    assert!(
        matches!(request.validate(), Err(GatewayError::Protocol(message)) if message == "NO MORE SUPPORTED PLEASE UPDATE")
    );
}

struct SaturatedAuthority {
    entered: std::sync::mpsc::SyncSender<()>,
    release: Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
}
impl appcore_distributed_contracts::control_plane::lease_authority::ServiceLeaseAuthority
    for SaturatedAuthority
{
    fn acquire<'a>(
        &'a self,
        _: &'a CoreIdentity,
        _: &'a ServiceId,
        _: u64,
    ) -> appcore_distributed_contracts::control_plane::v1::ControlPlaneFuture<
        'a,
        appcore_distributed_contracts::control_plane::v1::ServiceLeaderLease,
    > {
        Box::pin(async move {
            self.entered.send(()).unwrap();
            let (flag, cv) = &*self.release;
            let _wait = cv
                .wait_timeout_while(
                    flag.lock().unwrap(),
                    std::time::Duration::from_secs(3),
                    |released| !*released,
                )
                .unwrap();
            Err(appcore_distributed_contracts::control_plane::v1::ControlPlaneError::Offline)
        })
    }
    fn renew<'a>(
        &'a self,
        _: &'a appcore_distributed_contracts::control_plane::v1::ServiceLeaderLease,
        _: u64,
    ) -> appcore_distributed_contracts::control_plane::v1::ControlPlaneFuture<
        'a,
        appcore_distributed_contracts::control_plane::v1::ServiceLeaderLease,
    > {
        Box::pin(async {
            Err(appcore_distributed_contracts::control_plane::v1::ControlPlaneError::Offline)
        })
    }
    fn release<'a>(
        &'a self,
        _: &'a appcore_distributed_contracts::control_plane::v1::ServiceLeaderLease,
    ) -> appcore_distributed_contracts::control_plane::v1::ControlPlaneFuture<'a, ()> {
        Box::pin(async {
            Err(appcore_distributed_contracts::control_plane::v1::ControlPlaneError::Offline)
        })
    }
    fn inspect<'a>(
        &'a self,
        _: &'a CoreIdentity,
        _: &'a ServiceId,
    ) -> appcore_distributed_contracts::control_plane::v1::ControlPlaneFuture<
        'a,
        Option<appcore_distributed_contracts::control_plane::v1::ServiceLeaderLease>,
    > {
        Box::pin(async {
            Err(appcore_distributed_contracts::control_plane::v1::ControlPlaneError::Offline)
        })
    }
    fn check<'a>(
        &'a self,
        _: &'a appcore_distributed_contracts::control_plane::v1::ServiceLeaderLease,
    ) -> appcore_distributed_contracts::control_plane::v1::ControlPlaneFuture<'a, ()> {
        Box::pin(async {
            Err(appcore_distributed_contracts::control_plane::v1::ControlPlaneError::Offline)
        })
    }
}
#[test]
fn eight_blocked_operations_leave_async_thread_live_and_reject_ninth_without_queueing() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let state = state();
            let (entered, arrivals) = std::sync::mpsc::sync_channel(8);
            let release = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
            let authority = Arc::new(SaturatedAuthority {
                entered,
                release: release.clone(),
            });
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let router = make_service_lease_router(
                state.clone(),
                authority,
                Arc::new(appcore_peer_rpc::InMemoryPeerNonceStore::default()),
            );
            let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
            let mut clients = Vec::new();
            for i in 0..8 {
                let state = state.clone();
                clients.push(tokio::spawn(async move {
                    call(&state, address, request(), &format!("blocked-{i}")).await
                }));
            }
            tokio::task::spawn_blocking(move || {
                for _ in 0..8 {
                    arrivals
                        .recv_timeout(std::time::Duration::from_secs(2))
                        .unwrap();
                }
            })
            .await
            .unwrap();
            assert_eq!(call(&state, address, request(), "ninth").await.0, 503);
            *release.0.lock().unwrap() = true;
            release.1.notify_all();
            for client in clients {
                assert_eq!(client.await.unwrap().0, 503);
            }
            server.abort();
            let _ = server.await;
        });
}
