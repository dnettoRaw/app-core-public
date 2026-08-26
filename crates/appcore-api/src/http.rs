// =============================================================================
//        #######
//     ###       ###     F: http.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Minimal HTTP host for runtime health, status, and command ingress.

mod auth;
mod command;
mod handlers;
mod query;
mod response;
mod state;
mod trace;

pub use auth::{CommandTokenVerifier, HttpCommandAuth, RequestValidationDetails};
pub use state::{
    CommandCapabilityPolicy, CommandCapabilityPolicyError, HttpApiConfig, RuntimeStaticInfo,
    SyncLogView, SyncLogViewError,
};

#[cfg(test)]
use crate::command_contract::{CommandRequest, CommandResponse, CommandResponseEvent};
use crate::ApiRouter;
use appcore_core::{RuntimeController, RuntimeOperationalMode};
use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::Router;
use command::command_handler;
use handlers::{
    diagnostics_handler, health_handler, private_status_handler, public_status_handler,
    status_handler,
};
use parking_lot::Mutex;
use query::query_handler;
use state::HttpState;
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Embedded HTTP host for stable Runtime health, status, command, and query routes.
pub struct RuntimeHttpHost {
    config: HttpApiConfig,
    router: Router,
}

impl RuntimeHttpHost {
    /// Creates a host exposing only static operational information.
    pub fn new(config: HttpApiConfig, static_info: RuntimeStaticInfo) -> Self {
        Self::with_runtime_state(config, static_info, RuntimeHttpStateParts::default())
    }

    /// Creates a host connected to a Runtime controller.
    pub fn with_controller(
        config: HttpApiConfig,
        static_info: RuntimeStaticInfo,
        controller: Arc<Mutex<RuntimeController>>,
    ) -> Self {
        Self::with_runtime_state(
            config,
            static_info,
            RuntimeHttpStateParts {
                controller: Some(controller),
                ..RuntimeHttpStateParts::default()
            },
        )
    }

    /// Creates a host with controller, sync status, tick counter, and token policy.
    pub fn with_runtime_state_and_auth(
        config: HttpApiConfig,
        static_info: RuntimeStaticInfo,
        controller: Arc<Mutex<RuntimeController>>,
        sync_log: Option<Arc<dyn SyncLogView>>,
        tick_counter: Option<Arc<AtomicU64>>,
        auth: HttpCommandAuth,
    ) -> Self {
        Self::with_runtime_state(
            config,
            static_info,
            RuntimeHttpStateParts {
                controller: Some(controller),
                sync_log,
                tick_counter,
                auth,
                ..RuntimeHttpStateParts::default()
            },
        )
    }

    /// Creates an authenticated host with a live operational-mode source.
    pub fn with_runtime_state_auth_and_operation_mode(
        config: HttpApiConfig,
        static_info: RuntimeStaticInfo,
        controller: Arc<Mutex<RuntimeController>>,
        sync_log: Option<Arc<dyn SyncLogView>>,
        tick_counter: Option<Arc<AtomicU64>>,
        auth: HttpCommandAuth,
        operation_mode: Arc<Mutex<appcore_core::RuntimeOperationalMode>>,
    ) -> Self {
        Self::with_runtime_state(
            config,
            static_info,
            RuntimeHttpStateParts {
                controller: Some(controller),
                sync_log,
                tick_counter,
                operation_mode: Some(operation_mode),
                auth,
                ..RuntimeHttpStateParts::default()
            },
        )
    }

    /// Creates an authenticated host with application query routing.
    pub fn with_runtime_state_auth_and_app_queries(
        config: HttpApiConfig,
        static_info: RuntimeStaticInfo,
        controller: Arc<Mutex<RuntimeController>>,
        sync_log: Option<Arc<dyn SyncLogView>>,
        tick_counter: Option<Arc<AtomicU64>>,
        auth: HttpCommandAuth,
        app_query_router: Arc<Mutex<ApiRouter>>,
    ) -> Self {
        Self::with_runtime_state(
            config,
            static_info,
            RuntimeHttpStateParts {
                controller: Some(controller),
                sync_log,
                tick_counter,
                app_query_router: Some(app_query_router),
                auth,
                ..RuntimeHttpStateParts::default()
            },
        )
    }

    /// Creates a host from the complete set of optional shared state parts.
    pub fn with_state_parts(
        config: HttpApiConfig,
        static_info: RuntimeStaticInfo,
        parts: RuntimeHttpStateParts,
    ) -> Self {
        Self::with_runtime_state(config, static_info, parts)
    }

    fn with_runtime_state(
        config: HttpApiConfig,
        static_info: RuntimeStaticInfo,
        parts: RuntimeHttpStateParts,
    ) -> Self {
        if let Some(router) = &parts.app_query_router {
            router.lock().freeze_queries();
        }
        let state = HttpState {
            static_info,
            controller: parts.controller,
            app_query_router: parts.app_query_router,
            sync_log: parts.sync_log,
            tick_counter: parts.tick_counter,
            operation_mode: parts.operation_mode,
            command_policy: parts.command_policy,
            supervisor: parts.supervisor,
            auth: parts.auth,
            max_payload_bytes: config.max_payload_bytes,
            clock: Arc::new(appcore_core::SystemClock::new()),
        };
        let router = Router::new()
            .route("/v1/health", get(health_handler))
            .route("/v1/status", get(status_handler))
            .route("/v1/status/public", get(public_status_handler))
            .route("/v1/status/private", get(private_status_handler))
            .route("/v1/diagnostics", get(diagnostics_handler))
            .route("/v1/command", post(command_handler))
            .route("/v1/query", post(query_handler))
            .route("/health", get(update_required_handler))
            .route("/status", get(update_required_handler))
            .route("/status/public", get(update_required_handler))
            .route("/status/private", get(update_required_handler))
            .route("/diagnostics", get(update_required_handler))
            .route("/command", post(update_required_handler))
            .route("/query", post(update_required_handler))
            .layer(DefaultBodyLimit::max(config.max_payload_bytes))
            .with_state(state);
        Self { config, router }
    }

    /// Returns immutable listener and payload-limit configuration.
    pub fn config(&self) -> &HttpApiConfig {
        &self.config
    }

    /// Returns an Axum router suitable for embedding in another listener.
    pub fn router(&self) -> Router {
        self.router.clone()
    }

    /// Runs the configured listener until `shutdown` becomes true.
    pub fn run_until_shutdown(&self, shutdown: Arc<AtomicBool>) -> io::Result<()> {
        if !self.config.enabled {
            return Ok(());
        }
        let address = format!("{}:{}", self.config.host, self.config.port);
        let router = self.router();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(io::Error::other)?;
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::bind(address).await?;
            axum::serve(listener, router)
                .with_graceful_shutdown(wait_for_shutdown(shutdown))
                .await
        })
    }
}

async fn update_required_handler() -> (StatusCode, &'static str) {
    (
        StatusCode::UPGRADE_REQUIRED,
        "NO MORE SUPPORTED PLEASE UPDATE",
    )
}

#[derive(Default)]
/// Optional shared state used to compose a [`RuntimeHttpHost`].
pub struct RuntimeHttpStateParts {
    /// Runtime controller used by command and operational query routes.
    pub controller: Option<Arc<Mutex<RuntimeController>>>,
    /// Read-only synchronization-log view used by status routes.
    pub sync_log: Option<Arc<dyn SyncLogView>>,
    /// Runtime tick counter exposed through diagnostics.
    pub tick_counter: Option<Arc<AtomicU64>>,
    /// Application-owned query router.
    pub app_query_router: Option<Arc<Mutex<ApiRouter>>>,
    /// Live operational mode used to gate writes.
    pub operation_mode: Option<Arc<Mutex<RuntimeOperationalMode>>>,
    /// Capability and leadership authorization policy for commands.
    pub command_policy: Option<Arc<dyn CommandCapabilityPolicy>>,
    /// Managed-service supervisor exposed through private diagnostics.
    pub supervisor: Option<appcore_supervisor::Supervisor>,
    /// Bearer-token authorization configuration.
    pub auth: HttpCommandAuth,
}

async fn wait_for_shutdown(shutdown: Arc<AtomicBool>) {
    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod http_tests;
