// =============================================================================
//        #######
//     ###       ###     F: gateway_service_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/20 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/20 00:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{
    authorize_gateway_if_configured, gateway_config_from_manifest, selected_gateway_replay_path,
};
use crate::application::Application;
use crate::application_host::ManifestApplicationHost;
use crate::bootstrap::bootstrap_runtime;
use appcore_contracts::{
    ApplicationId, ApplicationManifestV1, DeploymentManifestV1, RuntimeMode, RuntimeRequirements,
    ServiceId,
};
use appcore_gateway::{GatewayRuntimeState, GATEWAY_RUNTIME_CAPABILITY};
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct EmptyApplication;

impl Application for EmptyApplication {}

struct ManifestFixture {
    root: PathBuf,
    application: PathBuf,
    deployment: PathBuf,
}

impl ManifestFixture {
    fn new(gateway_settings: Option<&str>) -> Self {
        let root = unique_directory();
        std::fs::create_dir_all(&root).unwrap();
        let application = root.join("application.toml");
        let deployment = root.join("deployment.toml");
        let manifest = ApplicationManifestV1::new(
            ApplicationId::new("gateway-host-test").unwrap(),
            "1.0.0",
            "Gateway Host Test",
            "dnettoRaw",
            ServiceId::new("gateway-host-test").unwrap(),
            RuntimeRequirements::new("1.0.0-rc.3", "1").unwrap(),
        )
        .unwrap();
        std::fs::write(&application, toml::to_string_pretty(&manifest).unwrap()).unwrap();
        std::fs::write(&deployment, deployment_manifest(gateway_settings)).unwrap();
        write_private_secret(&root.join("runtime.secret"));
        Self {
            root,
            application,
            deployment,
        }
    }
}

impl Drop for ManifestFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn disabled_gateway_starts_host_without_binding_a_gateway_port() {
    let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
    let fixture = ManifestFixture::new(None);
    let host =
        ManifestApplicationHost::load(&fixture.application, &fixture.deployment, &EmptyApplication)
            .unwrap();

    let report = host.probe_services(Duration::from_secs(1)).unwrap();

    assert!(!report.gateway_started);
    assert_eq!(report.gateway_state, None);
    assert_eq!(report.gateway_bind_address, None);
    assert!(occupied.local_addr().is_ok());
}

#[test]
fn enabled_gateway_is_composed_reported_and_stopped_with_the_host() {
    let address = available_address();
    let settings = gateway_settings(address);
    let fixture = ManifestFixture::new(Some(&settings));
    let host =
        ManifestApplicationHost::load(&fixture.application, &fixture.deployment, &EmptyApplication)
            .unwrap();
    assert!(host
        .runtime_manifest()
        .unwrap()
        .loaded_capabilities()
        .iter()
        .any(|capability| capability.as_str() == GATEWAY_RUNTIME_CAPABILITY));

    let report = host.probe_services(Duration::from_secs(2)).unwrap();

    assert!(report.gateway_started);
    assert_eq!(report.gateway_state, Some(GatewayRuntimeState::Running));
    assert_eq!(report.gateway_bind_address, Some(address));
    assert!(TcpListener::bind(address).is_ok());
    assert!(fixture
        .root
        .join("storage/security/gateway-connection-jti.lock")
        .is_file());
}

#[test]
fn cluster_gateway_requires_an_explicit_shared_replay_path() {
    let root = unique_directory();
    std::fs::create_dir_all(&root).unwrap();

    let error =
        selected_gateway_replay_path(RuntimeMode::Cluster, None, &root, "storage").unwrap_err();
    assert!(error
        .to_string()
        .contains("paths.gateway_replay on one shared writable volume"));

    let relative = selected_gateway_replay_path(
        RuntimeMode::Cluster,
        Some("shared/gateway-replay.json"),
        &root,
        "storage",
    )
    .unwrap_err();
    assert!(relative.to_string().contains("must be an absolute"));

    let absolute = root.join("shared/gateway-replay.json");
    let selected =
        selected_gateway_replay_path(RuntimeMode::Cluster, absolute.to_str(), &root, "storage")
            .unwrap();
    assert_eq!(selected, absolute);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_gateway_bind_and_auth_override_fail_during_manifest_bootstrap() {
    let invalid_bind = ManifestFixture::new(Some(
        "bind_address = \"invalid\"\ndomain_suffix = \"gateway.test\"",
    ));
    let bind_error = ManifestApplicationHost::load(
        &invalid_bind.application,
        &invalid_bind.deployment,
        &EmptyApplication,
    )
    .err()
    .unwrap();
    assert!(bind_error.to_string().contains("invalid bind_address"));

    let auth_override = ManifestFixture::new(Some(
        "bind_address = \"127.0.0.1:39091\"\ndomain_suffix = \"gateway.test\"\nauth = \"false\"",
    ));
    let auth_error = ManifestApplicationHost::load(
        &auth_override.application,
        &auth_override.deployment,
        &EmptyApplication,
    )
    .err()
    .unwrap();
    assert!(auth_error
        .to_string()
        .contains("unsupported gateway setting: auth"));
}

#[test]
fn occupied_gateway_port_fails_closed_instead_of_reporting_startup() {
    let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = occupied.local_addr().unwrap();
    let settings = gateway_settings(address);
    let fixture = ManifestFixture::new(Some(&settings));
    let host =
        ManifestApplicationHost::load(&fixture.application, &fixture.deployment, &EmptyApplication)
            .unwrap();

    let error = host.probe_services(Duration::from_secs(1)).unwrap_err();

    assert!(error
        .to_string()
        .contains("failed to bind gateway listener"));
    drop(occupied);
    assert!(TcpListener::bind(address).is_ok());
}

#[test]
fn configured_gateway_is_denied_when_capability_catalog_lacks_its_descriptor() {
    let address = available_address();
    let requested = ManifestFixture::new(Some(&gateway_settings(address)));
    let deployment = toml::from_str::<DeploymentManifestV1>(
        &std::fs::read_to_string(&requested.deployment).unwrap(),
    )
    .unwrap();
    let config = gateway_config_from_manifest(&deployment).unwrap().unwrap();
    let catalog_without_gateway = ManifestFixture::new(None);
    let runtime = bootstrap_runtime(catalog_without_gateway.deployment.to_str()).unwrap();

    let error =
        authorize_gateway_if_configured(&runtime.capability_policy, Some(&config)).unwrap_err();

    assert!(error.to_string().contains("gateway capability denied"));
    assert!(TcpListener::bind(address).is_ok());
}

fn deployment_manifest(gateway_settings: Option<&str>) -> String {
    let adapters = gateway_settings.map_or_else(
        || "adapters = {}".to_string(),
        |settings| {
            format!(
                "[adapters.gateway]\nprovider_id = \"appcore-gateway\"\nsettings = {{ {}}}\nsecret_refs = {{}}",
                settings.replace('\n', ", ")
            )
        },
    );
    format!(
        r#"manifest_version = 1
installation_id = "gateway-host-local"
application_id = "gateway-host-test"
mode = "standalone"
secrets = {{ runtime_security = "file:runtime.secret" }}
paths = {{ storage = "storage", backup = "backups" }}
volumes = []
environment = {{}}
{adapters}

[storage]
provider_id = "file"
settings = {{}}
secret_refs = {{}}

[network]
listen_addresses = []
peer_transport = "http"
command_transport = "http"

[network.tls]
enabled = false
"#
    )
}

fn gateway_settings(address: SocketAddr) -> String {
    format!(
        "bind_address = \"{address}\"\ndomain_suffix = \"gateway.test\"\nheartbeat_interval_ms = \"1000\"\nheartbeat_timeout_ms = \"3000\""
    )
}

fn available_address() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
}

fn unique_directory() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "appcore-gateway-host-{}-{timestamp}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

fn write_private_secret(path: &Path) {
    std::fs::write(
        path,
        b"key_id=test\ncreated_at_ms=1\nexpires_at_ms=none\nstatus=active\nsecret=hex:3031323334353637383961626364656630313233343536373839616263646566\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}
