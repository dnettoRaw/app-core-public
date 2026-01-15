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
    tenant_state
        .add_worker(new.clone(), vec![new_capability.clone()])
        .unwrap();

    assert!(tenant_state.registry.resolve(&old_capability).is_none());
    assert!(tenant_state.registry.resolve(&new_capability).is_some());
    assert!(!tenant_state.remove_worker_if_current(&installation, &core, old.generation()));
    assert_eq!(tenant_state.workers.len(), 1);

    let metrics = GatewayMetrics::new();
    metrics.worker_disconnected();
    metrics.client_disconnected();
    assert_eq!(metrics.active_workers(), 0);
    assert_eq!(metrics.active_clients(), 0);
}
