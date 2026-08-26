// =============================================================================
//        #######
//     ###       ###     F: commands.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Owns CLI command dispatch and local operational commands.

#[path = "commands/actions.rs"]
mod actions;
#[path = "commands/backup.rs"]
mod backup;
#[path = "commands/diagnostics.rs"]
mod diagnostics;
#[path = "commands/doctor.rs"]
mod doctor;

use crate::bootstrap::{bootstrap_runtime, load_config, now_ms, BootstrapError, BootstrapResult};
use crate::build_info::print_build_info;
use crate::cli::{CliArgs, RuntimeCliCommand};
use crate::constants::{default_host_constants, RuntimeHostConstants, DEFAULT_HELP_LINES};
use crate::local_lifecycle::{run_first_run, run_last_run, run_paths};
use crate::paths::{AppCorePaths, PathInputs};
use crate::security_cli::{
    run_security_keyring_init, run_security_keyring_recover, run_security_keyring_revoke,
    run_security_keyring_rotate, run_security_keyring_status, run_security_secret_rotate,
    run_security_secret_status, run_token_command, run_token_query, run_token_sync,
};
use crate::server::run_server_with_mode;
use crate::supervisor::{run_supervisor, SupervisorRunOptions};
use crate::sync_cli::run_sync_with_action;
use actions::{run_security_action, run_supervisor_action, run_token_action};
use appcore_core::{FileIdempotencyStore, RuntimeLifecycleState};
use appcore_ops::{HealthCheck, HeartbeatSource, RuntimeAvailabilityReport};
use appcore_storage::StorageProvider;
use backup::run_backup;
use diagnostics::{run_diagnostics, run_export};
use doctor::run_doctor;
use std::path::PathBuf;

fn run_status(config_path: Option<&str>, as_json: bool) -> Result<(), BootstrapError> {
    let app = bootstrap_runtime(config_path)?;
    if as_json {
        print_status_json(&app);
        return Ok(());
    }
    print_status_text(&app);
    Ok(())
}

fn print_status_text(app: &BootstrapResult) {
    let _ = app.observation_file_sink.flush();
    let health = app.storage_provider.health();
    let report = app.health_check.check();
    let heartbeat = app.heartbeat_source.heartbeat();
    let lifecycle = app.controller.lock().lifecycle().current();
    let availability =
        RuntimeAvailabilityReport::evaluate(report.status, *app.operation_mode.lock());
    println!("app_id: {}", app.config.app_id);
    println!("node_id: {}", app.config.node_id);
    println!("runtime_mode: {:?}", app.deployment_manifest.mode());
    println!(
        "application_manifest_version: {}",
        app.application_manifest.manifest_version()
    );
    println!(
        "runtime_manifest_version: {}",
        app.runtime_manifest.manifest_version()
    );
    println!("lifecycle: {:?}", lifecycle);
    if lifecycle == RuntimeLifecycleState::Stopped {
        println!("lifecycle_final: {:?}", lifecycle);
    }
    println!("storage_status: {:?}", health.status);
    println!("health_status: {:?}", report.status);
    println!("availability_state: {:?}", availability.state);
    println!("liveness: {}", availability.liveness);
    println!("local_readiness: {}", availability.local_readiness);
    println!(
        "distributed_readiness: {}",
        availability.distributed_readiness
    );
    println!("write_readiness: {}", availability.write_readiness);
    println!("security_ok: {}", app.security_ok);
    if let Some(warning) = &app.security_warning {
        println!("security_warning: {warning}");
    }
    println!("api_enabled: {}", app.config.api_enabled);
    println!("api_host: {}", app.config.api_host);
    println!("api_port: {}", app.config.api_port);
    println!("heartbeat_node_id: {}", heartbeat.node_id.as_str());
    println!("heartbeat_timestamp_ms: {}", heartbeat.timestamp_ms);
    println!("observation_count: {}", app.observations.len());
    let observation_stats = app.observation_file_sink.stats();
    println!("observation_written: {}", observation_stats.written);
    println!("observation_dropped: {}", observation_stats.dropped);
    println!("observation_errors: {}", observation_stats.errors);
    println!("metrics_count: {}", app.metrics.snapshot().len());
    if let Some(log) = &app.replication_log {
        match log.lock().len() {
            Ok(len) => println!("sync_log_len: {len}"),
            Err(_) => println!("sync_log_observation_error: true"),
        }
    }
    if let Some(path) = &app.replication_log_path {
        println!("sync_log_path: {}", path.display());
    }
    if let Some(path) = &app.checkpoint_path {
        println!("sync_checkpoint_path: {}", path.display());
    }
    print_local_auth_status_text(app);
}

fn print_status_json(app: &BootstrapResult) {
    println!("{}", status_json_value(app));
}

pub(super) fn status_json_value(app: &BootstrapResult) -> serde_json::Value {
    let _ = app.observation_file_sink.flush();
    let health = app.storage_provider.health();
    let report = app.health_check.check();
    let heartbeat = app.heartbeat_source.heartbeat();
    let lifecycle = app.controller.lock().lifecycle().current();
    let availability =
        RuntimeAvailabilityReport::evaluate(report.status, *app.operation_mode.lock());
    let (sync_log_len, sync_log_observation_ok) = if let Some(log) = &app.replication_log {
        match log.lock().len() {
            Ok(length) => (Some(length), true),
            Err(_) => (None, false),
        }
    } else {
        (Some(0), true)
    };
    let auth_status = local_auth_status(app);
    let runtime_manifest = crate::manifests::current_runtime_manifest(app).ok();
    let observation_stats = app.observation_file_sink.stats();
    let metrics = app.metrics.snapshot();
    serde_json::json!({
        "app_id": app.config.app_id,
        "node_id": app.config.node_id,
        "application_manifest": app.application_manifest,
        "deployment_manifest": app.deployment_manifest,
        "runtime_manifest": runtime_manifest,
        "lifecycle": format!("{lifecycle:?}"),
        "lifecycle_final": (lifecycle == RuntimeLifecycleState::Stopped).then_some("Stopped"),
        "storage_status": format!("{:?}", health.status),
        "health_status": format!("{:?}", report.status),
        "availability": availability,
        "security_ok": app.security_ok,
        "security_warning": app.security_warning,
        "api_enabled": app.config.api_enabled,
        "api_host": app.config.api_host,
        "api_port": app.config.api_port,
        "heartbeat_node_id": heartbeat.node_id.as_str(),
        "heartbeat_timestamp_ms": heartbeat.timestamp_ms,
        "observation_count": app.observations.len(),
        "observation_drain": {
            "written": observation_stats.written,
            "dropped": observation_stats.dropped,
            "errors": observation_stats.errors
        },
        "metrics": metrics,
        "sync_log_len": sync_log_len,
        "sync_log_observation_ok": sync_log_observation_ok,
        "sync_log_path": app.replication_log_path.as_ref().map(|path| path.to_string_lossy()),
        "sync_checkpoint_path": app.checkpoint_path.as_ref().map(|path| path.to_string_lossy()),
        "auth_server_required": auth_status.required,
        "auth_server_installed": auth_status.installed,
        "auth_server_secret_present": auth_status.secret_present,
        "auth_transport_secret_present": auth_status.transport_secret_present
    })
}

#[derive(Debug, Clone, Copy, Default)]
struct LocalAuthStatus {
    required: bool,
    installed: bool,
    secret_present: bool,
    transport_secret_present: bool,
}

fn print_local_auth_status_text(app: &BootstrapResult) {
    let status = local_auth_status(app);
    println!("auth_server_required: {}", status.required);
    println!("auth_server_installed: {}", status.installed);
    println!("auth_server_secret_present: {}", status.secret_present);
    println!(
        "auth_transport_secret_present: {}",
        status.transport_secret_present
    );
}

fn local_auth_status(app: &BootstrapResult) -> LocalAuthStatus {
    let Some(paths) = local_auth_paths(app) else {
        return LocalAuthStatus::default();
    };
    LocalAuthStatus {
        required: paths.auth_required(),
        installed: paths.auth_server_installed(),
        secret_present: paths.auth_server_secret_present(),
        transport_secret_present: paths.auth_transport_secret_present(),
    }
}

fn local_auth_paths(app: &BootstrapResult) -> Option<AppCorePaths> {
    let inputs = PathInputs {
        app_id: app.config.app_id.clone(),
        ..PathInputs::default()
    };
    AppCorePaths::from_inputs(inputs).ok()
}

fn run_health(config_path: Option<&str>, as_json: bool) -> Result<(), BootstrapError> {
    let app = bootstrap_runtime(config_path)?;
    let report = app.health_check.check();
    let journal_ok = {
        let controller = app.controller.lock();
        controller
            .instance()
            .audit_log()
            .durability_error()
            .is_none()
            && controller
                .instance()
                .event_bus()
                .durability_error()
                .is_none()
    };
    if as_json {
        println!(
            "{}",
            serde_json::json!({
                "status": format!("{:?}", report.status),
                "message": report.message,
                "storage": format!("{:?}", app.storage_provider.health().status),
                "security_ok": app.security_ok,
                "operational_journal_ok": journal_ok
            })
        );
    } else {
        println!("health_status: {:?}", report.status);
        println!(
            "health_message: {}",
            report.message.as_deref().unwrap_or("none")
        );
        println!("security_ok: {}", app.security_ok);
        println!("operational_journal_ok: {journal_ok}");
    }
    Ok(())
}

fn run_config_validate(config_path: Option<&str>, production: bool) -> Result<(), BootstrapError> {
    let config = load_config(config_path)?;
    if production {
        let deployment = crate::manifests::load_deployment_manifest_for_config(&config)?;
        crate::providers::validate_production_profile(&deployment)?;
    }
    println!("config_valid: true");
    println!("production_profile: {production}");
    println!("app_id: {}", config.app_id);
    println!("node_id: {}", config.node_id);
    println!("operation_mode: {}", config.operation_mode.as_str());
    Ok(())
}

fn run_vault() {
    println!("vault state: contract-only; no production vault implementation is bundled");
}

fn run_idempotency_compact(config_path: Option<&str>) -> Result<(), BootstrapError> {
    let config = load_config(config_path)?;
    let idempotency_path = PathBuf::from(&config.storage_path).join("idempotency.txt");
    let mut store =
        FileIdempotencyStore::new_with_ttl(&idempotency_path, Some(config.idempotency_ttl_ms))
            .map_err(|_| BootstrapError::Runtime("failed to init idempotency store".to_string()))?;
    let removed = store
        .compact(now_ms())
        .map_err(|_| BootstrapError::Runtime("idempotency compact failed".to_string()))?;
    println!("idempotency compact removed: {removed}");
    Ok(())
}

fn print_help(constants: &RuntimeHostConstants, path: &[String]) -> Result<(), BootstrapError> {
    if path.is_empty()
        && !constants
            .help_lines
            .iter()
            .map(String::as_str)
            .eq(DEFAULT_HELP_LINES.iter().copied())
    {
        println!("{} host", constants.app_name);
        println!("version: {}", constants.app_version);
        println!();
        for line in &constants.help_lines {
            println!("{line}");
        }
        return Ok(());
    }
    let help = crate::cli::render_help(constants, path).map_err(BootstrapError::Cli)?;
    print!("{help}");
    Ok(())
}

fn print_host_version(constants: &RuntimeHostConstants) {
    println!("{}", constants.app_version);
}

fn run_completion(parsed: CliArgs, constants: &RuntimeHostConstants) -> Result<(), BootstrapError> {
    match parsed.command {
        Some(RuntimeCliCommand::Completions) => {
            let shell = parsed
                .completion_shell
                .ok_or_else(|| BootstrapError::Cli("missing completion shell".to_string()))?;
            let script =
                crate::cli::completion_script(constants, shell).map_err(BootstrapError::Cli)?;
            print!("{script}");
            Ok(())
        }
        Some(RuntimeCliCommand::Complete) => {
            let cursor = parsed
                .completion_cursor_word
                .ok_or_else(|| BootstrapError::Cli("missing completion cursor".to_string()))?;
            for candidate in
                crate::cli::completion_candidates(constants, cursor, parsed.completion_words)
            {
                println!("{candidate}");
            }
            Ok(())
        }
        _ => Err(BootstrapError::Cli(
            "invalid completion command".to_string(),
        )),
    }
}

pub fn run_cli(parsed: CliArgs) -> Result<(), BootstrapError> {
    run_cli_with_constants(parsed, &default_host_constants())
}

pub fn run_cli_with_constants(
    parsed: CliArgs,
    constants: &RuntimeHostConstants,
) -> Result<(), BootstrapError> {
    if parsed.update_required {
        return Err(BootstrapError::Runtime(
            "NO MORE SUPPORTED PLEASE UPDATE".to_string(),
        ));
    }
    if let Some(err) = parsed.unknown_command {
        return Err(BootstrapError::Cli(format!("unknown command: {err}")));
    }
    match parsed.command {
        Some(RuntimeCliCommand::Help) => print_help(constants, &parsed.help_path),
        Some(RuntimeCliCommand::Server) => run_server_with_mode(
            parsed.config_path.as_deref(),
            parsed.watch,
            parsed.only_one,
            parsed.kill_others,
        ),
        Some(RuntimeCliCommand::Status) => {
            run_status(parsed.config_path.as_deref(), parsed.status_json)
        }
        Some(RuntimeCliCommand::Health) => {
            run_health(parsed.config_path.as_deref(), parsed.status_json)
        }
        Some(RuntimeCliCommand::Doctor) => {
            run_doctor(parsed.config_path.as_deref(), parsed.status_json)
        }
        Some(RuntimeCliCommand::ConfigValidate) => {
            run_config_validate(parsed.config_path.as_deref(), parsed.production)
        }
        Some(RuntimeCliCommand::Diagnostics) => {
            run_diagnostics(parsed.config_path.as_deref(), parsed.status_json)
        }
        Some(RuntimeCliCommand::Export) => run_export(
            parsed.config_path.as_deref(),
            parsed.security_out.as_deref(),
        ),
        Some(RuntimeCliCommand::Version) => {
            print_host_version(constants);
            Ok(())
        }
        Some(RuntimeCliCommand::BuildInfo) => {
            print_build_info();
            Ok(())
        }
        Some(RuntimeCliCommand::FirstRun) => {
            run_first_run(parsed.auth_server_app_password.as_deref())
        }
        Some(RuntimeCliCommand::Run) => {
            let paths = AppCorePaths::from_env().map_err(BootstrapError::Cli)?;
            let deployment_manifest = paths.deployment_manifest.display().to_string();
            run_server_with_mode(
                Some(&deployment_manifest),
                parsed.watch,
                parsed.only_one,
                parsed.kill_others,
            )
        }
        Some(RuntimeCliCommand::LastRun) => run_last_run(parsed.dry_run, parsed.purge),
        Some(RuntimeCliCommand::Paths) => run_paths(),
        Some(RuntimeCliCommand::Backup) => run_backup(
            parsed.config_path.as_deref(),
            parsed.backup_action.as_deref(),
            parsed.backup_name.as_deref(),
            parsed.backup_file.as_deref(),
            parsed.confirm_restore,
        ),
        Some(RuntimeCliCommand::Vault) => {
            run_vault();
            Ok(())
        }
        Some(RuntimeCliCommand::UpdateRequired) => Err(BootstrapError::Runtime(
            "NO MORE SUPPORTED PLEASE UPDATE".to_string(),
        )),
        Some(RuntimeCliCommand::Sync) => {
            run_sync_with_action(parsed.config_path.as_deref(), parsed.sync_action.as_deref())
        }
        Some(command @ RuntimeCliCommand::TokenCommand)
        | Some(command @ RuntimeCliCommand::TokenSync)
        | Some(command @ RuntimeCliCommand::TokenQuery) => run_token_action(command, &parsed),
        Some(RuntimeCliCommand::IdempotencyCompact) => {
            run_idempotency_compact(parsed.config_path.as_deref())
        }
        Some(RuntimeCliCommand::Supervisor) => run_supervisor_action(&parsed),
        Some(RuntimeCliCommand::Security) => run_security_action(&parsed),
        Some(RuntimeCliCommand::Completions) | Some(RuntimeCliCommand::Complete) => {
            run_completion(parsed, constants)
        }
        None => print_help(constants, &[]),
    }
}
