// =============================================================================
//        #######
//     ###       ###     F: socket_ownership.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.4-rc
// =============================================================================

//! Atomic ordering between shared HA ownership and local socket directories.

use crate::config::MAX_GATEWAY_CONNECTIONS;
use crate::{
    ClientConnection, GatewaySession, GatewayState, GatewayWorkerRegistration, WorkerConnection,
};
use appcore_contracts::InstallationId;
use appcore_types::{CapabilityName, ClusterId, CoreId, TenantId};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) async fn register_worker_ownership(
    state: &GatewayState,
    tenant_id: &TenantId,
    cluster_id: &ClusterId,
    installation_id: &InstallationId,
    core_id: &CoreId,
    connection: &WorkerConnection,
    capabilities: Vec<CapabilityName>,
) -> Result<bool, &'static str> {
    if let Some(coordinator) = state.ha_coordinator() {
        let registration = GatewayWorkerRegistration::new(
            installation_id.clone(),
            core_id.clone(),
            connection.generation(),
            capabilities.clone(),
        )
        .map_err(|_| "invalid HA worker ownership")?;
        coordinator
            .register_worker(tenant_id, cluster_id, registration, now_ms())
            .await
            .map_err(|_| "HA registry rejected worker ownership")?;
    }
    let local = {
        let _admission = state.lock_connection_admission();
        if state.connection_count() >= MAX_GATEWAY_CONNECTIONS {
            Err("global connection limit")
        } else {
            state
                .tenant_partition_or_insert(tenant_id)
                .map_err(|_| "tenant limit")
                .and_then(|tenant| {
                    let mut tenant = tenant.write();
                    let replaced = tenant.get_worker(installation_id, core_id).is_some();
                    tenant
                        .add_worker(connection.clone(), capabilities)
                        .map(|()| replaced)
                        .map_err(|_| "worker limit")
                })
        }
    };
    if local.is_err() {
        remove_worker_ownership(state, tenant_id, installation_id, core_id, connection).await;
    }
    local
}

pub(crate) async fn remove_worker_ownership(
    state: &GatewayState,
    tenant_id: &TenantId,
    installation_id: &InstallationId,
    core_id: &CoreId,
    connection: &WorkerConnection,
) {
    if let Some(coordinator) = state.ha_coordinator() {
        let _ = coordinator
            .remove_worker(tenant_id, installation_id, core_id, connection.generation())
            .await;
    }
}

pub(crate) async fn register_client_ownership(
    state: &GatewayState,
    tenant_id: &TenantId,
    cluster_id: &ClusterId,
    connection: &ClientConnection,
    session: GatewaySession,
) -> Result<(), &'static str> {
    if let Some(coordinator) = state.ha_coordinator() {
        coordinator
            .register_session(
                tenant_id,
                cluster_id,
                session.session_id.clone(),
                session.expires_at_ms,
                now_ms(),
            )
            .await
            .map_err(|_| "HA registry rejected session ownership")?;
    }
    let local = {
        let _admission = state.lock_connection_admission();
        if state.connection_count() >= MAX_GATEWAY_CONNECTIONS {
            Err("global connection limit")
        } else {
            state
                .tenant_partition_or_insert(tenant_id)
                .map_err(|_| "tenant limit")
                .and_then(|tenant| {
                    let mut tenant = tenant.write();
                    tenant
                        .try_add_client(connection.clone())
                        .map_err(|_| "client limit")?;
                    tenant.sessions.insert(session.session_id.clone(), session);
                    Ok(())
                })
        }
    };
    if local.is_err() {
        remove_client_ownership(state, tenant_id, &connection.session_id).await;
    }
    local
}

pub(crate) async fn remove_client_ownership(
    state: &GatewayState,
    tenant_id: &TenantId,
    session_id: &str,
) {
    if let Some(coordinator) = state.ha_coordinator() {
        let _ = coordinator.remove_session(tenant_id, session_id).await;
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}
