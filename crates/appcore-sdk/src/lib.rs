// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/04 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/04 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! # `AppCore` SDK
//!
//! `appcore-sdk` is the stable, application-facing entry point for `AppCore`.
//! It deliberately presents contracts and small adapters rather than owning a
//! host, provider selection, storage engine, scheduler, gateway, or protocol.
//! Those systems remain independently usable in their owning crates.
//!
//! ## Hello World
//!
//! ```
//! use appcore_sdk::prelude::*;
//!
//! fn main() -> AppResult<()> {
//!     appcore_sdk::run("hello-world", |app| {
//!         app.log("Hello, world!");
//!         Ok(())
//!     })
//! }
//! ```
//!
//! This local entry point validates the application identifier and provides a
//! bounded, structured logging boundary. It intentionally does not start a
//! network listener or infer deployment policy. A deployed application owns its
//! process boundary and uses explicit manifests; add specialized crates only
//! when a capability is needed.
//!
//! ## Learning path
//!
//! 1. Start with [`run`] and [`prelude`].
//! 2. Use [`App`] as the validated local application context.
//! 3. Replace its canonical defaults with explicit [`manifest`] contracts when
//!    installation policy must be supplied externally.
//! 4. Implement [`Application`] and call [`App::prepare`] to collect commands,
//!    events, states, decisions, handlers, queries and scheduled tasks.
//! 5. Enable `storage`, `api`, `sync`, `ai` or `filemaker` only when
//!    that capability is genuinely required.
//! 6. Enable `deployment` for provider-resolved bindings and `scheduler` for
//!    bounded task definitions.
//! 7. Select an explicit deployment process for distributed composition,
//!    clustering and lifecycle. The `SDK` never makes a low-level crate depend
//!    on it.
//!
//! ## Escape hatches
//!
//! The namespaced reexports are intentionally selective. Import an owning
//! `appcore-*` crate directly for an advanced API not represented here. That is
//! an explicit choice, not a limitation of the `SDK`.

#![deny(missing_docs)]

pub mod application;
#[cfg(feature = "deployment")]
pub mod context;
pub mod error;
pub mod prelude;
mod prepared;
mod simple_app;

/// Canonical manifest contracts accepted by every compatible `AppCore` Runtime.
pub mod manifest {
    pub use appcore_contracts::{
        ApplicationId, ApplicationManifestV1, DeploymentManifestV1, InstallationId, NetworkConfig,
        ProviderConfig, ProviderId, RuntimeHealthStatus, RuntimeMode, RuntimeRequirements,
        ServiceId,
    };
}

/// Bounded structured logging configuration used by [`App::logging`].
pub mod logging {
    pub use appcore_log::{
        FileArchiveConfig, FileSinkConfig, LogConfigError, LogOutputMode, LogPolicy, LoggerConfig,
        PathAliases, Sensitivity, Severity, Verbosity, LOG_SIZE_16_MIB, LOG_SIZE_1_MIB,
        LOG_SIZE_2_MIB, LOG_SIZE_32_MIB, LOG_SIZE_4_MIB, LOG_SIZE_64_MIB, LOG_SIZE_8_MIB,
    };
}

/// Opt-in HTTP query contracts; applications with only commands and events do
/// not need this capability. Enable with the `api` feature.
#[cfg(feature = "api")]
pub mod api {
    pub use appcore_api::{ApiRequest, ApiResponse, ApiRouter, QueryEndpoint, QueryName};
}

/// Opt-in provider-backed deployment binding contracts. Enable with the
/// `deployment` feature.
#[cfg(feature = "deployment")]
pub mod deployment {
    pub use crate::{DeploymentContext, DeploymentEnvironmentValue, ResolvedVolumeMount};
}

/// Opt-in scheduled-work contracts; background work is absent from the core
/// facade. Enable with the `scheduler` feature.
#[cfg(feature = "scheduler")]
pub mod scheduler {
    pub use crate::application::{
        ApplicationTaskRegistry, RegisteredApplicationTask, RetryPolicy, ScheduledTask,
        TaskContext, TaskResult, TaskSchedule,
    };
}

/// Opt-in storage contracts; applications that only register behavior do not
/// pay for a storage implementation. Enable with the `storage` feature.
#[cfg(feature = "storage")]
pub mod storage {
    pub use appcore_storage::{FileStorageProvider, StorageProvider, StorageStatus};
}

/// Opt-in replication contracts exposing common subsystem types. Enable with
/// the `sync` feature.
#[cfg(feature = "sync")]
pub mod sync {
    pub use appcore_sync::{
        InMemorySyncOutbox, ReplicationLog, SyncMessage, SyncOutbox, SyncResult,
    };
}

/// Opt-in AI contracts; they are never required by the deterministic SDK core.
/// Enable with the `ai` feature.
#[cfg(feature = "ai")]
pub mod ai {
    pub use appcore_ai::{
        AiExecutionMode, AiLimits, AiPrivacyMode, AiRequest, AiResponse, AiResult, AiRuntime,
        AiTask, CancellationToken,
    };
}

/// Opt-in deterministic document contracts. Enable with the `filemaker`
/// feature.
#[cfg(feature = "filemaker")]
pub mod filemaker {
    pub use appcore_filemaker::{
        export_bytes, Compiler, CompilerBuilder, DataValue, ExportContext, ExportFormat,
        ExportRequest, FileMakerError, FontAsset, FontManager, LayoutEngine, LayoutOptions,
        ResourceLimits, Result, TemplateSourceV1,
    };
}

pub use application::Application;
#[cfg(feature = "scheduler")]
pub use application::{ApplicationTaskRegistry, RegisteredApplicationTask};
#[cfg(feature = "deployment")]
pub use context::{DeploymentContext, DeploymentEnvironmentValue, ResolvedVolumeMount};
pub use error::{AppError, AppResult};
pub use prepared::PreparedApplication;
pub use simple_app::{run, App};
