// =============================================================================
//        #######
//     ###       ###     F: ownership_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.4-rc
// =============================================================================
// appcore-norm: test

use super::*;
use appcore_contracts::InstallationId;
use appcore_types::{ClusterId, CoreId};

#[test]
fn snapshot_rejects_unknown_tenant_duplicate_worker_and_expired_session() {
    let tenants = vec![GatewayHaTenantBinding {
        tenant_id: tenant("tenant-a"),
        cluster_id: ClusterId::new("cluster-a").unwrap(),
    }];
    let worker = GatewayHaWorkerSnapshot {
        tenant_id: tenant("tenant-a"),
        cluster_id: ClusterId::new("cluster-a").unwrap(),
        registration: GatewayWorkerRegistration::new(
            InstallationId::new("install-a").unwrap(),
            CoreId::new("core-a").unwrap(),
            1,
            Vec::new(),
        )
        .unwrap(),
    };
    let duplicate = GatewayHaOwnershipSnapshot {
        workers: vec![worker.clone(), worker],
        sessions: Vec::new(),
    };
    assert_eq!(
        duplicate.validate(&tenants, 1_000),
        Err(GatewayRegistryError::InvalidContract)
    );

    let expired = GatewayHaOwnershipSnapshot {
        workers: Vec::new(),
        sessions: vec![GatewayHaSessionSnapshot {
            tenant_id: tenant("tenant-a"),
            session_id: "session-a".to_string(),
            expires_at_ms: 1_000,
        }],
    };
    assert_eq!(
        expired.validate(&tenants, 1_000),
        Err(GatewayRegistryError::InvalidContract)
    );
}

fn tenant(value: &str) -> TenantId {
    TenantId::new(value).unwrap()
}
