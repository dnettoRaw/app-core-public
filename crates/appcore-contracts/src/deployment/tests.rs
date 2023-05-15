// =============================================================================
//        #######
//     ###       ###     F: tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;

fn storage() -> ProviderConfig {
    ProviderConfig::new(ProviderId::new("file").unwrap())
}

fn network() -> NetworkConfig {
    NetworkConfig::new(
        ProviderId::new("https").unwrap(),
        ProviderId::new("http").unwrap(),
    )
}

fn builder(mode: RuntimeMode) -> DeploymentManifestBuilder {
    DeploymentManifestV1::builder(
        InstallationId::new("example-dev").unwrap(),
        ApplicationId::new("example-app").unwrap(),
        mode,
        storage(),
        network(),
    )
}

#[test]
fn standalone_is_self_contained() {
    let deployment = builder(RuntimeMode::Standalone)
        .with_path("data", "/var/lib/example")
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(deployment.mode(), RuntimeMode::Standalone);
    assert!(deployment.control_plane().is_none());
}

#[test]
fn cluster_requires_coordination_and_discovery() {
    assert!(builder(RuntimeMode::Cluster).build().is_err());
    let deployment = builder(RuntimeMode::Cluster)
        .with_control_plane(ProviderConfig::new(ProviderId::new("vercel-neon").unwrap()))
        .with_peer_discovery(ProviderConfig::new(
            ProviderId::new("control-plane").unwrap(),
        ))
        .build();
    assert!(deployment.is_ok());
}

#[test]
fn deployment_exposes_optional_infrastructure_providers() {
    let deployment = builder(RuntimeMode::Cluster)
        .with_control_plane(ProviderConfig::new(ProviderId::new("control").unwrap()))
        .with_coordination_store(ProviderConfig::new(
            ProviderId::new("coordination").unwrap(),
        ))
        .with_secret_provider(ProviderConfig::new(ProviderId::new("environment").unwrap()))
        .with_job_provider(ProviderConfig::new(ProviderId::new("jobs").unwrap()))
        .with_peer_discovery(ProviderConfig::new(ProviderId::new("discovery").unwrap()))
        .build()
        .unwrap();

    assert_eq!(
        deployment
            .coordination_store()
            .unwrap()
            .provider_id()
            .as_str(),
        "coordination"
    );
    assert_eq!(
        deployment.secret_provider().unwrap().provider_id().as_str(),
        "environment"
    );
    assert_eq!(
        deployment.job_provider().unwrap().provider_id().as_str(),
        "jobs"
    );
}

#[test]
fn standalone_rejects_distributed_job_and_coordination_providers() {
    assert!(builder(RuntimeMode::Standalone)
        .with_job_provider(ProviderConfig::new(ProviderId::new("jobs").unwrap()))
        .build()
        .is_err());
    assert!(builder(RuntimeMode::Standalone)
        .with_coordination_store(ProviderConfig::new(
            ProviderId::new("coordination").unwrap(),
        ))
        .build()
        .is_err());
}

#[test]
fn literal_secrets_are_rejected() {
    let result = builder(RuntimeMode::Standalone)
        .with_environment_literal("DATABASE_PASSWORD", "raw-secret");
    assert!(result.is_err());
}

#[test]
fn deployment_round_trip_revalidates_mode() {
    let deployment = builder(RuntimeMode::Standalone).build().unwrap();
    let encoded = serde_json::to_string(&deployment).unwrap();
    let decoded: DeploymentManifestV1 = serde_json::from_str(&encoded).unwrap();
    assert_eq!(deployment, decoded);
}

#[test]
fn watchdog_defaults_are_safe_and_explicit_settings_round_trip() {
    let defaults = builder(RuntimeMode::Standalone).build().unwrap();
    assert!(defaults.supervisor().watchdog().is_enabled());
    assert_eq!(defaults.supervisor().watchdog().check_interval_ms(), 1_000);
    assert_eq!(defaults.supervisor().watchdog().stall_timeout_ms(), 15_000);

    let watchdog = DeploymentWatchdogConfig::new(true, 250, 5_000).unwrap();
    let deployment = builder(RuntimeMode::Standalone)
        .with_supervisor(DeploymentSupervisorConfig::new(watchdog))
        .build()
        .unwrap();
    assert_eq!(deployment.supervisor().watchdog().check_interval_ms(), 250);
    assert!(DeploymentWatchdogConfig::new(true, 1_000, 1_000).is_err());
}

#[test]
fn deployment_manifest_matches_v1_fixture() {
    let deployment = builder(RuntimeMode::Standalone)
        .with_secret(
            "runtime_security",
            SecretRef::new("env:APPCORE_SECRET").unwrap(),
        )
        .unwrap()
        .with_path("storage", "storage")
        .unwrap()
        .build()
        .unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/deployment-manifest-v1.json")).unwrap();
    assert_eq!(serde_json::to_value(&deployment).unwrap(), expected);
    let decoded: DeploymentManifestV1 = serde_json::from_value(expected).unwrap();
    assert_eq!(decoded, deployment);
}
