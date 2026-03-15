// =============================================================================
//        #######
//     ###       ###     F: application_supervisor.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Process supervisor for automatic manifest-first application updates.

use crate::bootstrap::BootstrapError;
use crate::managed_health::MANAGED_HEALTH_FILE_ENV;
use crate::manifest_bootstrap::load_manifest_input;
use crate::supervisor::fetch_health_progress;
use appcore_contracts::ProviderConfig;
use appcore_ops::{LogLevel, LogRecord, RuntimeLogger, StdoutLogger};
use appcore_update::{ActivationReceipt, ArtifactStore, FileArtifactStore};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

#[path = "application_supervisor_health.rs"]
mod health;
#[path = "application_supervisor_process.rs"]
mod process;

use health::{ProgressState, ProgressTracker};
use process::{
    canonicalize, child_error, install_ctrlc_handler, make_executable, now_ms, status_text,
    stop_managed_child, update_error,
};

const MANAGED_CHILD_ENV: &str = "APPCORE_MANAGED_APPLICATION_CHILD";
const DEFAULT_MAX_RESTARTS: u64 = 3;
const MAX_STARTUP_TIMEOUT_MS: u64 = 300_000;

// appcore-norm: allow(global-state) reason: process signal state requires lock-free cross-thread coordination
static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);
// appcore-norm: allow(global-state) reason: atomic sequence prevents managed child identifier collisions
static LAUNCH_SEQUENCE: AtomicU64 = AtomicU64::new(1);

struct ManagedApplicationSupervisor {
    initial_executable: PathBuf,
    application_manifest: PathBuf,
    deployment_manifest: PathBuf,
    update_store: FileArtifactStore,
    health_directory: PathBuf,
    health_url: Option<String>,
    startup_timeout: Duration,
    max_restarts: u64,
    health_check_interval: Duration,
    watchdog_stall_timeout: Duration,
    logger: StdoutLogger,
}

struct ManagedChild {
    process: Child,
    health_marker: PathBuf,
    progress: ProgressTracker,
    last_health_check: Instant,
}

pub(crate) fn is_required(
    application_manifest: &Path,
    deployment_manifest: &Path,
) -> Result<bool, BootstrapError> {
    if is_managed_child() {
        return Ok(false);
    }
    let input = load_manifest_input(application_manifest, deployment_manifest)?;
    Ok(input.application.update_policy().is_automatic())
}

pub(crate) fn is_managed_child() -> bool {
    std::env::var_os(MANAGED_CHILD_ENV).is_some()
}

pub(crate) fn run(
    application_manifest: &Path,
    deployment_manifest: &Path,
) -> Result<(), BootstrapError> {
    ManagedApplicationSupervisor::load(application_manifest, deployment_manifest)?.run()
}

impl ManagedApplicationSupervisor {
    fn load(
        application_manifest: &Path,
        deployment_manifest: &Path,
    ) -> Result<Self, BootstrapError> {
        let application_manifest = canonicalize(application_manifest, "application manifest")?;
        let deployment_manifest = canonicalize(deployment_manifest, "deployment manifest")?;
        let input = load_manifest_input(&application_manifest, &deployment_manifest)?;
        let update = input.deployment.update_provider().ok_or_else(|| {
            BootstrapError::Runtime(
                "automatic updates require a deployment update provider".to_string(),
            )
        })?;
        require_executable_artifact(update)?;
        let startup_timeout_ms = parse_u64_setting(
            update,
            "activation_health_timeout_ms",
            input.application.health_requirements().startup_grace_ms(),
        )?
        .min(MAX_STARTUP_TIMEOUT_MS);
        let initial_executable = std::env::current_exe().map_err(|error| {
            BootstrapError::Runtime(format!("failed to resolve application executable: {error}"))
        })?;
        let storage_path = PathBuf::from(input.config.storage_path);
        Ok(Self {
            initial_executable,
            application_manifest,
            deployment_manifest,
            update_store: FileArtifactStore::new(storage_path.join("updates")),
            health_directory: storage_path.join("managed-health"),
            health_url: health_url(
                input.config.api_enabled,
                &input.config.api_host,
                input.config.api_port,
            ),
            startup_timeout: Duration::from_millis(startup_timeout_ms.max(100)),
            max_restarts: parse_u64_setting(update, "max_restarts", DEFAULT_MAX_RESTARTS)?,
            health_check_interval: Duration::from_millis(
                input.config.supervisor_watchdog_check_interval_ms,
            ),
            watchdog_stall_timeout: Duration::from_millis(
                input.config.supervisor_watchdog_stall_timeout_ms,
            ),
            logger: StdoutLogger::new(),
        })
    }

    fn run(self) -> Result<(), BootstrapError> {
        install_ctrlc_handler()?;
        STOP_REQUESTED.store(false, Ordering::SeqCst);
        let (mut executable, mut child) = self.start_current_or_pending()?;
        let mut restart_count = 0_u64;
        self.log(LogLevel::Info, "managed application supervisor started");
        loop {
            if STOP_REQUESTED.load(Ordering::Acquire) {
                stop_managed_child(&mut child);
                self.log(LogLevel::Info, "managed application supervisor stopped");
                return Ok(());
            }
            let Some(status) = child.process.try_wait().map_err(child_error)? else {
                if self.child_requires_restart(&mut child) {
                    stop_managed_child(&mut child);
                    if restart_count >= self.max_restarts {
                        return Err(BootstrapError::Runtime(
                            "managed application restart limit reached after supervisor stall"
                                .to_string(),
                        ));
                    }
                    restart_count = restart_count.saturating_add(1);
                    self.log(
                        LogLevel::Warn,
                        "managed application restarting after supervisor stall",
                    );
                    child = self.spawn(&executable)?;
                }
                thread::sleep(Duration::from_millis(100));
                continue;
            };
            let _ = std::fs::remove_file(&child.health_marker);
            if let Some(receipt) = self
                .update_store
                .pending_activation_receipt()
                .map_err(update_error)?
            {
                let activation = self.activate_candidate(receipt)?;
                executable = activation.0;
                child = activation.1;
                restart_count = 0;
                continue;
            }
            if status.success() {
                return Ok(());
            }
            if restart_count >= self.max_restarts {
                return Err(BootstrapError::Runtime(format!(
                    "managed application restart limit reached after status {}",
                    status_text(status)
                )));
            }
            restart_count = restart_count.saturating_add(1);
            self.log(LogLevel::Warn, "managed application restarting after exit");
            child = self.spawn(&executable)?;
        }
    }

    fn start_current_or_pending(&self) -> Result<(PathBuf, ManagedChild), BootstrapError> {
        match self
            .update_store
            .pending_activation_receipt()
            .map_err(update_error)?
        {
            Some(receipt) => self.activate_candidate(receipt),
            None => {
                let executable = self.current_executable()?;
                let child = self.spawn(&executable)?;
                Ok((executable, child))
            }
        }
    }

    fn activate_candidate(
        &self,
        receipt: ActivationReceipt,
    ) -> Result<(PathBuf, ManagedChild), BootstrapError> {
        let candidate = self
            .prepare_artifact_executable(&receipt.activated)
            .and_then(|path| self.spawn(&path).map(|child| (path, child)));
        let (path, mut child) = match candidate {
            Ok(candidate) => candidate,
            Err(error) => {
                self.update_store.rollback(&receipt).map_err(update_error)?;
                let fallback = self.executable_for_previous(&receipt)?;
                self.log(
                    LogLevel::Error,
                    &format!("updated application could not start; rollback activated: {error}"),
                );
                return self.spawn(&fallback).map(|child| (fallback, child));
            }
        };
        if self.wait_until_healthy(&mut child)? {
            self.update_store.commit(&receipt).map_err(update_error)?;
            self.log(LogLevel::Info, "updated application passed health gate");
            return Ok((path, child));
        }
        stop_managed_child(&mut child);
        self.update_store.rollback(&receipt).map_err(update_error)?;
        let fallback = self.executable_for_previous(&receipt)?;
        self.log(
            LogLevel::Warn,
            "updated application failed health gate; rollback activated",
        );
        self.spawn(&fallback).map(|child| (fallback, child))
    }

    fn wait_until_healthy(&self, child: &mut ManagedChild) -> Result<bool, BootstrapError> {
        let started = Instant::now();
        loop {
            if STOP_REQUESTED.load(Ordering::Acquire) {
                return Ok(false);
            }
            if child.process.try_wait().map_err(child_error)?.is_some() {
                return Ok(false);
            }
            match &self.health_url {
                Some(url)
                    if child.progress.observe(
                        fetch_health_progress(url),
                        now_ms(),
                        self.watchdog_stall_timeout,
                    ) == ProgressState::Advanced =>
                {
                    return Ok(true);
                }
                None if child.health_marker.is_file() => return Ok(true),
                _ if started.elapsed() >= self.startup_timeout => return Ok(false),
                _ => thread::sleep(Duration::from_millis(100)),
            }
        }
    }

    fn child_requires_restart(&self, child: &mut ManagedChild) -> bool {
        let Some(url) = &self.health_url else {
            return false;
        };
        if child.last_health_check.elapsed() < self.health_check_interval {
            return false;
        }
        child.last_health_check = Instant::now();
        should_restart_for_progress(child.progress.observe(
            fetch_health_progress(url),
            now_ms(),
            self.watchdog_stall_timeout,
        ))
    }

    fn current_executable(&self) -> Result<PathBuf, BootstrapError> {
        match self.update_store.current().map_err(update_error)? {
            Some(active) => self.prepare_artifact_executable(&active),
            None => Ok(self.initial_executable.clone()),
        }
    }

    fn executable_for_previous(
        &self,
        receipt: &ActivationReceipt,
    ) -> Result<PathBuf, BootstrapError> {
        match &receipt.previous {
            Some(previous) => self.prepare_artifact_executable(previous),
            None => Ok(self.initial_executable.clone()),
        }
    }

    fn prepare_artifact_executable(
        &self,
        artifact: &appcore_update::ArtifactDescriptor,
    ) -> Result<PathBuf, BootstrapError> {
        let path = self.update_store.artifact_path(artifact.build_id());
        let metadata = std::fs::metadata(&path).map_err(|error| {
            BootstrapError::Runtime(format!(
                "activated application artifact is unavailable: {error}"
            ))
        })?;
        if !metadata.is_file() {
            return Err(BootstrapError::Runtime(
                "activated application artifact is not a regular file".to_string(),
            ));
        }
        make_executable(&path)?;
        Ok(path)
    }

    fn spawn(&self, executable: &Path) -> Result<ManagedChild, BootstrapError> {
        std::fs::create_dir_all(&self.health_directory).map_err(|error| {
            BootstrapError::Runtime(format!(
                "failed to create managed health directory: {error}"
            ))
        })?;
        let launch = LAUNCH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let health_marker = self
            .health_directory
            .join(format!("launch-{}-{launch}.ready", now_ms()));
        let process = Command::new(executable)
            .env(MANAGED_CHILD_ENV, "1")
            .env(MANAGED_HEALTH_FILE_ENV, &health_marker)
            .env("APPCORE_APPLICATION_MANIFEST", &self.application_manifest)
            .env("APPCORE_DEPLOYMENT_MANIFEST", &self.deployment_manifest)
            .spawn()
            .map_err(|error| {
                BootstrapError::Runtime(format!(
                    "failed to start managed application '{}': {error}",
                    executable.display()
                ))
            })?;
        Ok(ManagedChild {
            process,
            health_marker,
            progress: ProgressTracker::default(),
            last_health_check: Instant::now(),
        })
    }

    fn log(&self, level: LogLevel, message: &str) {
        self.logger.log(LogRecord {
            level,
            target: "runtime.application_supervisor".to_string(),
            message: message.to_string(),
            timestamp_ms: now_ms(),
        });
    }
}

fn should_restart_for_progress(state: ProgressState) -> bool {
    matches!(state, ProgressState::Stalled | ProgressState::Failed)
}

fn require_executable_artifact(config: &ProviderConfig) -> Result<(), BootstrapError> {
    match config.settings().get("artifact_kind").map(String::as_str) {
        Some("executable") => Ok(()),
        _ => Err(BootstrapError::Runtime(
            "automatic updates require update_provider.settings.artifact_kind=executable"
                .to_string(),
        )),
    }
}

fn parse_u64_setting(
    config: &ProviderConfig,
    name: &str,
    default: u64,
) -> Result<u64, BootstrapError> {
    match config.settings().get(name) {
        Some(value) => value.parse::<u64>().map_err(|_| {
            BootstrapError::Runtime(format!("update provider setting `{name}` must be a u64"))
        }),
        None => Ok(default),
    }
}

fn health_url(enabled: bool, host: &str, port: u16) -> Option<String> {
    if !enabled {
        return None;
    }
    let host = match host {
        "0.0.0.0" => "127.0.0.1".to_string(),
        "::" => "[::1]".to_string(),
        value if value.contains(':') && !value.starts_with('[') => format!("[{value}]"),
        value => value.to_string(),
    };
    Some(format!("http://{host}:{port}/v1/health"))
}

#[cfg(test)]
#[path = "application_supervisor_tests.rs"]
mod tests;
