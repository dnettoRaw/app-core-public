// =============================================================================
//        #######
//     ###       ###     F: providers_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/26 10:16:57 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::*;
use appcore_contracts::{
    ApplicationId, InstallationId, NetworkConfig, ProviderId, RuntimeMode, StorageDurability,
    StorageRequirements, TlsConfig,
};
use appcore_provider::ResolvedSecret;

fn standalone(storage_provider: &str) -> DeploymentManifestV1 {
    DeploymentManifestV1::builder(
        InstallationId::new("install-a").unwrap(),
        ApplicationId::new("app-a").unwrap(),
        RuntimeMode::Standalone,
        ProviderConfig::new(ProviderId::new(storage_provider).unwrap()),
        NetworkConfig::new(
            ProviderId::new("http").unwrap(),
            ProviderId::new("http").unwrap(),
        ),
    )
    .build()
    .unwrap()
}

fn production_manifest(mode: RuntimeMode, network: NetworkConfig) -> DeploymentManifestV1 {
    let builder = DeploymentManifestV1::builder(
        InstallationId::new("production-install").unwrap(),
        ApplicationId::new("production-app").unwrap(),
        mode,
        ProviderConfig::new(ProviderId::new("file").unwrap()),
        network,
    )
    .with_secret_provider(
        ProviderConfig::new(ProviderId::new("file-keyring-v1").unwrap())
            .with_setting("root", "/var/lib/appcore/security")
            .unwrap(),
    );
    let builder = if mode == RuntimeMode::Cluster {
        builder
            .with_control_plane(ProviderConfig::new(ProviderId::new("in-memory").unwrap()))
            .with_peer_discovery(ProviderConfig::new(
                ProviderId::new("control-plane").unwrap(),
            ))
    } else {
        builder
    };
    builder
        .with_secret(
            "runtime_security",
            SecretRef::new("provider:active").unwrap(),
        )
        .unwrap()
        .build()
        .unwrap()
}

fn production_manifest_with_update(update: ProviderConfig) -> DeploymentManifestV1 {
    DeploymentManifestV1::builder(
        InstallationId::new("production-update-install").unwrap(),
        ApplicationId::new("production-update-app").unwrap(),
        RuntimeMode::Standalone,
        ProviderConfig::new(ProviderId::new("file").unwrap()),
        NetworkConfig::new(
            ProviderId::new("http").unwrap(),
            ProviderId::new("http").unwrap(),
        )
        .with_listen_address("127.0.0.1:8080")
        .unwrap(),
    )
    .with_secret_provider(
        ProviderConfig::new(ProviderId::new("file-keyring-v1").unwrap())
            .with_setting("root", "/var/lib/appcore/security")
            .unwrap(),
    )
    .with_secret(
        "runtime_security",
        SecretRef::new("provider:active").unwrap(),
    )
    .unwrap()
    .with_update_provider(update)
    .build()
    .unwrap()
}

fn production_cluster_manifest_with_mesh_relay(endpoint: &str) -> DeploymentManifestV1 {
    DeploymentManifestV1::builder(
        InstallationId::new("production-mesh-install").unwrap(),
        ApplicationId::new("production-mesh-app").unwrap(),
        RuntimeMode::Cluster,
        ProviderConfig::new(ProviderId::new("file").unwrap()),
        NetworkConfig::new(
            ProviderId::new("mesh-relay").unwrap(),
            ProviderId::new("https").unwrap(),
        ),
    )
    .with_control_plane(ProviderConfig::new(ProviderId::new("in-memory").unwrap()))
    .with_peer_discovery(ProviderConfig::new(
        ProviderId::new("control-plane").unwrap(),
    ))
    .with_secret_provider(
        ProviderConfig::new(ProviderId::new("file-keyring-v1").unwrap())
            .with_setting("root", "/var/lib/appcore/security")
            .unwrap(),
    )
    .with_secret(
        "runtime_security",
        SecretRef::new("provider:active").unwrap(),
    )
    .unwrap()
    .with_adapter(
        "mesh-relay",
        ProviderConfig::new(ProviderId::new("mesh-relay").unwrap())
            .with_endpoint(endpoint)
            .unwrap(),
    )
    .unwrap()
    .build()
    .unwrap()
}

#[test]
fn plan_accepts_only_available_host_providers() {
    assert!(provider_plan(&standalone("file")).is_ok());
    assert!(provider_plan(&standalone("unknown-storage")).is_err());
}

#[test]
fn storage_preflight_accepts_only_exact_file_provider_guarantees() {
    let local = StorageRequirements::new(StorageDurability::Local, 0, false);
    let snapshot = ProviderConfig::new(ProviderId::new("file").unwrap())
        .with_setting("required_capabilities", "snapshot")
        .unwrap();
    assert!(validate_storage_preflight(&local, &snapshot).is_ok());

    let transactions = ProviderConfig::new(ProviderId::new("file").unwrap())
        .with_setting("required_capabilities", "transactions")
        .unwrap();
    let error = validate_storage_preflight(&local, &transactions)
        .unwrap_err()
        .to_string();
    assert!(error.contains("does not support required capability transactions"));
    assert!(!error.contains("path"));
}

#[test]
fn storage_preflight_maps_existing_shared_requirement_to_multi_host() {
    let shared = StorageRequirements::new(StorageDurability::Durable, 0, true);
    let file = ProviderConfig::new(ProviderId::new("file").unwrap());
    let error = validate_storage_preflight(&shared, &file)
        .unwrap_err()
        .to_string();
    assert!(error.contains("does not support required capability multi_host"));
}

#[test]
fn storage_preflight_rejects_unknown_requirement_without_echoing_it() {
    let local = StorageRequirements::new(StorageDurability::Local, 0, false);
    let config = ProviderConfig::new(ProviderId::new("file").unwrap())
        .with_setting("required_capabilities", "secret-looking-value")
        .unwrap();
    let error = validate_storage_preflight(&local, &config)
        .unwrap_err()
        .to_string();
    assert!(error.contains("requirement is unknown"));
    assert!(!error.contains("secret-looking-value"));
}

#[test]
fn debug_output_never_contains_resolved_secret() {
    let secret = ResolvedSecret::new("do-not-log").unwrap();
    assert_eq!(format!("{secret:?}"), "ResolvedSecret(REDACTED)");
}

#[test]
fn in_memory_control_plane_is_available_for_reference_clusters() {
    let deployment = DeploymentManifestV1::builder(
        InstallationId::new("cluster-install").unwrap(),
        ApplicationId::new("cluster-app").unwrap(),
        RuntimeMode::Cluster,
        ProviderConfig::new(ProviderId::new("file").unwrap()),
        NetworkConfig::new(
            ProviderId::new("http").unwrap(),
            ProviderId::new("http").unwrap(),
        ),
    )
    .with_control_plane(ProviderConfig::new(ProviderId::new("in-memory").unwrap()))
    .with_peer_discovery(ProviderConfig::new(
        ProviderId::new("control-plane").unwrap(),
    ))
    .build()
    .unwrap();
    let plan = provider_plan(&deployment).unwrap();

    assert!(control_plane_client(&plan).unwrap().is_some());
}

#[test]
fn file_reference_stack_requires_shared_coordination_path() {
    let path = std::env::temp_dir()
        .join(format!("appcore-provider-stack-{}", std::process::id()))
        .to_string_lossy()
        .into_owned();
    let control = ProviderConfig::new(ProviderId::new("file-control-plane").unwrap())
        .with_setting("path", &path)
        .unwrap();
    let coordination = ProviderConfig::new(ProviderId::new("file-coordination-v2").unwrap())
        .with_setting("path", &path)
        .unwrap();
    let deployment = DeploymentManifestV1::builder(
        InstallationId::new("cluster-file").unwrap(),
        ApplicationId::new("cluster-app").unwrap(),
        RuntimeMode::Cluster,
        ProviderConfig::new(ProviderId::new("file").unwrap()),
        NetworkConfig::new(
            ProviderId::new("http").unwrap(),
            ProviderId::new("http").unwrap(),
        ),
    )
    .with_control_plane(control)
    .with_coordination_store(coordination)
    .with_peer_discovery(ProviderConfig::new(
        ProviderId::new("control-plane").unwrap(),
    ))
    .build()
    .unwrap();
    let plan = provider_plan(&deployment).unwrap();

    assert!(coordination_store(&plan).unwrap().is_some());
    assert!(control_plane_client(&plan).unwrap().is_some());
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn local_mesh_reference_stack_uses_shared_coordination_path() {
    let path = std::env::temp_dir()
        .join(format!("appcore-local-mesh-stack-{}", std::process::id()))
        .to_string_lossy()
        .into_owned();
    let control = ProviderConfig::new(ProviderId::new("local-mesh").unwrap())
        .with_setting("path", &path)
        .unwrap();
    let coordination = ProviderConfig::new(ProviderId::new("file-coordination-v2").unwrap())
        .with_setting("path", &path)
        .unwrap();
    let deployment = DeploymentManifestV1::builder(
        InstallationId::new("cluster-local-mesh").unwrap(),
        ApplicationId::new("cluster-app").unwrap(),
        RuntimeMode::Cluster,
        ProviderConfig::new(ProviderId::new("file").unwrap()),
        NetworkConfig::new(
            ProviderId::new("http").unwrap(),
            ProviderId::new("http").unwrap(),
        ),
    )
    .with_control_plane(control)
    .with_coordination_store(coordination)
    .with_peer_discovery(ProviderConfig::new(
        ProviderId::new("control-plane").unwrap(),
    ))
    .build()
    .unwrap();
    let plan = provider_plan(&deployment).unwrap();

    assert!(coordination_store(&plan).unwrap().is_some());
    assert!(control_plane_client(&plan).unwrap().is_some());
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn production_standalone_accepts_loopback_and_keyring() {
    let deployment = production_manifest(
        RuntimeMode::Standalone,
        NetworkConfig::new(
            ProviderId::new("http").unwrap(),
            ProviderId::new("http").unwrap(),
        )
        .with_listen_address("127.0.0.1:8080")
        .unwrap(),
    );

    assert!(validate_production_profile(&deployment).is_ok());
}

#[test]
fn production_cluster_rejects_plain_http_transports() {
    let deployment = production_manifest(
        RuntimeMode::Cluster,
        NetworkConfig::new(
            ProviderId::new("http").unwrap(),
            ProviderId::new("http").unwrap(),
        ),
    );

    assert!(validate_production_profile(&deployment).is_err());
}

#[test]
fn production_cluster_accepts_mesh_relay_peer_transport_with_https_relay() {
    let deployment = production_cluster_manifest_with_mesh_relay("https://gateway.example.test");

    assert!(provider_plan(&deployment).is_ok());
    assert!(validate_production_profile(&deployment).is_ok());
}

#[test]
fn production_cluster_rejects_mesh_relay_without_https_relay() {
    let deployment = production_cluster_manifest_with_mesh_relay("http://gateway.example.test");

    assert!(provider_plan(&deployment).is_ok());
    assert!(validate_production_profile(&deployment).is_err());
}

#[test]
fn production_non_loopback_listener_requires_tls() {
    let deployment = production_manifest(
        RuntimeMode::Standalone,
        NetworkConfig::new(
            ProviderId::new("http").unwrap(),
            ProviderId::new("http").unwrap(),
        )
        .with_listen_address("0.0.0.0:8080")
        .unwrap(),
    );

    assert!(validate_production_profile(&deployment).is_err());
}

#[test]
fn production_non_loopback_listener_accepts_deployment_tls() {
    let network = NetworkConfig::new(
        ProviderId::new("http").unwrap(),
        ProviderId::new("http").unwrap(),
    )
    .with_listen_address("0.0.0.0:8080")
    .unwrap()
    .with_tls(TlsConfig::enabled(
        SecretRef::new("provider:certificate").unwrap(),
        SecretRef::new("provider:private-key").unwrap(),
    ))
    .unwrap();
    let deployment = production_manifest(RuntimeMode::Standalone, network);

    assert!(validate_production_profile(&deployment).is_ok());
}

#[test]
fn remote_control_plane_requires_https() {
    assert!(require_https_for_remote_endpoint("http://control.example.test").is_err());
    assert!(require_https_for_remote_endpoint("https://control.example.test").is_ok());
    assert!(require_https_for_remote_endpoint("http://127.0.0.1:8080").is_ok());
}

#[test]
fn production_updates_reject_trusted_local_bypass() {
    let update = ProviderConfig::new(ProviderId::new("file-update").unwrap())
        .with_setting("trusted_local", "true")
        .unwrap();
    let deployment = production_manifest_with_update(update);

    assert!(validate_production_profile(&deployment).is_err());
}

#[test]
fn production_updates_require_and_accept_explicit_trust_policy() {
    let incomplete = ProviderConfig::new(ProviderId::new("file-update").unwrap())
        .with_setting("allowed_channels", "stable")
        .unwrap();
    assert!(validate_production_profile(&production_manifest_with_update(incomplete)).is_err());

    let complete = ProviderConfig::new(ProviderId::new("file-update").unwrap())
        .with_setting(
            "signing_key.release-2026",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .unwrap()
        .with_setting("allowed_channels", "stable")
        .unwrap()
        .with_setting("allowed_origins", "https://releases.example.invalid")
        .unwrap();
    assert!(validate_production_profile(&production_manifest_with_update(complete)).is_ok());
}
