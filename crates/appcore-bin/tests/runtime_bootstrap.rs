// =============================================================================
//        #######
//     ###       ###     F: runtime_bootstrap.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 13:45:20 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

//! Process-level checks for the manifest-first Runtime CLI.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const UPDATE_WALL: &str = "NO MORE SUPPORTED PLEASE UPDATE";

struct RuntimeFixture {
    root: PathBuf,
    deployment: PathBuf,
}

impl RuntimeFixture {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "appcore-current-manifests-{}-{timestamp}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        write_application_manifest(&root);
        write_deployment_manifest(&root);
        write_secret(&root);
        Self {
            deployment: root.join("deployment.toml"),
            root,
        }
    }
}

impl Drop for RuntimeFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_application_manifest(root: &Path) {
    fs::write(
        root.join("application.toml"),
        r#"manifest_version = 1
application_id = "process-test"
application_version = "1.0.0"
display_name = "Process Test"
vendor = "AppCore Test"
service_id = "process-test"
capabilities = []
leadership = []
dependencies = []
modules = []
feature_flags = {}
metadata = {}

[runtime]
minimum_runtime_version = "1.0.0-rc.3"
protocol_version = "1"
required_features = []

[jobs]
enabled = false
max_concurrency = 0
retry_limit = 0

[storage]
durability = "local"
minimum_bytes = 0
shared = false

[scheduler]
required = false
max_concurrency = 0

[health]
startup_grace_ms = 30000
heartbeat_interval_ms = 10000
failure_threshold = 3

[update]
channel = "stable"
automatic = false
"#,
    )
    .unwrap();
}

fn write_deployment_manifest(root: &Path) {
    fs::write(
        root.join("deployment.toml"),
        r#"manifest_version = 1
installation_id = "process-test-local"
application_id = "process-test"
mode = "standalone"
secrets = { runtime_security = "file:runtime.secret" }
paths = { storage = "storage", backup = "backups" }
volumes = []
adapters = {}
environment = {}

[storage]
provider_id = "file"
settings = {}
secret_refs = {}

[network]
listen_addresses = []
peer_transport = "http"
command_transport = "http"

[network.tls]
enabled = false
"#,
    )
    .unwrap();
}

fn write_secret(root: &Path) {
    let path = root.join("runtime.secret");
    fs::write(
        &path,
        "key_id=process-test\ncreated_at_ms=1\nexpires_at_ms=none\nstatus=active\nsecret=hex:3031323334353637383961626364656630313233343536373839616263646566\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_appcore-bin"))
        .args(arguments)
        .output()
        .unwrap()
}

fn combined_output(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn current_manifests_bootstrap_the_status_command() {
    let fixture = RuntimeFixture::new();
    let output = run(&[
        "status",
        "--deployment",
        fixture.deployment.to_str().unwrap(),
        "--json",
    ]);
    assert!(output.status.success(), "{}", combined_output(&output));
    assert!(String::from_utf8_lossy(&output.stdout).contains("\"application_id\":\"process-test\""));
}

#[test]
fn incompatible_storage_capability_fails_before_runtime_startup() {
    let fixture = RuntimeFixture::new();
    let deployment = fs::read_to_string(&fixture.deployment).unwrap().replace(
        "settings = {}",
        "settings = { required_capabilities = \"transactions\" }",
    );
    fs::write(&fixture.deployment, deployment).unwrap();
    let output = run(&[
        "status",
        "--deployment",
        fixture.deployment.to_str().unwrap(),
        "--json",
    ]);
    assert!(!output.status.success());
    let message = combined_output(&output);
    assert!(message.contains("storage capability preflight failed"));
    assert!(message.contains("does not support required capability transactions"));
}

#[test]
fn removed_config_flag_hits_the_update_wall() {
    let output = run(&["status", "--config", "runtime.toml"]);
    assert!(!output.status.success());
    assert!(combined_output(&output).contains(UPDATE_WALL));
}

#[test]
fn removed_runtime_file_hits_the_update_wall() {
    let fixture = RuntimeFixture::new();
    let removed = fixture.root.join("runtime.toml");
    fs::write(&removed, "app_id = \"process-test\"\n").unwrap();
    let output = run(&["status", "--deployment", removed.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(combined_output(&output).contains(UPDATE_WALL));
}

#[test]
fn removed_migrate_command_hits_the_update_wall() {
    let output = run(&["migrate"]);
    assert!(!output.status.success());
    assert!(combined_output(&output).contains(UPDATE_WALL));
}
