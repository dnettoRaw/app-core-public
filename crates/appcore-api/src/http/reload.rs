//! Atomic HTTP routing-generation reload with bounded drain and rollback.

use super::RuntimeHttpHost;
use arc_swap::ArcSwap;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{Response, StatusCode};
use axum::routing::any;
use axum::Router;
use std::fmt;
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
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
    /// Generation identifiers must increase monotonically.
    StaleGeneration,
    /// Another reload transaction already owns the coordinator.
    ReloadInProgress,
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
            Self::StaleGeneration => formatter.write_str("HTTP routing generation must increase"),
            Self::ReloadInProgress => formatter.write_str("HTTP reload is already in progress"),
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

struct RoutingGeneration {
    id: u64,
    router: Router,
    accepting: AtomicBool,
    inflight: AtomicUsize,
}

impl RoutingGeneration {
    fn new(id: u64, router: Router) -> Self {
        Self {
            id,
            router,
            accepting: AtomicBool::new(true),
            inflight: AtomicUsize::new(0),
        }
    }

    fn try_admit(self: &Arc<Self>) -> Option<RoutingPermit> {
        if !self.accepting.load(Ordering::Acquire) {
            return None;
        }
        self.inflight
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current.checked_add(1)
            })
            .ok()?;
        if self.accepting.load(Ordering::Acquire) {
            return Some(RoutingPermit {
                generation: Arc::clone(self),
            });
        }
        self.inflight.fetch_sub(1, Ordering::AcqRel);
        None
    }
}

struct RoutingPermit {
    generation: Arc<RoutingGeneration>,
}

impl Drop for RoutingPermit {
    fn drop(&mut self) {
        self.generation.inflight.fetch_sub(1, Ordering::AcqRel);
    }
}

struct RoutingTable {
    active: ArcSwap<RoutingGeneration>,
}

/// Prepared, health-gated candidate that can be consumed by one reload.
pub struct PreparedRuntimeHttpGeneration {
    generation: Arc<RoutingGeneration>,
}

impl fmt::Debug for PreparedRuntimeHttpGeneration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedRuntimeHttpGeneration")
            .field("generation", &self.generation.id)
            .finish_non_exhaustive()
    }
}

/// HTTP host whose stable listener dispatches through one atomic generation.
pub struct ReloadableRuntimeHttpHost {
    config: super::HttpApiConfig,
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
        let initial = Arc::new(RoutingGeneration::new(initial_generation, host.router()));
        Ok(Self {
            config,
            routing: Arc::new(RoutingTable {
                active: ArcSwap::from(initial),
            }),
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
        let active = self.routing.active.load();
        if generation <= active.id {
            return Err(RuntimeHttpReloadError::StaleGeneration);
        }
        if !host.config().enabled {
            return Err(RuntimeHttpReloadError::ListenerDisabled);
        }
        if host.config().host != self.config.host || host.config().port != self.config.port {
            return Err(RuntimeHttpReloadError::ListenerAddressChanged);
        }
        Ok(PreparedRuntimeHttpGeneration {
            generation: Arc::new(RoutingGeneration::new(generation, host.router())),
        })
    }

    #[cfg(test)]
    pub(super) fn new_for_test(
        initial_generation: u64,
        config: super::HttpApiConfig,
        router: Router,
    ) -> Self {
        let initial = Arc::new(RoutingGeneration::new(initial_generation, router));
        Self {
            config,
            routing: Arc::new(RoutingTable {
                active: ArcSwap::from(initial),
            }),
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
            generation: Arc::new(RoutingGeneration::new(generation, router)),
        }
    }

    /// Returns a router that always dispatches through the active generation.
    pub fn router(&self) -> Router {
        dynamic_router(Arc::clone(&self.routing))
    }

    /// Runs the stable listener until cooperative shutdown is requested.
    pub fn run_until_shutdown(&self, shutdown: Arc<AtomicBool>) -> io::Result<()> {
        let address = format!("{}:{}", self.config.host, self.config.port);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(io::Error::other)?;
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::bind(address).await?;
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
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(io::Error::other)?;
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener)?;
            serve_listener(listener, router, shutdown).await
        })
    }

    /// Health-checks, atomically activates, and drains one prepared generation.
    pub async fn reload(
        &self,
        prepared: PreparedRuntimeHttpGeneration,
        policy: HttpReloadPolicy,
    ) -> Result<(), RuntimeHttpReloadError> {
        let _guard = match ReloadGuard::acquire(&self.reload_in_progress) {
            Ok(guard) => guard,
            Err(error) => return self.fail(error),
        };
        let previous = self.routing.active.load_full();
        if prepared.generation.id <= previous.id {
            return self.fail(RuntimeHttpReloadError::StaleGeneration);
        }
        if !probe_health(&prepared.generation.router, policy.health_timeout).await {
            return self.fail(RuntimeHttpReloadError::HealthGateFailed(
                HttpReloadPhase::Prepare,
            ));
        }

        previous.accepting.store(false, Ordering::Release);
        self.routing.active.store(Arc::clone(&prepared.generation));
        if !probe_health(&prepared.generation.router, policy.health_timeout).await {
            return self
                .rollback(previous, prepared.generation, policy)
                .await
                .and(Err(RuntimeHttpReloadError::HealthGateFailed(
                    HttpReloadPhase::Switch,
                )));
        }
        if !wait_for_drain(&previous, policy.drain_timeout).await {
            return self
                .rollback(previous, prepared.generation, policy)
                .await
                .and(Err(RuntimeHttpReloadError::DrainTimedOut));
        }
        increment(&self.successful_reloads);
        Ok(())
    }

    /// Returns the active generation and bounded operational counters.
    pub fn snapshot(&self) -> HttpReloadSnapshot {
        let active = self.routing.active.load();
        HttpReloadSnapshot {
            active_generation: active.id,
            active_inflight: active.inflight.load(Ordering::Acquire),
            reload_in_progress: self.reload_in_progress.load(Ordering::Acquire),
            successful_reloads: self.successful_reloads.load(Ordering::Relaxed),
            failed_reloads: self.failed_reloads.load(Ordering::Relaxed),
            rollbacks: self.rollbacks.load(Ordering::Relaxed),
        }
    }

    async fn rollback(
        &self,
        previous: Arc<RoutingGeneration>,
        failed: Arc<RoutingGeneration>,
        policy: HttpReloadPolicy,
    ) -> Result<(), RuntimeHttpReloadError> {
        failed.accepting.store(false, Ordering::Release);
        previous.accepting.store(true, Ordering::Release);
        self.routing.active.store(previous);
        increment(&self.rollbacks);
        increment(&self.failed_reloads);
        if wait_for_drain(&failed, policy.drain_timeout).await {
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
}

impl<'a> ReloadGuard<'a> {
    fn acquire(flag: &'a AtomicBool) -> Result<Self, RuntimeHttpReloadError> {
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| RuntimeHttpReloadError::ReloadInProgress)?;
        Ok(Self { flag })
    }
}

impl Drop for ReloadGuard<'_> {
    fn drop(&mut self) {
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
        let generation = routing.active.load_full();
        if let Some(_permit) = generation.try_admit() {
            return generation
                .router
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
        if generation.inflight.load(Ordering::Acquire) == 0 {
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
    axum::serve(listener, router)
        .with_graceful_shutdown(super::wait_for_shutdown(shutdown))
        .await
}

fn increment(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_add(1))
    });
}
