// =============================================================================
//        #######
//     ###       ###     F: connection.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/26 08:53:09 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:48:56 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Connection descriptors for workers and clients.

use appcore_contracts::InstallationId;
use appcore_types::{ClusterId, CoreId, TenantId};
use axum::extract::ws::Message;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::Sender;

/// Maximum number of outbound WebSocket frames buffered per connection.
pub const CONNECTION_BUFFER_CAPACITY: usize = 128;

// appcore-norm: allow(global-state) reason: atomic generation distinguishes replaced gateway connections
static CONNECTION_GENERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkerSendFailure {
    Saturated,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkerAdmissionFailure {
    Closed,
    AtCapacity,
}

#[derive(Debug)]
pub(crate) struct WorkerRoutePermit {
    inflight: Arc<AtomicU64>,
    admitted: u64,
}

impl WorkerRoutePermit {
    pub(crate) fn admitted(&self) -> u64 {
        self.admitted
    }
}

impl Drop for WorkerRoutePermit {
    fn drop(&mut self) {
        saturating_decrement(&self.inflight);
    }
}

/// Unique identifier for a worker connection.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkerConnectionKey {
    /// Tenant boundary.
    pub tenant_id: TenantId,
    /// Specific installation target.
    pub installation_id: InstallationId,
    /// Specific core/worker identity.
    pub core_id: CoreId,
}

/// Models an active, authenticated worker WebSocket connection.
#[derive(Debug, Clone)]
pub struct WorkerConnection {
    /// Authentication metadata.
    pub key: WorkerConnectionKey,
    /// Channel sender to push frames onto the worker's writer loop.
    pub sender: Sender<Message>,
    cluster_id: Option<ClusterId>,
    generation: u64,
    /// Last recorded heartbeat epoch in milliseconds.
    last_heartbeat_ms: Arc<AtomicU64>,
    inflight: Arc<AtomicU64>,
}

impl WorkerConnection {
    /// Creates a new worker connection handle.
    pub fn new(key: WorkerConnectionKey, sender: Sender<Message>, now_ms: u64) -> Self {
        Self::new_inner(key, None, sender, now_ms)
    }

    /// Creates a worker connection bound to an explicit cluster.
    pub fn new_in_cluster(
        key: WorkerConnectionKey,
        cluster_id: ClusterId,
        sender: Sender<Message>,
        now_ms: u64,
    ) -> Self {
        Self::new_inner(key, Some(cluster_id), sender, now_ms)
    }

    fn new_inner(
        key: WorkerConnectionKey,
        cluster_id: Option<ClusterId>,
        sender: Sender<Message>,
        now_ms: u64,
    ) -> Self {
        Self {
            key,
            sender,
            cluster_id,
            generation: CONNECTION_GENERATION.fetch_add(1, Ordering::Relaxed),
            last_heartbeat_ms: Arc::new(AtomicU64::new(now_ms)),
            inflight: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Returns the cluster bound during authenticated connection setup.
    pub fn cluster_id(&self) -> Option<&ClusterId> {
        self.cluster_id.as_ref()
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// Updates the heartbeat timestamp.
    pub fn update_heartbeat(&self, now_ms: u64) {
        self.last_heartbeat_ms.store(now_ms, Ordering::SeqCst);
    }

    /// Gets the last heartbeat timestamp.
    pub fn last_heartbeat(&self) -> u64 {
        self.last_heartbeat_ms.load(Ordering::SeqCst)
    }

    /// Returns requests currently admitted to this worker connection.
    pub fn inflight(&self) -> u64 {
        self.inflight.load(Ordering::Acquire)
    }

    pub(crate) fn is_open_and_healthy(&self, now_ms: u64, timeout: Duration) -> bool {
        let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
        !self.sender.is_closed()
            && timeout_ms > 0
            && now_ms.saturating_sub(self.last_heartbeat()) <= timeout_ms
    }

    pub(crate) fn outbound_queue_depth(&self) -> usize {
        self.sender
            .max_capacity()
            .saturating_sub(self.sender.capacity())
    }

    pub(crate) fn outbound_queue_remaining(&self) -> usize {
        self.sender.capacity()
    }

    pub(crate) fn try_admit_route(
        &self,
        max_inflight: u64,
    ) -> Result<WorkerRoutePermit, WorkerAdmissionFailure> {
        if self.sender.is_closed() {
            return Err(WorkerAdmissionFailure::Closed);
        }
        let admitted = self
            .inflight
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                (current < max_inflight).then_some(current.saturating_add(1))
            })
            .map(|previous| previous.saturating_add(1))
            .map_err(|_| WorkerAdmissionFailure::AtCapacity)?;
        Ok(WorkerRoutePermit {
            inflight: Arc::clone(&self.inflight),
            admitted,
        })
    }

    /// Sends a WebSocket message to the worker.
    pub fn send(&self, message: Message) -> Result<(), crate::error::GatewayError> {
        self.send_routed(message).map_err(|failure| {
            let reason = match failure {
                WorkerSendFailure::Saturated => "worker connection queue saturated",
                WorkerSendFailure::Closed => "worker connection closed",
            };
            crate::error::GatewayError::Transport(reason.to_string())
        })
    }

    pub(crate) fn send_routed(&self, message: Message) -> Result<(), WorkerSendFailure> {
        self.sender.try_send(message).map_err(|error| match error {
            TrySendError::Full(_) => WorkerSendFailure::Saturated,
            TrySendError::Closed(_) => WorkerSendFailure::Closed,
        })
    }
}

fn saturating_decrement(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
        Some(value.saturating_sub(1))
    });
}

/// Models an active client WebSocket connection.
#[derive(Debug, Clone)]
pub struct ClientConnection {
    /// Unique identifier for this connection instance.
    pub connection_id: String,
    /// Tenant boundary.
    pub tenant_id: TenantId,
    /// Session identifier if authenticated.
    pub session_id: String,
    /// Channel sender to push frames onto the client's writer loop.
    pub sender: Sender<Message>,
}

impl ClientConnection {
    /// Creates a new client connection handle.
    pub fn new(
        connection_id: String,
        tenant_id: TenantId,
        session_id: String,
        sender: Sender<Message>,
    ) -> Self {
        Self {
            connection_id,
            tenant_id,
            session_id,
            sender,
        }
    }

    /// Sends a WebSocket message to the client.
    pub fn send(&self, message: Message) -> Result<(), crate::error::GatewayError> {
        self.sender.try_send(message).map_err(|_| {
            crate::error::GatewayError::Transport("client connection closed".to_string())
        })
    }
}
