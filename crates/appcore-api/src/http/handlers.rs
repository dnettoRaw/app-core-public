// =============================================================================
//        #######
//     ###       ###     F: handlers.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Health, status, and diagnostics handlers.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::sync::atomic::Ordering;

use super::auth::{authorize_private_status, authorize_status};
use super::state::HttpState;

pub(crate) async fn health_handler(State(state): State<HttpState>) -> Response {
    if let Some(supervisor) = &state.supervisor {
        let _ = supervisor.evaluate_watchdog(state.clock.now_ms());
    }
    let healthy = runtime_is_healthy(&state);
    let supervisor = state
        .supervisor
        .as_ref()
        .map(|supervisor| supervisor_progress_json(supervisor, state.clock.now_ms()));
    let payload = serde_json::json!({
        "status": if healthy { "healthy" } else { "unhealthy" },
        "supervisor": supervisor
    });
    (
        if healthy {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(payload),
    )
        .into_response()
}

pub(crate) async fn status_handler(State(state): State<HttpState>, headers: HeaderMap) -> Response {
    match authorize_status(&state.auth, &headers) {
        Err(status_code) => status_code.into_response(),
        Ok(true) => private_status_response(&state),
        Ok(false) => public_status_response(&state),
    }
}

pub(crate) async fn public_status_handler(State(state): State<HttpState>) -> Response {
    if !state.auth.public_status {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    public_status_response(&state)
}

pub(crate) async fn private_status_handler(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Response {
    match authorize_private_status(&state.auth, &headers, "runtime.status") {
        Ok(()) => private_status_response(&state),
        Err(status) => status.into_response(),
    }
}

pub(crate) async fn diagnostics_handler(
    State(state): State<HttpState>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = authorize_private_status(&state.auth, &headers, "runtime.diagnostics") {
        return status.into_response();
    }
    let supervisor = state.supervisor.as_ref().map(supervisor_json);
    let payload = serde_json::json!({
        "status": private_status_json(&state),
        "sync": {
            "log_path": state.static_info.sync_log_path,
            "checkpoint_path": state.static_info.sync_checkpoint_path,
            "peers": state.static_info.sync_peers,
            "dns_enabled": state.static_info.sync_dns_enabled,
            "dns_seeds": state.static_info.sync_dns_seeds,
            "dns_default_port": state.static_info.sync_dns_default_port
        },
        "idempotency": {
            "ttl_ms": state.static_info.idempotency_ttl_ms,
            "path": state.static_info.idempotency_path
        },
        "supervisor": supervisor
    });
    (StatusCode::OK, Json(payload)).into_response()
}

fn public_status_response(state: &HttpState) -> Response {
    let payload = serde_json::json!({
        "status": "online",
        "lifecycle": lifecycle(state)
    });
    (StatusCode::OK, Json(payload)).into_response()
}

fn private_status_response(state: &HttpState) -> Response {
    (StatusCode::OK, Json(private_status_json(state))).into_response()
}

fn private_status_json(state: &HttpState) -> serde_json::Value {
    let (sync_log_len, sync_log_observation_ok) = match state.sync_log.as_ref() {
        Some(log) => match log.len() {
            Ok(length) => (Some(length), true),
            Err(_) => (None, false),
        },
        None => (Some(state.static_info.sync_log_len), true),
    };
    let tick_count = state
        .tick_counter
        .as_ref()
        .map(|counter| counter.load(Ordering::SeqCst));
    serde_json::json!({
        "app_id": state.static_info.app_id,
        "node_id": state.static_info.node_id,
        "tenant_id": state.static_info.tenant_id,
        "cluster_id": state.static_info.cluster_id,
        "core_id": state.static_info.core_id,
        "operation_mode": current_operation_mode(state),
        "lifecycle": lifecycle(state),
        "storage_status": state.static_info.storage_status,
        "security_ok": state.static_info.security_ok,
        "api_enabled": state.static_info.api_enabled,
        "sync_enabled": state.static_info.sync_enabled,
        "sync_role": state.static_info.sync_role,
        "sync_log_len": sync_log_len,
        "sync_log_observation_ok": sync_log_observation_ok,
        "tick_count": tick_count,
        "supervisor": state
            .supervisor
            .as_ref()
            .map(|supervisor| supervisor_progress_json(supervisor, state.clock.now_ms()))
    })
}

fn supervisor_json(supervisor: &appcore_supervisor::Supervisor) -> serde_json::Value {
    let services = supervisor
        .snapshots()
        .into_iter()
        .map(|snapshot| {
            serde_json::json!({
                "name": snapshot.name,
                "health": format!("{:?}", snapshot.health),
                "dependencies": snapshot.dependencies,
                "restart_count": snapshot.restart_count,
                "operator_required": snapshot.operator_required,
                "quarantined": snapshot.quarantined,
                "activation": format!("{:?}", snapshot.activation),
                "enabled": snapshot.enabled,
                "configured": snapshot.configured,
                "running": snapshot.running,
                "runtime_state": format!("{:?}", snapshot.runtime_state),
                "restart_state": format!("{:?}", snapshot.restart_state),
                "critical": snapshot.critical
            })
        })
        .collect::<Vec<_>>();
    let events = supervisor
        .events()
        .into_iter()
        .map(|event| {
            serde_json::json!({
                "service_id": event.service_id,
                "kind": format!("{:?}", event.kind),
                "timestamp_ms": event.timestamp_ms,
                "attempt": event.attempt,
                "reason": event.reason,
                "previous_state": event.previous_state,
                "new_state": event.new_state,
                "trace_id": event.trace_id
            })
        })
        .collect::<Vec<_>>();
    let diagnosis = supervisor.diagnose();
    serde_json::json!({
        "state": format!("{:?}", diagnosis.watchdog.state).to_ascii_lowercase(),
        "last_reconcile_at_ms": diagnosis.watchdog.last_reconcile_at_ms,
        "last_progress_at_ms": diagnosis.watchdog.last_progress_at_ms,
        "reconcile_sequence": diagnosis.watchdog.reconcile_sequence,
        "stalled_for_ms": diagnosis.watchdog.stalled_for_ms,
        "critical_services_healthy": diagnosis.watchdog.critical_services_healthy,
        "restart_executor": {
            "healthy": diagnosis.restart_executor.healthy,
            "pending": diagnosis.restart_executor.pending,
            "queue_capacity": diagnosis.restart_executor.queue_capacity,
            "worker_count": diagnosis.restart_executor.worker_count
        },
        "services": services,
        "events": events
    })
}

fn supervisor_progress_json(
    supervisor: &appcore_supervisor::Supervisor,
    timestamp_ms: u64,
) -> serde_json::Value {
    let snapshot = supervisor.evaluate_watchdog(timestamp_ms);
    serde_json::json!({
        "state": format!("{:?}", snapshot.state).to_ascii_lowercase(),
        "last_reconcile_at_ms": snapshot.last_reconcile_at_ms,
        "last_progress_at_ms": snapshot.last_progress_at_ms,
        "reconcile_sequence": snapshot.reconcile_sequence,
        "stalled_for_ms": snapshot.stalled_for_ms,
        "critical_services_healthy": snapshot.critical_services_healthy,
        "stall_timeout_ms": snapshot.stall_timeout_ms
    })
}

fn lifecycle(state: &HttpState) -> String {
    state
        .controller
        .as_ref()
        .map(|controller| format!("{:?}", controller.lock().lifecycle().current()))
        .unwrap_or_else(|| "Restricted".to_string())
}

fn current_operation_mode(state: &HttpState) -> String {
    state
        .operation_mode
        .as_ref()
        .map(|mode| mode.lock().as_str().to_string())
        .unwrap_or_else(|| state.static_info.operation_mode.clone())
}

fn runtime_is_healthy(state: &HttpState) -> bool {
    if !state.static_info.security_ok || state.static_info.storage_status != "Online" {
        return false;
    }
    if state.controller.as_ref().is_some_and(|controller| {
        controller.lock().lifecycle().current() != appcore_core::RuntimeLifecycleState::Running
    }) {
        return false;
    }
    state
        .supervisor
        .as_ref()
        .is_none_or(|supervisor| supervisor.is_healthy(state.clock.now_ms()))
}
