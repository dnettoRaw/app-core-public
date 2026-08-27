// =============================================================================
//        #######
//     ###       ###     F: types_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.4-rc
// =============================================================================
// appcore-norm: test

use super::*;

#[test]
fn federation_url_requires_https_or_loopback_http_and_redacts_debug() {
    assert!(GatewayFederationUrl::new("https://gateway-a.example.com:8443").is_ok());
    assert!(GatewayFederationUrl::new("http://127.0.0.1:8080").is_ok());
    assert!(GatewayFederationUrl::new("http://[::1]:8080").is_ok());
    assert!(GatewayFederationUrl::new("http://gateway-a.example.com").is_err());
    assert!(GatewayFederationUrl::new("https://gateway-a.example.com:").is_err());
    assert!(GatewayFederationUrl::new("https://user:secret@gateway.example.com").is_err());
    let url = GatewayFederationUrl::new("https://private.example.com").unwrap();
    assert!(!format!("{url:?}").contains("private.example.com"));
}

#[test]
fn records_reject_cross_tenant_and_worker_expiry_outside_owner_lease() {
    let owner = GatewayInstanceLease::new(
        TenantId::new("tenant-a").unwrap(),
        ClusterId::new("cluster-a").unwrap(),
        InstanceId::new("gateway-a").unwrap(),
        GatewayFederationUrl::new("https://gateway-a.example.com").unwrap(),
        1,
        1_000,
    )
    .unwrap();
    let registration = GatewayWorkerRegistration::new(
        InstallationId::new("install-a").unwrap(),
        CoreId::new("core-a").unwrap(),
        1,
        vec![CapabilityName::new("runtime.query").unwrap()],
    )
    .unwrap();
    assert!(GatewayWorkerRecord::new(owner.clone(), registration.clone(), 1_001).is_err());
    let worker = GatewayWorkerRecord::new(owner.clone(), registration, 900).unwrap();
    let other = GatewayInstanceLease::new(
        TenantId::new("tenant-b").unwrap(),
        ClusterId::new("cluster-a").unwrap(),
        InstanceId::new("gateway-b").unwrap(),
        GatewayFederationUrl::new("https://gateway-b.example.com").unwrap(),
        1,
        1_000,
    )
    .unwrap();
    assert!(GatewayRequestFence::new(&other, &worker, "request-a", 800).is_err());
    let request = GatewayRequestFence::new(&owner, &worker, "private-request", 800).unwrap();
    let session = GatewaySessionRecord::new(owner, "private-session", 800).unwrap();
    assert!(!format!("{request:?}").contains("private-request"));
    assert!(!format!("{session:?}").contains("private-session"));
}

#[test]
fn sessions_and_requests_may_cross_a_lease_renewal_boundary() {
    let owner = GatewayInstanceLease::new(
        TenantId::new("tenant-a").unwrap(),
        ClusterId::new("cluster-a").unwrap(),
        InstanceId::new("gateway-a").unwrap(),
        GatewayFederationUrl::new("https://gateway-a.example.com").unwrap(),
        7,
        1_000,
    )
    .unwrap();
    let registration = GatewayWorkerRegistration::new(
        InstallationId::new("install-a").unwrap(),
        CoreId::new("core-a").unwrap(),
        2,
        Vec::new(),
    )
    .unwrap();
    let worker = GatewayWorkerRecord::new(owner.clone(), registration, 900).unwrap();

    assert!(GatewaySessionRecord::new(owner.clone(), "session-a", 60_000).is_ok());
    assert!(GatewayRequestFence::new(&owner, &worker, "request-a", 30_000).is_ok());
}

#[test]
fn capability_registration_is_sorted_deduplicated_and_bounded() {
    let capability = CapabilityName::new("runtime.query").unwrap();
    let registration = GatewayWorkerRegistration::new(
        InstallationId::new("install-a").unwrap(),
        CoreId::new("core-a").unwrap(),
        1,
        vec![capability.clone(), capability],
    )
    .unwrap();
    assert_eq!(registration.capabilities.len(), 1);
    let too_many = (0..=MAX_GATEWAY_CAPABILITIES)
        .map(|index| CapabilityName::new(format!("runtime.capability-{index}")).unwrap())
        .collect();
    assert!(GatewayWorkerRegistration::new(
        InstallationId::new("install-a").unwrap(),
        CoreId::new("core-a").unwrap(),
        1,
        too_many,
    )
    .is_err());
}
