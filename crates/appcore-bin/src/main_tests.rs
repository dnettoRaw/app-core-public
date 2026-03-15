// =============================================================================
//        #######
//     ###       ###     F: main_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use crate::bootstrap::bootstrap_runtime;
use crate::cli::{parse_cli_args, RuntimeCliCommand};
use crate::commands::run_cli_with_constants;
use crate::constants::RuntimeHostConstants;
use crate::server::{run_server_with_mode, RuntimeServer, ShutdownToken};
use appcore_contracts::{ApplicationId, ApplicationManifestV1, RuntimeRequirements, ServiceId};
use appcore_core::{
    AppFamily, AppId, CommandEnvelope, CommandName, NodeId, RuntimeContext, RuntimeContractVersion,
    RuntimeLifecycleState, SyncGroup,
};
use appcore_ops::{HeartbeatSource, StdoutLogger};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDirGuard(PathBuf);
static TEMP_DIR_COUNTER: AtomicUsize = AtomicUsize::new(0);

impl TempDirGuard {
    fn new(name: &str) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let counter = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "appcore-{name}-{}-{timestamp}-{counter}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write_private_file(path: &Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
    std::fs::write(path, contents)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn application_manifest() -> ApplicationManifestV1 {
    ApplicationManifestV1::new(
        ApplicationId::new("runtime-test").unwrap(),
        "1.0.0",
        "Runtime Test",
        "AppCore Test",
        ServiceId::new("runtime-test").unwrap(),
        RuntimeRequirements::new("1.0.0-rc.3", "1").unwrap(),
    )
    .unwrap()
}

fn manifest_fixture(application: &ApplicationManifestV1) -> (TempDirGuard, PathBuf) {
    let fixture = TempDirGuard::new("manifest-runtime");
    let application_path = fixture.path().join("application.toml");
    let deployment_path = fixture.path().join("deployment.toml");
    std::fs::write(
        application_path,
        toml::to_string_pretty(application).unwrap(),
    )
    .unwrap();
    std::fs::write(
        &deployment_path,
        format!(
            r#"manifest_version = 1
installation_id = "runtime-test-local"
application_id = "{}"
mode = "standalone"
secrets = {{ runtime_security = "file:runtime.secret" }}
paths = {{ storage = "storage", backup = "backups" }}
volumes = []
adapters = {{}}
environment = {{}}

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
"#,
            application.application_id()
        ),
    )
    .unwrap();
    write_private_file(
        &fixture.path().join("runtime.secret"),
        b"key_id=test\ncreated_at_ms=1\nexpires_at_ms=none\nstatus=active\nsecret=hex:3031323334353637383961626364656630313233343536373839616263646566\n",
    )
    .unwrap();
    (fixture, deployment_path)
}

fn current_manifest_fixture() -> (TempDirGuard, PathBuf) {
    manifest_fixture(&application_manifest())
}

struct StaticContext {
    app_id: AppId,
    app_family: AppFamily,
    sync_group: SyncGroup,
    runtime_contract: RuntimeContractVersion,
    node_id: NodeId,
}

impl RuntimeContext for StaticContext {
    fn app_id(&self) -> &AppId {
        &self.app_id
    }
    fn app_family(&self) -> &AppFamily {
        &self.app_family
    }
    fn sync_group(&self) -> &SyncGroup {
        &self.sync_group
    }
    fn runtime_contract(&self) -> RuntimeContractVersion {
        self.runtime_contract
    }
    fn node_id(&self) -> &NodeId {
        &self.node_id
    }
}

fn args(command: &str) -> Vec<String> {
    vec!["appcore-bin".to_string(), command.to_string()]
}

#[test]
fn parse_server() {
    let parsed = parse_cli_args(&args("server"));
    assert_eq!(parsed.command, Some(RuntimeCliCommand::Server));
}

#[test]
fn parse_doctor() {
    let parsed = parse_cli_args(&args("doctor"));
    assert_eq!(parsed.command, Some(RuntimeCliCommand::Doctor));
}

#[test]
fn parse_unknown_marks_error() {
    let parsed = parse_cli_args(&args("x"));
    assert_eq!(parsed.unknown_command.as_deref(), Some("x"));
}

#[test]
fn parse_unknown_flag_marks_error() {
    let parsed = parse_cli_args(&[
        "appcore-bin".to_string(),
        "status".to_string(),
        "--unknown".to_string(),
    ]);
    assert_eq!(parsed.unknown_command.as_deref(), Some("--unknown"));
}

#[test]
fn parse_help_commands() {
    let help = parse_cli_args(&args("help"));
    assert_eq!(help.command, Some(RuntimeCliCommand::Help));
    let short = parse_cli_args(&["appcore-bin".to_string(), "-h".to_string()]);
    assert_eq!(short.command, Some(RuntimeCliCommand::Help));
    let long = parse_cli_args(&["appcore-bin".to_string(), "--help".to_string()]);
    assert_eq!(long.command, Some(RuntimeCliCommand::Help));
}

#[test]
fn parse_deployment_path() {
    let args = vec![
        "appcore-bin".to_string(),
        "status".to_string(),
        "--deployment".to_string(),
        "deployment.toml".to_string(),
        "--json".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.command, Some(RuntimeCliCommand::Status));
    assert_eq!(parsed.config_path.as_deref(), Some("deployment.toml"));
    assert!(parsed.status_json);
}

#[test]
fn parse_operational_commands() {
    assert_eq!(
        parse_cli_args(&args("health")).command,
        Some(RuntimeCliCommand::Health)
    );
    assert_eq!(
        parse_cli_args(&args("diagnostics")).command,
        Some(RuntimeCliCommand::Diagnostics)
    );
    let config = parse_cli_args(&[
        "appcore-bin".to_string(),
        "config".to_string(),
        "validate".to_string(),
    ]);
    assert_eq!(config.command, Some(RuntimeCliCommand::ConfigValidate));
    let production = parse_cli_args(&[
        "appcore-bin".to_string(),
        "config".to_string(),
        "validate".to_string(),
        "--production".to_string(),
    ]);
    assert!(production.production);
    let export = parse_cli_args(&[
        "appcore-bin".to_string(),
        "export".to_string(),
        "--out".to_string(),
        "diagnostics.json".to_string(),
    ]);
    assert_eq!(export.command, Some(RuntimeCliCommand::Export));
    assert_eq!(export.security_out.as_deref(), Some("diagnostics.json"));
}

#[test]
fn parse_security_keyring_flags() {
    let parsed = parse_cli_args(&[
        "appcore-bin".to_string(),
        "security".to_string(),
        "secret".to_string(),
        "keyring-revoke".to_string(),
        "--keyring".to_string(),
        "/var/lib/appcore/security".to_string(),
        "--key-id".to_string(),
        "key-1".to_string(),
    ]);

    assert_eq!(parsed.command, Some(RuntimeCliCommand::Security));
    assert_eq!(
        parsed.security_secret_action.as_deref(),
        Some("keyring-revoke")
    );
    assert_eq!(
        parsed.security_keyring.as_deref(),
        Some("/var/lib/appcore/security")
    );
    assert_eq!(parsed.security_key_id.as_deref(), Some("key-1"));
}

#[test]
fn parse_security_keyring_recovery_action() {
    let parsed = parse_cli_args(&[
        "appcore-bin".to_string(),
        "security".to_string(),
        "secret".to_string(),
        "keyring-recover".to_string(),
        "--keyring".to_string(),
        "/var/lib/appcore/security".to_string(),
    ]);

    assert_eq!(
        parsed.security_secret_action.as_deref(),
        Some("keyring-recover")
    );
    assert_eq!(
        parsed.security_keyring.as_deref(),
        Some("/var/lib/appcore/security")
    );
}

#[test]
fn parse_backup_file() {
    let args = vec![
        "appcore-bin".to_string(),
        "backup".to_string(),
        "--file".to_string(),
        "foo.txt".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.command, Some(RuntimeCliCommand::Backup));
    assert_eq!(parsed.backup_file.as_deref(), Some("foo.txt"));
}

#[test]
fn parse_backup_restore_requires_explicit_fields() {
    let args = vec![
        "appcore-bin".to_string(),
        "backup".to_string(),
        "restore".to_string(),
        "--name".to_string(),
        "baseline".to_string(),
        "--confirm-restore".to_string(),
    ];
    let parsed = crate::cli::parse_cli_args(&args);
    assert_eq!(parsed.command, Some(RuntimeCliCommand::Backup));
    assert_eq!(parsed.backup_action.as_deref(), Some("restore"));
    assert_eq!(parsed.backup_name.as_deref(), Some("baseline"));
    assert!(parsed.confirm_restore);
}

#[test]
fn parse_version_command() {
    let parsed = parse_cli_args(&args("version"));
    assert_eq!(parsed.command, Some(RuntimeCliCommand::Version));
    let short = parse_cli_args(&["appcore-bin".to_string(), "-V".to_string()]);
    assert_eq!(short.command, Some(RuntimeCliCommand::Version));
    let long = parse_cli_args(&["appcore-bin".to_string(), "--version".to_string()]);
    assert_eq!(long.command, Some(RuntimeCliCommand::Version));
}

#[test]
fn parse_completion_commands() {
    let script = parse_cli_args(&[
        "appcore-bin".to_string(),
        "completions".to_string(),
        "powershell".to_string(),
    ]);
    assert_eq!(script.command, Some(RuntimeCliCommand::Completions));
    assert_eq!(
        script.completion_shell,
        Some(appcore_args::Shell::PowerShell)
    );

    let complete = parse_cli_args(&[
        "appcore-bin".to_string(),
        "complete".to_string(),
        "bash".to_string(),
        "1".to_string(),
        "st".to_string(),
    ]);
    assert_eq!(complete.command, Some(RuntimeCliCommand::Complete));
    assert_eq!(complete.completion_cursor_word, Some(1));
    assert_eq!(complete.completion_words, vec!["st"]);
}

#[test]
fn completion_candidates_come_from_the_runtime_spec() {
    let candidates = crate::cli::completion_candidates(
        &crate::constants::default_host_constants(),
        0,
        vec!["sta".to_string()],
    );

    assert!(candidates.contains(&"status".to_string()));
}

#[test]
fn removed_cli_inputs_hit_the_update_wall_before_parsing() {
    for removed in ["--config", "--payload", "--onlyone", "--killothers"] {
        let parsed = parse_cli_args(&[
            "appcore-bin".to_string(),
            "status".to_string(),
            removed.to_string(),
            "removed".to_string(),
        ]);
        assert!(parsed.update_required, "{removed} bypassed the update wall");
    }
}

#[test]
fn parse_build_info_command() {
    let parsed = parse_cli_args(&args("build-info"));
    assert_eq!(parsed.command, Some(RuntimeCliCommand::BuildInfo));
}

#[test]
fn run_cli_accepts_custom_host_constants() {
    let constants = RuntimeHostConstants::new("Demo Host", "9.9.9").expect("constants");
    let parsed = parse_cli_args(&args("version"));
    assert!(run_cli_with_constants(parsed, &constants).is_ok());
}

#[test]
fn parse_first_run_command() {
    let parsed = parse_cli_args(&args("first-run"));
    assert_eq!(parsed.command, Some(RuntimeCliCommand::FirstRun));
}

#[test]
fn parse_first_run_auth_server_gate() {
    let args = vec![
        "appcore-bin".to_string(),
        "first-run".to_string(),
        "--auth-server-app".to_string(),
        "secret-gate".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.command, Some(RuntimeCliCommand::FirstRun));
    assert_eq!(
        parsed.auth_server_app_password.as_deref(),
        Some("secret-gate")
    );
}

#[test]
fn parse_run_command() {
    let parsed = parse_cli_args(&args("run"));
    assert_eq!(parsed.command, Some(RuntimeCliCommand::Run));
}

#[test]
fn parse_paths_command() {
    let parsed = parse_cli_args(&args("paths"));
    assert_eq!(parsed.command, Some(RuntimeCliCommand::Paths));
}

#[test]
fn parse_last_run_flags() {
    let args = vec![
        "appcore-bin".to_string(),
        "last-run".to_string(),
        "--dry-run".to_string(),
        "--purge".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.command, Some(RuntimeCliCommand::LastRun));
    assert!(parsed.dry_run);
    assert!(parsed.purge);
}

#[test]
fn parse_watch_flag() {
    let args = vec![
        "appcore-bin".to_string(),
        "server".to_string(),
        "--watch".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.command, Some(RuntimeCliCommand::Server));
    assert!(parsed.watch);
}

#[test]
fn parse_token_command() {
    let args = vec![
        "appcore-bin".to_string(),
        "token".to_string(),
        "command".to_string(),
        "--command".to_string(),
        "runtime.ping".to_string(),
        "--subject".to_string(),
        "node-a".to_string(),
        "--scope".to_string(),
        "*".to_string(),
        "--ttl-ms".to_string(),
        "1000".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.command, Some(RuntimeCliCommand::TokenCommand));
    assert_eq!(parsed.token_command.as_deref(), Some("runtime.ping"));
    assert_eq!(parsed.token_subject.as_deref(), Some("node-a"));
    assert_eq!(parsed.token_scope.as_deref(), Some("*"));
    assert_eq!(parsed.token_ttl_ms, Some(1000));
}

#[test]
fn parse_token_sync() {
    let args = vec![
        "appcore-bin".to_string(),
        "token".to_string(),
        "sync".to_string(),
        "--subject".to_string(),
        "node-a".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.command, Some(RuntimeCliCommand::TokenSync));
    assert_eq!(parsed.token_subject.as_deref(), Some("node-a"));
}

#[test]
fn parse_token_query() {
    let args = vec![
        "appcore-bin".to_string(),
        "token".to_string(),
        "query".to_string(),
        "--query".to_string(),
        "runtime.status".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.command, Some(RuntimeCliCommand::TokenQuery));
    assert_eq!(parsed.token_query.as_deref(), Some("runtime.status"));
}

#[test]
fn parse_sync_status() {
    let args = vec![
        "appcore-bin".to_string(),
        "sync".to_string(),
        "status".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.command, Some(RuntimeCliCommand::Sync));
    assert_eq!(parsed.sync_action.as_deref(), Some("status"));
}

#[test]
fn parse_sync_push() {
    let args = vec![
        "appcore-bin".to_string(),
        "sync".to_string(),
        "push".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.command, Some(RuntimeCliCommand::Sync));
    assert_eq!(parsed.sync_action.as_deref(), Some("push"));
}

#[test]
fn parse_idempotency_compact() {
    let args = vec![
        "appcore-bin".to_string(),
        "idempotency".to_string(),
        "compact".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.command, Some(RuntimeCliCommand::IdempotencyCompact));
}

#[test]
fn parse_supervisor_with_max_restarts() {
    let args = vec![
        "appcore-bin".to_string(),
        "supervisor".to_string(),
        "--max-restarts".to_string(),
        "2".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.command, Some(RuntimeCliCommand::Supervisor));
    assert_eq!(parsed.max_restarts, Some(2));
}

#[test]
fn parse_supervisor_health_flags() {
    let args = vec![
        "appcore-bin".to_string(),
        "supervisor".to_string(),
        "--health-url".to_string(),
        "http://127.0.0.1:9000/v1/health".to_string(),
        "--health-check-every-ticks".to_string(),
        "5".to_string(),
        "--health-fail-limit".to_string(),
        "2".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.command, Some(RuntimeCliCommand::Supervisor));
    assert_eq!(
        parsed.health_url.as_deref(),
        Some("http://127.0.0.1:9000/v1/health")
    );
    assert_eq!(parsed.health_check_every_ticks, Some(5));
    assert_eq!(parsed.health_fail_limit, Some(2));
}

#[test]
fn supervisor_without_health_url_preserves_default_behavior() {
    let args = vec!["appcore-bin".to_string(), "supervisor".to_string()];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.command, Some(RuntimeCliCommand::Supervisor));
    assert_eq!(parsed.health_url, None);
}

#[test]
fn max_restarts_zero_does_not_restart() {
    assert!(!crate::supervisor::should_restart(0, 0));
}

#[test]
fn invalid_child_command_returns_controlled_error() {
    let result = crate::supervisor::run_supervisor(crate::supervisor::SupervisorRunOptions {
        config_path: Some("deployment.toml"),
        max_restarts: Some(0),
        child_args: Some("definitely-invalid-subcommand"),
        health_url: None,
        health_check_every_ticks: None,
        health_fail_limit: None,
        only_one: None,
        kill_others: None,
    });
    assert!(result.is_err());
}

#[test]
fn removed_runtime_configuration_hits_update_wall() {
    let fixture = TempDirGuard::new("removed-runtime-config");
    let path = fixture.path().join("runtime.toml");
    std::fs::write(&path, "app_id = \"removed\"\n").unwrap();
    let error = bootstrap_runtime(path.to_str()).err().unwrap();
    assert_eq!(error.to_string(), "NO MORE SUPPORTED PLEASE UPDATE");
}

#[test]
fn manifest_bootstrap_reaches_running() {
    let (_fixture, path) = current_manifest_fixture();
    let app = bootstrap_runtime(path.to_str());
    assert!(app.is_ok(), "bootstrap failed: {:?}", app.as_ref().err());
    let app = match app {
        Ok(app) => app,
        Err(_) => return,
    };
    let lifecycle = app.controller.lock().lifecycle().current();
    assert_eq!(lifecycle, RuntimeLifecycleState::Running);
    assert!(app.security_ok);
}

#[test]
fn bootstrap_reports_the_foundation_phase_order() {
    let (_fixture, path) = current_manifest_fixture();
    let app = bootstrap_runtime(path.to_str()).unwrap();
    let expected = [
        "runtime.deployment_manifest.ready",
        "runtime.secret_provider.ready",
        "runtime.application_manifest.ready",
        "runtime.configuration.ready",
        "runtime.runtime_manifest.ready",
        "runtime.providers.ready",
        "runtime.execution.ready",
        "runtime.services.ready",
        "runtime.bootstrap.ready",
    ];
    let actual = app
        .observations
        .snapshot()
        .into_iter()
        .map(|event| event.name)
        .filter(|name| expected.contains(&name.as_str()))
        .collect::<Vec<_>>();

    assert_eq!(actual, expected);
}

#[test]
fn runtime_server_starts_running() {
    let (_fixture, path) = current_manifest_fixture();
    let app = bootstrap_runtime(path.to_str());
    assert!(app.is_ok());
    let app = match app {
        Ok(app) => app,
        Err(_) => return,
    };
    let server = RuntimeServer::new(app, StdoutLogger::new());
    assert!(server.is_running());
}

#[test]
fn runtime_server_tick_increments() {
    let (_fixture, path) = current_manifest_fixture();
    let app = bootstrap_runtime(path.to_str());
    assert!(app.is_ok());
    let app = match app {
        Ok(app) => app,
        Err(_) => return,
    };
    let mut server = RuntimeServer::new(app, StdoutLogger::new());
    assert!(server.tick().is_ok());
    assert_eq!(server.tick_count, 1);
}

#[test]
fn runtime_server_shutdown_leads_to_stopped() {
    let (_fixture, path) = current_manifest_fixture();
    let app = bootstrap_runtime(path.to_str());
    assert!(app.is_ok());
    let app = match app {
        Ok(app) => app,
        Err(_) => return,
    };
    let mut server = RuntimeServer::new(app, StdoutLogger::new());
    assert!(server.request_shutdown().is_ok());
    assert!(!server.is_running());
    assert_eq!(
        server.app.controller.lock().lifecycle().current(),
        RuntimeLifecycleState::Stopped
    );
}

#[test]
fn server_without_watch_terminates_ok() {
    let (_fixture, path) = current_manifest_fixture();
    assert!(run_server_with_mode(path.to_str(), false, Some(false), None).is_ok());
}

#[test]
fn shutdown_token_request_changes_state() {
    let mut token = ShutdownToken::default();
    assert!(!token.is_requested());
    token.request();
    assert!(token.is_requested());
}

#[test]
fn run_until_shutdown_with_limit_runs_expected_ticks() {
    let (_fixture, path) = current_manifest_fixture();
    let app = bootstrap_runtime(path.to_str());
    assert!(app.is_ok());
    let app = match app {
        Ok(app) => app,
        Err(_) => return,
    };
    let mut server = RuntimeServer::new(app, StdoutLogger::new());
    assert!(server.run_until_shutdown(Some(3)).is_ok());
    assert_eq!(server.tick_count, 3);
    assert!(server.is_running());
}

#[test]
fn shutdown_requested_before_loop_leads_to_stopped() {
    let (_fixture, path) = current_manifest_fixture();
    let app = bootstrap_runtime(path.to_str());
    assert!(app.is_ok());
    let app = match app {
        Ok(app) => app,
        Err(_) => return,
    };
    let mut server = RuntimeServer::new(app, StdoutLogger::new());
    server.shutdown_token.request();
    assert!(server.run_until_shutdown(Some(5)).is_ok());
    assert!(!server.is_running());
    assert_eq!(
        server.app.controller.lock().lifecycle().current(),
        RuntimeLifecycleState::Stopped
    );
}

#[test]
fn shutdown_requested_during_tick_leads_to_stopped() {
    let (_fixture, path) = current_manifest_fixture();
    let app = bootstrap_runtime(path.to_str());
    assert!(app.is_ok());
    let app = match app {
        Ok(app) => app,
        Err(_) => return,
    };
    let mut server = RuntimeServer::new(app, StdoutLogger::new());
    assert!(server.tick().is_ok());
    server.shutdown_token.request();
    assert!(server.tick().is_ok());
    assert!(!server.is_running());
    assert_eq!(
        server.app.controller.lock().lifecycle().current(),
        RuntimeLifecycleState::Stopped
    );
}

#[test]
fn bootstrap_heartbeat_contains_node_id() {
    let (_fixture, path) = current_manifest_fixture();
    let app = bootstrap_runtime(path.to_str());
    assert!(app.is_ok());
    let app = match app {
        Ok(app) => app,
        Err(_) => return,
    };
    let hb = app.heartbeat_source.heartbeat();
    assert_eq!(hb.node_id.as_str(), app.config.node_id);
}

#[test]
fn runtime_ping_dispatch_records_event_and_audit() {
    let (_fixture, path) = current_manifest_fixture();
    let app = bootstrap_runtime(path.to_str());
    assert!(app.is_ok());
    let app = match app {
        Ok(app) => app,
        Err(_) => return,
    };
    let identity = app.controller.lock().instance().identity().clone();
    let context = StaticContext {
        app_id: identity.app_id.clone(),
        app_family: identity.app_family.clone(),
        sync_group: identity.sync_group.clone(),
        runtime_contract: identity.runtime_contract,
        node_id: identity.node_id.clone(),
    };
    let command = CommandEnvelope::new(
        CommandName::new("runtime.ping".to_string()).unwrap(),
        "cmd-1".to_string(),
        identity.app_id.clone(),
        identity.node_id.clone(),
        0,
        None,
        b"hello".to_vec(),
    );
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };
    let result = app.controller.lock().dispatch_command(&command, &context);
    assert!(result.is_ok());
    let result = match result {
        Ok(result) => result,
        Err(_) => return,
    };
    assert!(result.is_accepted());
    let (event_count, audit_count) = {
        let controller = app.controller.lock();
        (
            controller.instance().event_bus().len(),
            controller.instance().audit_log().len(),
        )
    };
    assert_eq!(event_count, 1);
    assert_eq!(audit_count, 1);
}

#[test]
fn parse_only_one_and_kill_others_flags() {
    let args = vec![
        "appcore-bin".to_string(),
        "server".to_string(),
        "--only-one".to_string(),
        "false".to_string(),
        "--kill-others".to_string(),
        "true".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.only_one, Some(false));
    assert_eq!(parsed.kill_others, Some(true));
}

#[test]
fn parse_only_one_and_kill_others_negatives() {
    let args = vec![
        "appcore-bin".to_string(),
        "server".to_string(),
        "--no-only-one".to_string(),
        "--no-kill-others".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.only_one, Some(false));
    assert_eq!(parsed.kill_others, Some(false));
}

#[test]
fn parse_only_one_and_kill_others_implict_flags() {
    let args = vec![
        "appcore-bin".to_string(),
        "server".to_string(),
        "--only-one".to_string(),
        "--kill-others".to_string(),
    ];
    let parsed = parse_cli_args(&args);
    assert_eq!(parsed.only_one, Some(true));
    assert_eq!(parsed.kill_others, Some(true));
}

struct TestCustomAppPlugin;

impl appcore_core::AppPlugin for TestCustomAppPlugin {
    fn application_manifest(&self) -> appcore_contracts::ApplicationManifestV1 {
        appcore_contracts::ApplicationManifestV1::new(
            appcore_contracts::ApplicationId::new("my-custom-test-app").unwrap(),
            "2.0.0",
            "Custom Test App",
            "Test Vendor",
            appcore_contracts::ServiceId::new("custom-service").unwrap(),
            appcore_contracts::RuntimeRequirements::new("1.0.0", "1").unwrap(),
        )
        .unwrap()
    }

    fn identity(&self, node_id: NodeId) -> appcore_core::RuntimeIdentity {
        appcore_core::RuntimeIdentity {
            app_id: AppId::new("my-custom-test-app".to_string()).unwrap(),
            app_family: AppFamily::new("my-custom-test-family".to_string()).unwrap(),
            sync_group: SyncGroup::new("test-group".to_string()).unwrap(),
            runtime_contract: RuntimeContractVersion::new(1),
            node_id,
        }
    }

    fn register_commands(
        &self,
        registry: &mut appcore_core::CommandRegistry,
    ) -> appcore_core::RuntimeResult<()> {
        registry.register(CommandName::new("custom.do_something".to_string()).unwrap())
    }

    fn register_events(
        &self,
        registry: &mut appcore_core::EventRegistry,
    ) -> appcore_core::RuntimeResult<()> {
        registry.register(appcore_core::EventName::new("custom.done".to_string()).unwrap())
    }

    fn register_states(
        &self,
        _registry: &mut appcore_core::StateRegistry,
    ) -> appcore_core::RuntimeResult<()> {
        Ok(())
    }

    fn register_decisions(
        &self,
        _registry: &mut appcore_core::DecisionRegistry,
    ) -> appcore_core::RuntimeResult<()> {
        Ok(())
    }
}

#[test]
fn bootstrap_runtime_with_custom_app_plugin() {
    let plugin = TestCustomAppPlugin;
    let manifest = appcore_core::AppPlugin::application_manifest(&plugin);
    let (_fixture, deployment_path) = manifest_fixture(&manifest);
    let res = crate::bootstrap::bootstrap_runtime_with_plugin(
        Some(deployment_path.to_str().unwrap()),
        Some(&plugin),
    );
    assert!(res.is_ok(), "bootstrap failed with error: {:?}", res.err());
    let app = res.unwrap();
    assert_eq!(app.config.app_id, "my-custom-test-app");
    assert_eq!(app.config.service_id, "custom-service");
    assert_eq!(app.application_manifest.vendor(), "Test Vendor");
    assert_eq!(
        app.runtime_manifest.core_profile().service_id().as_str(),
        "custom-service"
    );

    // Verify both our command and the default ping command are registered
    let controller = app.controller.lock();
    assert!(controller
        .instance()
        .commands()
        .contains(&CommandName::new("custom.do_something".to_string()).unwrap()));
    assert!(controller
        .instance()
        .commands()
        .contains(&CommandName::new("runtime.ping".to_string()).unwrap()));

    drop(controller);
}
