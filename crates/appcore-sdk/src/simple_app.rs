// =============================================================================
//        #######
//     ###       ###     F: simple_app.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/04 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/04 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Minimal local application context and canonical zero-config manifests.
//!
//! A manifest is always part of an `AppCore` application contract. Files are not
//! required for the smallest local program: this module constructs the same V1
//! contract values that a manifest reader validates, then exposes them for
//! inspection or replacement before execution.

use crate::{AppError, AppResult, Application, PreparedApplication};
use appcore_contracts::{
    ApplicationId, ApplicationManifestV1, DeploymentManifestV1, InstallationId, NetworkConfig,
    ProviderConfig, ProviderId, RuntimeMode, RuntimeRequirements, ServiceId,
};
use appcore_core::AppId;
use appcore_core::NodeId;
use appcore_log::{ConfiguredLogger, LogBuilder, LoggerConfig};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_APPLICATION_VERSION: &str = "0.1.0";
const DEFAULT_RUNTIME_VERSION: &str = "1.0.0";
const DEFAULT_PROTOCOL_VERSION: &str = "1";
const DEFAULT_VENDOR: &str = "local";
const DEFAULT_STORAGE_PROVIDER: &str = "file";
const DEFAULT_TRANSPORT_PROVIDER: &str = "http";

/// A small local application context with effective validated manifests.
///
/// `App::new` creates a standalone, local-only deployment: no listener,
/// cluster, gateway, peer RPC, AI, storage path or external service is started
/// implicitly. The default storage and transport provider identifiers exist in
/// the deployment contract only; an application-owned deployment selects their
/// implementation when it later opts into explicit manifest execution.
pub struct App {
    id: AppId,
    application_manifest: ApplicationManifestV1,
    deployment_manifest: DeploymentManifestV1,
    logger: ConfiguredLogger,
    logger_component: String,
}

impl std::fmt::Debug for App {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("App")
            .field("id", &self.id)
            .field("application_manifest", &self.application_manifest)
            .field("deployment_manifest", &self.deployment_manifest)
            .finish_non_exhaustive()
    }
}

impl App {
    /// Creates a local application with validated V1 standalone defaults.
    ///
    /// The resulting manifests are equivalent to the minimal canonical
    /// contracts, not a second `SDK`-specific configuration format.
    pub fn new(name: &str) -> AppResult<Self> {
        let id = AppId::new(name)?;
        let application_manifest = default_application_manifest(name)?;
        let deployment_manifest = default_deployment_manifest(name)?;
        Self::from_manifests(id, application_manifest, deployment_manifest)
    }

    /// Replaces the application manifest after normal contract validation.
    ///
    /// Explicit configuration always wins over `SDK` defaults. Invalid input is
    /// returned unchanged as an error and is never silently replaced.
    pub fn application_manifest(mut self, manifest: ApplicationManifestV1) -> AppResult<Self> {
        manifest.validate()?;
        self.application_manifest = manifest;
        self.ensure_manifest_identity()?;
        Ok(self)
    }

    /// Replaces the deployment manifest after normal contract validation.
    ///
    /// An explicit deployment never inherits cluster, provider or networking
    /// values from the `SDK` default; it entirely owns installation policy.
    pub fn deployment_manifest(mut self, manifest: DeploymentManifestV1) -> AppResult<Self> {
        manifest.validate()?;
        self.deployment_manifest = manifest;
        self.ensure_manifest_identity()?;
        Ok(self)
    }

    /// Returns the validated application-owned manifest currently in effect.
    pub fn effective_application_manifest(&self) -> &ApplicationManifestV1 {
        &self.application_manifest
    }

    /// Returns the validated installation-owned manifest currently in effect.
    pub fn effective_deployment_manifest(&self) -> &DeploymentManifestV1 {
        &self.deployment_manifest
    }

    /// Returns the validated application identifier.
    pub fn id(&self) -> &AppId {
        &self.id
    }

    /// Selects explicit bounded logging destinations for this application.
    ///
    /// File, terminal, combined, disabled and crash-only modes are provided by
    /// [`crate::logging::LoggerConfig`]. Invalid limits fail during setup.
    pub fn logging(mut self, config: LoggerConfig) -> AppResult<Self> {
        self.logger = config.build()?;
        Ok(self)
    }

    /// Emits one informational message through `AppCore`'s redacting logger.
    ///
    /// Logging is an observability side effect; setup and business failures
    /// must still be returned by the caller's callback.
    pub fn log(&self, message: &str) {
        self.logger().info(message);
    }

    /// Returns a fluent logger scoped to this application.
    ///
    /// Use `app.logger().info("started")` for an explicit severity, or add
    /// `.component("sync").verbosity(7)` before the final severity method.
    pub fn logger(&self) -> LogBuilder<'_> {
        self.logger
            .dispatcher()
            .event(now_ms(), self.logger_component.as_str())
    }

    /// Collects an [`Application`]'s behavior into executable Core registries.
    ///
    /// Preparation performs no I/O and starts no service. It validates and
    /// freezes registrations so an explicit deployment can inspect or execute
    /// them without importing `RuntimeBuilder`.
    ///
    /// # Errors
    ///
    /// Returns the exact controlled Runtime error produced by a registration
    /// hook, an invalid node identity or inconsistent decision registration.
    ///
    /// # Examples
    ///
    /// ```
    /// use appcore_sdk::application::NodeId;
    /// use appcore_sdk::{App, Application, AppResult};
    ///
    /// struct Business;
    /// impl Application for Business {}
    ///
    /// # fn example() -> AppResult<()> {
    /// let app = App::new("prepared-example")?;
    /// let prepared = app.prepare(&Business, NodeId::new("local-node")?)?;
    /// assert!(prepared.runtime().commands().is_empty());
    /// # Ok(())
    /// # }
    /// ```
    pub fn prepare<A>(&self, application: &A, node_id: NodeId) -> AppResult<PreparedApplication>
    where
        A: Application + ?Sized,
    {
        PreparedApplication::new(
            self.application_manifest.clone(),
            &self.deployment_manifest,
            application,
            node_id,
        )
    }

    /// Applies resolved deployment bindings and then prepares application behavior.
    ///
    /// This is the integration boundary for an explicit deployment. The SDK
    /// neither resolves the supplied bindings nor retains their secret values.
    ///
    /// # Errors
    ///
    /// Returns the controlled error from `Application::configure` or from any
    /// subsequent registration hook.
    #[cfg(feature = "deployment")]
    pub fn prepare_with_deployment<A>(
        &self,
        application: &A,
        node_id: NodeId,
        deployment: &crate::DeploymentContext,
    ) -> AppResult<PreparedApplication>
    where
        A: Application + ?Sized,
    {
        application.configure(deployment)?;
        self.prepare(application, node_id)
    }

    /// Persists the bounded crash ring when crash-only logging is configured.
    ///
    /// Other output modes return zero without creating a file.
    pub fn dump_crash_log(&self) -> AppResult<usize> {
        Ok(self.logger.dump_crash()?)
    }

    /// Runs a local callback using the effective validated manifests.
    ///
    /// This deliberately does not create a hidden host. When an application
    /// needs provider composition or long-running services, it owns an explicit
    /// deployment process; the same manifest values can then be serialized.
    pub fn run<F>(self, configure: F) -> AppResult<()>
    where
        F: FnOnce(&App) -> AppResult<()>,
    {
        configure(&self)
    }

    fn from_manifests(
        id: AppId,
        application_manifest: ApplicationManifestV1,
        deployment_manifest: DeploymentManifestV1,
    ) -> AppResult<Self> {
        let logger_component = format!("application.{}", id.as_str());
        let app = Self {
            id,
            application_manifest,
            deployment_manifest,
            logger: LoggerConfig::default().build()?,
            logger_component,
        };
        app.ensure_manifest_identity()?;
        Ok(app)
    }

    fn ensure_manifest_identity(&self) -> AppResult<()> {
        if self.application_manifest.application_id() != self.deployment_manifest.application_id()
            || self.application_manifest.application_id().as_str() != self.id.as_str()
        {
            return Err(AppError::ManifestIdentityMismatch);
        }
        Ok(())
    }
}

/// Runs a local application callback with canonical standalone defaults.
///
/// # Errors
///
/// Returns identifier or manifest-validation errors, and preserves the exact
/// error from `configure`. It never starts background services or suppresses
/// failures.
pub fn run<F>(name: &str, configure: F) -> AppResult<()>
where
    F: FnOnce(&App) -> AppResult<()>,
{
    App::new(name)?.run(configure)
}

fn default_application_manifest(name: &str) -> AppResult<ApplicationManifestV1> {
    Ok(ApplicationManifestV1::new(
        ApplicationId::new(name)?,
        DEFAULT_APPLICATION_VERSION,
        name,
        DEFAULT_VENDOR,
        ServiceId::new(format!("{name}.local"))?,
        RuntimeRequirements::new(DEFAULT_RUNTIME_VERSION, DEFAULT_PROTOCOL_VERSION)?,
    )?)
}

fn default_deployment_manifest(name: &str) -> AppResult<DeploymentManifestV1> {
    Ok(DeploymentManifestV1::builder(
        InstallationId::new(format!("{name}-local"))?,
        ApplicationId::new(name)?,
        RuntimeMode::Standalone,
        ProviderConfig::new(ProviderId::new(DEFAULT_STORAGE_PROVIDER)?),
        NetworkConfig::new(
            ProviderId::new(DEFAULT_TRANSPORT_PROVIDER)?,
            ProviderId::new(DEFAULT_TRANSPORT_PROVIDER)?,
        ),
    )
    .build()?)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().try_into().unwrap_or(u64::MAX))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_config_uses_validated_standalone_manifests() {
        let app = App::new("hello-sdk").expect("defaults are valid");
        assert_eq!(
            app.effective_deployment_manifest().mode(),
            RuntimeMode::Standalone
        );
        app.effective_application_manifest()
            .validate()
            .expect("application manifest validates");
        app.effective_deployment_manifest()
            .validate()
            .expect("deployment manifest validates");
    }

    #[test]
    fn application_accepts_disabled_logging_without_a_sink() {
        let app = App::new("quiet-sdk")
            .unwrap()
            .logging(LoggerConfig {
                output: appcore_log::LogOutputMode::Disabled,
                ..LoggerConfig::default()
            })
            .unwrap();

        app.log("this performs no sink work");
        assert!(app.logger.dump_crash().is_ok_and(|written| written == 0));
    }

    #[test]
    fn callback_receives_validated_identity() {
        run("hello-sdk", |app| {
            assert_eq!(app.id().as_str(), "hello-sdk");
            Ok(())
        })
        .expect("run succeeds");
    }

    #[test]
    fn invalid_identity_is_returned() {
        assert!(run("", |_| Ok(())).is_err());
    }

    #[test]
    fn explicit_manifest_identity_mismatch_is_not_masked_by_defaults() {
        let other = App::new("other-sdk").expect("other defaults");
        let error = App::new("hello-sdk")
            .expect("hello defaults")
            .application_manifest(other.effective_application_manifest().clone())
            .expect_err("explicit application identity must win and fail");
        assert_eq!(error, AppError::ManifestIdentityMismatch);
    }

    #[test]
    fn explicit_deployment_replaces_the_standalone_default() {
        let deployment = DeploymentManifestV1::builder(
            InstallationId::new("hello-sdk-explicit").expect("installation identity"),
            ApplicationId::new("hello-sdk").expect("application identity"),
            RuntimeMode::Standalone,
            ProviderConfig::new(ProviderId::new("memory").expect("storage provider")),
            NetworkConfig::new(
                ProviderId::new("loopback").expect("peer transport"),
                ProviderId::new("loopback").expect("command transport"),
            ),
        )
        .build()
        .expect("explicit deployment");
        let app = App::new("hello-sdk")
            .expect("target defaults")
            .deployment_manifest(deployment)
            .expect("matching explicit deployment");
        assert_eq!(
            app.effective_deployment_manifest()
                .installation_id()
                .as_str(),
            "hello-sdk-explicit"
        );
        assert_eq!(
            app.effective_deployment_manifest().mode(),
            RuntimeMode::Standalone
        );
    }
}
