// =============================================================================
//        #######
//     ###       ###     F: runtime_dispatch.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Synchronous forwarding to the existing Runtime owners; deployment supplies blocking isolation.
use crate::{UiCancellation, UiError, UiInvocationV1, UiLimits, UiResult};
use appcore_api::{
    ApiMethod, ApiRequest, ApiRouter, CommandResponse, CommandResponseEvent, QueryName,
};
use appcore_core::{Clock, RuntimeContext, RuntimeController, RuntimeError, SystemClock};
use serde_json::Value;
use std::sync::Arc;
/// An explicit dispatch integration with already composed Runtime registries.
///
/// This value does not start services, apply lifecycle transitions or create an
/// executor. Invoke it through a bounded deployment-owned blocking boundary when
/// handlers can block. Cancellation before invocation prevents dispatch; cancellation
/// after a command starts does not imply rollback. `RuntimeController` owns idempotency.
pub struct UiRuntimeDispatch {
    controller: RuntimeController,
    queries: ApiRouter,
    context: Arc<dyn RuntimeContext + Send + Sync>,
    max_payload_bytes: usize,
}
impl UiRuntimeDispatch {
    /// Wires existing owners and rejects a context for another installation.
    pub fn new(
        controller: RuntimeController,
        queries: ApiRouter,
        context: Arc<dyn RuntimeContext + Send + Sync>,
        limits: UiLimits,
    ) -> UiResult<Self> {
        let limits = limits.validate()?;
        let identity = controller.instance().identity();
        if context.app_id() != &identity.app_id
            || context.app_family() != &identity.app_family
            || context.sync_group() != &identity.sync_group
            || context.node_id() != &identity.node_id
            || context.runtime_contract() != identity.runtime_contract
        {
            return Err(UiError::Forbidden);
        }
        Ok(Self {
            controller,
            queries,
            context,
            max_payload_bytes: limits.max_payload_bytes,
        })
    }
    /// Forwards one operation to its existing handler/query and returns bounded JSON.
    /// The outer `UiSurface` performs caller/capability authorization before this boundary.
    pub fn invoke(
        &self,
        invocation: UiInvocationV1,
        cancellation: &UiCancellation,
    ) -> UiResult<Value> {
        if cancellation.is_cancelled() {
            return Err(UiError::Cancelled);
        }
        let value = match invocation {
            UiInvocationV1::Command(command) => {
                if command.idempotency_key.is_none() {
                    return Err(UiError::IdempotencyRequired);
                }
                command
                    .validate(self.max_payload_bytes)
                    .map_err(|_| UiError::InvalidRequest)?;
                let envelope = command
                    .into_envelope(
                        self.context.app_id().clone(),
                        self.context.node_id().clone(),
                        SystemClock.now_ms(),
                        self.max_payload_bytes,
                    )
                    .map_err(runtime_error)?;
                let result = self
                    .controller
                    .dispatch_command(&envelope, self.context.as_ref())
                    .map_err(runtime_error)?;
                let response = CommandResponse {
                    accepted: result.is_accepted(),
                    message: result.message().map(str::to_string),
                    events: result
                        .events()
                        .iter()
                        .map(|event| CommandResponseEvent {
                            event_name: event.event_name.as_str().to_string(),
                            event_id: event.event_id.clone(),
                        })
                        .collect(),
                };
                crate::json_budget::json_bytes(&response, self.max_payload_bytes)?;
                serde_json::to_value(response).map_err(|_| UiError::BackendFailure)?
            }
            UiInvocationV1::Query(query) => {
                query
                    .validate(self.max_payload_bytes)
                    .map_err(|_| UiError::InvalidRequest)?;
                let name = QueryName::new(query.query_name.clone()).map_err(runtime_error)?;
                let response = self
                    .queries
                    .dispatch_query(
                        &name,
                        ApiRequest {
                            method: ApiMethod::Query,
                            path: format!("/v1/query/{}", query.query_name),
                            payload: query.payload_bytes(),
                        },
                    )
                    .map_err(runtime_error)?;
                if !(200..300).contains(&response.status_code) {
                    return Err(UiError::BackendFailure);
                }
                if response.payload.len() > self.max_payload_bytes {
                    return Err(UiError::PayloadTooLarge);
                }
                serde_json::from_slice(&response.payload).map_err(|_| UiError::InvalidFrame)?
            }
        };
        crate::json_budget::json_bytes(&value, self.max_payload_bytes)?;
        Ok(value)
    }
}
fn runtime_error(error: RuntimeError) -> UiError {
    match error {
        RuntimeError::IdempotencyPending { .. } => UiError::IdempotencyPending,
        RuntimeError::IdempotencyConflict { .. } => UiError::IdempotencyConflict,
        RuntimeError::InvalidIdempotencyKey { .. } => UiError::InvalidRequest,
        RuntimeError::IdempotencyStoreIo {
            operation: "validate_request_domain",
            ..
        } => UiError::UnsupportedVersion,
        _ => UiError::BackendFailure,
    }
}
