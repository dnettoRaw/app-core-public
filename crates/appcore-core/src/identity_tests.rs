// =============================================================================
//        #######
//     ###       ###     F: identity_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/20 23:03:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{
    CompatibilityStatus, CoreCompatibilityPolicy, CoreCompatibilityStatus, CoreIdentity, CoreKind,
    RuntimeIdentity,
};
use crate::ids::{
    AppFamily, AppId, CapabilityName, ClusterId, CoreId, InstanceId, NodeId, ProtocolVersion,
    RuntimeContractVersion, SyncGroup, TenantId,
};

fn identity(node_id: &str) -> RuntimeIdentity {
    RuntimeIdentity {
        app_id: AppId::new("example-app".to_string()).unwrap(),
        app_family: AppFamily::new("example-family".to_string()).unwrap(),
        sync_group: SyncGroup::new("dev".to_string()).unwrap(),
        runtime_contract: RuntimeContractVersion::new(1),
        node_id: NodeId::new(node_id.to_string()).unwrap(),
    }
}

fn core_identity(tenant_id: &str, cluster_id: &str, core_id: &str) -> CoreIdentity {
    CoreIdentity {
        tenant_id: TenantId::new(tenant_id).unwrap(),
        cluster_id: ClusterId::new(cluster_id).unwrap(),
        core_id: CoreId::new(core_id).unwrap(),
        instance_id: InstanceId::new(format!("{core_id}-instance")).unwrap(),
        kind: CoreKind::operational(),
        protocol_version: ProtocolVersion::new(1),
        runtime: identity(core_id),
    }
}

#[test]
fn equal_identity_with_different_node_id_is_compatible() {
    let left = identity("node-a");
    let right = identity("node-b");

    assert_eq!(
        left.check_compatibility(&right),
        CompatibilityStatus::Compatible
    );
}

#[test]
fn different_app_id_is_rejected() {
    let left = identity("node-a");
    let mut right = identity("node-b");
    right.app_id = AppId::new("other-app".to_string()).unwrap();

    assert_eq!(
        left.check_compatibility(&right),
        CompatibilityStatus::DifferentAppId
    );
}

#[test]
fn different_app_family_is_rejected() {
    let left = identity("node-a");
    let mut right = identity("node-b");
    right.app_family = AppFamily::new("other-family".to_string()).unwrap();

    assert_eq!(
        left.check_compatibility(&right),
        CompatibilityStatus::DifferentAppFamily
    );
}

#[test]
fn different_sync_group_is_rejected() {
    let left = identity("node-a");
    let mut right = identity("node-b");
    right.sync_group = SyncGroup::new("production".to_string()).unwrap();

    assert_eq!(
        left.check_compatibility(&right),
        CompatibilityStatus::DifferentSyncGroup
    );
}

#[test]
fn different_runtime_contract_is_rejected() {
    let left = identity("node-a");
    let mut right = identity("node-b");
    right.runtime_contract = RuntimeContractVersion::new(2);

    assert_eq!(
        left.check_compatibility(&right),
        CompatibilityStatus::DifferentRuntimeContract
    );
}

#[test]
fn ensure_compatible_returns_ok_for_compatible() {
    let left = identity("node-a");
    let right = identity("node-b");

    assert!(left.ensure_compatible(&right).is_ok());
}

#[test]
fn ensure_compatible_returns_err_for_incompatible() {
    let left = identity("node-a");
    let mut right = identity("node-b");
    right.app_id = AppId::new("other-app".to_string()).unwrap();

    assert!(left.ensure_compatible(&right).is_err());
}

#[test]
fn distributed_identity_rejects_different_tenant() {
    let left = core_identity("tenant-a", "cluster-a", "node-a");
    let right = core_identity("tenant-b", "cluster-a", "node-b");

    assert_eq!(
        left.check_compatibility(&right, &CoreCompatibilityPolicy::default(), &[]),
        CoreCompatibilityStatus::DifferentTenant
    );
}

#[test]
fn distributed_identity_rejects_different_cluster_when_required() {
    let left = core_identity("tenant-a", "cluster-a", "node-a");
    let right = core_identity("tenant-a", "cluster-b", "node-b");

    assert_eq!(
        left.check_compatibility(&right, &CoreCompatibilityPolicy::default(), &[]),
        CoreCompatibilityStatus::DifferentCluster
    );
}

#[test]
fn distributed_identity_allows_different_cluster_when_policy_allows() {
    let left = core_identity("tenant-a", "cluster-a", "node-a");
    let right = core_identity("tenant-a", "cluster-b", "node-b");
    let policy = CoreCompatibilityPolicy {
        require_same_cluster: false,
        required_capability: None,
    };

    assert_eq!(
        left.check_compatibility(&right, &policy, &[]),
        CoreCompatibilityStatus::Compatible
    );
}

#[test]
fn distributed_identity_rejects_protocol_mismatch() {
    let left = core_identity("tenant-a", "cluster-a", "node-a");
    let mut right = core_identity("tenant-a", "cluster-a", "node-b");
    right.protocol_version = ProtocolVersion::new(2);

    assert_eq!(
        left.check_compatibility(&right, &CoreCompatibilityPolicy::default(), &[]),
        CoreCompatibilityStatus::IncompatibleProtocolVersion
    );
}

#[test]
fn distributed_identity_rejects_runtime_mismatch() {
    let left = core_identity("tenant-a", "cluster-a", "node-a");
    let mut right = core_identity("tenant-a", "cluster-a", "node-b");
    right.runtime.sync_group = SyncGroup::new("production").unwrap();

    assert_eq!(
        left.check_compatibility(&right, &CoreCompatibilityPolicy::default(), &[]),
        CoreCompatibilityStatus::IncompatibleRuntime(CompatibilityStatus::DifferentSyncGroup)
    );
}

#[test]
fn distributed_identity_rejects_missing_capability() {
    let left = core_identity("tenant-a", "cluster-a", "node-a");
    let right = core_identity("tenant-a", "cluster-a", "node-b");
    let policy = CoreCompatibilityPolicy {
        require_same_cluster: true,
        required_capability: Some(CapabilityName::new("runtime.query").unwrap()),
    };

    assert_eq!(
        left.check_compatibility(&right, &policy, &[]),
        CoreCompatibilityStatus::MissingCapability(CapabilityName::new("runtime.query").unwrap())
    );
}

#[test]
fn distributed_identity_accepts_required_capability() {
    let left = core_identity("tenant-a", "cluster-a", "node-a");
    let right = core_identity("tenant-a", "cluster-a", "node-b");
    let policy = CoreCompatibilityPolicy {
        require_same_cluster: true,
        required_capability: Some(CapabilityName::new("runtime.query").unwrap()),
    };

    assert_eq!(
        left.check_compatibility(
            &right,
            &policy,
            &[CapabilityName::new("runtime.query").unwrap()]
        ),
        CoreCompatibilityStatus::Compatible
    );
}
