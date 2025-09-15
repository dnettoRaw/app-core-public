// =============================================================================
//        #######
//     ###       ###     F: query.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! `/v1/query` validation and runtime-state dispatch.

use crate::api::{ApiMethod, ApiRequest, ApiResponse};
use crate::query_contract::{QueryRequest, QueryRequestValidationError, QueryResponse};
use crate::{ApiRouter, QueryName};
use appcore_core::{
    AuditCategory, AuditEntry, AuditOutcome, RuntimeController, RuntimeError, TraceContext,
};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::auth::authorize_query;
use super::state::{CommandCapabilityPolicyError, HttpState, RuntimeStaticInfo, SyncLogView};
use super::trace::request_trace;

pub(crate) async fn query_handler(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(request): Json<QueryRequest>,
) -> Response {
    let started_at_ms = state.clock.now_ms();
    if let Some(response) = validate_http_query(&request, state.max_payload_bytes) {
        return response;
    }
    let trace = match request_trace(&headers, &request.query_id, &state.static_info) {
        Ok(trace) => Some(trace),
        Err(()) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(QueryResponse::rejected("trace context invalid")),
            )
                .into_response()
        }
    };
    if let Some(response) = authorize_query(&state.auth, &headers, &request) {
        audit_query(
            &state,
            &request,
            started_at_ms,
            AuditOutcome::Rejected,
            Some("query authorization rejected".to_string()),
            trace,
        );
        return response.into_response();
    }
    if let Some(response) =
        authorize_application_capability(&state, &request, started_at_ms, trace.clone())
    {
        return response;
    }
    let request_for_dispatch = request.clone();
    let state_for_dispatch = state.clone();
    let dispatch = tokio::task::spawn_blocking(move || {
        dispatch_query_request(&request_for_dispatch, &state_for_dispatch)
    })
    .await;
    let dispatch = match dispatch {
        Ok(dispatch) => dispatch,
        Err(_) => {
            audit_query(
                &state,
                &request,
                started_at_ms,
                AuditOutcome::Error,
                Some("query dispatch failed".to_string()),
                trace,
            );
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(QueryResponse::rejected("query dispatch failed")),
            )
                .into_response();
        }
    };
    match dispatch {
        Ok(response) => {
            let outcome = if response.ok {
                AuditOutcome::Accepted
            } else {
                AuditOutcome::Rejected
            };
            audit_query(
                &state,
                &request,
                started_at_ms,
                outcome,
                response.message.clone(),
                trace,
            );
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(status) => {
            audit_query(
                &state,
                &request,
                started_at_ms,
                AuditOutcome::Error,
                Some(format!("query failed with HTTP status {}", status.as_u16())),
                trace,
            );
            (
                status,
                Json(QueryResponse::rejected("query request invalid")),
            )
                .into_response()
        }
    }
}

fn validate_http_query(request: &QueryRequest, max_payload_bytes: usize) -> Option<Response> {
    let status = match request.validate(max_payload_bytes) {
        Ok(()) => return None,
        Err(QueryRequestValidationError::PayloadTooLarge) => StatusCode::PAYLOAD_TOO_LARGE,
        Err(_) => StatusCode::BAD_REQUEST,
    };
    Some(
        (
            status,
            Json(QueryResponse::rejected("query request invalid")),
        )
            .into_response(),
    )
}

fn authorize_application_capability(
    state: &HttpState,
    request: &QueryRequest,
    started_at_ms: u64,
    trace: Option<TraceContext>,
) -> Option<Response> {
    if is_runtime_query(&request.query_name) {
        return None;
    }
    let policy = state.command_policy.as_ref()?;
    let error = policy
        .authorize_query(&request.query_name, state.clock.now_ms())
        .err()?;
    audit_query(
        state,
        request,
        started_at_ms,
        AuditOutcome::Rejected,
        Some("query capability policy rejected".to_string()),
        trace,
    );
    Some(map_query_policy_error(error).into_response())
}

fn is_runtime_query(name: &str) -> bool {
    matches!(
        name,
        "runtime.status"
            | "runtime.sync"
            | "runtime.idempotency"
            | "runtime.audit"
            | "runtime.events"
    )
}

fn map_query_policy_error(
    error: CommandCapabilityPolicyError,
) -> (StatusCode, Json<QueryResponse>) {
    let (status, message) = match error {
        CommandCapabilityPolicyError::CapabilityNotDeclared => {
            (StatusCode::FORBIDDEN, "capability_not_declared")
        }
        CommandCapabilityPolicyError::MissingIdempotencyKey => {
            (StatusCode::BAD_REQUEST, "missing_idempotency_key")
        }
        CommandCapabilityPolicyError::RequiresLeader => (StatusCode::CONFLICT, "requires_leader"),
        CommandCapabilityPolicyError::LeaseExpired => {
            (StatusCode::CONFLICT, "leader_lease_expired")
        }
        CommandCapabilityPolicyError::StaleEpoch => {
            (StatusCode::CONFLICT, "leader_lease_stale_epoch")
        }
        CommandCapabilityPolicyError::ReadOnly => {
            (StatusCode::FORBIDDEN, "operation_mode_read_only")
        }
        CommandCapabilityPolicyError::Rejected(_) => (StatusCode::FORBIDDEN, "capability_rejected"),
    };
    (status, Json(QueryResponse::rejected(message)))
}

fn audit_query(
    state: &HttpState,
    request: &QueryRequest,
    started_at_ms: u64,
    outcome: AuditOutcome,
    message: Option<String>,
    trace: Option<TraceContext>,
) {
    let Some(controller) = &state.controller else {
        return;
    };
    let completed_at_ms = state.clock.now_ms();
    let guard = controller.lock();
    let identity = guard.instance().identity();
    guard.instance().audit_log().push_entry(
        AuditEntry::new(
            AuditCategory::Query,
            request.query_id.clone(),
            request.query_name.clone(),
            started_at_ms,
            completed_at_ms,
            outcome,
        )
        .with_runtime_scope(&identity.app_id, &identity.node_id)
        .with_message(message)
        .with_trace(trace),
    );
}

fn dispatch_query_request(
    request: &QueryRequest,
    state: &HttpState,
) -> Result<QueryResponse, StatusCode> {
    match request.validate(state.max_payload_bytes) {
        Ok(()) => {}
        Err(QueryRequestValidationError::PayloadTooLarge) => {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
        Err(_) => return Err(StatusCode::BAD_REQUEST),
    }
    match request.query_name.as_str() {
        "runtime.status" => {
            let val = handle_runtime_status_query(
                state.controller.as_ref(),
                &state.static_info,
                state.sync_log.as_ref(),
                state.tick_counter.as_ref(),
                state.operation_mode.as_ref(),
            );
            Ok(QueryResponse::ok(val))
        }
        "runtime.sync" => {
            let val = handle_runtime_sync_query(&state.static_info, state.sync_log.as_ref());
            Ok(QueryResponse::ok(val))
        }
        "runtime.idempotency" => {
            let val =
                handle_runtime_idempotency_query(state.controller.as_ref(), &state.static_info);
            Ok(QueryResponse::ok(val))
        }
        "runtime.audit" => {
            let Some(limit) = parse_limit(request)? else {
                return Err(StatusCode::BAD_REQUEST);
            };
            let val = handle_runtime_audit_query(state.controller.as_ref(), limit);
            Ok(QueryResponse::ok(val))
        }
        "runtime.events" => {
            let Some(limit) = parse_limit(request)? else {
                return Err(StatusCode::BAD_REQUEST);
            };
            let val = handle_runtime_events_query(state.controller.as_ref(), limit);
            Ok(QueryResponse::ok(val))
        }
        _ => dispatch_app_query_request(request, state.app_query_router.as_ref()),
    }
}

fn dispatch_app_query_request(
    request: &QueryRequest,
    router: Option<&Arc<Mutex<ApiRouter>>>,
) -> Result<QueryResponse, StatusCode> {
    let Some(router) = router else {
        return Ok(QueryResponse::rejected("query not found"));
    };
    let api_request = api_request_from_query(request)?;
    let router = router.lock();
    let query_name =
        QueryName::new(request.query_name.clone()).map_err(|_| StatusCode::BAD_REQUEST)?;
    match router.dispatch_query(&query_name, api_request) {
        Ok(response) => Ok(api_response_to_query_response(response)),
        Err(RuntimeError::RegistryItemNotFound { .. }) => {
            Ok(QueryResponse::rejected("query not found"))
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

fn api_request_from_query(request: &QueryRequest) -> Result<ApiRequest, StatusCode> {
    let payload = serde_json::to_vec(&request.payload).map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(ApiRequest {
        method: ApiMethod::Query,
        path: request.query_name.clone(),
        payload,
    })
}

fn api_response_to_query_response(response: ApiResponse) -> QueryResponse {
    if !(200..300).contains(&response.status_code) {
        return QueryResponse::rejected("query rejected by app");
    }
    QueryResponse::ok(api_payload_to_json(response.payload))
}

fn api_payload_to_json(payload: Vec<u8>) -> serde_json::Value {
    if payload.is_empty() {
        return serde_json::json!({});
    }
    serde_json::from_slice(&payload).unwrap_or_else(|_| {
        serde_json::json!({
            "payload": String::from_utf8_lossy(&payload)
        })
    })
}

fn handle_runtime_status_query(
    controller: Option<&Arc<Mutex<RuntimeController>>>,
    static_info: &RuntimeStaticInfo,
    sync_log: Option<&Arc<dyn SyncLogView>>,
    tick_counter: Option<&Arc<AtomicU64>>,
    operation_mode: Option<&Arc<Mutex<appcore_core::RuntimeOperationalMode>>>,
) -> serde_json::Value {
    let operation_mode = operation_mode
        .map(|mode| mode.lock().as_str().to_string())
        .unwrap_or_else(|| static_info.operation_mode.clone());
    serde_json::json!({
        "app_id": static_info.app_id,
        "node_id": static_info.node_id,
        "tenant_id": static_info.tenant_id,
        "cluster_id": static_info.cluster_id,
        "core_id": static_info.core_id,
        "operation_mode": operation_mode,
        "lifecycle": controller
            .map(|c| format!("{:?}", c.lock().lifecycle().current()))
            .unwrap_or_else(|| "Restricted".to_string()),
        "storage_status": static_info.storage_status,
        "security_ok": static_info.security_ok,
        "api_enabled": static_info.api_enabled,
        "sync_enabled": static_info.sync_enabled,
        "sync_role": static_info.sync_role,
        "sync_log_len": sync_log.map(|log| log.len()).unwrap_or(static_info.sync_log_len),
        "tick_count": tick_counter.map(|counter| counter.load(Ordering::SeqCst))
    })
}

fn handle_runtime_sync_query(
    static_info: &RuntimeStaticInfo,
    sync_log: Option<&Arc<dyn SyncLogView>>,
) -> serde_json::Value {
    serde_json::json!({
        "sync_enabled": static_info.sync_enabled,
        "sync_role": static_info.sync_role,
        "sync_log_len": sync_log.map(|log| log.len()).unwrap_or(static_info.sync_log_len),
        "sync_log_path": static_info.sync_log_path,
        "sync_checkpoint_path": static_info.sync_checkpoint_path,
        "sync_peers": static_info.sync_peers,
        "sync_dns_enabled": static_info.sync_dns_enabled,
        "sync_dns_seeds": static_info.sync_dns_seeds,
        "sync_dns_default_port": static_info.sync_dns_default_port
    })
}

fn handle_runtime_idempotency_query(
    controller: Option<&Arc<Mutex<RuntimeController>>>,
    static_info: &RuntimeStaticInfo,
) -> serde_json::Value {
    let idempotency_len = controller
        .map(|controller| controller.lock().idempotency_len())
        .unwrap_or(0);
    serde_json::json!({
        "idempotency_len": idempotency_len,
        "ttl_ms": static_info.idempotency_ttl_ms,
        "idempotency_path": static_info.idempotency_path
    })
}

fn handle_runtime_audit_query(
    controller: Option<&Arc<Mutex<RuntimeController>>>,
    limit: usize,
) -> serde_json::Value {
    let records = controller
        .map(|controller| {
            let guard = controller.lock();
            let items = guard.instance().audit_log().records();
            let start = items.len().saturating_sub(limit);
            items[start..]
                .iter()
                .map(|item| {
                    serde_json::json!({
                        "command_id": item.command_id,
                        "command_name": item.command_name.as_str(),
                        "app_id": item.app_id.as_str(),
                        "node_id": item.node_id.as_str(),
                        "timestamp_ms": item.timestamp_ms,
                        "outcome": format!("{:?}", item.outcome),
                        "message": item.message,
                        "trace_id": item.trace.as_ref().map(|trace| trace.trace_id.clone()),
                        "span_id": item.trace.as_ref().map(|trace| trace.span_id.clone())
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let entries = controller
        .map(|controller| {
            let guard = controller.lock();
            let items = guard.instance().audit_log().entries();
            let start = items.len().saturating_sub(limit);
            items[start..].to_vec()
        })
        .unwrap_or_default();
    serde_json::json!({ "records": records, "entries": entries })
}

fn handle_runtime_events_query(
    controller: Option<&Arc<Mutex<RuntimeController>>>,
    limit: usize,
) -> serde_json::Value {
    let events = controller
        .map(|controller| {
            let guard = controller.lock();
            let items = guard.instance().event_bus().events();
            let start = items.len().saturating_sub(limit);
            items[start..]
                .iter()
                .map(|item| {
                    serde_json::json!({
                        "event_name": item.event_name.as_str(),
                        "event_id": item.event_id,
                        "app_id": item.app_id.as_str(),
                        "node_id": item.node_id.as_str(),
                        "occurred_at_ms": item.occurred_at_ms,
                        "trace_id": item.trace.as_ref().map(|trace| trace.trace_id.clone()),
                        "span_id": item.trace.as_ref().map(|trace| trace.span_id.clone())
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    serde_json::json!({ "events": events })
}

fn parse_limit(request: &QueryRequest) -> Result<Option<usize>, StatusCode> {
    let Some(value) = request.payload.get("limit") else {
        return Ok(Some(20));
    };
    let Some(limit) = value.as_u64() else {
        return Err(StatusCode::BAD_REQUEST);
    };
    if limit == 0 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut limit = limit as usize;
    if limit > 1000 {
        limit = 1000;
    }
    Ok(Some(limit))
}
