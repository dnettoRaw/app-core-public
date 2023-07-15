// =============================================================================
//        #######
//     ###       ###     F: tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 10:59:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use appcore_types::{
    AppFamily, AppId, CoreKind, InstanceId, NodeId, ProtocolVersion, RuntimeContractVersion,
    RuntimeIdentity, SyncGroup,
};

fn identity() -> CoreIdentity {
    CoreIdentity {
        tenant_id: TenantId::new("tenant-a").unwrap(),
        cluster_id: ClusterId::new("cluster-a").unwrap(),
        core_id: CoreId::new("core-a").unwrap(),
        instance_id: InstanceId::new("instance-a").unwrap(),
        kind: CoreKind::operational(),
        protocol_version: ProtocolVersion::new(1),
        runtime: RuntimeIdentity {
            app_id: AppId::new("app-a").unwrap(),
            app_family: AppFamily::new("family-a").unwrap(),
            sync_group: SyncGroup::new("group-a").unwrap(),
            runtime_contract: RuntimeContractVersion::new(1),
            node_id: NodeId::new("node-a").unwrap(),
        },
    }
}

#[test]
fn service_lease_request_has_stable_json_shape() {
    let request = ServiceLeaseRequest {
        identity: identity(),
        service_id: ServiceId::new("runtime.query").unwrap(),
        ttl_ms: 30_000,
        now_ms: 42,
    };
    let encoded = serde_json::to_value(request).unwrap();
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../fixtures/control-plane-service-lease-v1.json"
    ))
    .unwrap();
    assert_eq!(encoded, fixture);
}
