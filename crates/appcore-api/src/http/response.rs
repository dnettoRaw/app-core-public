// =============================================================================
//        #######
//     ###       ###     F: response.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/09 08:35:21 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! HTTP response mapping helpers.

use crate::command_contract::{CommandResponse, CommandResponseEvent};
use appcore_core::{CommandResult, RuntimeError};
use axum::http::StatusCode;
use axum::Json;

pub(crate) fn command_not_configured() -> (StatusCode, Json<CommandResponse>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(CommandResponse::rejected(
            "command dispatcher not configured",
        )),
    )
}

pub(crate) fn command_unauthorized(message: &str) -> (StatusCode, Json<CommandResponse>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(CommandResponse::rejected(message)),
    )
}

pub(crate) fn command_forbidden(message: &str) -> (StatusCode, Json<CommandResponse>) {
    (
        StatusCode::FORBIDDEN,
        Json(CommandResponse::rejected(message)),
    )
}

pub(crate) fn command_result_to_response(result: CommandResult) -> CommandResponse {
    let events = result
        .events()
        .iter()
        .map(|event| CommandResponseEvent {
            event_name: event.event_name.as_str().to_string(),
            event_id: event.event_id.clone(),
        })
        .collect::<Vec<_>>();
    if result.is_accepted() {
        return CommandResponse::accepted(events);
    }
    CommandResponse {
        accepted: false,
        message: result.message().map(ToOwned::to_owned),
        events,
    }
}

pub(crate) fn map_dispatch_error(error: RuntimeError) -> (StatusCode, Json<CommandResponse>) {
    let status = match error {
        RuntimeError::HandlerNotFound(_) | RuntimeError::EmptyCommandId => StatusCode::BAD_REQUEST,
        RuntimeError::InvalidRequest {
            kind: "command",
            reason: "payload_too_large",
        } => StatusCode::PAYLOAD_TOO_LARGE,
        RuntimeError::InvalidRequest {
            kind: "command", ..
        } => StatusCode::BAD_REQUEST,
        RuntimeError::IdempotencyConflict { .. } => StatusCode::CONFLICT,
        RuntimeError::IdempotencyPending { .. } => StatusCode::TOO_MANY_REQUESTS,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(CommandResponse {
            accepted: false,
            message: Some(format!("{error:?}")),
            events: Vec::new(),
        }),
    )
}
