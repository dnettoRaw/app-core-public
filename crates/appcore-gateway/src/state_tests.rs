// =============================================================================
//        #######
//     ###       ###     F: state_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.4-rc
// =============================================================================
// appcore-norm: test

use super::*;
use crate::{GatewayHaMode, GatewayRegistryError};
use appcore_contracts::InstallationId;
use appcore_peer_rpc::{BoundedReplayStore, ReplayStoreConfig};
use appcore_types::{CapabilityName, ClusterId, CoreId, TenantId};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn single_instance_state_keeps_existing_admission_behavior() {
    let state = GatewayState::new(config(), token_provider()).unwrap();
    assert_eq!(state.admit_ha_work(), Ok(()));
    assert_eq!(state.ha_lifecycle_snapshot(), None);
}

#[test]
fn opt_in_state_fails_closed_across_lifecycle_transitions() {
    let lifecycle = Arc::new(GatewayHaLifecycle::new());
    let state = GatewayState::with_ha_lifecycle(
        config(),
        token_provider(),
        Arc::new(BoundedReplayStore::new(ReplayStoreConfig::default())),
        Arc::clone(&lifecycle),
    )
    .unwrap();
    assert_eq!(
        state.admit_ha_work(),
        Err(GatewayRegistryError::Unavailable)
    );
    lifecycle.begin_recovery(1_000).unwrap();
    assert!(state.admit_ha_work().is_err());
    lifecycle.mark_healthy(1_010).unwrap();
    assert_eq!(state.admit_ha_work(), Ok(()));
    lifecycle.isolate().unwrap();
    assert!(state.admit_ha_work().is_err());
    state.request_shutdown();
    assert_eq!(
        state.ha_lifecycle_snapshot().unwrap().mode,
        GatewayHaMode::Stopped
    );
}

#[test]
fn ownership_snapshot_captures_cluster_generation_capabilities_and_live_sessions() {
    let state = GatewayState::new(config(), token_provider()).unwrap();
    let tenant_id = TenantId::new("tenant-a").unwrap();
    let installation_id = InstallationId::new("install-a").unwrap();
    let core_id = CoreId::new("core-a").unwrap();
    let cluster_id = ClusterId::new("cluster-a").unwrap();
    let (sender, _receiver) = tokio::sync::mpsc::channel(1);
    let worker = crate::WorkerConnection::new_in_cluster(
        crate::WorkerConnectionKey {
            tenant_id: tenant_id.clone(),
            installation_id,
            core_id,
        },
        cluster_id.clone(),
        sender,
        1_000,
    );
    let tenant = state.tenant_partition_or_insert(&tenant_id).unwrap();
    let mut tenant = tenant.write();
    tenant
        .add_worker(worker, vec![CapabilityName::new("runtime.query").unwrap()])
        .unwrap();
    tenant.sessions.insert(
        "session-a".to_string(),
        crate::GatewaySession::new("session-a".to_string(), tenant_id, 1_000, 10_000, None),
    );
    drop(tenant);

    let snapshot = GatewayHaOwnershipSource::snapshot(&state, 2_000).unwrap();
    assert_eq!(snapshot.workers.len(), 1);
    assert_eq!(snapshot.workers[0].cluster_id, cluster_id);
    assert_eq!(snapshot.workers[0].registration.capabilities.len(), 1);
    assert_eq!(snapshot.sessions.len(), 1);
}

fn config() -> GatewayConfig {
    GatewayConfig::new(([127, 0, 0, 1], 0).into(), "gateway.test")
}

fn token_provider() -> HashTokenProvider {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        .to_le_bytes();
    HashTokenProvider::from_secret(seed.repeat(2)).unwrap()
}
