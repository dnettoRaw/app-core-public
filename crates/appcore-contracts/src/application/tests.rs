// =============================================================================
//        #######
//     ###       ###     F: tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 11:51:10 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use crate::{CapabilityClass, CapabilityId, CapabilityMode, CapabilityVisibility};

fn manifest() -> ApplicationManifestV1 {
    ApplicationManifestV1::new(
        ApplicationId::new("example-app").unwrap(),
        "1.0.0",
        "Example App",
        "Example Vendor",
        ServiceId::new("example.service").unwrap(),
        RuntimeRequirements::new("0.6.1", "1").unwrap(),
    )
    .unwrap()
}

#[test]
fn application_manifest_round_trips_and_validates() {
    let manifest = manifest()
        .with_capability(
            CapabilityDeclaration::new(
                CapabilityId::new("document.extract").unwrap(),
                "1",
                CapabilityMode::Command,
                CapabilityVisibility::Cluster,
            )
            .unwrap(),
        )
        .unwrap();
    let encoded = serde_json::to_string(&manifest).unwrap();
    let decoded: ApplicationManifestV1 = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, manifest);
}

#[test]
fn application_manifest_matches_v1_fixture() {
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/application-manifest-v1.json")).unwrap();
    assert_eq!(serde_json::to_value(manifest()).unwrap(), expected);
    let decoded: ApplicationManifestV1 = serde_json::from_value(expected).unwrap();
    assert_eq!(decoded, manifest());
}

#[test]
fn application_manifest_rejects_secrets_and_paths() {
    assert!(manifest().with_metadata("api_token", "raw").is_err());
    assert!(manifest().with_metadata("cache", "/var/app/cache").is_err());
    assert!(manifest()
        .with_metadata("control_endpoint", "https://runtime.example")
        .is_err());
}

#[test]
fn leadership_capability_requires_service_policy() {
    let capability = CapabilityDeclaration::new(
        CapabilityId::new("document.extract").unwrap(),
        "1",
        CapabilityMode::Command,
        CapabilityVisibility::Cluster,
    )
    .unwrap()
    .with_leadership(true);
    assert!(manifest().with_capability(capability).is_err());
}

#[test]
fn application_manifest_rejects_infrastructure_capabilities() {
    let capability = CapabilityDeclaration::new(
        CapabilityId::new("runtime.health").unwrap(),
        "1",
        CapabilityMode::Query,
        CapabilityVisibility::Local,
    )
    .unwrap()
    .with_class(CapabilityClass::Infrastructure);

    assert!(manifest().with_capability(capability).is_err());
}

#[test]
fn application_manifest_rejects_reserved_capability_namespaces() {
    for capability_id in [
        "appcore.health",
        "runtime.status",
        "infrastructure.storage",
        "Runtime.Status",
    ] {
        let capability = CapabilityDeclaration::new(
            CapabilityId::new(capability_id).unwrap(),
            "1",
            CapabilityMode::Query,
            CapabilityVisibility::Local,
        )
        .unwrap();

        assert!(manifest().with_capability(capability).is_err());
    }
}

#[test]
fn functional_class_keeps_the_v1_wire_shape() {
    let capability = CapabilityDeclaration::new(
        CapabilityId::new("document.extract").unwrap(),
        "1",
        CapabilityMode::Command,
        CapabilityVisibility::Cluster,
    )
    .unwrap();
    let encoded = serde_json::to_value(capability).unwrap();
    assert!(encoded.get("class").is_none());
}
