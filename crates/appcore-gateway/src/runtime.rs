// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/20 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/20 00:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Owned listener, task, health, and shutdown lifecycle for one Gateway.

use crate::{
    make_gateway_router, spawn_heartbeat_pruner, GatewayConfig, GatewayError, GatewayHaCoordinator,
    GatewayHaCoordinatorSnapshot, GatewayHaOwnershipSource, GatewayMetrics, GatewayResult,
    GatewayState, GatewayTelemetrySnapshot,
};
use appcore_peer_rpc::{BoundedReplayStore, PeerNonceStore, ReplayStoreConfig};
use appcore_security::HashTokenProvider;
use parking_lot::Mutex;
use std::future::IntoFuture;
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tokio::sync::watch;

const MAX_SHUTDOWN_JOIN_RESERVE: Duration = Duration::from_millis(100);

/// Concrete execution state of a Gateway runtime instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayRuntimeState {
    /// No listener or task is active.
    Stopped,
    /// Listener preparation is in progress.
    Starting,
    /// The listener and owned tasks are running.
    Running,
    /// Cooperative shutdown is in progress.
    Stopping,
    /// The runtime terminated with a controlled failure.
    Failed,
    /// The runtime thread failed to honor even the forced shutdown deadline.
    Orphaned,
}

/// Safe point-in-time Gateway lifecycle and metric snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayRuntimeSnapshot {
    /// Current execution state.
    pub state: GatewayRuntimeState,
    /// Deployment-configured listener address.
    pub configured_bind_address: SocketAddr,
    /// Actual bound address while or after an instance has run.
    pub bound_address: Option<SocketAddr>,
    /// Active authenticated worker sockets.
    pub active_workers: u64,
    /// Active authenticated client sockets.
    pub active_clients: u64,
    /// Successfully routed messages since this instance started.
    pub messages_routed: u64,
    /// Failed routing attempts since this instance started.
    pub routing_failures: u64,
    /// Detailed bounded vendor-neutral route telemetry.
    pub telemetry: GatewayTelemetrySnapshot,
    /// Opt-in shared-registry ownership state, when HA is configured.
    pub ha: Option<GatewayHaCoordinatorSnapshot>,
    /// Sanitized lifecycle failure, when present.
    pub last_error: Option<String>,
}

struct RunningGateway {
    shutdown: watch::Sender<Option<Duration>>,
    handle: JoinHandle<GatewayResult<()>>,
}

struct RuntimeInner {
    state: GatewayRuntimeState,
    running: Option<RunningGateway>,
    gateway_state: Option<Arc<GatewayState>>,
    bound_address: Option<SocketAddr>,
    last_error: Option<String>,
}

/// Restartable owner of one Gateway listener and all work spawned beneath it.
pub struct GatewayRuntime {
    config: GatewayConfig,
    token_provider: HashTokenProvider,
    connection_replay: Arc<dyn PeerNonceStore>,
    ha_coordinator: Option<Arc<GatewayHaCoordinator>>,
    inner: Mutex<RuntimeInner>,
}

impl GatewayRuntime {
    /// Creates a stopped runtime after validating owner-defined configuration.
    pub fn new(config: GatewayConfig, token_provider: HashTokenProvider) -> GatewayResult<Self> {
        Self::with_replay_store(
            config,
            token_provider,
            Arc::new(BoundedReplayStore::new(ReplayStoreConfig::default())),
        )
    }

    /// Creates a stopped runtime using an explicit durable or shared replay
    /// store for one-use Gateway connection credentials.
    pub fn with_replay_store(
        config: GatewayConfig,
        token_provider: HashTokenProvider,
        connection_replay: Arc<dyn PeerNonceStore>,
    ) -> GatewayResult<Self> {
        config.validate()?;
        Ok(Self {
            config,
            token_provider,
            connection_replay,
            ha_coordinator: None,
            inner: Mutex::new(RuntimeInner {
                state: GatewayRuntimeState::Stopped,
                running: None,
                gateway_state: None,
                bound_address: None,
                last_error: None,
            }),
        })
    }

    /// Creates a stopped HA runtime with an explicit shared replay store and
    /// shared-registry coordinator.
    pub fn with_ha_coordinator(
        config: GatewayConfig,
        token_provider: HashTokenProvider,
        connection_replay: Arc<dyn PeerNonceStore>,
        ha_coordinator: Arc<GatewayHaCoordinator>,
    ) -> GatewayResult<Self> {
        config.validate()?;
        Ok(Self {
            config,
            token_provider,
            connection_replay,
            ha_coordinator: Some(ha_coordinator),
            inner: Mutex::new(RuntimeInner {
                state: GatewayRuntimeState::Stopped,
                running: None,
                gateway_state: None,
                bound_address: None,
                last_error: None,
            }),
        })
    }

    /// Binds synchronously and starts the listener on one owned runtime thread.
    ///
    /// Bind, runtime construction, and thread creation failures are returned
    /// before this method reports success.
    pub fn start(&self) -> GatewayResult<()> {
        let mut inner = self.inner.lock();
        refresh_runtime(&mut inner);
        match inner.state {
            GatewayRuntimeState::Running => return Ok(()),
            GatewayRuntimeState::Starting | GatewayRuntimeState::Stopping => {
                return Err(GatewayError::Transport(
                    "gateway lifecycle transition already in progress".to_string(),
                ));
            }
            GatewayRuntimeState::Orphaned => {
                return Err(GatewayError::Transport(
                    "orphaned gateway instance cannot be restarted".to_string(),
                ));
            }
            GatewayRuntimeState::Stopped | GatewayRuntimeState::Failed => {}
        }
        inner.state = GatewayRuntimeState::Starting;
        match self.prepare_instance() {
            Ok(prepared) => {
                inner.bound_address = Some(prepared.bound_address);
                inner.gateway_state = Some(prepared.state);
                inner.running = Some(prepared.running);
                inner.last_error = None;
                inner.state = GatewayRuntimeState::Running;
                Ok(())
            }
            Err(error) => {
                inner.state = GatewayRuntimeState::Failed;
                inner.last_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    /// Requests graceful shutdown, force-cancels the server future before the
    /// deadline when needed, and joins all owned listener and task work.
    pub fn stop(&self, timeout: Duration) -> GatewayResult<()> {
        let running = {
            let mut inner = self.inner.lock();
            refresh_runtime(&mut inner);
            let Some(running) = inner.running.take() else {
                if inner.state != GatewayRuntimeState::Orphaned {
                    inner.state = GatewayRuntimeState::Stopped;
                }
                return Ok(());
            };
            inner.state = GatewayRuntimeState::Stopping;
            running
                .shutdown
                .send_replace(Some(graceful_shutdown_budget(timeout)));
            running
        };
        let deadline = Instant::now().checked_add(timeout);
        while !running.handle.is_finished()
            && deadline.is_none_or(|deadline| Instant::now() < deadline)
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        if !running.handle.is_finished() {
            let mut inner = self.inner.lock();
            inner.running = Some(running);
            inner.state = GatewayRuntimeState::Orphaned;
            inner.last_error = Some("gateway shutdown timed out".to_string());
            return Err(GatewayError::Transport(
                "gateway shutdown timed out".to_string(),
            ));
        }
        let result = join_runtime(running);
        let mut inner = self.inner.lock();
        inner.state = if result.is_ok() {
            GatewayRuntimeState::Stopped
        } else {
            GatewayRuntimeState::Failed
        };
        inner.last_error = result.as_ref().err().map(ToString::to_string);
        result
    }

    /// Returns lifecycle, listener, and bounded metric state without exposing
    /// credentials or token material.
    pub fn snapshot(&self) -> GatewayRuntimeSnapshot {
        let mut inner = self.inner.lock();
        refresh_runtime(&mut inner);
        let metrics = inner
            .gateway_state
            .as_ref()
            .map(|state| Arc::clone(&state.metrics));
        snapshot_from_parts(&self.config, &inner, metrics.as_deref())
    }

    fn prepare_instance(&self) -> GatewayResult<PreparedGateway> {
        let state = Arc::new(match &self.ha_coordinator {
            Some(coordinator) => GatewayState::with_ha_coordinator(
                self.config.clone(),
                self.token_provider.clone(),
                Arc::clone(&self.connection_replay),
                Arc::clone(coordinator),
            )?,
            None => GatewayState::with_replay_store(
                self.config.clone(),
                self.token_provider.clone(),
                Arc::clone(&self.connection_replay),
            )?,
        });
        let listener = bind_listener(self.config.bind_address)?;
        let bound_address = listener.local_addr().map_err(transport_error)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(transport_error)?;
        let listener = {
            let _entered = runtime.enter();
            tokio::net::TcpListener::from_std(listener).map_err(transport_error)?
        };
        let (shutdown, shutdown_request) = watch::channel(None);
        let thread_state = Arc::clone(&state);
        let handle = std::thread::Builder::new()
            .name("appcore-gateway".to_string())
            .spawn(move || run_gateway(runtime, listener, thread_state, shutdown_request))
            .map_err(transport_error)?;
        Ok(PreparedGateway {
            bound_address,
            state,
            running: RunningGateway { shutdown, handle },
        })
    }
}

struct PreparedGateway {
    bound_address: SocketAddr,
    state: Arc<GatewayState>,
    running: RunningGateway,
}

impl Drop for GatewayRuntime {
    fn drop(&mut self) {
        let _ = self.stop(Duration::from_secs(10));
    }
}

fn bind_listener(address: SocketAddr) -> GatewayResult<TcpListener> {
    let listener = TcpListener::bind(address).map_err(|error| {
        GatewayError::Transport(format!(
            "failed to bind gateway listener {address}: {error}"
        ))
    })?;
    listener.set_nonblocking(true).map_err(transport_error)?;
    Ok(listener)
}

fn run_gateway(
    runtime: tokio::runtime::Runtime,
    listener: tokio::net::TcpListener,
    state: Arc<GatewayState>,
    mut shutdown: watch::Receiver<Option<Duration>>,
) -> GatewayResult<()> {
    runtime.block_on(async move {
        let coordinator = state.ha_coordinator().map(|coordinator| {
            let source: Arc<dyn GatewayHaOwnershipSource> = state.clone();
            tokio::spawn(coordinator.run(source, state.subscribe_shutdown()))
        });
        let pruner = spawn_heartbeat_pruner(
            Arc::clone(&state),
            state.config().heartbeat_interval,
            state.config().heartbeat_timeout,
        );
        let router = make_gateway_router(Arc::clone(&state));
        let graceful_state = Arc::clone(&state);
        let mut server = Box::pin(
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    graceful_state.wait_for_shutdown().await;
                })
                .into_future(),
        );
        let exit = tokio::select! {
            result = server.as_mut() => GatewayServerExit::Completed(result),
            grace = wait_for_shutdown_request(&mut shutdown) => {
                state.request_shutdown();
                match tokio::time::timeout(grace, server.as_mut()).await {
                    Ok(result) => GatewayServerExit::Completed(result),
                    Err(_) => GatewayServerExit::Forced,
                }
            }
        };
        drop(server);
        state.request_shutdown();
        let pruner_result = pruner.await.map_err(|error| {
            GatewayError::Transport(format!("gateway heartbeat pruner failed: {error}"))
        });
        let coordinator_result = match coordinator {
            Some(coordinator) => coordinator.await.map_err(|error| {
                GatewayError::Transport(format!("gateway HA coordinator failed: {error}"))
            }),
            None => Ok(()),
        };
        let result = match exit {
            GatewayServerExit::Completed(result) => result.map_err(transport_error),
            GatewayServerExit::Forced => Err(GatewayError::Transport(
                "gateway graceful shutdown timed out; forced cancellation completed".to_string(),
            )),
        };
        result.and(pruner_result).and(coordinator_result)
    })
}

enum GatewayServerExit {
    Completed(std::io::Result<()>),
    Forced,
}

async fn wait_for_shutdown_request(shutdown: &mut watch::Receiver<Option<Duration>>) -> Duration {
    loop {
        if let Some(grace) = *shutdown.borrow() {
            return grace;
        }
        if shutdown.changed().await.is_err() {
            return Duration::ZERO;
        }
    }
}

fn graceful_shutdown_budget(timeout: Duration) -> Duration {
    timeout.saturating_sub(timeout.min(MAX_SHUTDOWN_JOIN_RESERVE))
}

fn refresh_runtime(inner: &mut RuntimeInner) {
    let finished = inner
        .running
        .as_ref()
        .is_some_and(|running| running.handle.is_finished());
    if !finished {
        return;
    }
    let Some(running) = inner.running.take() else {
        return;
    };
    let requested = running.shutdown.borrow().is_some();
    let result = join_runtime(running);
    inner.state = if requested && result.is_ok() {
        GatewayRuntimeState::Stopped
    } else {
        GatewayRuntimeState::Failed
    };
    inner.last_error = result.err().map(|error| error.to_string());
}

fn join_runtime(running: RunningGateway) -> GatewayResult<()> {
    running
        .handle
        .join()
        .map_err(|_| GatewayError::Transport("gateway runtime thread panicked".to_string()))?
}

fn snapshot_from_parts(
    config: &GatewayConfig,
    inner: &RuntimeInner,
    metrics: Option<&GatewayMetrics>,
) -> GatewayRuntimeSnapshot {
    GatewayRuntimeSnapshot {
        state: inner.state,
        configured_bind_address: config.bind_address,
        bound_address: inner.bound_address,
        active_workers: metrics.map_or(0, GatewayMetrics::active_workers),
        active_clients: metrics.map_or(0, GatewayMetrics::active_clients),
        messages_routed: metrics.map_or(0, GatewayMetrics::messages_routed),
        routing_failures: metrics.map_or(0, GatewayMetrics::routing_failures),
        telemetry: metrics.map_or_else(
            GatewayTelemetrySnapshot::default,
            GatewayMetrics::telemetry_snapshot,
        ),
        ha: inner
            .gateway_state
            .as_ref()
            .and_then(|state| state.ha_coordinator())
            .map(|coordinator| coordinator.snapshot()),
        last_error: inner.last_error.clone(),
    }
}

fn transport_error(error: impl std::fmt::Display) -> GatewayError {
    GatewayError::Transport(error.to_string())
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
