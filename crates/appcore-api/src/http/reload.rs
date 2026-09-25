// =============================================================================
//        #######
//     ###       ###     F: reload.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Atomic HTTP routing-generation reload with bounded drain and rollback.

use super::reload_generation::{HttpRoutingGenerationsSnapshot, RoutingGeneration, RoutingTable};
use super::RuntimeHttpHost;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{Response, StatusCode};
use axum::routing::any;
use axum::Router;
use std::fmt;
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower::ServiceExt;

const ROUTING_RETRY_DELAY: Duration = Duration::from_millis(1);
const MAX_RELOAD_TIMEOUT: Duration = Duration::from_secs(60);

/// Reload phase associated with a controlled failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpReloadPhase {
    /// Candidate validation and health before activation.
    Prepare,
    /// Atomic routing activation and health confirmation.
    Switch,
    /// Bounded completion of requests admitted by the old generation.
    Drain,
    /// Restoration and drain after a failed activation.
    Rollback,
}

/// Bounded health and drain policy for one routing reload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpReloadPolicy {
    health_timeout: Duration,
    drain_timeout: Duration,
}

impl HttpReloadPolicy {
    /// Creates a policy with non-zero health and drain deadlines.
    pub fn new(
        health_timeout: Duration,
        drain_timeout: Duration,
    ) -> Result<Self, RuntimeHttpReloadError> {
        if health_timeout.is_zero()
            || drain_timeout.is_zero()
            || health_timeout > MAX_RELOAD_TIMEOUT
            || drain_timeout > MAX_RELOAD_TIMEOUT
        {
            return Err(RuntimeHttpReloadError::InvalidPolicy);
        }
        Ok(Self {
            health_timeout,
            drain_timeout,
        })
    }

    /// Returns the candidate health deadline.
    pub fn health_timeout(self) -> Duration {
        self.health_timeout
    }

    /// Returns the old-generation drain deadline.
    pub fn drain_timeout(self) -> Duration {
        self.drain_timeout
    }
}

/// Payload-free state and counters for a reloadable HTTP host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HttpReloadSnapshot {
    /// Generation currently selected for new requests.
    pub active_generation: u64,
    /// Requests currently executing on the selected generation.
    pub active_inflight: usize,
    /// Whether one prepare/switch/drain transaction is active.
    pub reload_in_progress: bool,
    /// Reloads that switched and drained successfully.
    pub successful_reloads: u64,
    /// Failed reload attempts, including controlled rollbacks.
    pub failed_reloads: u64,
    /// Switches restored to the prior generation.
    pub rollbacks: u64,
}

/// Controlled, redacted reload failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeHttpReloadError {
    /// Reload requires an enabled listener.
    ListenerDisabled,
    /// A candidate attempted to change the listener address in-place.
    ListenerAddressChanged,
    /// A candidate attempted to change whether ingress authentication is required.
    AuthenticationPolicyChanged,
    /// Generation identifiers must increase monotonically.
    StaleGeneration,
    /// Another reload transaction already owns the coordinator.
    ReloadInProgress,
    /// A failed or cancelled generation still owns in-flight requests.
    RetiringGenerationBusy,
    /// Health or drain deadlines must be non-zero.
    InvalidPolicy,
    /// The candidate or active generation failed its health gate.
    HealthGateFailed(HttpReloadPhase),
    /// The outgoing generation did not drain before rollback.
    DrainTimedOut,
    /// The failed generation remained active past the rollback drain deadline.
    RollbackDrainTimedOut,
}

impl fmt::Display for RuntimeHttpReloadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ListenerDisabled => formatter.write_str("HTTP reload listener is disabled"),
            Self::ListenerAddressChanged => {
                formatter.write_str("HTTP reload requires a prepared listener generation")
            }
            Self::AuthenticationPolicyChanged => {
                formatter.write_str("HTTP reload cannot change the authentication boundary")
            }
            Self::StaleGeneration => formatter.write_str("HTTP routing generation must increase"),
            Self::ReloadInProgress => formatter.write_str("HTTP reload is already in progress"),
            Self::RetiringGenerationBusy => {
                formatter.write_str("HTTP routing generation is still draining")
            }
            Self::InvalidPolicy => formatter.write_str("HTTP reload policy is invalid"),
            Self::HealthGateFailed(phase) => {
                write!(formatter, "HTTP reload health gate failed during {phase:?}")
            }
            Self::DrainTimedOut => formatter.write_str("HTTP routing generation drain timed out"),
            Self::RollbackDrainTimedOut => {
                formatter.write_str("HTTP rollback generation drain timed out")
            }
        }
    }
}

impl std::error::Error for RuntimeHttpReloadError {}

/// Prepared, health-gated candidate that can be consumed by one reload.
pub struct PreparedRuntimeHttpGeneration {
    generation: Arc<RoutingGeneration>,
}

impl fmt::Debug for PreparedRuntimeHttpGeneration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedRuntimeHttpGeneration")
            .field("generation", &self.generation.id())
            .finish_non_exhaustive()
    }
}

/// HTTP host whose stable listener dispatches through one atomic generation.
pub struct ReloadableRuntimeHttpHost {
    config: super::HttpApiConfig,
    authentication_required: bool,
    routing: Arc<RoutingTable>,
    reload_in_progress: AtomicBool,
    successful_reloads: AtomicU64,
    failed_reloads: AtomicU64,
    rollbacks: AtomicU64,
}

impl ReloadableRuntimeHttpHost {
    /// Creates a reloadable host from an already composed initial host.
    pub fn new(
        initial_generation: u64,
        host: RuntimeHttpHost,
    ) -> Result<Self, RuntimeHttpReloadError> {
        if !host.config().enabled {
            return Err(RuntimeHttpReloadError::ListenerDisabled);
        }
        if initial_generation == 0 {
            return Err(RuntimeHttpReloadError::StaleGeneration);
        }
        let config = host.config().clone();
        let authentication_required = host.authentication_required();
        Ok(Self {
            config,
            authentication_required,
            routing: Arc::new(RoutingTable::new(initial_generation, host.router())),
            reload_in_progress: AtomicBool::new(false),
            successful_reloads: AtomicU64::new(0),
            failed_reloads: AtomicU64::new(0),
            rollbacks: AtomicU64::new(0),
        })
    }

    /// Validates and owns a candidate without changing live routing.
    pub fn prepare(
        &self,
        generation: u64,
        host: RuntimeHttpHost,
    ) -> Result<PreparedRuntimeHttpGeneration, RuntimeHttpReloadError> {
        let active = self.routing.active();
        if generation <= active.id() {
            return Err(RuntimeHttpReloadError::StaleGeneration);
        }
        if !host.config().enabled {
            return Err(RuntimeHttpReloadError::ListenerDisabled);
        }
        if host.config().host != self.config.host || host.config().port != self.config.port {
            return Err(RuntimeHttpReloadError::ListenerAddressChanged);
        }
        if host.authentication_required() != self.authentication_required {
            return Err(RuntimeHttpReloadError::AuthenticationPolicyChanged);
        }
        Ok(PreparedRuntimeHttpGeneration {
            generation: self.routing.generation(generation, host.router()),
        })
    }

    #[cfg(test)]
    pub(super) fn new_for_test(
        initial_generation: u64,
        config: super::HttpApiConfig,
        router: Router,
    ) -> Self {
        Self {
            config,
            authentication_required: true,
            routing: Arc::new(RoutingTable::new(initial_generation, router)),
            reload_in_progress: AtomicBool::new(false),
            successful_reloads: AtomicU64::new(0),
            failed_reloads: AtomicU64::new(0),
            rollbacks: AtomicU64::new(0),
        }
    }

    #[cfg(test)]
    pub(super) fn prepare_router_for_test(
        &self,
        generation: u64,
        router: Router,
    ) -> PreparedRuntimeHttpGeneration {
        PreparedRuntimeHttpGeneration {
            generation: self.routing.generation(generation, router),
        }
    }

    /// Returns a router that always dispatches through the active generation.
    pub fn router(&self) -> Router {
        dynamic_router(Arc::clone(&self.routing))
    }

    /// Runs the stable listener until cooperative shutdown is requested.
    pub fn run_until_shutdown(&self, shutdown: Arc<AtomicBool>) -> io::Result<()> {
        let address = format!("{}:{}", self.config.host, self.config.port);
        let runtime = super::build_http_runtime()?;
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::bind(address).await?;
            super::validate_listener_auth_boundary(&listener, self.authentication_required)?;
            serve_listener(listener, self.router(), shutdown).await
        })
    }

    /// Runs on a listener that the composition root already bound and checked.
    pub fn run_on_listener_until_shutdown(
        &self,
        listener: std::net::TcpListener,
        shutdown: Arc<AtomicBool>,
    ) -> io::Result<()> {
        listener.set_nonblocking(true)?;
        let router = self.router();
        let runtime = super::build_http_runtime()?;
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener)?;
            super::validate_listener_auth_boundary(&listener, self.authentication_required)?;
            serve_listener(listener, router, shutdown).await
        })
    }

    /// Health-checks, atomically activates, and drains one prepared generation.
    pub async fn reload(
        &self,
        prepared: PreparedRuntimeHttpGeneration,
        policy: HttpReloadPolicy,
    ) -> Result<(), RuntimeHttpReloadError> {
        let mut guard = match ReloadGuard::acquire(
            &self.reload_in_progress,
            &self.routing,
            &self.failed_reloads,
            &self.rollbacks,
        ) {
            Ok(guard) => guard,
            Err(error) => return self.fail(error),
        };
        self.routing.release_drained_retiring();
        if self.routing.generations_snapshot().retiring.is_some() {
            return self.fail(RuntimeHttpReloadError::RetiringGenerationBusy);
        }
        let previous = self.routing.active();
        if prepared.generation.id() <= previous.id() {
            return self.fail(RuntimeHttpReloadError::StaleGeneration);
        }
        if !probe_health(prepared.generation.router(), policy.health_timeout).await {
            return self.fail(RuntimeHttpReloadError::HealthGateFailed(
                HttpReloadPhase::Prepare,
            ));
        }

        previous.stop_accepting();
        if !self.routing.retire(Arc::clone(&previous)) {
            previous.start_accepting();
            return self.fail(RuntimeHttpReloadError::RetiringGenerationBusy);
        }
        guard.arm(Arc::clone(&previous), Arc::clone(&prepared.generation));
        self.routing.activate(Arc::clone(&prepared.generation));
        if !probe_health(prepared.generation.router(), policy.health_timeout).await {
            guard.disarm();
            return self
                .rollback(previous, prepared.generation, policy)
                .await
                .and(Err(RuntimeHttpReloadError::HealthGateFailed(
                    HttpReloadPhase::Switch,
                )));
        }
        if !wait_for_drain(&previous, policy.drain_timeout).await {
            guard.disarm();
            return self
                .rollback(previous, prepared.generation, policy)
                .await
                .and(Err(RuntimeHttpReloadError::DrainTimedOut));
        }
        self.routing.release_retiring(previous.id());
        guard.disarm();
        increment(&self.successful_reloads);
        Ok(())
    }

    /// Returns the active generation and bounded operational counters.
    pub fn snapshot(&self) -> HttpReloadSnapshot {
        let active = self.routing.active();
        HttpReloadSnapshot {
            active_generation: active.id(),
            active_inflight: active.inflight(),
            reload_in_progress: self.reload_in_progress.load(Ordering::Acquire),
            successful_reloads: self.successful_reloads.load(Ordering::Relaxed),
            failed_reloads: self.failed_reloads.load(Ordering::Relaxed),
            rollbacks: self.rollbacks.load(Ordering::Relaxed),
        }
    }

    /// Returns the active and optional retiring generation without request data.
    pub fn generation_snapshot(&self) -> HttpRoutingGenerationsSnapshot {
        self.routing.generations_snapshot()
    }

    async fn rollback(
        &self,
        previous: Arc<RoutingGeneration>,
        failed: Arc<RoutingGeneration>,
        policy: HttpReloadPolicy,
    ) -> Result<(), RuntimeHttpReloadError> {
        failed.stop_accepting();
        previous.start_accepting();
        self.routing.activate(Arc::clone(&previous));
        self.routing.release_retiring(previous.id());
        if !self.routing.retire(Arc::clone(&failed)) {
            return Err(RuntimeHttpReloadError::RetiringGenerationBusy);
        }
        increment(&self.rollbacks);
        increment(&self.failed_reloads);
        if wait_for_drain(&failed, policy.drain_timeout).await {
            self.routing.release_retiring(failed.id());
            Ok(())
        } else {
            Err(RuntimeHttpReloadError::RollbackDrainTimedOut)
        }
    }

    fn fail<T>(&self, error: RuntimeHttpReloadError) -> Result<T, RuntimeHttpReloadError> {
        increment(&self.failed_reloads);
        Err(error)
    }
}

struct ReloadGuard<'a> {
    flag: &'a AtomicBool,
    routing: &'a RoutingTable,
    failed_reloads: &'a AtomicU64,
    rollbacks: &'a AtomicU64,
    rollback: Option<(Arc<RoutingGeneration>, Arc<RoutingGeneration>)>,
}

impl<'a> ReloadGuard<'a> {
    fn acquire(
        flag: &'a AtomicBool,
        routing: &'a RoutingTable,
        failed_reloads: &'a AtomicU64,
        rollbacks: &'a AtomicU64,
    ) -> Result<Self, RuntimeHttpReloadError> {
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| RuntimeHttpReloadError::ReloadInProgress)?;
        Ok(Self {
            flag,
            routing,
            failed_reloads,
            rollbacks,
            rollback: None,
        })
    }

    fn arm(&mut self, previous: Arc<RoutingGeneration>, candidate: Arc<RoutingGeneration>) {
        self.rollback = Some((previous, candidate));
    }

    fn disarm(&mut self) {
        self.rollback = None;
    }
}

impl Drop for ReloadGuard<'_> {
    fn drop(&mut self) {
        if let Some((previous, failed)) = self.rollback.take() {
            failed.stop_accepting();
            previous.start_accepting();
            self.routing.activate(Arc::clone(&previous));
            self.routing.release_retiring(previous.id());
            if self.routing.retire(failed) {
                increment(self.rollbacks);
                increment(self.failed_reloads);
            }
        }
        self.routing.release_drained_retiring();
        self.flag.store(false, Ordering::Release);
    }
}

fn dynamic_router(routing: Arc<RoutingTable>) -> Router {
    Router::new()
        .fallback(any(dispatch_active_generation))
        .with_state(routing)
}

async fn dispatch_active_generation(
    State(routing): State<Arc<RoutingTable>>,
    request: Request,
) -> Response<Body> {
    loop {
        let generation = routing.active();
        if let Some(_permit) = generation.try_admit() {
            return generation
                .router()
                .clone()
                .oneshot(request)
                .await
                .unwrap_or_else(|never| match never {});
        }
        tokio::time::sleep(ROUTING_RETRY_DELAY).await;
    }
}

async fn probe_health(router: &Router, timeout: Duration) -> bool {
    let request = Request::get("/v1/health").body(Body::empty());
    let Ok(request) = request else {
        return false;
    };
    tokio::time::timeout(timeout, router.clone().oneshot(request))
        .await
        .ok()
        .and_then(Result::ok)
        .is_some_and(|response| response.status() == StatusCode::OK)
}

async fn wait_for_drain(generation: &RoutingGeneration, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if generation.inflight() == 0 {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(ROUTING_RETRY_DELAY).await;
    }
}

async fn serve_listener(
    listener: tokio::net::TcpListener,
    router: Router,
    shutdown: Arc<AtomicBool>,
) -> io::Result<()> {
    axum::serve(super::connection::with_read_timeout(listener), router)
        .with_graceful_shutdown(super::wait_for_shutdown(shutdown))
        .await
}

fn increment(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_add(1))
    });
}
