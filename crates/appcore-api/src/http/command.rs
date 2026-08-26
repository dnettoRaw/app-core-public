// =============================================================================
//        #######
//     ###       ###     F: command.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! `/v1/command` request dispatch.

use crate::command_contract::{CommandRequest, CommandResponse};
use appcore_core::{
    AppFamily, AppId, AuditCategory, AuditEntry, AuditOutcome, NodeId, RuntimeContext,
    RuntimeContractVersion, RuntimeController, RuntimeError, RuntimeIdentity, SyncGroup,
};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use parking_lot::Mutex;
use std::sync::Arc;

use super::auth::authorize_command;
use super::response::{command_not_configured, command_result_to_response, map_dispatch_error};
use super::state::{CommandCapabilityPolicy, CommandCapabilityPolicyError, HttpState};
use super::trace::request_trace;

pub(crate) fn dispatch_command_request(
    request: &CommandRequest,
    controller: &Arc<Mutex<RuntimeController>>,
    max_payload_bytes: usize,
    clock: &dyn appcore_core::Clock,
    command_policy: Option<&Arc<dyn CommandCapabilityPolicy>>,
    trace: Option<appcore_core::TraceContext>,
) -> Result<CommandResponse, CommandDispatchError> {
    let controller = controller.lock().clone();
    let identity = controller.instance().identity().clone();
    let mut envelope = request
        .to_envelope(
            identity.app_id.clone(),
            identity.node_id.clone(),
            clock.now_ms(),
            max_payload_bytes,
        )
        .map_err(CommandDispatchError::Runtime)?;
    if let Some(trace) = trace {
        envelope = envelope.with_trace(
            trace
                .with_command_id(request.command_id.clone())
                .map_err(CommandDispatchError::Runtime)?,
        );
    }
    if let Some(policy) = command_policy {
        policy
            .authorize_command(
                envelope.command_name.as_str(),
                envelope.idempotency_key.as_deref(),
                clock.now_ms(),
            )
            .map_err(CommandDispatchError::Policy)?;
    }
    let context = HttpRuntimeContext::from_identity(&identity);
    let result = controller
        .dispatch_command(&envelope, &context)
        .map_err(CommandDispatchError::Runtime)?;

    Ok(command_result_to_response(result))
}

#[derive(Debug)]
pub(crate) enum CommandDispatchError {
    Runtime(RuntimeError),
    Policy(CommandCapabilityPolicyError),
}

#[derive(Debug, Clone)]
struct HttpRuntimeContext {
    app_id: AppId,
    app_family: AppFamily,
    sync_group: SyncGroup,
    runtime_contract: RuntimeContractVersion,
    node_id: NodeId,
}

impl HttpRuntimeContext {
    fn from_identity(identity: &RuntimeIdentity) -> Self {
        Self {
            app_id: identity.app_id.clone(),
            app_family: identity.app_family.clone(),
            sync_group: identity.sync_group.clone(),
            runtime_contract: identity.runtime_contract,
            node_id: identity.node_id.clone(),
        }
    }
}

impl RuntimeContext for HttpRuntimeContext {
    fn app_id(&self) -> &AppId {
        &self.app_id
    }
    fn app_family(&self) -> &AppFamily {
        &self.app_family
    }
    fn sync_group(&self) -> &SyncGroup {
        &self.sync_group
    }
    fn runtime_contract(&self) -> RuntimeContractVersion {
        self.runtime_contract
    }
    fn node_id(&self) -> &NodeId {
        &self.node_id
    }
}

pub(crate) async fn command_handler(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(request): Json<CommandRequest>,
) -> Response {
    let started_at_ms = state.clock.now_ms();
    if let Some(response) = authorize_command(&state.auth, &headers, &request) {
        audit_command_authorization_rejection(&state, &headers, &request, started_at_ms);
        return response.into_response();
    }
    let Some(controller) = &state.controller else {
        return command_not_configured().into_response();
    };
    let trace = match request_trace(&headers, &request.command_id, &state.static_info) {
        Ok(trace) => Some(trace),
        Err(()) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(CommandResponse::rejected("trace context invalid")),
            )
                .into_response()
        }
    };
    let request_for_dispatch = request.clone();
    let controller = Arc::clone(controller);
    let clock = Arc::clone(&state.clock);
    let command_policy = state.command_policy.clone();
    let max_payload_bytes = state.max_payload_bytes;
    let dispatch = tokio::task::spawn_blocking(move || {
        dispatch_command_request(
            &request_for_dispatch,
            &controller,
            max_payload_bytes,
            &*clock,
            command_policy.as_ref(),
            trace,
        )
    })
    .await;
    let dispatch = match dispatch {
        Ok(dispatch) => dispatch,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(CommandResponse::rejected("command dispatch failed")),
            )
                .into_response()
        }
    };
    match dispatch {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(CommandDispatchError::Runtime(error)) => map_dispatch_error(error).into_response(),
        Err(CommandDispatchError::Policy(error)) => map_policy_error(error).into_response(),
    }
}

fn audit_command_authorization_rejection(
    state: &HttpState,
    headers: &HeaderMap,
    request: &CommandRequest,
    started_at_ms: u64,
) {
    let Some(controller) = &state.controller else {
        return;
    };
    let (operation_id, operation_name) = if request.validate(state.max_payload_bytes).is_ok() {
        (request.command_id.clone(), request.command_name.clone())
    } else {
        (
            "invalid-command-id".to_string(),
            "invalid-command".to_string(),
        )
    };
    let trace = request_trace(headers, &operation_id, &state.static_info).ok();
    let completed_at_ms = state.clock.now_ms();
    let guard = controller.lock();
    let identity = guard.instance().identity();
    guard.instance().audit_log().push_entry(
        AuditEntry::new(
            AuditCategory::Command,
            operation_id,
            operation_name,
            started_at_ms,
            completed_at_ms,
            AuditOutcome::Rejected,
        )
        .with_runtime_scope(&identity.app_id, &identity.node_id)
        .with_message(Some("command authorization rejected".to_string()))
        .with_trace(trace),
    );
}

fn map_policy_error(error: CommandCapabilityPolicyError) -> (StatusCode, Json<CommandResponse>) {
    let (status, message) = match error {
        CommandCapabilityPolicyError::CapabilityNotDeclared => {
            (StatusCode::FORBIDDEN, "capability_not_declared".to_string())
        }
        CommandCapabilityPolicyError::MissingIdempotencyKey => (
            StatusCode::BAD_REQUEST,
            "missing_idempotency_key".to_string(),
        ),
        CommandCapabilityPolicyError::RequiresLeader => {
            (StatusCode::CONFLICT, "requires_leader".to_string())
        }
        CommandCapabilityPolicyError::LeaseExpired => {
            (StatusCode::CONFLICT, "leader_lease_expired".to_string())
        }
        CommandCapabilityPolicyError::StaleEpoch => {
            (StatusCode::CONFLICT, "leader_lease_stale_epoch".to_string())
        }
        CommandCapabilityPolicyError::ReadOnly => (
            StatusCode::FORBIDDEN,
            "operation_mode_read_only".to_string(),
        ),
        CommandCapabilityPolicyError::Rejected(message) => (StatusCode::FORBIDDEN, message),
    };
    (status, Json(CommandResponse::rejected(message)))
}
