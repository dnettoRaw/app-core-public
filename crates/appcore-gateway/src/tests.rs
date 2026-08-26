// =============================================================================
//        #######
//     ###       ###     F: tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/26 08:53:09 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:48:56 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

//! Unit and integration tests for AppCore Gateway.

use super::*;
use crate::connection::CONNECTION_BUFFER_CAPACITY;
use appcore_contracts::{InstallationId, ProviderConfig, ProviderId};
use appcore_distributed_contracts::{PeerRpcEnvelope, PeerRpcResponse};
use appcore_peer_rpc::{PeerRpcHttpRequest, PeerRpcHttpResponse};
use appcore_security::HashTokenProvider;
use appcore_types::{CapabilityName, ClusterId, CoreId, TenantId};
use axum::extract::ws::Message;
use axum::http::HeaderMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

mod registry;

const TEST_DOMAIN: &str = "gateway.test.local";

fn mock_state() -> Arc<GatewayState> {
    let provider = HashTokenProvider::from_secret(vec![0; 32]).unwrap();
    let config = GatewayConfig::new(([127, 0, 0, 1], 8080).into(), TEST_DOMAIN);
    Arc::new(GatewayState::new(config, provider).unwrap())
}

fn pending_route_fixture() -> (
    Arc<GatewayState>,
    TenantId,
    mpsc::Receiver<Message>,
    PeerRpcEnvelope,
) {
    let state = mock_state();
    let tenant = TenantId::new("tenant-pending-route").unwrap();
    let cluster = ClusterId::new("cluster-pending-route").unwrap();
    let capability = CapabilityName::new("runtime.pending-route").unwrap();
    let (tx, rx) = mpsc::channel(CONNECTION_BUFFER_CAPACITY);
    let worker = WorkerConnection::new_in_cluster(
        WorkerConnectionKey {
            tenant_id: tenant.clone(),
            installation_id: InstallationId::new("installation-pending-route").unwrap(),
            core_id: CoreId::new("core-pending-route").unwrap(),
        },
        cluster.clone(),
        tx,
        1000,
    );
    state
        .tenant_partition_or_insert(&tenant)
        .unwrap()
        .write()
        .add_worker(worker, vec![capability.clone()])
        .unwrap();
    let now = test_now_ms();
    let envelope = PeerRpcEnvelope::new(
        "pending-route",
        "trace-pending-route",
        CoreId::new("source-pending-route").unwrap(),
        CoreId::new("core-pending-route").unwrap(),
        tenant.clone(),
        cluster,
        now,
        now + 30_000,
        "nonce-pending-route",
        capability,
        Vec::new(),
        None,
        None,
    );
    (state, tenant, rx, envelope)
}

#[test]
fn tenant_partitions_do_not_share_their_state_lock() {
    let state = mock_state();
    let tenant_a = TenantId::new("tenant-lock-a").unwrap();
    let tenant_b = TenantId::new("tenant-lock-b").unwrap();
    let partition_a = state.tenant_partition_or_insert(&tenant_a).unwrap();
    state.tenant_partition_or_insert(&tenant_b).unwrap();
    let held_a = partition_a.write();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let state_for_b = Arc::clone(&state);

    let worker = std::thread::spawn(move || {
        let partition_b = state_for_b.tenant_partition(&tenant_b).unwrap();
        let _tenant_b = partition_b.write();
        ready_tx.send(()).unwrap();
    });

    assert!(ready_rx.recv_timeout(Duration::from_secs(1)).is_ok());
    drop(held_a);
    worker.join().unwrap();
}

#[test]
fn tenant_directory_enforces_its_global_capacity() {
    let state = mock_state();
    for index in 0..crate::config::MAX_GATEWAY_TENANTS {
        let tenant = TenantId::new(format!("bounded-tenant-{index}")).unwrap();
        assert!(state.tenant_partition_or_insert(&tenant).is_ok());
    }
    assert_eq!(state.tenant_count(), crate::config::MAX_GATEWAY_TENANTS);
    let overflow = TenantId::new("bounded-tenant-overflow").unwrap();
    assert!(state.tenant_partition_or_insert(&overflow).is_err());
}

#[test]
fn gateway_configuration_is_authenticated_by_default() {
    let loopback = GatewayConfig::new(([127, 0, 0, 1], 8080).into(), TEST_DOMAIN);
    assert!(loopback.requires_authentication());
    assert!(loopback.validate().is_ok());

    let public = GatewayConfig::new(([0, 0, 0, 0], 8080).into(), TEST_DOMAIN);
    assert!(public.requires_authentication());
    assert!(public.validate().is_ok());
}

#[test]
fn shutdown_request_is_retained_before_tasks_subscribe() {
    let state = mock_state();

    state.request_shutdown();

    assert!(state.is_shutting_down());
}

#[test]
fn gateway_insecure_test_mode_is_restricted_to_loopback() {
    let public = GatewayConfig::new(([0, 0, 0, 0], 8080).into(), TEST_DOMAIN);
    assert!(public.insecure_local_for_testing().is_err());

    let local = GatewayConfig::new(([127, 0, 0, 1], 8080).into(), TEST_DOMAIN)
        .insecure_local_for_testing()
        .unwrap();
    assert!(!local.requires_authentication());
    assert!(local.validate().is_ok());

    let mut rebound = local;
    rebound.bind_address = ([0, 0, 0, 0], 8080).into();
    let provider = HashTokenProvider::from_secret(vec![1; 32]).unwrap();
    assert!(GatewayState::new(rebound, provider).is_err());
}

#[test]
fn deployment_adapter_parses_only_authenticated_gateway_settings() {
    let provider = ProviderConfig::new(ProviderId::new(GATEWAY_PROVIDER_ID).unwrap())
        .with_setting("bind_address", "127.0.0.1:8080")
        .unwrap()
        .with_setting("domain_suffix", TEST_DOMAIN)
        .unwrap()
        .with_setting("heartbeat_interval_ms", "2000")
        .unwrap()
        .with_setting("heartbeat_timeout_ms", "5000")
        .unwrap();

    let config = GatewayConfig::from_provider_config(&provider).unwrap();

    assert!(config.requires_authentication());
    assert_eq!(config.heartbeat_interval, Duration::from_secs(2));
    assert_eq!(config.heartbeat_timeout, Duration::from_secs(5));
}

#[test]
fn deployment_adapter_rejects_security_downgrade_and_invalid_bind() {
    let insecure = ProviderConfig::new(ProviderId::new(GATEWAY_PROVIDER_ID).unwrap())
        .with_setting("bind_address", "127.0.0.1:8080")
        .unwrap()
        .with_setting("domain_suffix", TEST_DOMAIN)
        .unwrap()
        .with_setting("auth", "false")
        .unwrap();
    assert!(GatewayConfig::from_provider_config(&insecure).is_err());

    let invalid = ProviderConfig::new(ProviderId::new(GATEWAY_PROVIDER_ID).unwrap())
        .with_setting("bind_address", "not-an-address")
        .unwrap()
        .with_setting("domain_suffix", TEST_DOMAIN)
        .unwrap();
    assert!(GatewayConfig::from_provider_config(&invalid).is_err());
}

#[test]
fn runtime_gateway_descriptor_is_tenant_stream_infrastructure() {
    let descriptor = gateway_capability_descriptor().unwrap();

    assert_eq!(descriptor.name.as_str(), GATEWAY_RUNTIME_CAPABILITY);
    assert_eq!(descriptor.mode, appcore_types::CapabilityMode::Stream);
    assert_eq!(
        descriptor.visibility,
        appcore_types::CapabilityVisibility::Tenant
    );
    assert!(!descriptor.requirements.read_only);
}

fn test_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[tokio::test]
async fn test_mesh_relay_routes_http_request_to_target_core() {
    let state = mock_state();
    let tenant_a = TenantId::new("tenant-a").unwrap();
    let inst_a = InstallationId::new("inst-a").unwrap();
    let core_a = CoreId::new("core-a").unwrap();

    let (tx, mut rx) = mpsc::channel(CONNECTION_BUFFER_CAPACITY);
    let key = WorkerConnectionKey {
        tenant_id: tenant_a.clone(),
        installation_id: inst_a.clone(),
        core_id: core_a.clone(),
    };
    let worker =
        WorkerConnection::new_in_cluster(key, ClusterId::new("cluster-a").unwrap(), tx, 1000);

    {
        state
            .tenant_partition_or_insert(&tenant_a)
            .unwrap()
            .write()
            .add_worker(worker, Vec::new())
            .unwrap();
    }

    let request = MeshPeerRequest::new(
        "mesh-req-1",
        tenant_a.clone(),
        core_a.clone(),
        PeerRpcHttpRequest {
            method: "GET".to_string(),
            path: "/v1/peer/health".to_string(),
            body: Vec::new(),
            bearer_token: Some("relay-token".to_string()),
            timeout_ms: 1_000,
            max_response_bytes: 4_096,
        },
    );

    let route_state = state.clone();
    let route_task = tokio::spawn(async move {
        EnvelopeRouter::route_mesh_request(route_state, request, Duration::from_secs(5)).await
    });

    let msg = rx.recv().await.unwrap();
    let Message::Text(text) = msg else {
        panic!("Expected mesh request text");
    };
    let routed = serde_json::from_str::<MeshPeerRequest>(&text).unwrap();
    assert_eq!(routed.request_id, "mesh-req-1");
    assert_eq!(routed.target_tenant_id, tenant_a);
    assert_eq!(routed.target_core_id, core_a);
    assert_eq!(routed.bearer_token.as_deref(), Some("relay-token"));

    let response = MeshPeerResponse::ok(
        "mesh-req-1",
        PeerRpcHttpResponse {
            status_code: 200,
            body: br#"{"ok":true}"#.to_vec(),
        },
    );
    EnvelopeRouter::handle_worker_mesh_response(state.clone(), &tenant_a, response).unwrap();

    let response = route_task.await.unwrap();
    assert_eq!(response.status_code, 200);
    assert_eq!(response.body, br#"{"ok":true}"#);
}

#[test]
fn test_mesh_request_debug_redacts_bearer_token() {
    let request = MeshPeerRequest::new(
        "mesh-req-1",
        TenantId::new("tenant-a").unwrap(),
        CoreId::new("core-a").unwrap(),
        PeerRpcHttpRequest {
            method: "POST".to_string(),
            path: "/v1/peer/query".to_string(),
            body: Vec::new(),
            bearer_token: Some("sensitive-token".to_string()),
            timeout_ms: 1_000,
            max_response_bytes: 4_096,
        },
    );

    let output = format!("{request:?}");
    assert!(output.contains("REDACTED"));
    assert!(!output.contains("sensitive-token"));

    let response = MeshPeerResponse {
        schema: MESH_HTTP_SCHEMA_V1.to_string(),
        request_id: "mesh-req-1".to_string(),
        status_code: 500,
        body: b"sensitive-token".to_vec(),
        error: Some("sensitive-token".to_string()),
    };
    assert!(!format!("{response:?}").contains("sensitive-token"));
}

#[test]
fn connection_params_debug_redacts_query_credentials() {
    let params = service::ConnectionParams {
        tenant: Some("tenant-a".to_string()),
        cluster: Some("cluster-a".to_string()),
        installation: None,
        core: None,
        device: None,
        token: Some("secret-marker-must-not-appear".to_string()),
        capabilities: None,
    };
    let output = format!("{params:?}");
    assert!(output.contains("REDACTED"));
    assert!(!output.contains("secret-marker-must-not-appear"));
}

#[tokio::test]
async fn test_tenant_resolution_from_hostname() {
    let mut headers = HeaderMap::new();
    headers.insert("host", "tenant-a.gateway.test.local:8080".parse().unwrap());

    let params = service::ConnectionParams {
        tenant: None,
        cluster: None,
        installation: None,
        core: None,
        device: None,
        token: None,
        capabilities: None,
    };

    let resolved = service::resolve_tenant(&headers, &params, TEST_DOMAIN);
    assert_eq!(resolved, Some(TenantId::new("tenant-a").unwrap()));
}

#[tokio::test]
async fn test_tenant_resolution_fallback_to_query() {
    let headers = HeaderMap::new();
    let params = service::ConnectionParams {
        tenant: Some("tenant-b".to_string()),
        cluster: None,
        installation: None,
        core: None,
        device: None,
        token: None,
        capabilities: None,
    };

    let resolved = service::resolve_tenant(&headers, &params, TEST_DOMAIN);
    assert_eq!(resolved, Some(TenantId::new("tenant-b").unwrap()));
}

#[tokio::test]
async fn test_multi_tenant_worker_routing() {
    let state = mock_state();
    let tenant_a = TenantId::new("tenant-a").unwrap();
    let tenant_b = TenantId::new("tenant-b").unwrap();

    let inst_a = InstallationId::new("inst-a").unwrap();
    let core_a = CoreId::new("core-a").unwrap();
    let cluster_a = ClusterId::new("cluster-a").unwrap();
    let capability = CapabilityName::new("compute").unwrap();
    let now = test_now_ms();

    let (tx, mut rx) = mpsc::channel(CONNECTION_BUFFER_CAPACITY);

    let key = WorkerConnectionKey {
        tenant_id: tenant_a.clone(),
        installation_id: inst_a.clone(),
        core_id: core_a.clone(),
    };
    let worker_conn = WorkerConnection::new_in_cluster(key, cluster_a.clone(), tx, 1000);

    // Register worker under Tenant A
    {
        let tenant_state = state.tenant_partition_or_insert(&tenant_a).unwrap();
        let mut tenant_state = tenant_state.write();
        tenant_state
            .add_worker(worker_conn, vec![capability.clone()])
            .unwrap();
    }

    // Attempt to route envelope targeting Tenant B (should fail boundary validation / find no worker)
    let envelope_b = PeerRpcEnvelope::new(
        "req-1",
        "trace-1",
        CoreId::new("source").unwrap(),
        core_a.clone(),
        tenant_b.clone(),
        cluster_a.clone(),
        now,
        now + 30_000,
        "nonce-1",
        capability.clone(),
        vec![],
        None,
        None,
    );

    let response =
        EnvelopeRouter::route_request(state.clone(), envelope_b, Duration::from_secs(1)).await;
    assert!(!response.ok);
    assert!(response
        .error
        .unwrap()
        .contains("compatible_worker_unavailable"));

    // Route envelope targeting Tenant A (should succeed and forward request to worker channel)
    let envelope_a = PeerRpcEnvelope::new(
        "req-2",
        "trace-2",
        CoreId::new("source").unwrap(),
        core_a.clone(),
        tenant_a.clone(),
        cluster_a,
        now,
        now + 30_000,
        "nonce-2",
        capability.clone(),
        vec![],
        None,
        None,
    );

    let state_clone = state.clone();
    let route_task = tokio::spawn(async move {
        EnvelopeRouter::route_request(state_clone, envelope_a, Duration::from_secs(5)).await
    });

    // Worker receives the message from WebSocket channel
    let msg = rx.recv().await.unwrap();
    if let Message::Text(text) = msg {
        let routed_envelope = serde_json::from_str::<PeerRpcEnvelope>(&text).unwrap();
        assert_eq!(routed_envelope.request_id, "req-2");

        // Worker sends the response back to gateway
        let response = PeerRpcResponse::ok("req-2", vec![42]);
        EnvelopeRouter::handle_worker_response(state.clone(), &tenant_a, response).unwrap();
    } else {
        panic!("Expected text message");
    }

    let response = route_task.await.unwrap();
    assert!(response.ok);
    assert_eq!(response.payload, vec![42]);
}

#[tokio::test]
async fn routing_uses_cluster_and_core_worker_index() {
    let state = mock_state();
    let tenant = TenantId::new("tenant-cluster-index").unwrap();
    let core = CoreId::new("core-shared-index").unwrap();
    let cluster_a = ClusterId::new("cluster-index-a").unwrap();
    let cluster_b = ClusterId::new("cluster-index-b").unwrap();
    let capability = CapabilityName::new("runtime.indexed-route").unwrap();
    let (tx_a, mut rx_a) = mpsc::channel(CONNECTION_BUFFER_CAPACITY);
    let worker_a = WorkerConnection::new_in_cluster(
        WorkerConnectionKey {
            tenant_id: tenant.clone(),
            installation_id: InstallationId::new("installation-index-a").unwrap(),
            core_id: core.clone(),
        },
        cluster_a.clone(),
        tx_a,
        1,
    );
    let (tx_b, mut rx_b) = mpsc::channel(CONNECTION_BUFFER_CAPACITY);
    let worker_b = WorkerConnection::new_in_cluster(
        WorkerConnectionKey {
            tenant_id: tenant.clone(),
            installation_id: InstallationId::new("installation-index-b").unwrap(),
            core_id: core.clone(),
        },
        cluster_b,
        tx_b,
        1,
    );
    {
        let partition = state.tenant_partition_or_insert(&tenant).unwrap();
        let mut partition = partition.write();
        partition
            .add_worker(worker_a.clone(), vec![capability.clone()])
            .unwrap();
        partition
            .add_worker(worker_b, vec![capability.clone()])
            .unwrap();
    }
    let now = test_now_ms();
    let envelope = PeerRpcEnvelope::new(
        "cluster-index-request",
        "cluster-index-trace",
        CoreId::new("source-index").unwrap(),
        core,
        tenant.clone(),
        cluster_a,
        now,
        now + 30_000,
        "cluster-index-nonce",
        capability,
        Vec::new(),
        None,
        None,
    );
    let route_state = Arc::clone(&state);
    let route = tokio::spawn(async move {
        EnvelopeRouter::route_request(route_state, envelope, Duration::from_secs(5)).await
    });

    let routed = rx_a.recv().await.unwrap();
    assert!(matches!(routed, Message::Text(_)));
    assert!(rx_b.try_recv().is_err());
    EnvelopeRouter::handle_worker_response_from(
        Arc::clone(&state),
        &tenant,
        &worker_a,
        PeerRpcResponse::ok("cluster-index-request", vec![7]),
    )
    .unwrap();
    assert_eq!(route.await.unwrap().payload, vec![7]);
}

#[tokio::test]
async fn test_heartbeat_pruner() {
    let state = mock_state();
    let tenant_a = TenantId::new("tenant-a").unwrap();
    let inst_a = InstallationId::new("inst-a").unwrap();
    let core_a = CoreId::new("core-a").unwrap();
    let capability = CapabilityName::new("ping").unwrap();

    let (tx, _rx) = mpsc::channel(CONNECTION_BUFFER_CAPACITY);
    let key = WorkerConnectionKey {
        tenant_id: tenant_a.clone(),
        installation_id: inst_a.clone(),
        core_id: core_a.clone(),
    };

    // Create a connection with last heartbeat = 1000 (extremely old)
    let cluster = ClusterId::new("cluster-a").unwrap();
    let worker_conn = WorkerConnection::new_in_cluster(key, cluster.clone(), tx, 1000);

    {
        let tenant_state = state.tenant_partition_or_insert(&tenant_a).unwrap();
        let mut tenant_state = tenant_state.write();
        tenant_state
            .add_worker(worker_conn, vec![capability.clone()])
            .unwrap();
        assert_eq!(tenant_state.workers.len(), 1);
    }

    // Spawn pruner with 50ms interval and 100ms timeout
    let pruner = spawn_heartbeat_pruner(
        state.clone(),
        Duration::from_millis(50),
        Duration::from_millis(100),
    );

    // Await pruner run
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Verify worker has been pruned
    {
        let tenant_state = state.tenant_partition(&tenant_a).unwrap();
        let tenant_state = tenant_state.read();
        assert_eq!(tenant_state.workers.len(), 0);
        assert!(tenant_state.get_worker_by_core(&core_a).is_none());
        assert!(tenant_state
            .get_worker_in_cluster(&cluster, &core_a)
            .is_none());
        assert_eq!(tenant_state.worker_index_inconsistencies(), 0);
    }
    state.request_shutdown();
    pruner.await.unwrap();
}

#[tokio::test]
async fn test_worker_response_stays_inside_tenant_partition() {
    let state = mock_state();
    let tenant_a = TenantId::new("tenant-a").unwrap();
    let tenant_b = TenantId::new("tenant-b").unwrap();
    let capability = CapabilityName::new("compute").unwrap();
    let now = test_now_ms();

    let inst_a = InstallationId::new("inst-a").unwrap();
    let core_a = CoreId::new("core-a").unwrap();
    let (tx_a, mut rx_a) = mpsc::channel(CONNECTION_BUFFER_CAPACITY);
    let key_a = WorkerConnectionKey {
        tenant_id: tenant_a.clone(),
        installation_id: inst_a,
        core_id: core_a,
    };
    let cluster_a = ClusterId::new("cluster-a").unwrap();
    let worker_a = WorkerConnection::new_in_cluster(key_a, cluster_a.clone(), tx_a, 1000);

    let inst_b = InstallationId::new("inst-b").unwrap();
    let core_b = CoreId::new("core-b").unwrap();
    let (tx_b, _rx_b) = mpsc::channel(CONNECTION_BUFFER_CAPACITY);
    let key_b = WorkerConnectionKey {
        tenant_id: tenant_b.clone(),
        installation_id: inst_b,
        core_id: core_b,
    };
    let worker_b =
        WorkerConnection::new_in_cluster(key_b, ClusterId::new("cluster-b").unwrap(), tx_b, 1000);

    {
        state
            .tenant_partition_or_insert(&tenant_a)
            .unwrap()
            .write()
            .add_worker(worker_a, vec![capability.clone()])
            .unwrap();
        state
            .tenant_partition_or_insert(&tenant_b)
            .unwrap()
            .write()
            .add_worker(worker_b, vec![capability.clone()])
            .unwrap();
    }

    let envelope = PeerRpcEnvelope::new(
        "shared-req",
        "trace-1",
        CoreId::new("source").unwrap(),
        CoreId::new("core-a").unwrap(),
        tenant_a.clone(),
        cluster_a,
        now,
        now + 30_000,
        "nonce-1",
        capability,
        vec![],
        None,
        None,
    );

    let route_state = state.clone();
    let route_task = tokio::spawn(async move {
        EnvelopeRouter::route_request(route_state, envelope, Duration::from_secs(5)).await
    });

    let msg = rx_a.recv().await.unwrap();
    assert!(matches!(msg, Message::Text(_)));

    let cross_tenant_response = PeerRpcResponse::ok("shared-req", vec![99]);
    let err =
        EnvelopeRouter::handle_worker_response(state.clone(), &tenant_b, cross_tenant_response)
            .unwrap_err();
    assert!(err.to_string().contains("tenant tenant-b"));

    let tenant_response = PeerRpcResponse::ok("shared-req", vec![42]);
    EnvelopeRouter::handle_worker_response(state.clone(), &tenant_a, tenant_response).unwrap();

    let response = route_task.await.unwrap();
    assert!(response.ok);
    assert_eq!(response.payload, vec![42]);
}

#[tokio::test]
async fn response_must_come_from_the_selected_worker_and_request_id_is_unique() {
    let state = mock_state();
    let tenant = TenantId::new("tenant-a").unwrap();
    let cluster = ClusterId::new("cluster-a").unwrap();
    let capability = CapabilityName::new("runtime.compute").unwrap();
    let now = test_now_ms();
    let (tx_a, mut rx_a) = mpsc::channel(CONNECTION_BUFFER_CAPACITY);
    let worker_a = WorkerConnection::new_in_cluster(
        WorkerConnectionKey {
            tenant_id: tenant.clone(),
            installation_id: InstallationId::new("installation-a").unwrap(),
            core_id: CoreId::new("core-a").unwrap(),
        },
        cluster.clone(),
        tx_a,
        1000,
    );
    let (tx_b, _rx_b) = mpsc::channel(CONNECTION_BUFFER_CAPACITY);
    let worker_b = WorkerConnection::new_in_cluster(
        WorkerConnectionKey {
            tenant_id: tenant.clone(),
            installation_id: InstallationId::new("installation-b").unwrap(),
            core_id: CoreId::new("core-b").unwrap(),
        },
        cluster.clone(),
        tx_b,
        1000,
    );
    {
        let tenant_state = state.tenant_partition_or_insert(&tenant).unwrap();
        let mut tenant_state = tenant_state.write();
        tenant_state
            .add_worker(worker_a.clone(), vec![capability.clone()])
            .unwrap();
        tenant_state
            .add_worker(worker_b.clone(), vec![capability.clone()])
            .unwrap();
    }
    let envelope = PeerRpcEnvelope::new(
        "request-bound",
        "trace-bound",
        CoreId::new("source-core").unwrap(),
        CoreId::new("core-a").unwrap(),
        tenant.clone(),
        cluster,
        now,
        now + 30_000,
        "nonce-bound",
        capability,
        b"opaque".to_vec(),
        None,
        None,
    );
    let mut expired = envelope.clone();
    expired.timestamp_ms = now.saturating_sub(2);
    expired.expires_at_ms = now.saturating_sub(1);
    let rejected =
        EnvelopeRouter::route_request(state.clone(), expired, Duration::from_secs(1)).await;
    assert_eq!(rejected.error.as_deref(), Some("envelope_expired"));
    let route_state = state.clone();
    let first_envelope = envelope.clone();
    let task = tokio::spawn(async move {
        EnvelopeRouter::route_request(route_state, first_envelope, Duration::from_secs(5)).await
    });
    assert!(matches!(rx_a.recv().await, Some(Message::Text(_))));

    let duplicate =
        EnvelopeRouter::route_request(state.clone(), envelope, Duration::from_secs(1)).await;
    assert_eq!(duplicate.error.as_deref(), Some("pending_request_rejected"));

    let response = PeerRpcResponse::ok("request-bound", vec![9]);
    assert!(EnvelopeRouter::handle_worker_response_from(
        state.clone(),
        &tenant,
        &worker_b,
        response
    )
    .is_err());
    let response = PeerRpcResponse::ok("request-bound", vec![7]);
    EnvelopeRouter::handle_worker_response_from(state.clone(), &tenant, &worker_a, response)
        .unwrap();
    assert_eq!(task.await.unwrap().payload, vec![7]);
}

#[tokio::test]
async fn pending_routes_cleanup_after_timeout_cancellation_and_shutdown() {
    let (state, tenant, mut worker_rx, envelope) = pending_route_fixture();
    let mut timeout_envelope = envelope.clone();
    timeout_envelope.request_id = "request-timeout".to_string();
    let timeout_state = state.clone();
    let timeout_task = tokio::spawn(async move {
        EnvelopeRouter::route_request(timeout_state, timeout_envelope, Duration::from_millis(10))
            .await
    });
    assert!(matches!(worker_rx.recv().await, Some(Message::Text(_))));
    assert_eq!(
        timeout_task.await.unwrap().error.as_deref(),
        Some("worker_response_timeout")
    );
    assert_eq!(
        state
            .tenant_partition(&tenant)
            .unwrap()
            .read()
            .pending_request_count(),
        0
    );

    let mut cancelled_envelope = envelope.clone();
    cancelled_envelope.request_id = "request-cancelled".to_string();
    let cancelled_state = state.clone();
    let cancelled_task = tokio::spawn(async move {
        EnvelopeRouter::route_request(cancelled_state, cancelled_envelope, Duration::from_secs(5))
            .await
    });
    assert!(matches!(worker_rx.recv().await, Some(Message::Text(_))));
    cancelled_task.abort();
    assert!(cancelled_task.await.unwrap_err().is_cancelled());
    assert_eq!(
        state
            .tenant_partition(&tenant)
            .unwrap()
            .read()
            .pending_request_count(),
        0
    );

    let mut shutdown_envelope = envelope;
    shutdown_envelope.request_id = "request-shutdown".to_string();
    let shutdown_state = state.clone();
    let shutdown_task = tokio::spawn(async move {
        EnvelopeRouter::route_request(shutdown_state, shutdown_envelope, Duration::from_secs(5))
            .await
    });
    assert!(matches!(worker_rx.recv().await, Some(Message::Text(_))));
    state.request_shutdown();
    assert_eq!(
        shutdown_task.await.unwrap().error.as_deref(),
        Some("gateway_shutting_down")
    );
    assert_eq!(
        state
            .tenant_partition(&tenant)
            .unwrap()
            .read()
            .pending_request_count(),
        0
    );
}
