// =============================================================================
//        #######
//     ###       ###     F: registry.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 12:48:56 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:48:56 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;

#[tokio::test]
async fn pending_entries_are_removed_on_every_terminal_path() {
    let tenant = TenantId::new("tenant-pending").unwrap();
    let mut tenant_state = TenantState::new(tenant);
    let future = || std::time::Instant::now() + Duration::from_secs(5);

    let response = tenant_state
        .register_pending_request("successful".to_string(), 7, future())
        .unwrap();
    assert!(tenant_state
        .register_pending_mesh_request("successful".to_string(), 7, future(), 64)
        .is_err());
    assert!(!tenant_state.complete_pending_request(
        "successful",
        Some(8),
        PeerRpcResponse::ok("successful", vec![1]),
    ));
    assert_eq!(tenant_state.pending_request_count(), 1);
    assert!(tenant_state.complete_pending_request(
        "successful",
        Some(7),
        PeerRpcResponse::ok("successful", vec![2]),
    ));
    assert_eq!(response.await.unwrap().payload, vec![2]);
    assert_eq!(tenant_state.pending_request_count(), 0);

    let expired = tenant_state
        .register_pending_request("expired".to_string(), 9, std::time::Instant::now())
        .unwrap();
    assert!(!tenant_state.complete_pending_request(
        "expired",
        Some(9),
        PeerRpcResponse::ok("expired", Vec::new()),
    ));
    assert!(expired.await.is_err());
    assert_eq!(tenant_state.pending_request_count(), 0);

    let oversized = tenant_state
        .register_pending_mesh_request("oversized".to_string(), 10, future(), 1)
        .unwrap();
    assert!(!tenant_state.complete_pending_mesh_request(
        "oversized",
        Some(10),
        MeshPeerResponse::ok(
            "oversized",
            PeerRpcHttpResponse {
                status_code: 200,
                body: vec![1, 2],
            },
        ),
    ));
    assert!(oversized.await.is_err());
    assert_eq!(tenant_state.pending_request_count(), 0);

    let peer_cancelled = tenant_state
        .register_pending_request("cancel-peer".to_string(), 11, future())
        .unwrap();
    let mesh_cancelled = tenant_state
        .register_pending_mesh_request("cancel-mesh".to_string(), 11, future(), 64)
        .unwrap();
    assert_eq!(tenant_state.cancel_pending_for_generation(11), 2);
    assert!(peer_cancelled.await.is_err());
    assert!(mesh_cancelled.await.is_err());
    assert_eq!(tenant_state.pending_request_count(), 0);
}

#[test]
fn worker_replacement_preserves_new_registration_and_metrics_do_not_underflow() {
    let tenant = TenantId::new("tenant-a").unwrap();
    let cluster = ClusterId::new("cluster-a").unwrap();
    let installation = InstallationId::new("installation-a").unwrap();
    let core = CoreId::new("core-a").unwrap();
    let key = WorkerConnectionKey {
        tenant_id: tenant.clone(),
        installation_id: installation.clone(),
        core_id: core.clone(),
    };
    let (old_tx, _old_rx) = mpsc::channel(CONNECTION_BUFFER_CAPACITY);
    let old = WorkerConnection::new_in_cluster(key.clone(), cluster.clone(), old_tx, 1);
    let (new_tx, _new_rx) = mpsc::channel(CONNECTION_BUFFER_CAPACITY);
    let new = WorkerConnection::new_in_cluster(key, cluster, new_tx, 2);
    let old_capability = CapabilityName::new("runtime.old").unwrap();
    let new_capability = CapabilityName::new("runtime.new").unwrap();
    let mut tenant_state = TenantState::new(tenant);
    tenant_state
        .add_worker(old.clone(), vec![old_capability.clone()])
        .unwrap();
    let mut old_response = tenant_state
        .register_pending_request(
            "old-generation".to_string(),
            old.generation(),
            std::time::Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
    tenant_state
        .add_worker(new.clone(), vec![new_capability.clone()])
        .unwrap();

    assert!(tenant_state.registry.resolve(&old_capability).is_none());
    assert!(tenant_state.registry.resolve(&new_capability).is_some());
    assert!(!tenant_state.remove_worker_if_current(&installation, &core, old.generation()));
    assert_eq!(tenant_state.workers.len(), 1);
    assert_eq!(tenant_state.pending_request_count(), 0);
    assert!(old_response.try_recv().is_err());

    let mut new_response = tenant_state
        .register_pending_request(
            "new-generation".to_string(),
            new.generation(),
            std::time::Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
    assert!(tenant_state.remove_worker_if_current(&installation, &core, new.generation()));
    assert_eq!(tenant_state.pending_request_count(), 0);
    assert!(new_response.try_recv().is_err());

    let metrics = GatewayMetrics::new();
    metrics.worker_disconnected();
    metrics.client_disconnected();
    assert_eq!(metrics.active_workers(), 0);
    assert_eq!(metrics.active_clients(), 0);
}

#[test]
fn worker_indexes_select_cluster_target_and_rebuild_after_disconnect() {
    let tenant = TenantId::new("tenant-index").unwrap();
    let core = CoreId::new("core-shared").unwrap();
    let cluster_a = ClusterId::new("cluster-a").unwrap();
    let cluster_b = ClusterId::new("cluster-b").unwrap();
    let installation_a = InstallationId::new("installation-a").unwrap();
    let installation_b = InstallationId::new("installation-b").unwrap();
    let (tx_a, _rx_a) = mpsc::channel(CONNECTION_BUFFER_CAPACITY);
    let worker_a = WorkerConnection::new_in_cluster(
        WorkerConnectionKey {
            tenant_id: tenant.clone(),
            installation_id: installation_a.clone(),
            core_id: core.clone(),
        },
        cluster_a.clone(),
        tx_a,
        1,
    );
    let (tx_b, _rx_b) = mpsc::channel(CONNECTION_BUFFER_CAPACITY);
    let worker_b = WorkerConnection::new_in_cluster(
        WorkerConnectionKey {
            tenant_id: tenant.clone(),
            installation_id: installation_b.clone(),
            core_id: core.clone(),
        },
        cluster_b.clone(),
        tx_b,
        2,
    );
    let mut state = TenantState::new(tenant);
    state.add_worker(worker_a.clone(), Vec::new()).unwrap();
    state.add_worker(worker_b.clone(), Vec::new()).unwrap();

    assert_eq!(
        state
            .get_worker_in_cluster(&cluster_a, &core)
            .map(WorkerConnection::generation),
        Some(worker_a.generation())
    );
    assert_eq!(
        state
            .get_worker_in_cluster(&cluster_b, &core)
            .map(WorkerConnection::generation),
        Some(worker_b.generation())
    );
    assert_eq!(
        state
            .get_worker_by_core(&core)
            .map(WorkerConnection::generation),
        Some(worker_b.generation())
    );

    state.remove_worker(&installation_b, &core);
    assert!(state.get_worker_in_cluster(&cluster_b, &core).is_none());
    assert_eq!(
        state
            .get_worker_by_core(&core)
            .map(WorkerConnection::generation),
        Some(worker_a.generation())
    );
    assert_eq!(state.worker_index_rebuilds(), 2);
    assert_eq!(state.worker_index_inconsistencies(), 0);
}

#[test]
fn worker_reconnect_updates_indexes_before_stale_disconnect() {
    let tenant = TenantId::new("tenant-reconnect-index").unwrap();
    let cluster = ClusterId::new("cluster-reconnect-index").unwrap();
    let installation = InstallationId::new("installation-reconnect-index").unwrap();
    let core = CoreId::new("core-reconnect-index").unwrap();
    let key = WorkerConnectionKey {
        tenant_id: tenant.clone(),
        installation_id: installation.clone(),
        core_id: core.clone(),
    };
    let (old_tx, _old_rx) = mpsc::channel(CONNECTION_BUFFER_CAPACITY);
    let old = WorkerConnection::new_in_cluster(key.clone(), cluster.clone(), old_tx, 1);
    let (new_tx, _new_rx) = mpsc::channel(CONNECTION_BUFFER_CAPACITY);
    let new = WorkerConnection::new_in_cluster(key, cluster.clone(), new_tx, 2);
    let mut state = TenantState::new(tenant);
    state.add_worker(old.clone(), Vec::new()).unwrap();
    state.add_worker(new.clone(), Vec::new()).unwrap();

    assert_eq!(
        state
            .get_worker_in_cluster(&cluster, &core)
            .map(WorkerConnection::generation),
        Some(new.generation())
    );
    assert!(!state.remove_worker_if_current(&installation, &core, old.generation()));
    assert_eq!(
        state
            .get_worker_by_core(&core)
            .map(WorkerConnection::generation),
        Some(new.generation())
    );
    assert_eq!(state.worker_index_inconsistencies(), 0);
}
