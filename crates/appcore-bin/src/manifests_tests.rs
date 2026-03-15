// =============================================================================
//        #######
//     ###       ###     F: manifests_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use appcore_contracts::{
    DeploymentManifestV1, InstallationId, JobPolicy, LeadershipMode, LeadershipRequirement,
    NetworkConfig, ProviderConfig, ProviderId, RuntimeMode, RuntimeRequirements,
    SchedulerRequirements, ServiceId, UpdatePolicy,
};

fn application_with_requirements(minimum: &str, protocol: &str) -> ApplicationManifestV1 {
    ApplicationManifestV1::new(
        ApplicationId::new("example-app").unwrap(),
        "1.0.0",
        "Example App",
        "Example Vendor",
        ServiceId::new("example.service").unwrap(),
        RuntimeRequirements::new(minimum, protocol).unwrap(),
    )
    .unwrap()
}

#[test]
fn standalone_fixture_is_valid_and_has_no_embedded_secret() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/deployment.standalone.toml"
    );
    let manifest = load_deployment_manifest(path).unwrap();

    assert_eq!(manifest.mode(), appcore_contracts::RuntimeMode::Standalone);
    assert!(manifest.control_plane().is_none());
    assert_eq!(manifest.storage().provider_id().as_str(), "file");
    assert_eq!(
        manifest
            .secrets()
            .get("runtime_security")
            .map(SecretRef::as_str),
        Some("env:APPCORE_RUNTIME_SECRET")
    );
}

#[test]
fn production_standalone_fixture_selects_rotation_aware_keyring() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/deployment.production.standalone.toml"
    );
    let manifest = load_deployment_manifest(path).unwrap();

    assert_eq!(manifest.mode(), RuntimeMode::Standalone);
    assert_eq!(
        manifest
            .secret_provider()
            .map(|provider| provider.provider_id().as_str()),
        Some("file-keyring-v1")
    );
    assert_eq!(
        manifest
            .secrets()
            .get("runtime_security")
            .map(SecretRef::as_str),
        Some("provider:active")
    );
    assert!(crate::providers::validate_production_profile(&manifest).is_ok());
}

#[test]
fn vercel_neon_cluster_fixture_has_only_secret_references() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/deployment.cluster.vercel-neon.toml"
    );
    let manifest = load_deployment_manifest(path).unwrap();
    let control_plane = manifest.control_plane().unwrap();

    assert_eq!(manifest.mode(), appcore_contracts::RuntimeMode::Cluster);
    assert_eq!(control_plane.provider_id().as_str(), "vercel-neon");
    assert_eq!(
        control_plane
            .secret_refs()
            .get("auth_token")
            .map(SecretRef::as_str),
        Some("env:APPCORE_CONTROL_PLANE_TOKEN")
    );
    assert!(serde_json::to_string(&manifest)
        .unwrap()
        .find("postgres://")
        .is_none());
}

#[test]
fn runtime_requirements_reject_newer_runtime_and_wrong_protocol() {
    assert!(validate_runtime_requirements(
        &application_with_requirements("99.0.0", "1"),
        "0.6.1",
        "1",
    )
    .is_err());
    assert!(validate_runtime_requirements(
        &application_with_requirements("0.6.0", "2"),
        "0.6.1",
        "1",
    )
    .is_err());
}

#[test]
fn runtime_requirements_accept_supported_version_and_protocol() {
    assert!(validate_runtime_requirements(
        &application_with_requirements("0.6.0", "1"),
        "0.6.1",
        "1",
    )
    .is_ok());
}

fn deployment(mode: RuntimeMode) -> appcore_contracts::DeploymentManifestBuilder {
    DeploymentManifestV1::builder(
        InstallationId::new("example-installation").unwrap(),
        ApplicationId::new("example-app").unwrap(),
        mode,
        ProviderConfig::new(ProviderId::new("file").unwrap()),
        NetworkConfig::new(
            ProviderId::new("http").unwrap(),
            ProviderId::new("http").unwrap(),
        ),
    )
}

#[test]
fn standalone_rejects_application_jobs_and_leadership() {
    let standalone = deployment(RuntimeMode::Standalone).build().unwrap();
    let jobs = application_with_requirements("0.6.0", "1")
        .with_job_policy(JobPolicy::new(true, 1, 0).unwrap())
        .unwrap();
    assert!(validate_manifest_compatibility(&jobs, &standalone).is_err());

    let leadership = application_with_requirements("0.6.0", "1")
        .with_leadership(
            LeadershipRequirement::new(
                ServiceId::new("example.service").unwrap(),
                LeadershipMode::Required,
                30_000,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(validate_manifest_compatibility(&leadership, &standalone).is_err());
}

#[test]
fn cluster_jobs_require_an_explicit_provider() {
    let application = application_with_requirements("0.6.0", "1")
        .with_job_policy(JobPolicy::new(true, 1, 0).unwrap())
        .unwrap();
    let base = deployment(RuntimeMode::Cluster)
        .with_control_plane(ProviderConfig::new(ProviderId::new("control").unwrap()))
        .with_peer_discovery(ProviderConfig::new(ProviderId::new("discovery").unwrap()));
    let without_jobs = base.clone().build().unwrap();
    assert!(validate_manifest_compatibility(&application, &without_jobs).is_err());

    let with_jobs = base
        .with_job_provider(ProviderConfig::new(ProviderId::new("jobs").unwrap()))
        .build()
        .unwrap();
    assert!(validate_manifest_compatibility(&application, &with_jobs).is_ok());
}

#[test]
fn background_execution_requires_owned_scheduler_and_update_policies() {
    let standalone = deployment(RuntimeMode::Standalone).build().unwrap();
    let scheduled = application_with_requirements("0.6.0", "1")
        .with_scheduler_requirements(SchedulerRequirements::new(true, 1).unwrap())
        .unwrap();
    assert!(validate_manifest_compatibility(&scheduled, &standalone).is_ok());

    let automatic_update = application_with_requirements("0.6.0", "1")
        .with_update_policy(UpdatePolicy::new("stable", true).unwrap())
        .unwrap();
    assert!(validate_manifest_compatibility(&automatic_update, &standalone).is_err());

    let update_provider = ProviderConfig::new(ProviderId::new("file-update").unwrap())
        .with_setting("artifact_kind", "executable")
        .unwrap();
    let managed = deployment(RuntimeMode::Standalone)
        .with_update_provider(update_provider)
        .build()
        .unwrap();
    assert!(validate_manifest_compatibility(&automatic_update, &managed).is_ok());
}
