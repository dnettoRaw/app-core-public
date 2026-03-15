// =============================================================================
//        #######
//     ###       ###     F: supervisor.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/01 15:47:06 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Owns the local supervisor loop for appcore-bin child process restarts.

use appcore_ops::{LogLevel, LogRecord, RuntimeLogger, StdoutLogger};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::bootstrap::BootstrapError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SupervisorHealthProgress {
    pub(crate) status_ok: bool,
    pub(crate) state: String,
    pub(crate) reconcile_sequence: u64,
    pub(crate) last_progress_at_ms: u64,
    pub(crate) critical_services_healthy: bool,
}

#[derive(serde::Deserialize)]
struct HealthDocument {
    supervisor: Option<HealthSupervisorDocument>,
}

#[derive(serde::Deserialize)]
struct HealthSupervisorDocument {
    state: String,
    reconcile_sequence: u64,
    last_progress_at_ms: u64,
    critical_services_healthy: bool,
}

// appcore-norm: allow(global-state) reason: process signal state requires lock-free cross-thread coordination
static SUPERVISOR_STOP_REQUESTED: AtomicBool = AtomicBool::new(false);
// appcore-norm: allow(global-state) reason: signal handler installation must occur once per process
static SUPERVISOR_CTRL_C_HANDLER_INIT: Once = Once::new();

#[derive(Debug, Clone)]
pub(crate) struct SupervisorConfig {
    pub(crate) config_path: String,
    pub(crate) max_restarts: u64,
    pub(crate) child_args: Option<String>,
    pub(crate) health_url: Option<String>,
    pub(crate) health_check_every_ticks: u64,
    pub(crate) health_fail_limit: u64,
    pub(crate) only_one: Option<bool>,
    pub(crate) kill_others: Option<bool>,
}

pub(crate) struct Supervisor {
    config: SupervisorConfig,
    logger: StdoutLogger,
    restart_count: u64,
    tick_count: u64,
    health_fail_count: u64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SupervisorRunOptions<'a> {
    pub(crate) config_path: Option<&'a str>,
    pub(crate) max_restarts: Option<u64>,
    pub(crate) child_args: Option<&'a str>,
    pub(crate) health_url: Option<&'a str>,
    pub(crate) health_check_every_ticks: Option<u64>,
    pub(crate) health_fail_limit: Option<u64>,
    pub(crate) only_one: Option<bool>,
    pub(crate) kill_others: Option<bool>,
}

impl Supervisor {
    pub(crate) fn new(config: SupervisorConfig) -> Self {
        Self {
            config,
            logger: StdoutLogger::new(),
            restart_count: 0,
            tick_count: 0,
            health_fail_count: 0,
        }
    }

    pub(crate) fn run(&mut self) -> Result<(), BootstrapError> {
        install_supervisor_ctrlc_handler()?;
        SUPERVISOR_STOP_REQUESTED.store(false, Ordering::SeqCst);
        self.log(LogLevel::Info, "supervisor starting");
        loop {
            let mut child = self.spawn_child()?;
            self.log(LogLevel::Info, &format!("child started pid={}", child.id()));
            if self.monitor_child(&mut child)? {
                continue;
            }
            return Ok(());
        }
    }

    fn spawn_child(&self) -> Result<Child, BootstrapError> {
        let mut command = Command::new(std::env::current_exe().map_err(|_| {
            BootstrapError::Runtime("failed to resolve current executable".to_string())
        })?);
        if let Some(args) = &self.config.child_args {
            for token in args.split_whitespace() {
                command.arg(token);
            }
        } else {
            command.arg("server");
            command.arg("--watch");
            command.arg("--deployment");
            command.arg(&self.config.config_path);
            if let Some(val) = self.config.only_one {
                if val {
                    command.arg("--only-one");
                } else {
                    command.arg("--no-only-one");
                }
            }
            if let Some(val) = self.config.kill_others {
                if val {
                    command.arg("--kill-others");
                } else {
                    command.arg("--no-kill-others");
                }
            }
        }
        command
            .spawn()
            .map_err(|_| BootstrapError::Runtime("failed to start supervisor child".to_string()))
    }

    fn monitor_child(&mut self, child: &mut Child) -> Result<bool, BootstrapError> {
        loop {
            self.tick_count = self.tick_count.saturating_add(1);
            if SUPERVISOR_STOP_REQUESTED.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                self.log(LogLevel::Info, "supervisor shutdown");
                return Ok(false);
            }
            if let Some(status) = child.try_wait().map_err(|_| {
                BootstrapError::Runtime("failed to inspect child status".to_string())
            })? {
                self.log(
                    LogLevel::Warn,
                    &format!("child exited code={:?}", status.code()),
                );
                if should_restart(self.restart_count, self.config.max_restarts) {
                    self.restart_count += 1;
                    self.log(LogLevel::Warn, "child restarting");
                    return Ok(true);
                }
                self.log(LogLevel::Error, "restart limit reached");
                return Err(BootstrapError::Runtime(
                    "supervisor restart limit reached".to_string(),
                ));
            }
            if self.should_run_health_check() {
                let healthy = self.run_health_check();
                if healthy {
                    self.health_fail_count = 0;
                } else {
                    self.health_fail_count = self.health_fail_count.saturating_add(1);
                    if should_restart_for_health(
                        self.health_fail_count,
                        self.config.health_fail_limit,
                    ) {
                        let _ = child.kill();
                        let _ = child.wait();
                        self.log(LogLevel::Warn, "healthcheck failed, child killed");
                        if should_restart(self.restart_count, self.config.max_restarts) {
                            self.restart_count += 1;
                            self.health_fail_count = 0;
                            self.log(LogLevel::Warn, "child restarting");
                            return Ok(true);
                        }
                        self.log(LogLevel::Error, "restart limit reached");
                        return Err(BootstrapError::Runtime(
                            "supervisor restart limit reached".to_string(),
                        ));
                    }
                }
            }
            thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    fn should_run_health_check(&self) -> bool {
        self.config.health_url.is_some()
            && should_check_health_tick(self.tick_count, self.config.health_check_every_ticks)
    }

    fn run_health_check(&self) -> bool {
        if let Some(url) = &self.config.health_url {
            return check_health_url(url);
        }
        true
    }

    fn log(&self, level: LogLevel, message: &str) {
        self.logger.log(LogRecord {
            level,
            target: "runtime.supervisor".to_string(),
            message: message.to_string(),
            timestamp_ms: now_ms(),
        });
    }
}

pub(crate) fn should_restart(restart_count: u64, max_restarts: u64) -> bool {
    restart_count < max_restarts
}

pub(crate) fn should_check_health_tick(tick_count: u64, every_ticks: u64) -> bool {
    tick_count > 0 && tick_count.is_multiple_of(every_ticks.max(1))
}

pub(crate) fn should_restart_for_health(fail_count: u64, fail_limit: u64) -> bool {
    fail_limit > 0 && fail_count >= fail_limit
}

pub(crate) fn run_supervisor(options: SupervisorRunOptions<'_>) -> Result<(), BootstrapError> {
    let config = SupervisorConfig {
        config_path: options.config_path.unwrap_or("deployment.toml").to_string(),
        max_restarts: options.max_restarts.unwrap_or(0),
        child_args: options.child_args.map(|value| value.to_string()),
        health_url: options.health_url.map(|value| value.to_string()),
        health_check_every_ticks: options.health_check_every_ticks.unwrap_or(10),
        health_fail_limit: options.health_fail_limit.unwrap_or(3),
        only_one: options.only_one,
        kill_others: options.kill_others,
    };
    Supervisor::new(config).run()
}

pub(crate) fn check_health_url(url: &str) -> bool {
    fetch_health_progress(url).is_some_and(|progress| progress.status_ok)
}

pub(crate) fn fetch_health_progress(url: &str) -> Option<SupervisorHealthProgress> {
    let (host_port, path) = parse_http_url(url)?;
    let mut stream = TcpStream::connect(host_port.as_str()).ok()?;
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(1000)));
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(1000)));
    let request = format!("GET {path} HTTP/1.1\r\nHost: {host_port}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return None;
    }
    let mut response = String::new();
    if stream.read_to_string(&mut response).is_err() {
        return None;
    }
    parse_health_progress(&response)
}

fn parse_health_progress(response: &str) -> Option<SupervisorHealthProgress> {
    let status_ok = response.starts_with("HTTP/1.1 200") || response.starts_with("HTTP/1.0 200");
    let (_, body) = response.split_once("\r\n\r\n")?;
    let document = serde_json::from_str::<HealthDocument>(body).ok()?;
    let supervisor = document.supervisor?;
    Some(SupervisorHealthProgress {
        status_ok,
        state: supervisor.state,
        reconcile_sequence: supervisor.reconcile_sequence,
        last_progress_at_ms: supervisor.last_progress_at_ms,
        critical_services_healthy: supervisor.critical_services_healthy,
    })
}

fn parse_http_url(url: &str) -> Option<(String, String)> {
    if !url.starts_with("http://") {
        return None;
    }
    let rest = &url[7..];
    let mut split = rest.splitn(2, '/');
    let host_port = split.next()?.to_string();
    if host_port.is_empty() {
        return None;
    }
    let path = match split.next() {
        Some(p) if !p.is_empty() => format!("/{p}"),
        _ => "/".to_string(),
    };
    Some((host_port, path))
}

fn install_supervisor_ctrlc_handler() -> Result<(), BootstrapError> {
    let mut install_result = Ok(());
    SUPERVISOR_CTRL_C_HANDLER_INIT.call_once(|| {
        install_result = ctrlc::set_handler(|| {
            SUPERVISOR_STOP_REQUESTED.store(true, Ordering::SeqCst);
        })
        .map_err(|_| BootstrapError::Runtime("failed to install Ctrl+C handler".to_string()));
    });
    install_result
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "supervisor_tests.rs"]
mod tests;
