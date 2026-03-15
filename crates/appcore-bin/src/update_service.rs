// =============================================================================
//        #######
//     ###       ###     F: update_service.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 11:51:10 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Automatic update polling selected by application and deployment manifests.

mod authenticity;
mod smoke;

use crate::bootstrap::{now_ms, BootstrapError};
use crate::server::RuntimeServer;
use appcore_contracts::ProviderConfig;
use appcore_core::{AuditCategory, AuditEntry, AuditOutcome, RuntimeController};
use appcore_ops::{
    InMemoryObservationSink, ObservationEvent, ObservationKind, ObservationSeverity,
    ObservationSink,
};
use appcore_supervisor::ManagedService;
use appcore_update::{
    ArtifactAuthenticityVerifier, ArtifactStore, FileArtifactStore, SharedUpdateProvider,
    StagedArtifact, UpdateCoordinator, UpdatePreparation, UpdateRequest, UpdateResult,
    UpdateStaging,
};
use authenticity::build_authenticity_verifier;
use smoke::run_smoke_test;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const DEFAULT_POLL_INTERVAL_MS: u64 = 60_000;
const DEFAULT_MAX_ARTIFACT_BYTES: usize = 128 * 1024 * 1024;
const DEFAULT_SMOKE_TEST_TIMEOUT_MS: u64 = 10_000;
const MAX_SMOKE_TEST_TIMEOUT_MS: u64 = 60_000;

struct UpdateLoop {
    provider: SharedUpdateProvider,
    store: FileArtifactStore,
    request: UpdateRequest,
    runtime_version: String,
    protocol_version: String,
    poll_interval: Duration,
    smoke_test_timeout: Duration,
    max_artifact_bytes: usize,
    runtime_shutdown: Arc<AtomicBool>,
    observations: InMemoryObservationSink,
    controller: Arc<parking_lot::Mutex<RuntimeController>>,
    application_id: String,
    node_id: String,
    authenticity: Arc<dyn ArtifactAuthenticityVerifier>,
    unsigned_local_artifacts: bool,
}

pub(crate) fn update_service_if_enabled(
    server: &RuntimeServer,
) -> Result<Option<Arc<dyn ManagedService>>, BootstrapError> {
    if !server
        .app
        .application_manifest
        .update_policy()
        .is_automatic()
    {
        return Ok(None);
    }
    let provider = server.app.update_provider.clone().ok_or_else(|| {
        BootstrapError::Runtime(
            "automatic updates require a deployment update provider".to_string(),
        )
    })?;
    let config = server.app.provider_plan.update().ok_or_else(|| {
        BootstrapError::Runtime("automatic update provider configuration is absent".to_string())
    })?;
    let authenticity = build_authenticity_verifier(config)?;
    let update_loop = Arc::new(UpdateLoop {
        provider,
        store: server.app.update_store.clone(),
        request: UpdateRequest {
            application_id: server.app.application_manifest.application_id().clone(),
            current_version: server
                .app
                .application_manifest
                .application_version()
                .to_string(),
            channel: server
                .app
                .application_manifest
                .update_policy()
                .channel()
                .to_string(),
        },
        runtime_version: env!("CARGO_PKG_VERSION").to_string(),
        protocol_version: server
            .app
            .application_manifest
            .runtime_requirements()
            .protocol_version()
            .to_string(),
        poll_interval: Duration::from_millis(parse_u64_setting(
            config,
            "poll_interval_ms",
            DEFAULT_POLL_INTERVAL_MS,
        )?),
        smoke_test_timeout: Duration::from_millis(
            parse_u64_setting(
                config,
                "smoke_test_timeout_ms",
                DEFAULT_SMOKE_TEST_TIMEOUT_MS,
            )?
            .clamp(100, MAX_SMOKE_TEST_TIMEOUT_MS),
        ),
        max_artifact_bytes: parse_usize_setting(
            config,
            "max_artifact_bytes",
            DEFAULT_MAX_ARTIFACT_BYTES,
        )?,
        authenticity: authenticity.verifier,
        unsigned_local_artifacts: authenticity.unsigned_local_artifacts,
        runtime_shutdown: Arc::clone(&server.service_shutdown),
        observations: server.app.observations.clone(),
        controller: Arc::clone(&server.app.controller),
        application_id: server.app.config.app_id.clone(),
        node_id: server.app.config.node_id.clone(),
    });
    let descriptor = crate::runtime_services::service_descriptor(
        crate::runtime_services::UPDATE_SERVICE,
        appcore_supervisor::ManagedResource::Update,
        &[crate::runtime_services::SECURITY_SERVICE],
    )?;
    Ok(Some(Arc::new(
        appcore_supervisor::ManagedThreadService::new(descriptor, move |shutdown| {
            let update_loop = Arc::clone(&update_loop);
            thread::Builder::new()
                .name("appcore-update".to_string())
                .spawn(move || update_loop.run(shutdown).map_err(|error| error.to_string()))
                .map_err(|error| error.to_string())
        }),
    )))
}

impl UpdateLoop {
    fn run(&self, shutdown: Arc<AtomicBool>) -> Result<(), BootstrapError> {
        self.emit(ObservationSeverity::Info, "runtime.update.started", None);
        if self.unsigned_local_artifacts {
            eprintln!(
                "[warning] unsigned local application artifacts are enabled for this deployment"
            );
            self.emit(
                ObservationSeverity::Warning,
                "runtime.update.unsigned_local_artifacts_enabled",
                None,
            );
            self.audit(
                "runtime.update.unsigned_local_artifacts_enabled",
                AuditOutcome::Accepted,
                Some("explicit feature-gated deployment policy"),
            );
        }
        let mut pending_reported = false;
        while !shutdown.load(Ordering::Acquire) {
            if self
                .store
                .pending_activation_receipt()
                .map_err(update_bootstrap_error)?
                .is_some()
            {
                if !pending_reported {
                    self.emit(
                        ObservationSeverity::Info,
                        "runtime.update.awaiting_supervisor_health",
                        None,
                    );
                    pending_reported = true;
                }
                interruptible_sleep(&shutdown, Duration::from_millis(100));
                continue;
            }
            pending_reported = false;
            match self.check_once() {
                Ok(UpdatePreparation::NoUpdate) => {
                    self.emit(
                        ObservationSeverity::Info,
                        "runtime.update.no_candidate",
                        None,
                    );
                }
                Ok(UpdatePreparation::AwaitingHealth(artifact)) => {
                    self.emit(
                        ObservationSeverity::Info,
                        "runtime.update.restart_required",
                        Some(("build_id", artifact.build_id().as_str())),
                    );
                    self.runtime_shutdown.store(true, Ordering::Release);
                    return Ok(());
                }
                Err(error) => {
                    self.emit(
                        ObservationSeverity::Error,
                        "runtime.update.check_failed",
                        Some(("error_kind", update_error_kind(&error))),
                    );
                }
            }
            interruptible_sleep(&shutdown, self.poll_interval);
        }
        self.emit(ObservationSeverity::Info, "runtime.update.stopped", None);
        Ok(())
    }

    fn check_once(&self) -> UpdateResult<UpdatePreparation> {
        let coordinator = UpdateCoordinator::new_for_preparation(
            self.provider.as_ref(),
            &self.store,
            self.authenticity.as_ref(),
            self.max_artifact_bytes,
        )?;
        match coordinator.stage_candidate(
            &self.request,
            &self.runtime_version,
            &self.protocol_version,
        )? {
            UpdateStaging::NoUpdate => Ok(UpdatePreparation::NoUpdate),
            UpdateStaging::Staged(staged) => self.smoke_and_activate(*staged),
        }
    }

    fn smoke_and_activate(&self, staged: StagedArtifact) -> UpdateResult<UpdatePreparation> {
        let staged_path = self.store.staged_artifact_path(&staged);
        self.emit(
            ObservationSeverity::Info,
            "runtime.update.staged",
            Some(("build_id", staged.descriptor.build_id().as_str())),
        );
        if let Err(error) = run_smoke_test(&staged_path, self.smoke_test_timeout) {
            self.store.discard_staged(&staged)?;
            self.audit(
                "runtime.update.smoke_test",
                AuditOutcome::Rejected,
                Some(update_error_kind(&error)),
            );
            return Err(error);
        }
        self.audit("runtime.update.smoke_test", AuditOutcome::Accepted, None);
        let receipt = self.store.activate(staged)?;
        self.audit(
            "runtime.update.pending_activation",
            AuditOutcome::Accepted,
            Some(receipt.activated.build_id().as_str()),
        );
        Ok(UpdatePreparation::AwaitingHealth(Box::new(
            receipt.activated,
        )))
    }

    fn emit(&self, severity: ObservationSeverity, name: &str, attribute: Option<(&str, &str)>) {
        let mut event = ObservationEvent::new(ObservationKind::Lifecycle, severity, name, now_ms());
        if let Some((key, value)) = attribute {
            event = event.with_attribute(key, value);
        }
        self.observations.emit(event);
    }

    fn audit(&self, name: &str, outcome: AuditOutcome, message: Option<&str>) {
        let timestamp = now_ms();
        let mut entry = AuditEntry::new(
            AuditCategory::Runtime,
            format!("{name}-{timestamp}"),
            name,
            timestamp,
            timestamp,
            outcome,
        );
        entry.app_id = Some(self.application_id.clone());
        entry.node_id = Some(self.node_id.clone());
        entry.message = message.map(str::to_string);
        self.controller
            .lock()
            .instance()
            .audit_log()
            .push_entry(entry);
    }
}

pub(crate) fn validate_update_authenticity_config(
    config: &ProviderConfig,
) -> Result<(), BootstrapError> {
    build_authenticity_verifier(config).map(|_| ())
}

fn parse_u64_setting(
    config: &ProviderConfig,
    name: &str,
    default: u64,
) -> Result<u64, BootstrapError> {
    let Some(value) = config.settings().get(name) else {
        return Ok(default);
    };
    value.parse::<u64>().map_err(|_| {
        BootstrapError::Runtime(format!("update provider setting `{name}` must be a u64"))
    })
}

fn parse_usize_setting(
    config: &ProviderConfig,
    name: &str,
    default: usize,
) -> Result<usize, BootstrapError> {
    let Some(value) = config.settings().get(name) else {
        return Ok(default);
    };
    value.parse::<usize>().map_err(|_| {
        BootstrapError::Runtime(format!("update provider setting `{name}` must be a usize"))
    })
}

fn update_bootstrap_error(error: appcore_update::UpdateError) -> BootstrapError {
    BootstrapError::Runtime(format!("invalid update authenticity policy: {error}"))
}

fn interruptible_sleep(shutdown: &AtomicBool, duration: Duration) {
    let mut remaining = duration;
    while !remaining.is_zero() && !shutdown.load(Ordering::Acquire) {
        let step = remaining.min(Duration::from_millis(100));
        thread::sleep(step);
        remaining = remaining.saturating_sub(step);
    }
}

fn update_error_kind(error: &appcore_update::UpdateError) -> &'static str {
    use appcore_update::UpdateError;
    match error {
        UpdateError::InvalidArtifact(_) => "invalid_artifact",
        UpdateError::Incompatible(_) => "incompatible",
        UpdateError::Provider(_) => "provider",
        UpdateError::ArtifactTooLarge { .. } => "artifact_too_large",
        UpdateError::ChecksumMismatch => "checksum_mismatch",
        UpdateError::Authenticity(_) => "authenticity",
        UpdateError::Store(_) => "store",
        UpdateError::Health(_) => "health",
        UpdateError::InjectedFault(_) => "injected_fault",
        UpdateError::RollbackFailed { .. } => "rollback_failed",
    }
}

#[cfg(test)]
#[path = "update_service_tests.rs"]
mod tests;
