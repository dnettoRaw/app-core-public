// =============================================================================
//        #######
//     ###       ###     F: socket_lifecycle.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 23:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 23:00:00 by dnettoRaw
//      ###########      S: 1.0.6-rc
// =============================================================================

//! Activates and releases bounded Gateway socket ownership.

use crate::connection::{ClientConnection, WorkerConnection, WorkerConnectionKey};
use crate::socket::{now_ms, ClientBoundary, WorkerSocketContext};
use crate::socket_ownership::{
    register_client_ownership, register_worker_ownership, remove_client_ownership,
    remove_worker_ownership,
};
use crate::{GatewaySession, GatewayState};
use appcore_contracts::InstallationId;
use appcore_security::RuntimeTokenClaims;
use appcore_types::{ClusterId, CoreId, TenantId};
use axum::extract::ws::Message;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::{JoinHandle, JoinSet};
use tracing::warn;

pub(super) struct ActiveWorkerSocket {
    pub(super) tenant_id: TenantId,
    pub(super) installation_id: InstallationId,
    pub(super) core_id: CoreId,
    pub(super) connection: WorkerConnection,
    pub(super) expires_at_ms: u64,
}

pub(super) async fn activate_worker_socket(
    state: &Arc<GatewayState>,
    context: WorkerSocketContext,
    sender: mpsc::Sender<Message>,
) -> Option<ActiveWorkerSocket> {
    if state
        .admit_ha_boundary(&context.tenant_id, &context.cluster_id)
        .is_err()
    {
        warn!("Gateway HA admission rejected worker connection");
        return None;
    }
    let key = WorkerConnectionKey {
        tenant_id: context.tenant_id.clone(),
        installation_id: context.installation_id.clone(),
        core_id: context.core_id.clone(),
    };
    let connection =
        WorkerConnection::new_in_cluster(key, context.cluster_id.clone(), sender, now_ms());
    let replaced = match register_worker_ownership(
        state,
        &context.tenant_id,
        &context.cluster_id,
        &context.installation_id,
        &context.core_id,
        &connection,
        context.capabilities,
    )
    .await
    {
        Ok(replaced) => replaced,
        Err(reason) => {
            warn!(reason, "Gateway rejected worker connection");
            return None;
        }
    };
    if replaced {
        state.metrics.worker_reconnected();
    } else {
        state.metrics.worker_connected();
    }
    Some(ActiveWorkerSocket {
        tenant_id: context.tenant_id,
        installation_id: context.installation_id,
        core_id: context.core_id,
        connection,
        expires_at_ms: context.expires_at_ms,
    })
}

pub(super) async fn deactivate_worker_socket(
    state: &Arc<GatewayState>,
    active: &ActiveWorkerSocket,
    writer_task: JoinHandle<()>,
) {
    let removed = state
        .tenant_partition(&active.tenant_id)
        .is_some_and(|tenant| {
            tenant.write().remove_worker_if_current(
                &active.installation_id,
                &active.core_id,
                active.connection.generation(),
            )
        });
    writer_task.abort();
    let _ = writer_task.await;
    if removed {
        remove_worker_ownership(
            state,
            &active.tenant_id,
            &active.installation_id,
            &active.core_id,
            &active.connection,
        )
        .await;
        state.metrics.worker_disconnected();
    }
}

pub(super) struct ActiveClientSocket {
    pub(super) tenant_id: TenantId,
    pub(super) session_id: String,
    pub(super) connection_id: String,
    pub(super) connection: ClientConnection,
    pub(super) boundary: ClientBoundary,
}

pub(super) async fn activate_client_socket(
    state: &Arc<GatewayState>,
    tenant_id: TenantId,
    cluster_id: ClusterId,
    claims: RuntimeTokenClaims,
    sender: mpsc::Sender<Message>,
) -> Option<ActiveClientSocket> {
    if state.admit_ha_boundary(&tenant_id, &cluster_id).is_err() {
        warn!("Gateway HA admission rejected client connection");
        return None;
    }
    let session_id = super::socket::unique_id("sess");
    let connection_id = super::socket::unique_id("conn");
    let connection = ClientConnection::new(
        connection_id.clone(),
        tenant_id.clone(),
        session_id.clone(),
        sender,
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
    if let Err(reason) = register_client_ownership(
        state,
        &tenant_id,
        &boundary.cluster_id,
        &connection,
        session,
    )
    .await
    {
        warn!(reason, "Gateway rejected client connection");
        return None;
    }
    state.metrics.client_connected();
    Some(ActiveClientSocket {
        tenant_id,
        session_id,
        connection_id,
        connection,
        boundary,
    })
}

pub(super) async fn deactivate_client_socket(
    state: &Arc<GatewayState>,
    active: &ActiveClientSocket,
    mut request_tasks: JoinSet<()>,
    writer_task: JoinHandle<()>,
) {
    if let Some(tenant) = state.tenant_partition(&active.tenant_id) {
        let mut tenant = tenant.write();
        tenant.remove_client(&active.connection_id);
        tenant.sessions.remove(&active.session_id);
    }
    request_tasks.abort_all();
    while request_tasks.join_next().await.is_some() {}
    writer_task.abort();
    let _ = writer_task.await;
    remove_client_ownership(state, &active.tenant_id, &active.session_id).await;
    state.metrics.client_disconnected();
}
