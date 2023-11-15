// =============================================================================
//        #######
//     ###       ###     F: manifest_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{CapabilityDescriptor, CapabilityMode, CapabilityVisibility, DistributedCoreManifest};
use crate::identity::{CoreCompatibilityPolicy, CoreIdentity, RuntimeIdentity};
use crate::ids::{AppFamily, AppId, CapabilityName, NodeId, RuntimeContractVersion, SyncGroup};
use appcore_contracts::{
    ApplicationId, ApplicationManifestV1, CapabilityDeclaration,
    CapabilityId as ContractCapabilityId, CapabilityMode as ContractCapabilityMode,
    CapabilityVisibility as ContractCapabilityVisibility, RuntimeRequirements, ServiceId,
};

fn runtime_identity() -> RuntimeIdentity {
    RuntimeIdentity {
        app_id: AppId::new("example-app").unwrap(),
        app_family: AppFamily::new("example-family").unwrap(),
        sync_group: SyncGroup::new("dev").unwrap(),
        runtime_contract: RuntimeContractVersion::new(1),
        node_id: NodeId::new("node-a").unwrap(),
    }
}

fn application_manifest() -> ApplicationManifestV1 {
    let manifest = ApplicationManifestV1::new(
        ApplicationId::new("example-app").unwrap(),
        "1.0.0",
        "Example App",
        "Example Vendor",
        ServiceId::new("example-service").unwrap(),
        RuntimeRequirements::new("1.0.0", "1").unwrap(),
    )
    .unwrap();
    let command = CapabilityDeclaration::new(
        ContractCapabilityId::new("command.execute").unwrap(),
        "1",
        ContractCapabilityMode::Command,
        ContractCapabilityVisibility::Cluster,
    )
    .unwrap()
    .with_idempotency(true);
    manifest.with_capability(command).unwrap()
}

fn core_manifest() -> DistributedCoreManifest {
    let identity = CoreIdentity::from_runtime_defaults(runtime_identity()).unwrap();
    DistributedCoreManifest::from_application_manifest(&application_manifest(), identity).unwrap()
}

#[test]
fn application_manifest_derives_structured_core_manifest() {
    let core = core_manifest();

    assert_eq!(core.identity.tenant_id.as_str(), "example-app");
    assert_eq!(core.identity.cluster_id.as_str(), "dev");
    assert!(core.supports_capability(&CapabilityName::new("command.execute").unwrap()));
    assert_eq!(
        core.capabilities[0].visibility,
        CapabilityVisibility::Cluster
    );
    assert!(core.capabilities[0].requirements.idempotency_required);
}

#[test]
fn capability_descriptor_keeps_mode_visibility_and_requirements() {
    let descriptor = CapabilityDescriptor::new(
        CapabilityName::new("runtime.status").unwrap(),
        "1",
        CapabilityMode::Query,
        CapabilityVisibility::Cluster,
    );

    assert_eq!(descriptor.name.as_str(), "runtime.status");
    assert_eq!(descriptor.mode, CapabilityMode::Query);
    assert_eq!(descriptor.visibility, CapabilityVisibility::Cluster);
    assert!(!descriptor.requirements.requires_leader);
}

#[test]
fn core_manifest_compatibility_checks_capability() {
    let left = core_manifest();
    let mut right = core_manifest();
    right.capabilities.clear();
    let policy = CoreCompatibilityPolicy {
        require_same_cluster: true,
        required_capability: Some(CapabilityName::new("command.execute").unwrap()),
    };

    assert_ne!(
        left.check_peer_compatibility(&right, &policy),
        crate::CoreCompatibilityStatus::Compatible
    );
}
