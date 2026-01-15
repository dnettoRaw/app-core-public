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
use tokio::sync::mpsc::Sender;

/// Maximum number of outbound WebSocket frames buffered per connection.
pub const CONNECTION_BUFFER_CAPACITY: usize = 128;

// appcore-norm: allow(global-state) reason: atomic generation distinguishes replaced gateway connections
static CONNECTION_GENERATION: AtomicU64 = AtomicU64::new(1);

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

    /// Sends a WebSocket message to the worker.
    pub fn send(&self, message: Message) -> Result<(), crate::error::GatewayError> {
        self.sender.try_send(message).map_err(|_| {
            crate::error::GatewayError::Transport("worker connection closed".to_string())
        })
    }
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
