// =============================================================================
//        #######
//     ###       ###     F: federated_route_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.4-rc
// =============================================================================
// appcore-norm: test

use crate::connection::CONNECTION_BUFFER_CAPACITY;
use crate::ha::coordinator::tests::TestProvider;
use crate::*;
use appcore_contracts::InstallationId;
use appcore_distributed_contracts::PeerRpcEnvelope;
use appcore_peer_rpc::{
    envelope_signing_hash, BoundedReplayStore, PeerRpcHttpRequest, PeerRpcHttpResponse,
    ReplayStoreConfig, PEER_QUERY_PATH,
};
use appcore_security::{CommandTokenFactory, HashTokenProvider};
use appcore_types::{CapabilityName, ClusterId, CoreId, InstanceId, TenantId};
use axum::extract::ws::Message;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

#[tokio::test]
async fn two_gateway_states_route_one_fenced_request_to_remote_socket_owner() {
    let provider: Arc<dyn GatewayRegistryProvider> = Arc::new(TestProvider::default());
    run_two_gateway_states(
        Arc::clone(&provider),
        provider,
        None,
        Duration::from_secs(30),
    )
    .await;
}

#[tokio::test]
#[ignore = "requires APPCORE_GATEWAY_REDIS_URL and APPCORE_GATEWAY_REDIS_CREDENTIAL"]
async fn real_redis_routes_one_fenced_request_between_two_gateway_states() {
    let Some((endpoint, credential)) = redis_environment() else {
        return;
    };
    let namespace = format!("appcore-federation-e2e-{}-{}", std::process::id(), now_ms());
    let config = RedisGatewayRegistryConfig::new(&endpoint, namespace, 2_000, 8).unwrap();
    let origin: Arc<dyn GatewayRegistryProvider> = Arc::new(
        RedisGatewayRegistryProvider::connect(
            config.clone(),
            RedisGatewayCredential::new(Zeroizing::new(credential.clone())).unwrap(),
        )
        .await
        .unwrap(),
    );
    let target: Arc<dyn GatewayRegistryProvider> = Arc::new(
        RedisGatewayRegistryProvider::connect(
            config,
            RedisGatewayCredential::new(Zeroizing::new(credential)).unwrap(),
        )
        .await
        .unwrap(),
    );
    run_two_gateway_states(origin, target, None, Duration::from_secs(30)).await;
}

#[tokio::test]
#[ignore = "requires Redis plus APPCORE_GATEWAY_TARGET_BIND and APPCORE_GATEWAY_FEDERATION_PROXY_URL"]
async fn real_redis_and_external_proxy_route_between_two_gateway_states() {
    let Some((endpoint, credential, target_bind, proxy_url)) = proxy_environment() else {
        return;
    };
    let namespace = format!("appcore-proxy-e2e-{}-{}", std::process::id(), now_ms());
    let config = RedisGatewayRegistryConfig::new(&endpoint, namespace, 2_000, 8).unwrap();
    let origin = redis_provider(config.clone(), credential.clone()).await;
    let target = redis_provider(config, credential).await;
    run_two_gateway_states(
        origin,
        target,
        Some((target_bind, proxy_url)),
        Duration::from_secs(30),
    )
    .await;
}

#[tokio::test]
#[ignore = "requires APPCORE_GATEWAY_REDIS_URL and APPCORE_GATEWAY_REDIS_CREDENTIAL"]
async fn real_redis_recovers_higher_epochs_after_ungraceful_owner_loss() {
    let Some((endpoint, credential)) = redis_environment() else {
        return;
    };
    let namespace = format!("appcore-failover-e2e-{}-{}", std::process::id(), now_ms());
    let config = RedisGatewayRegistryConfig::new(&endpoint, namespace, 2_000, 8).unwrap();
    let origin = redis_provider(config.clone(), credential.clone()).await;
    let target = redis_provider(config, credential).await;
    let lease_ttl = Duration::from_secs(1);
    let first_epoch =
        run_two_gateway_states(Arc::clone(&origin), Arc::clone(&target), None, lease_ttl).await;
    let recovery_started = Instant::now();
    tokio::time::sleep(Duration::from_millis(1_200)).await;
    let recovered_epoch = run_two_gateway_states(origin, target, None, lease_ttl).await;
    assert!(recovered_epoch > first_epoch);
    assert!(recovery_started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
#[ignore = "requires Redis plus APPCORE_GATEWAY_TARGET_BIND and APPCORE_GATEWAY_FEDERATION_PROXY_URL"]
async fn real_redis_and_external_proxy_recover_after_ungraceful_owner_loss() {
    let Some((endpoint, credential, target_bind, proxy_url)) = proxy_environment() else {
        return;
    };
    let namespace = format!("appcore-proxy-failover-{}-{}", std::process::id(), now_ms());
    let config = RedisGatewayRegistryConfig::new(&endpoint, namespace, 2_000, 8).unwrap();
    let origin = redis_provider(config.clone(), credential.clone()).await;
    let target = redis_provider(config, credential).await;
    let lease_ttl = Duration::from_secs(1);
    let proxy = Some((target_bind, proxy_url));
    let first_epoch = run_two_gateway_states(
        Arc::clone(&origin),
        Arc::clone(&target),
        proxy.clone(),
        lease_ttl,
    )
    .await;
    let recovery_started = Instant::now();
    tokio::time::sleep(Duration::from_millis(1_200)).await;
    let recovered_epoch = run_two_gateway_states(origin, target, proxy, lease_ttl).await;
    assert!(recovered_epoch > first_epoch);
    assert!(recovery_started.elapsed() < Duration::from_secs(5));
}

async fn run_two_gateway_states(
    origin_provider: Arc<dyn GatewayRegistryProvider>,
    target_provider: Arc<dyn GatewayRegistryProvider>,
    proxy: Option<(String, String)>,
    lease_ttl: Duration,
) -> u64 {
    let bind = proxy
        .as_ref()
        .map_or("127.0.0.1:0", |value| value.0.as_str());
    let listener = tokio::net::TcpListener::bind(bind).await.unwrap();
    let target_address = listener.local_addr().unwrap();
    let target_url = proxy
        .map(|value| value.1)
        .unwrap_or_else(|| format!("http://{target_address}"));
    let target_coordinator = Arc::new(coordinator(
        target_provider,
        "gateway-target",
        target_url,
        lease_ttl,
    ));
    let origin_coordinator = Arc::new(coordinator(
        origin_provider,
        "gateway-origin",
        "http://127.0.0.1:9".to_string(),
        lease_ttl,
    ));
    let token_provider = random_token_provider();
    let target_state = state(Arc::clone(&target_coordinator), token_provider.clone());
    let origin_state = state(Arc::clone(&origin_coordinator), token_provider);
    let now = now_ms();
    target_coordinator
        .recover(target_state.as_ref(), now)
        .await
        .unwrap();
    origin_coordinator
        .recover(origin_state.as_ref(), now)
        .await
        .unwrap();
    let RemoteWorkerFixture {
        tenant,
        cluster,
        core,
        capability,
        worker,
        receiver,
    } = install_remote_worker(&target_coordinator, &target_state, now).await;
    let server_state = Arc::clone(&target_state);
    let server = tokio::spawn(async move {
        axum::serve(listener, make_gateway_router(server_state))
            .await
            .unwrap();
    });
    let worker_task =
        spawn_worker_response(Arc::clone(&target_state), tenant.clone(), worker, receiver);
    let request = federated_request(&origin_state, &tenant, cluster, core, capability, now);
    let response = EnvelopeRouter::route_mesh_request(
        Arc::clone(&origin_state),
        request,
        Duration::from_secs(5),
    )
    .await
    .into_peer_response()
    .unwrap();

    assert_eq!(response.body, b"remote-ok");
    assert_eq!(origin_coordinator.snapshot().request_claims, 1);
    assert_eq!(origin_coordinator.snapshot().request_completions, 1);
    assert_eq!(origin_coordinator.snapshot().remote_forwards, 1);
    let target_epoch = target_coordinator.lease_for(&tenant).unwrap().epoch();
    worker_task.await.unwrap();
    server.abort();
    let _ = server.await;
    target_epoch
}

struct RemoteWorkerFixture {
    tenant: TenantId,
    cluster: ClusterId,
    core: CoreId,
    capability: CapabilityName,
    worker: WorkerConnection,
    receiver: mpsc::Receiver<Message>,
}

async fn install_remote_worker(
    coordinator: &GatewayHaCoordinator,
    state: &GatewayState,
    now: u64,
) -> RemoteWorkerFixture {
    let tenant = TenantId::new("tenant-a").unwrap();
    let cluster = ClusterId::new("cluster-a").unwrap();
    let core = CoreId::new("core-remote").unwrap();
    let capability = CapabilityName::new("runtime.query").unwrap();
    let installation = InstallationId::new("install-remote").unwrap();
    let (sender, receiver) = mpsc::channel(CONNECTION_BUFFER_CAPACITY);
    let worker = WorkerConnection::new_in_cluster(
        WorkerConnectionKey {
            tenant_id: tenant.clone(),
            installation_id: installation.clone(),
            core_id: core.clone(),
        },
        cluster.clone(),
        sender,
        now,
    );
    coordinator
        .register_worker(
            &tenant,
            &cluster,
            GatewayWorkerRegistration::new(
                installation,
                core.clone(),
                worker.generation(),
                vec![capability.clone()],
            )
            .unwrap(),
            now,
        )
        .await
        .unwrap();
    state
        .tenant_partition_or_insert(&tenant)
        .unwrap()
        .write()
        .add_worker(worker.clone(), vec![capability.clone()])
        .unwrap();
    RemoteWorkerFixture {
        tenant,
        cluster,
        core,
        capability,
        worker,
        receiver,
    }
}

fn spawn_worker_response(
    state: Arc<GatewayState>,
    tenant: TenantId,
    worker: WorkerConnection,
    mut receiver: mpsc::Receiver<Message>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Some(Message::Text(payload)) = receiver.recv().await else {
            panic!("federated worker did not receive a text request");
        };
        let request: MeshPeerRequest = serde_json::from_str(&payload).unwrap();
        EnvelopeRouter::handle_worker_mesh_response_from(
            state,
            &tenant,
            &worker,
            MeshPeerResponse::ok(
                request.request_id,
                PeerRpcHttpResponse {
                    status_code: 200,
                    body: b"remote-ok".to_vec(),
                },
            ),
        )
        .unwrap();
    })
}

fn federated_request(
    state: &GatewayState,
    tenant: &TenantId,
    cluster: ClusterId,
    core: CoreId,
    capability: CapabilityName,
    now: u64,
) -> MeshPeerRequest {
    let envelope = PeerRpcEnvelope::new(
        "federated-request",
        "federated-trace",
        CoreId::new("core-origin").unwrap(),
        core.clone(),
        tenant.clone(),
        cluster,
        now,
        now + 30_000,
        "federated-inner-nonce",
        capability,
        b"opaque".to_vec(),
        None,
        None,
    );
    let inner_hash = envelope_signing_hash(&envelope);
    let inner_credential = CommandTokenFactory::new(&state.token_provider, gateway_token_claims())
        .create_v1_with_jti_and_hash(
            "peer",
            None,
            None,
            None,
            now,
            30_000,
            Some(format!("inner-{}", envelope.request_id)),
            Some(inner_hash),
        )
        .unwrap();
    MeshPeerRequest::new(
        "federated-request",
        tenant.clone(),
        core,
        PeerRpcHttpRequest {
            method: "POST".to_string(),
            path: PEER_QUERY_PATH.to_string(),
            body: serde_json::to_vec(&envelope).unwrap(),
            bearer_token: Some(inner_credential),
            timeout_ms: 5_000,
            max_response_bytes: 4_096,
        },
    )
}

async fn redis_provider(
    config: RedisGatewayRegistryConfig,
    credential: String,
) -> Arc<dyn GatewayRegistryProvider> {
    Arc::new(
        RedisGatewayRegistryProvider::connect(
            config,
            RedisGatewayCredential::new(Zeroizing::new(credential)).unwrap(),
        )
        .await
        .unwrap(),
    )
}

fn coordinator(
    provider: Arc<dyn GatewayRegistryProvider>,
    instance: &str,
    federation_url: String,
    lease_ttl: Duration,
) -> GatewayHaCoordinator {
    GatewayHaCoordinator::new(
        provider,
        Arc::new(GatewayHaLifecycle::new()),
        GatewayHaCoordinatorConfig {
            instance_id: InstanceId::new(instance).unwrap(),
            federation_url: GatewayFederationUrl::new(federation_url).unwrap(),
            tenants: vec![GatewayHaTenantBinding {
                tenant_id: TenantId::new("tenant-a").unwrap(),
                cluster_id: ClusterId::new("cluster-a").unwrap(),
            }],
            lease_ttl,
            renewal_interval: lease_ttl / 4,
        },
    )
    .unwrap()
}

fn random_token_provider() -> HashTokenProvider {
    let mut secret = [0u8; 32];
    getrandom::getrandom(&mut secret).unwrap();
    HashTokenProvider::from_secret(secret.to_vec()).unwrap()
}

fn redis_environment() -> Option<(String, String)> {
    match (
        std::env::var("APPCORE_GATEWAY_REDIS_URL"),
        std::env::var("APPCORE_GATEWAY_REDIS_CREDENTIAL"),
    ) {
        (Ok(endpoint), Ok(credential)) => Some((endpoint, credential)),
        (Err(std::env::VarError::NotPresent), Err(std::env::VarError::NotPresent)) => None,
        _ => panic!("Redis Gateway test environment is incomplete or invalid"),
    }
}

fn proxy_environment() -> Option<(String, String, String, String)> {
    let redis_url = std::env::var("APPCORE_GATEWAY_REDIS_URL");
    let redis_credential = std::env::var("APPCORE_GATEWAY_REDIS_CREDENTIAL");
    let target_bind = std::env::var("APPCORE_GATEWAY_TARGET_BIND");
    let proxy_url = std::env::var("APPCORE_GATEWAY_FEDERATION_PROXY_URL");
    match (redis_url, redis_credential, target_bind, proxy_url) {
        (Ok(endpoint), Ok(credential), Ok(bind), Ok(proxy)) => {
            Some((endpoint, credential, bind, proxy))
        }
        (
            Err(std::env::VarError::NotPresent),
            Err(std::env::VarError::NotPresent),
            Err(std::env::VarError::NotPresent),
            Err(std::env::VarError::NotPresent),
        ) => None,
        _ => panic!("Gateway proxy test environment is incomplete or invalid"),
    }
}

fn state(
    coordinator: Arc<GatewayHaCoordinator>,
    token_provider: HashTokenProvider,
) -> Arc<GatewayState> {
    Arc::new(
        GatewayState::with_ha_coordinator(
            GatewayConfig::new(([127, 0, 0, 1], 0).into(), "gateway.test"),
            token_provider,
            Arc::new(BoundedReplayStore::new(ReplayStoreConfig::default())),
            coordinator,
        )
        .unwrap(),
    )
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
