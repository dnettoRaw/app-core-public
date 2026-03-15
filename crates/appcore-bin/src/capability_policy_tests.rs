// =============================================================================
//        #######
//     ###       ###     F: capability_policy_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/20 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/20 00:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use appcore_core::{
    AppFamily, AppId, CapabilityDescriptor, CapabilityRequirements, CapabilityVisibility,
    ClusterId, CoreId, CoreKind, InstanceId, NodeId, ProtocolVersion, RuntimeContractVersion,
    RuntimeIdentity, SyncGroup, TenantId,
};
use std::collections::BTreeMap;

fn identity() -> CoreIdentity {
    CoreIdentity {
        tenant_id: TenantId::new("tenant-a").unwrap(),
        cluster_id: ClusterId::new("cluster-a").unwrap(),
        core_id: CoreId::new("core-a").unwrap(),
        instance_id: InstanceId::new("core-a-1").unwrap(),
        kind: CoreKind::operational(),
        protocol_version: ProtocolVersion::new(1),
        runtime: RuntimeIdentity {
            app_id: AppId::new("app-a").unwrap(),
            app_family: AppFamily::new("family-a").unwrap(),
            sync_group: SyncGroup::new("cluster-a").unwrap(),
            runtime_contract: RuntimeContractVersion::new(1),
            node_id: NodeId::new("node-a").unwrap(),
        },
    }
}

fn policy(
    lease: Option<ServiceLeaderLease>,
    operation_mode: RuntimeOperationalMode,
) -> RuntimeCapabilityPolicy {
    let identity = identity();
    let manifest = DistributedCoreManifest {
        identity: identity.clone(),
        app_name: "App".to_string(),
        app_version: "0.1.0".to_string(),
        runtime_min_version: "1.0.0-rc.3".to_string(),
        runtime_max_version: None,
        capabilities: vec![CapabilityDescriptor::new(
            CapabilityName::new("runtime.write").unwrap(),
            "1",
            CapabilityMode::Command,
            CapabilityVisibility::Cluster,
        )
        .with_requirements(CapabilityRequirements {
            requires_leader: true,
            read_only: false,
            idempotency_required: true,
        })],
        endpoints: Vec::new(),
        metadata: BTreeMap::new(),
    };
    RuntimeCapabilityPolicy::from_manifest(
        &manifest,
        Arc::new(Mutex::new(operation_mode)),
        ServiceId::new("runtime.service").unwrap(),
        Arc::new(Mutex::new(lease)),
    )
    .unwrap()
}

#[test]
fn host_policy_uses_owner_for_declaration_idempotency_and_leadership() {
    let policy = policy(None, RuntimeOperationalMode::ReadWrite);
    assert_eq!(
        policy.authorize_command("runtime.missing", Some("idem-1"), 10),
        Err(CommandCapabilityPolicyError::CapabilityNotDeclared)
    );
    assert_eq!(
        policy.authorize_command("runtime.write", None, 10),
        Err(CommandCapabilityPolicyError::MissingIdempotencyKey)
    );
    assert_eq!(
        policy.authorize_command("runtime.write", Some("idem-1"), 10),
        Err(CommandCapabilityPolicyError::RequiresLeader)
    );
}

#[test]
fn host_policy_applies_operation_mode_and_current_service_lease() {
    let identity = identity();
    let lease = ServiceLeaderLease {
        service_id: ServiceId::new("runtime.service").unwrap(),
        tenant_id: identity.tenant_id,
        cluster_id: identity.cluster_id,
        holder_core_id: identity.core_id,
        epoch: 1,
        acquired_at_ms: 0,
        expires_at_ms: 20,
    };
    let read_only = policy(Some(lease.clone()), RuntimeOperationalMode::ReadOnly);
    assert_eq!(
        read_only.authorize_command("runtime.write", Some("idem-1"), 10),
        Err(CommandCapabilityPolicyError::ReadOnly)
    );

    let writable = policy(Some(lease), RuntimeOperationalMode::ReadWrite);
    assert!(writable
        .authorize_command("runtime.write", Some("idem-1"), 19)
        .is_ok());
    assert_eq!(
        writable.authorize_command("runtime.write", Some("idem-1"), 20),
        Err(CommandCapabilityPolicyError::LeaseExpired)
    );
}
