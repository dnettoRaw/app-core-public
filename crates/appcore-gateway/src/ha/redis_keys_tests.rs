// =============================================================================
//        #######
//     ###       ###     F: redis_keys_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.6-rc
// =============================================================================
// appcore-norm: test

use super::*;

#[test]
fn tenant_keys_share_one_cluster_hash_slot() {
    let keys = RedisGatewayKeys::new("gateway");
    let tenant = TenantId::new("tenant-a").unwrap();
    let instance = InstanceId::new("gateway-a").unwrap();
    let cluster = ClusterId::new("cluster-a").unwrap();
    let core = CoreId::new("core-a").unwrap();
    let capability = CapabilityName::new("runtime.query").unwrap();
    let values = [
        keys.epoch(&tenant, &instance),
        keys.lease(&tenant, &instance),
        keys.worker(&tenant, &cluster, &core),
        keys.worker_capabilities(&tenant, &cluster, &core),
        keys.workers(&tenant),
        keys.capability(&tenant, &capability),
        keys.session(&tenant, "session-a"),
        keys.sessions(&tenant),
        keys.request(&tenant, "request-a"),
        keys.requests(&tenant),
    ];
    assert!(values.iter().all(|key| key.contains("{tenant-a}")));
}

#[test]
fn tenants_never_share_the_same_hash_tag() {
    let keys = RedisGatewayKeys::new("gateway");
    let instance = InstanceId::new("gateway-a").unwrap();
    let tenant_a = TenantId::new("tenant-a").unwrap();
    let tenant_b = TenantId::new("tenant-b").unwrap();
    assert_ne!(
        keys.lease(&tenant_a, &instance),
        keys.lease(&tenant_b, &instance)
    );
}
