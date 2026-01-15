// =============================================================================
//        #######
//     ###       ###     F: socket.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 12:48:56 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:48:56 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Bounded Gateway worker and client WebSocket loops.

use crate::config::{MAX_GATEWAY_CONNECTIONS, MAX_GATEWAY_MESSAGE_BYTES, MAX_GATEWAY_TENANTS};
use crate::connection::{
    ClientConnection, WorkerConnection, WorkerConnectionKey, CONNECTION_BUFFER_CAPACITY,
};
use crate::{EnvelopeRouter, GatewaySession, GatewayState, MeshPeerResponse, TenantState};
use appcore_contracts::InstallationId;
use appcore_distributed_contracts::{PeerRpcEnvelope, PeerRpcResponse};
use appcore_security::RuntimeTokenClaims;
use appcore_types::{CapabilityName, ClusterId, CoreId, TenantId};
use axum::extract::ws::{Message, WebSocket};
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, Semaphore};
use tokio::task::{JoinHandle, JoinSet};
use tracing::{info, warn};

const MAX_IN_FLIGHT_CLIENT_REQUESTS: usize = 4_096;
// appcore-norm: allow(global-state) reason: process-wide semaphore enforces the configured request limit
static REQUEST_SLOTS: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(MAX_IN_FLIGHT_CLIENT_REQUESTS)));

pub(crate) struct WorkerSocketContext {
    pub(crate) tenant_id: TenantId,
    pub(crate) cluster_id: ClusterId,
    pub(crate) installation_id: InstallationId,
    pub(crate) core_id: CoreId,
    pub(crate) capabilities: Vec<CapabilityName>,
    pub(crate) expires_at_ms: u64,
}

pub(crate) async fn handle_worker_socket(
    state: Arc<GatewayState>,
    context: WorkerSocketContext,
    socket: WebSocket,
) {
    let WorkerSocketContext {
        tenant_id,
        cluster_id,
        installation_id,
        core_id,
        capabilities,
        expires_at_ms,
    } = context;
    let (sink, mut stream) = socket.split();
    let (tx, rx) = mpsc::channel::<Message>(CONNECTION_BUFFER_CAPACITY);
    let key = WorkerConnectionKey {
        tenant_id: tenant_id.clone(),
        installation_id: installation_id.clone(),
        core_id: core_id.clone(),
    };
    let conn = WorkerConnection::new_in_cluster(key, cluster_id, tx, now_ms());
    let replaced = {
        let mut tenants = state.tenants.write();
        if !tenants.contains_key(&tenant_id) && tenants.len() >= MAX_GATEWAY_TENANTS {
            warn!("Gateway tenant limit rejected worker connection");
            return;
        }
        if connection_count(&tenants) >= MAX_GATEWAY_CONNECTIONS {
            warn!("Gateway global connection limit rejected worker connection");
            return;
        }
        let tenant = tenants
            .entry(tenant_id.clone())
            .or_insert_with(|| TenantState::new(tenant_id.clone()));
        let replaced = tenant.get_worker(&installation_id, &core_id).is_some();
        if tenant.add_worker(conn.clone(), capabilities).is_err() {
            warn!("Gateway worker limit rejected connection");
            return;
        }
        replaced
    };
    if !replaced {
        state.metrics.worker_connected();
    }
    info!(
        "Worker connected: tenant={}, installation={}, core={}",
        tenant_id.as_str(),
        installation_id.as_str(),
        core_id.as_str()
    );

    let writer_task = spawn_socket_writer(&state, sink, rx);
    let mut socket_shutdown = state.subscribe_shutdown();
    loop {
        let message = tokio::select! {
            biased;
            result = socket_shutdown.changed() => {
                if result.is_err() || *socket_shutdown.borrow() {
                    break;
                }
                continue;
            }
            result = tokio::time::timeout(
                session_wait(state.config().heartbeat_timeout, expires_at_ms),
                stream.next(),
            ) => match result {
                Ok(Some(Ok(message))) => message,
                _ => break,
            }
        };
        if !handle_worker_message(&state, &tenant_id, &conn, message) {
            break;
        }
    }
    let removed = {
        let mut tenants = state.tenants.write();
        tenants.get_mut(&tenant_id).is_some_and(|tenant| {
            tenant.remove_worker_if_current(&installation_id, &core_id, conn.generation())
        })
    };
    writer_task.abort();
    let _ = writer_task.await;
    if removed {
        state.metrics.worker_disconnected();
    }
    info!(
        "Worker disconnected: tenant={}, installation={}, core={}",
        tenant_id.as_str(),
        installation_id.as_str(),
        core_id.as_str()
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_client_socket(
    state: Arc<GatewayState>,
    tenant_id: TenantId,
    cluster_id: ClusterId,
    claims: RuntimeTokenClaims,
    socket: WebSocket,
) {
    let session_id = unique_id("sess");
    let connection_id = unique_id("conn");
    let (sink, mut stream) = socket.split();
    let (tx, rx) = mpsc::channel::<Message>(CONNECTION_BUFFER_CAPACITY);
    let connection = ClientConnection::new(
        connection_id.clone(),
        tenant_id.clone(),
        session_id.clone(),
        tx,
    );
    let boundary = ClientBoundary {
        cluster_id,
        expires_at_ms: claims.expires_at_ms,
    };
    let session = GatewaySession::new(
        session_id.clone(),
        tenant_id.clone(),
        now_ms(),
        claims.expires_at_ms,
        claims.subject,
    );
    {
        let mut tenants = state.tenants.write();
        if !tenants.contains_key(&tenant_id) && tenants.len() >= MAX_GATEWAY_TENANTS {
            warn!("Gateway tenant limit rejected client connection");
            return;
        }
        if connection_count(&tenants) >= MAX_GATEWAY_CONNECTIONS {
            warn!("Gateway global connection limit rejected client connection");
            return;
        }
        let tenant = tenants
            .entry(tenant_id.clone())
            .or_insert_with(|| TenantState::new(tenant_id.clone()));
        if tenant.try_add_client(connection.clone()).is_err() {
            warn!("Gateway client limit rejected connection");
            return;
        }
        tenant.sessions.insert(session_id.clone(), session);
    }
    state.metrics.client_connected();
    info!(
        "Client connected: tenant={}, connection_id={}",
        tenant_id.as_str(),
        connection_id
    );

    let writer_task = spawn_socket_writer(&state, sink, rx);
    let mut request_tasks = JoinSet::new();
    let mut socket_shutdown = state.subscribe_shutdown();
    loop {
        let message = tokio::select! {
            biased;
            result = socket_shutdown.changed() => {
                if result.is_err() || *socket_shutdown.borrow() {
                    break;
                }
                continue;
            }
            result = tokio::time::timeout(
                session_wait(state.config().heartbeat_timeout, boundary.expires_at_ms),
                stream.next(),
            ) => match result {
                Ok(Some(Ok(message))) => message,
                _ => break,
            }
        };
        while request_tasks.try_join_next().is_some() {}
        if !handle_client_message(&state, &connection, &boundary, message, &mut request_tasks) {
            break;
        }
    }
    {
        let mut tenants = state.tenants.write();
        if let Some(tenant) = tenants.get_mut(&tenant_id) {
            tenant.remove_client(&connection_id);
            tenant.sessions.remove(&session_id);
        }
    }
    request_tasks.abort_all();
    while request_tasks.join_next().await.is_some() {}
    writer_task.abort();
    let _ = writer_task.await;
    state.metrics.client_disconnected();
    info!(
        "Client disconnected: tenant={}, connection_id={}",
        tenant_id.as_str(),
        connection_id
    );
}

fn spawn_socket_writer(
    state: &GatewayState,
    mut sink: SplitSink<WebSocket, Message>,
    mut receiver: mpsc::Receiver<Message>,
) -> JoinHandle<()> {
    let mut shutdown = state.subscribe_shutdown();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;
                result = shutdown.changed() => {
                    if result.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                message = receiver.recv() => {
                    let Some(message) = message else {
                        break;
                    };
                    if sink.send(message).await.is_err() {
                        break;
                    }
                }
            }
        }
    })
}

fn handle_worker_message(
    state: &Arc<GatewayState>,
    tenant_id: &TenantId,
    connection: &WorkerConnection,
    message: Message,
) -> bool {
    match message {
        Message::Text(text) if text.len() <= MAX_GATEWAY_MESSAGE_BYTES => {
            if is_heartbeat(&text) {
                connection.update_heartbeat(now_ms());
                return true;
            }
            if let Ok(response) = serde_json::from_str::<MeshPeerResponse>(&text) {
                connection.update_heartbeat(now_ms());
                return EnvelopeRouter::handle_worker_mesh_response_from(
                    Arc::clone(state),
                    tenant_id,
                    connection,
                    response,
                )
                .is_ok();
            }
            if let Ok(response) = serde_json::from_str::<PeerRpcResponse>(&text) {
                connection.update_heartbeat(now_ms());
                return EnvelopeRouter::handle_worker_response_from(
                    Arc::clone(state),
                    tenant_id,
                    connection,
                    response,
                )
                .is_ok();
            }
            false
        }
        Message::Ping(payload) => {
            connection.update_heartbeat(now_ms());
            connection.send(Message::Pong(payload)).is_ok()
        }
        Message::Pong(_) => {
            connection.update_heartbeat(now_ms());
            true
        }
        Message::Close(_) => false,
        _ => false,
    }
}

fn handle_client_message(
    state: &Arc<GatewayState>,
    connection: &ClientConnection,
    boundary: &ClientBoundary,
    message: Message,
    request_tasks: &mut JoinSet<()>,
) -> bool {
    if now_ms() >= boundary.expires_at_ms {
        return false;
    }
    let text = match message {
        Message::Text(text) => text,
        Message::Ping(payload) => return connection.send(Message::Pong(payload)).is_ok(),
        Message::Pong(_) => return true,
        Message::Close(_) => return false,
        _ => return false,
    };
    if text.len() > MAX_GATEWAY_MESSAGE_BYTES {
        return false;
    }
    let Ok(envelope) = serde_json::from_str::<PeerRpcEnvelope>(&text) else {
        return false;
    };
    if envelope.tenant_id != connection.tenant_id || envelope.cluster_id != boundary.cluster_id {
        send_rejection(
            connection,
            envelope.request_id,
            "session_boundary_violation",
        );
        return true;
    }
    let Ok(permit) = Arc::clone(&REQUEST_SLOTS).try_acquire_owned() else {
        send_rejection(connection, envelope.request_id, "gateway_overloaded");
        return true;
    };
    let state = Arc::clone(state);
    let connection = connection.clone();
    request_tasks.spawn(async move {
        let _permit = permit;
        let response =
            EnvelopeRouter::route_request(state, envelope, Duration::from_secs(10)).await;
        if let Ok(json) = serde_json::to_string(&response) {
            let _ = connection.send(Message::Text(json.into()));
        }
    });
    true
}

struct ClientBoundary {
    cluster_id: ClusterId,
    expires_at_ms: u64,
}

fn send_rejection(connection: &ClientConnection, request_id: String, reason: &'static str) {
    let response = PeerRpcResponse::rejected(request_id, reason);
    if let Ok(json) = serde_json::to_string(&response) {
        let _ = connection.send(Message::Text(json.into()));
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HeartbeatFrame {
    #[serde(rename = "type")]
    kind: String,
}

fn is_heartbeat(text: &str) -> bool {
    serde_json::from_str::<HeartbeatFrame>(text)
        .map(|heartbeat| heartbeat.kind == "heartbeat")
        .unwrap_or(false)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn unique_id(prefix: &str) -> String {
    // appcore-norm: allow(global-state) reason: atomic sequence prevents process-local temporary path collisions
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}-{}-{}",
        prefix,
        std::process::id(),
        now_ms(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

fn session_wait(idle_timeout: Duration, expires_at_ms: u64) -> Duration {
    let remaining = Duration::from_millis(expires_at_ms.saturating_sub(now_ms()).max(1));
    idle_timeout.min(remaining)
}

fn connection_count(tenants: &std::collections::HashMap<TenantId, TenantState>) -> usize {
    tenants.values().fold(0usize, |total, tenant| {
        total
            .saturating_add(tenant.workers.len())
            .saturating_add(tenant.clients.len())
    })
}

#[cfg(test)]
mod tests {
    use super::{handle_client_message, is_heartbeat, ClientBoundary};
    use crate::{ClientConnection, GatewayConfig, GatewayState};
    use appcore_security::HashTokenProvider;
    use appcore_types::{ClusterId, TenantId};
    use axum::extract::ws::Message;
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio::task::JoinSet;

    #[test]
    fn heartbeat_requires_the_exact_schema() {
        assert!(is_heartbeat(r#"{"type":"heartbeat"}"#));
        assert!(!is_heartbeat("heartbeat"));
        assert!(!is_heartbeat(r#"{"type":"not-heartbeat"}"#));
        assert!(!is_heartbeat(
            r#"{"type":"heartbeat","credential":"secret"}"#
        ));
    }

    #[tokio::test]
    async fn client_ping_receives_pong() {
        let provider = HashTokenProvider::from_secret(vec![9; 32]).unwrap();
        let state = Arc::new(
            GatewayState::new(
                GatewayConfig::new(([127, 0, 0, 1], 8080).into(), "gateway.test"),
                provider,
            )
            .unwrap(),
        );
        let tenant = TenantId::new("tenant-a").unwrap();
        let cluster = ClusterId::new("cluster-a").unwrap();
        let (sender, mut receiver) = mpsc::channel(1);
        let connection = ClientConnection::new(
            "connection-a".to_string(),
            tenant,
            "session-a".to_string(),
            sender,
        );
        let boundary = ClientBoundary {
            cluster_id: cluster,
            expires_at_ms: u64::MAX,
        };
        let mut request_tasks = JoinSet::new();

        assert!(handle_client_message(
            &state,
            &connection,
            &boundary,
            Message::Ping(vec![1, 2, 3].into()),
            &mut request_tasks,
        ));
        assert_eq!(
            receiver.recv().await,
            Some(Message::Pong(vec![1, 2, 3].into()))
        );
    }
}
