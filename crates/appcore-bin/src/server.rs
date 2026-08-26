// =============================================================================
//        #######
//     ###       ###     F: server.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Owns the runtime server loop and HTTP host wiring.

#[path = "server_http.rs"]
pub(crate) mod server_http;
#[path = "server_single_instance.rs"]
mod server_single_instance;
use crate::application_host::ApplicationServiceReport;
use crate::application_tasks::RegisteredApplicationTask;
use crate::bootstrap::{bootstrap_runtime_with_plugin, now_ms, BootstrapError, BootstrapResult};
use crate::runtime_services::RuntimeServices;
use crate::sync_cli::push_sync_to_peers;
use appcore_core::{RuntimeLifecycleEvent, RuntimeLifecycleState};
use appcore_ops::{LogLevel, LogRecord, RuntimeLogger, StdoutLogger};
use appcore_storage::StorageProvider;
use server_single_instance::PidFileGuard;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Once};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(test)]
pub(crate) use server_http::RuntimeCommandTokenVerifier;

pub(crate) struct RuntimeServer {
    pub(crate) app: BootstrapResult,
    pub(crate) logger: StdoutLogger,
    pub(crate) shutdown_token: ShutdownToken,
    pub(crate) running: bool,
    pub(crate) tick_count: u64,
    pub(crate) tick_counter: Arc<AtomicU64>,
    pub(crate) application_tasks: Vec<RegisteredApplicationTask>,
    pub(crate) service_shutdown: Arc<AtomicBool>,
    pub(crate) service_supervisor: appcore_supervisor::Supervisor,
    #[cfg(feature = "ai-alpha")]
    pub(crate) ai_service: Option<Arc<dyn appcore_supervisor::ManagedService>>,
}

#[derive(Debug, Default)]
pub(crate) struct ShutdownToken {
    requested: bool,
}

// appcore-norm: allow(global-state) reason: process signal state requires lock-free cross-thread coordination
static CTRL_C_REQUESTED: AtomicBool = AtomicBool::new(false);
// appcore-norm: allow(global-state) reason: signal handler installation must occur once per process
static CTRL_C_HANDLER_INIT: Once = Once::new();

impl RuntimeServer {
    pub(crate) fn new(app: BootstrapResult, logger: StdoutLogger) -> Self {
        let running = current_lifecycle(&app)
            .map(|state| state == RuntimeLifecycleState::Running)
            .unwrap_or(false);
        let watchdog = appcore_supervisor::WatchdogConfig {
            enabled: app.config.supervisor_watchdog_enabled,
            check_interval_ms: app.config.supervisor_watchdog_check_interval_ms,
            stall_timeout_ms: app.config.supervisor_watchdog_stall_timeout_ms,
        };
        let service_supervisor = appcore_supervisor::Supervisor::with_watchdog_config(watchdog)
            .unwrap_or_else(|_| appcore_supervisor::Supervisor::new());
        Self {
            app,
            logger,
            shutdown_token: ShutdownToken::default(),
            running,
            tick_count: 0,
            tick_counter: Arc::new(AtomicU64::new(0)),
            application_tasks: Vec::new(),
            service_shutdown: Arc::new(AtomicBool::new(false)),
            service_supervisor,
            #[cfg(feature = "ai-alpha")]
            ai_service: None,
        }
    }

    pub(crate) fn with_application_tasks(
        app: BootstrapResult,
        logger: StdoutLogger,
        application_tasks: Vec<RegisteredApplicationTask>,
    ) -> Self {
        let mut server = Self::new(app, logger);
        server.application_tasks = application_tasks;
        server
    }

    #[cfg(feature = "ai-alpha")]
    fn with_ai_service(
        mut self,
        service: Option<Arc<dyn appcore_supervisor::ManagedService>>,
    ) -> Self {
        self.ai_service = service;
        self
    }

    pub(crate) fn tick(&mut self) -> Result<(), BootstrapError> {
        if !self.running {
            return Ok(());
        }
        if self.shutdown_token.is_requested() {
            return self.request_shutdown();
        }
        if self.service_shutdown.load(Ordering::Acquire) {
            return self.request_shutdown();
        }
        self.tick_count += 1;
        self.tick_counter.store(self.tick_count, Ordering::SeqCst);
        self.log(LogLevel::Debug, "runtime.server", "tick");
        self.try_auto_sync_push()
    }

    pub(crate) fn run_for_ticks(&mut self, n: u64) -> Result<(), BootstrapError> {
        for _ in 0..n {
            if !self.running {
                break;
            }
            self.tick()?;
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn run_until_shutdown(
        &mut self,
        max_ticks: Option<u64>,
    ) -> Result<(), BootstrapError> {
        while self.is_running() {
            if max_ticks.is_some_and(|limit| self.tick_count >= limit) {
                break;
            }
            self.apply_ctrlc_signal();
            self.tick()?;
            self.sleep_if_unbounded(max_ticks);
        }
        Ok(())
    }

    pub(crate) fn request_shutdown(&mut self) -> Result<(), BootstrapError> {
        if !self.running {
            return Ok(());
        }
        self.shutdown_token.request();
        self.log(LogLevel::Info, "runtime.server", "shutdown requested");
        apply_lifecycle(&self.app, RuntimeLifecycleEvent::ShutdownRequested)?;
        let controller = self.app.controller.lock().clone();
        crate::application_host::drain_commands(&controller, Duration::from_secs(30))?;
        apply_lifecycle(&self.app, RuntimeLifecycleEvent::ShutdownCompleted)?;
        self.running = false;
        self.log(LogLevel::Info, "runtime.server", "shutdown completed");
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn is_running(&self) -> bool {
        self.running
    }

    fn try_auto_sync_push(&self) -> Result<(), BootstrapError> {
        if !self.should_auto_push() {
            return Ok(());
        }
        self.log(LogLevel::Info, "runtime.sync", "sync push started");
        match push_sync_to_peers(
            &self.app.config,
            self.app.replication_log.as_ref(),
            Some(&self.app.security_provider),
        ) {
            Ok(()) => self.finish_auto_push(true),
            Err(err) => {
                self.finish_auto_push(false)?;
                Err(err)
            }
        }
    }

    fn should_auto_push(&self) -> bool {
        if !self.app.config.sync_enabled || self.app.config.sync_role != "leader" {
            return false;
        }
        if self.app.config.sync_peers.is_empty() {
            return false;
        }
        self.tick_count
            .is_multiple_of(self.app.config.sync_push_every_ticks.max(1))
    }

    fn finish_auto_push(&self, success: bool) -> Result<(), BootstrapError> {
        let message = if success {
            "sync push completed"
        } else {
            "sync push failed"
        };
        let level = if success {
            LogLevel::Info
        } else {
            LogLevel::Warn
        };
        self.log(level, "runtime.sync", message);
        Ok(())
    }

    fn apply_ctrlc_signal(&mut self) {
        if CTRL_C_REQUESTED.load(Ordering::SeqCst) {
            self.shutdown_token.request();
        }
    }

    fn sleep_if_unbounded(&self, max_ticks: Option<u64>) {
        if max_ticks.is_none() {
            thread::sleep(std::time::Duration::from_millis(250));
        }
    }

    fn log(&self, level: LogLevel, target: &str, message: &str) {
        self.logger.log(LogRecord {
            level,
            target: target.to_string(),
            message: message.to_string(),
            timestamp_ms: now_ms(),
        });
    }
}

impl ShutdownToken {
    pub(crate) fn request(&mut self) {
        self.requested = true;
    }

    pub(crate) fn is_requested(&self) -> bool {
        self.requested
    }
}

pub fn run_server_with_mode(
    config_path: Option<&str>,
    watch: bool,
    only_one_cli: Option<bool>,
    kill_others_cli: Option<bool>,
) -> Result<(), BootstrapError> {
    run_server_with_plugin(config_path, watch, only_one_cli, kill_others_cli, None)
}

pub fn run_server_with_plugin(
    config_path: Option<&str>,
    watch: bool,
    only_one_cli: Option<bool>,
    kill_others_cli: Option<bool>,
    app_plugin: Option<&dyn appcore_core::AppPlugin>,
) -> Result<(), BootstrapError> {
    let logger = StdoutLogger::new();
    log_boot_started(&logger);
    let app = bootstrap_runtime_with_plugin(config_path, app_plugin)?;
    run_bootstrapped(app, watch, only_one_cli, kill_others_cli)
}

pub(crate) fn run_bootstrapped(
    app: BootstrapResult,
    watch: bool,
    only_one_cli: Option<bool>,
    kill_others_cli: Option<bool>,
) -> Result<(), BootstrapError> {
    run_bootstrapped_with_tasks(app, watch, only_one_cli, kill_others_cli, Vec::new())
}

#[cfg(not(feature = "ai-alpha"))]
pub(crate) fn run_application_bootstrapped(
    app: BootstrapResult,
    application_tasks: Vec<RegisteredApplicationTask>,
) -> Result<(), BootstrapError> {
    run_bootstrapped_with_tasks(app, true, None, None, application_tasks)
}

#[cfg(feature = "ai-alpha")]
pub(crate) fn run_application_bootstrapped_with_ai(
    app: BootstrapResult,
    application_tasks: Vec<RegisteredApplicationTask>,
    ai_service: Option<Arc<dyn appcore_supervisor::ManagedService>>,
) -> Result<(), BootstrapError> {
    let logger = StdoutLogger::new();
    let _pid_guard = claim_single_instance(&app, None, None)?;
    ensure_running_lifecycle(&app)?;
    let server = RuntimeServer::with_application_tasks(app, logger, application_tasks)
        .with_ai_service(ai_service);
    run_watch_mode(server)
}

fn run_bootstrapped_with_tasks(
    app: BootstrapResult,
    watch: bool,
    only_one_cli: Option<bool>,
    kill_others_cli: Option<bool>,
    application_tasks: Vec<RegisteredApplicationTask>,
) -> Result<(), BootstrapError> {
    let logger = StdoutLogger::new();
    let _pid_guard = claim_single_instance(&app, only_one_cli, kill_others_cli)?;
    ensure_running_lifecycle(&app)?;
    let server = RuntimeServer::with_application_tasks(app, logger, application_tasks);

    if watch {
        return run_watch_mode(server);
    }
    log_boot_ready(&server)?;
    run_single_tick(server)
}

fn log_boot_started(logger: &StdoutLogger) {
    logger.log(LogRecord {
        level: LogLevel::Info,
        target: "runtime.server".to_string(),
        message: "boot started".to_string(),
        timestamp_ms: now_ms(),
    });
}

fn claim_single_instance(
    app: &BootstrapResult,
    only_one_cli: Option<bool>,
    kill_others_cli: Option<bool>,
) -> Result<Option<PidFileGuard>, BootstrapError> {
    let pid_file = PathBuf::from(&app.config.storage_path).join("appcore.pid");
    if only_one_cli.unwrap_or(app.config.only_one) {
        return server_single_instance::claim_single_instance(
            &pid_file,
            &app.config.app_id,
            kill_others_cli.unwrap_or(app.config.kill_others),
        )
        .map(Some);
    }
    Ok(None)
}

fn ensure_running_lifecycle(app: &BootstrapResult) -> Result<(), BootstrapError> {
    if current_lifecycle(app)? == RuntimeLifecycleState::Running {
        return Ok(());
    }
    Err(BootstrapError::Runtime(
        "runtime is not running".to_string(),
    ))
}

fn current_lifecycle(app: &BootstrapResult) -> Result<RuntimeLifecycleState, BootstrapError> {
    Ok(app.controller.lock().lifecycle().current())
}

fn log_boot_ready(server: &RuntimeServer) -> Result<(), BootstrapError> {
    let health = server.app.storage_provider.health();
    let lifecycle = current_lifecycle(&server.app)?;
    let message = format!(
        "boot ready app_id={} node_id={} lifecycle={:?} storage={:?} security_ok={}",
        server.app.config.app_id,
        server.app.config.node_id,
        lifecycle,
        health.status,
        server.app.security_ok
    );
    server.log(LogLevel::Info, "runtime.server", &message);
    println!("AppCore-Runtime lifecycle: Running");
    Ok(())
}

fn run_watch_mode(mut server: RuntimeServer) -> Result<(), BootstrapError> {
    install_ctrlc_handler()?;
    CTRL_C_REQUESTED.store(false, Ordering::SeqCst);
    let services = RuntimeServices::start(&mut server)?;
    if let Err(error) = log_boot_ready(&server) {
        return Err(fail_after_services_started(&mut server, services, error));
    }
    let _managed_health = match crate::managed_health::ManagedHealthGuard::ready() {
        Ok(health) => health,
        Err(error) => return Err(fail_after_services_started(&mut server, services, error)),
    };
    let run_result = server.run_until_shutdown(None);
    let shutdown_result = services.shutdown();
    run_result?;
    shutdown_result?;
    print_final_state(&server)
}

fn fail_after_services_started(
    server: &mut RuntimeServer,
    services: RuntimeServices,
    error: BootstrapError,
) -> BootstrapError {
    let mut details = vec![error.to_string()];
    if let Err(shutdown) = server.request_shutdown() {
        details.push(format!("runtime shutdown failed: {shutdown}"));
    }
    if let Err(shutdown) = services.shutdown() {
        details.push(format!("service shutdown failed: {shutdown}"));
    }
    BootstrapError::Runtime(details.join("; "))
}

#[cfg(not(feature = "ai-alpha"))]
pub(crate) fn probe_application_bootstrapped(
    app: BootstrapResult,
    application_tasks: Vec<RegisteredApplicationTask>,
    timeout: Duration,
) -> Result<ApplicationServiceReport, BootstrapError> {
    ensure_running_lifecycle(&app)?;
    let mut server =
        RuntimeServer::with_application_tasks(app, StdoutLogger::new(), application_tasks);
    probe_prepared_application(&mut server, timeout)
}

#[cfg(feature = "ai-alpha")]
pub(crate) fn probe_application_bootstrapped_with_ai(
    app: BootstrapResult,
    application_tasks: Vec<RegisteredApplicationTask>,
    timeout: Duration,
    ai_service: Option<Arc<dyn appcore_supervisor::ManagedService>>,
) -> Result<ApplicationServiceReport, BootstrapError> {
    ensure_running_lifecycle(&app)?;
    let mut server =
        RuntimeServer::with_application_tasks(app, StdoutLogger::new(), application_tasks)
            .with_ai_service(ai_service);
    probe_prepared_application(&mut server, timeout)
}

fn probe_prepared_application(
    server: &mut RuntimeServer,
    timeout: Duration,
) -> Result<ApplicationServiceReport, BootstrapError> {
    let services = RuntimeServices::start(server)?;
    let selected = services.report(&server.app);
    let readiness = wait_for_service_probe(server, timeout, selected.control_plane_started);
    let report = ApplicationServiceReport {
        discovery_ready: server.app.peer_directory.lock().is_some(),
        service_lease_active: server.app.leader_lease.lock().is_some(),
        ..selected
    };
    let runtime_shutdown = server.request_shutdown();
    let service_shutdown = services.shutdown();
    readiness?;
    runtime_shutdown?;
    service_shutdown?;
    Ok(report)
}

fn wait_for_service_probe(
    server: &mut RuntimeServer,
    timeout: Duration,
    control_plane_started: bool,
) -> Result<(), BootstrapError> {
    let deadline = Instant::now() + timeout;
    loop {
        server.tick()?;
        let coordination_ready =
            server.app.peer_directory.lock().is_some() && server.app.leader_lease.lock().is_some();
        if !control_plane_started || coordination_ready {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(BootstrapError::Runtime(
                "runtime service probe timed out waiting for control plane".to_string(),
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn run_single_tick(mut server: RuntimeServer) -> Result<(), BootstrapError> {
    server.run_for_ticks(1)?;
    server.request_shutdown()?;
    print_final_state(&server)
}

fn print_final_state(server: &RuntimeServer) -> Result<(), BootstrapError> {
    println!("AppCore-Runtime tick_count: {}", server.tick_count);
    println!(
        "AppCore-Runtime lifecycle final: {:?}",
        current_lifecycle(&server.app)?
    );
    Ok(())
}

fn apply_lifecycle(
    app: &BootstrapResult,
    event: RuntimeLifecycleEvent,
) -> Result<(), BootstrapError> {
    app.controller
        .lock()
        .apply_lifecycle_event(event)
        .map(|_| ())
        .map_err(|_| BootstrapError::Runtime("failed to apply shutdown lifecycle".to_string()))
}

fn install_ctrlc_handler() -> Result<(), BootstrapError> {
    let mut install_result = Ok(());
    CTRL_C_HANDLER_INIT.call_once(|| {
        install_result = ctrlc::set_handler(|| {
            CTRL_C_REQUESTED.store(true, Ordering::SeqCst);
        })
        .map_err(|_| BootstrapError::Runtime("failed to install Ctrl+C handler".to_string()));
    });
    install_result
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
