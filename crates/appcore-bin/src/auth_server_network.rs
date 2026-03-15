// =============================================================================
//        #######
//     ###       ###     F: auth_server_network.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/07 12:31:50 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Bounded Axum transport for the optional auth-server companion.

use appcore_peer_rpc::{
    BoundedReplayStore, PeerNonceStore, PeerRpcError, ReplayStore, ReplayStoreConfig,
    ReplayStoreMetrics,
};
use appcore_security::{parse_secret_material, HashTokenProvider, SecuritySecretStatus};
use appcore_storage::{
    now_ms, open_remote_request, process_remote_request, seal_remote_response,
    DEFAULT_AUTH_REMOTE_MAX_BYTES,
};
use appcore_supervisor::{
    ManagedResource, ManagedThreadService, RestartPolicy, ServiceDescriptor, Supervisor,
};
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Once};
use std::thread;
use std::time::Duration;
use tokio::sync::Semaphore;

#[path = "auth_server_hosting.rs"]
mod hosting;

pub(crate) use hosting::run_auth_server_serve;
#[cfg(test)]
use hosting::{start_hosted_auth_service, supervisor_for_hosting};

pub(crate) const DEFAULT_AUTH_BIND: &str = "127.0.0.1:39877";
const AUTH_REPLAY_MAX_ENTRIES: usize = 32_768;
const AUTH_REPLAY_TTL_MS: u64 = 60_000;
const AUTH_REPLAY_CLEANUP_MS: u64 = 1_000;
const AUTH_MAX_CONCURRENCY: usize = 32;
const AUTH_RATE_LIMIT_PER_SECOND: u64 = 120;
const AUTH_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

// appcore-norm: allow(global-state) reason: process signal state requires lock-free cross-thread coordination
static AUTH_STOP_REQUESTED: AtomicBool = AtomicBool::new(false);
// appcore-norm: allow(global-state) reason: signal handler installation must occur once per process
static AUTH_CTRL_C_INIT: Once = Once::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthServerServeOptions {
    pub(crate) data_secret_path: String,
    pub(crate) transport_secret_path: String,
    pub(crate) bind: String,
    pub(crate) auto_restart: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthServeError {
    pub(crate) status: u16,
    pub(crate) message: String,
}

#[derive(Clone)]
pub enum AuthServerHosting {
    RuntimeManaged(Arc<Supervisor>),
    StandaloneCompanion,
}

/// Auth-server wrapper around the bounded shared replay-store contract.
#[derive(Debug)]
pub(crate) struct AuthReplayCache {
    store: BoundedReplayStore,
}

impl Default for AuthReplayCache {
    fn default() -> Self {
        let config = ReplayStoreConfig::new(
            AUTH_REPLAY_MAX_ENTRIES,
            AUTH_REPLAY_TTL_MS,
            AUTH_REPLAY_CLEANUP_MS,
        )
        .unwrap_or_default();
        Self {
            store: BoundedReplayStore::new(config),
        }
    }
}

impl AuthReplayCache {
    #[cfg(test)]
    pub(crate) fn with_config(config: ReplayStoreConfig) -> Self {
        Self {
            store: BoundedReplayStore::new(config),
        }
    }

    pub(crate) fn metrics(&self) -> ReplayStoreMetrics {
        self.store.metrics()
    }
}

#[derive(Clone)]
struct AuthHttpState {
    transport: HashTokenProvider,
    data: HashTokenProvider,
    replay: Arc<AuthReplayCache>,
    concurrency: Arc<Semaphore>,
    rate_limit: Arc<RateLimiter>,
    timeout: Duration,
    supervisor: Arc<Supervisor>,
}

#[derive(Debug)]
struct RateLimiter {
    max_requests: u64,
    window_ms: u64,
    state: Mutex<RateWindow>,
}

#[derive(Debug, Default)]
struct RateWindow {
    started_at_ms: u64,
    requests: u64,
}

impl RateLimiter {
    fn new(max_requests: u64, window_ms: u64) -> Self {
        Self {
            max_requests: max_requests.max(1),
            window_ms: window_ms.max(1),
            state: Mutex::new(RateWindow::default()),
        }
    }

    fn allow(&self, timestamp_ms: u64) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        if state.started_at_ms == 0
            || timestamp_ms.saturating_sub(state.started_at_ms) >= self.window_ms
        {
            state.started_at_ms = timestamp_ms;
            state.requests = 0;
        }
        if state.requests >= self.max_requests {
            return false;
        }
        state.requests = state.requests.saturating_add(1);
        true
    }
}

pub(crate) fn handle_auth_storage_token(
    token: &str,
    transport: &HashTokenProvider,
    data: &HashTokenProvider,
    replay: &AuthReplayCache,
    timestamp_ms: u64,
) -> Result<String, AuthServeError> {
    let request = open_remote_request(token, transport, timestamp_ms).map_err(map_open_error)?;
    replay
        .store
        .check_and_record(&request.nonce, request.expires_at_ms, timestamp_ms)
        .map_err(map_replay_error)?;
    let response = process_remote_request(&request, data).map_err(map_process_error)?;
    seal_remote_response(&response, transport).map_err(map_process_error)
}

fn auth_managed_service(
    options: &AuthServerServeOptions,
    state: AuthHttpState,
) -> Result<Arc<ManagedThreadService>, String> {
    let policy = if options.auto_restart {
        RestartPolicy::bounded(5, Duration::from_secs(600)).map_err(|error| error.to_string())?
    } else {
        RestartPolicy::never()
    };
    let descriptor = ServiceDescriptor::new("auth-server", ManagedResource::AuthServer, policy)
        .and_then(|descriptor| descriptor.with_dependency("security"))
        .map_err(|error| error.to_string())?;
    let bind = options.bind.clone();
    Ok(Arc::new(ManagedThreadService::new(
        descriptor,
        move |shutdown| {
            let bind = bind.clone();
            let state = state.clone();
            thread::Builder::new()
                .name("appcore-auth-http".to_string())
                .spawn(move || run_auth_http(bind, state, shutdown))
                .map_err(|error| error.to_string())
        },
    )))
}

fn run_auth_http(
    bind: String,
    state: AuthHttpState,
    shutdown: Arc<AtomicBool>,
) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(async move {
        let router = Router::new()
            .route("/auth/storage", post(auth_storage_handler))
            .route("/health", get(auth_health_handler))
            .layer(DefaultBodyLimit::max(DEFAULT_AUTH_REMOTE_MAX_BYTES))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind(&bind)
            .await
            .map_err(|error| error.to_string())?;
        axum::serve(listener, router)
            .with_graceful_shutdown(wait_for_shutdown(shutdown))
            .await
            .map_err(|error| error.to_string())
    })
}

async fn auth_health_handler(State(state): State<AuthHttpState>) -> Response {
    let snapshot = state.supervisor.evaluate_watchdog(now_ms());
    let diagnosis = state.supervisor.diagnose();
    let healthy = snapshot.is_healthy()
        && snapshot.critical_services_healthy
        && diagnosis.restart_executor.healthy;
    let payload = serde_json::json!({
        "status": if healthy { "healthy" } else { "unhealthy" },
        "supervisor": {
            "state": format!("{:?}", snapshot.state).to_ascii_lowercase(),
            "reconcile_sequence": snapshot.reconcile_sequence,
            "last_progress_at_ms": snapshot.last_progress_at_ms,
            "stalled_for_ms": snapshot.stalled_for_ms,
            "restart_executor_healthy": diagnosis.restart_executor.healthy,
            "restart_budget": diagnosis.services.iter().find(|service| {
                service.name == "auth-server"
            }).map(|service| service.restart_count)
        }
    });
    (
        if healthy {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        axum::Json(payload),
    )
        .into_response()
}

async fn auth_storage_handler(State(state): State<AuthHttpState>, body: Bytes) -> Response {
    if !state.rate_limit.allow(now_ms()) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    let permit = match Arc::clone(&state.concurrency).try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let token = match String::from_utf8(body.to_vec()) {
        Ok(token) if !token.is_empty() => token,
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    let timeout = state.timeout;
    let task = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        handle_auth_storage_token(
            &token,
            &state.transport,
            &state.data,
            &state.replay,
            now_ms(),
        )
    });
    match tokio::time::timeout(timeout, task).await {
        Ok(Ok(Ok(body))) => (StatusCode::OK, body).into_response(),
        Ok(Ok(Err(error))) => status_code(error.status).into_response(),
        Ok(Err(_)) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        Err(_) => StatusCode::REQUEST_TIMEOUT.into_response(),
    }
}

async fn wait_for_shutdown(shutdown: Arc<AtomicBool>) {
    while !shutdown.load(Ordering::Acquire) && !AUTH_STOP_REQUESTED.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn load_provider(path: &Path, label: &str) -> Result<HashTokenProvider, String> {
    let raw = fs::read(path).map_err(|_| format!("auth {label} secret file missing"))?;
    let material =
        parse_secret_material(&raw).map_err(|_| format!("auth {label} secret file invalid"))?;
    reject_unusable_secret(
        &material.metadata.status,
        material.is_expired(now_ms()),
        label,
    )?;
    HashTokenProvider::from_secret(material.secret.clone())
        .map_err(|_| format!("auth {label} secret too weak"))
}

fn reject_unusable_secret(
    status: &SecuritySecretStatus,
    expired: bool,
    label: &str,
) -> Result<(), String> {
    if *status == SecuritySecretStatus::Revoked {
        return Err(format!("auth {label} secret revoked"));
    }
    if expired {
        return Err(format!("auth {label} secret expired"));
    }
    Ok(())
}

fn install_auth_ctrlc_handler() -> Result<(), String> {
    let mut result = Ok(());
    AUTH_CTRL_C_INIT.call_once(|| {
        result = ctrlc::set_handler(|| {
            AUTH_STOP_REQUESTED.store(true, Ordering::Release);
        })
        .map_err(|error| error.to_string());
    });
    result
}

fn status_code(status: u16) -> StatusCode {
    StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
}

fn map_replay_error(error: PeerRpcError) -> AuthServeError {
    match error {
        PeerRpcError::NonceReplay => AuthServeError {
            status: 409,
            message: "auth replay rejected".to_string(),
        },
        PeerRpcError::NonceCacheFull => AuthServeError {
            status: 503,
            message: "auth replay capacity reached".to_string(),
        },
        _ => AuthServeError {
            status: 401,
            message: "auth replay identity rejected".to_string(),
        },
    }
}

fn map_open_error(error: appcore_storage::StorageError) -> AuthServeError {
    match error {
        appcore_storage::StorageError::InvalidPath(message) => AuthServeError {
            status: 400,
            message,
        },
        _ => AuthServeError {
            status: 401,
            message: "auth transport rejected".to_string(),
        },
    }
}

fn map_process_error(error: appcore_storage::StorageError) -> AuthServeError {
    AuthServeError {
        status: 403,
        message: format!("{error:?}"),
    }
}

#[cfg(test)]
#[path = "auth_server_network_tests.rs"]
mod tests;
