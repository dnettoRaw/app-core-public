// =============================================================================
//        #######
//     ###       ###     F: application_host_contract.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/20 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/20 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Manifest-to-business registration contract validation.

use super::runtime_error;
use crate::application::Application;
use crate::application_tasks::{ApplicationTaskRegistry, RegisteredApplicationTask};
use crate::bootstrap::{BootstrapError, BootstrapResult};
use appcore_api::{ApiResponse, ApiRouter, QueryRequestValidationError, QueryResponse};
use appcore_contracts::{CapabilityMode, SchedulerRequirements};
use appcore_core::{CommandName, RuntimeError};
use parking_lot::Mutex;
use std::sync::Arc;

pub(super) fn validate_business_contract(
    runtime: &BootstrapResult,
    application_tasks: &[RegisteredApplicationTask],
) -> Result<(), BootstrapError> {
    validate_command_contract(runtime)?;
    validate_query_contract(runtime)?;
    validate_stream_contract(runtime)?;
    validate_scheduler_contract(
        runtime.application_manifest.scheduler_requirements(),
        application_tasks,
    )
}

fn validate_command_contract(runtime: &BootstrapResult) -> Result<(), BootstrapError> {
    let controller = runtime.controller.lock();
    let instance = controller.instance();
    for capability in runtime
        .application_manifest
        .capabilities()
        .iter()
        .filter(|capability| capability.mode() == CapabilityMode::Command)
    {
        let name = CommandName::new(capability.id().as_str()).map_err(runtime_error)?;
        if !instance.commands().contains(&name) || !instance.command_bus().contains_handler(&name) {
            return Err(BootstrapError::Runtime(format!(
                "business code does not implement declared command capability: {}",
                capability.id()
            )));
        }
    }
    for name in instance.commands().list() {
        if name.as_str() == "runtime.ping" {
            continue;
        }
        let declared = runtime
            .application_manifest
            .capabilities()
            .iter()
            .any(|capability| {
                capability.id().as_str() == name.as_str()
                    && capability.mode() == CapabilityMode::Command
            });
        if !declared {
            return Err(BootstrapError::Runtime(format!(
                "business command is absent from the application manifest: {}",
                name.as_str()
            )));
        }
    }
    Ok(())
}

fn validate_query_contract(runtime: &BootstrapResult) -> Result<(), BootstrapError> {
    let router = runtime
        .app_query_router
        .as_ref()
        .ok_or_else(|| BootstrapError::Runtime("application query router is absent".to_string()))?
        .lock();
    for capability in runtime
        .application_manifest
        .capabilities()
        .iter()
        .filter(|capability| capability.mode() == CapabilityMode::Query)
    {
        let name = appcore_api::QueryName::new(capability.id().as_str()).map_err(runtime_error)?;
        if !router.has_query(&name) {
            return Err(BootstrapError::Runtime(format!(
                "business code does not implement declared query capability: {}",
                capability.id()
            )));
        }
    }
    for name in router.query_names() {
        let declared = runtime
            .application_manifest
            .capabilities()
            .iter()
            .any(|capability| {
                capability.id().as_str() == name.as_str()
                    && capability.mode() == CapabilityMode::Query
            });
        if !declared {
            return Err(BootstrapError::Runtime(format!(
                "business query is absent from the application manifest: {}",
                name.as_str()
            )));
        }
    }
    Ok(())
}

fn validate_stream_contract(runtime: &BootstrapResult) -> Result<(), BootstrapError> {
    let unsupported = runtime
        .application_manifest
        .capabilities()
        .iter()
        .find(|capability| capability.mode() == CapabilityMode::Stream);
    match unsupported {
        Some(capability) => Err(BootstrapError::Runtime(format!(
            "stream capability is reserved and unsupported by the 1.0 host: {}",
            capability.id()
        ))),
        None => Ok(()),
    }
}

pub(super) fn validate_scheduler_contract(
    requirements: &SchedulerRequirements,
    application_tasks: &[RegisteredApplicationTask],
) -> Result<(), BootstrapError> {
    if requirements.is_required() && application_tasks.is_empty() {
        return Err(BootstrapError::Runtime(
            "application requires a scheduler but business code registered no tasks".to_string(),
        ));
    }
    if !requirements.is_required() && !application_tasks.is_empty() {
        return Err(BootstrapError::Runtime(
            "business tasks are absent from the application manifest scheduler requirements"
                .to_string(),
        ));
    }
    Ok(())
}

pub(super) fn build_query_router(
    business: &dyn Application,
) -> Result<Arc<Mutex<ApiRouter>>, BootstrapError> {
    let mut router = ApiRouter::new();
    business
        .register_queries(&mut router)
        .map_err(runtime_error)?;
    router.freeze_queries();
    Ok(Arc::new(Mutex::new(router)))
}

pub(super) fn build_task_registry(
    business: &dyn Application,
) -> Result<Vec<RegisteredApplicationTask>, BootstrapError> {
    let mut registry = ApplicationTaskRegistry::new();
    business
        .register_tasks(&mut registry)
        .map_err(runtime_error)?;
    Ok(registry.into_tasks())
}

pub(super) fn query_validation_error(error: QueryRequestValidationError) -> RuntimeError {
    let reason = match error {
        QueryRequestValidationError::EmptyQueryName => "empty_query_name",
        QueryRequestValidationError::EmptyQueryId => "empty_query_id",
        QueryRequestValidationError::InvalidQueryName => "invalid_query_name",
        QueryRequestValidationError::InvalidQueryId => "invalid_query_id",
        QueryRequestValidationError::PayloadTooLarge => "payload_too_large",
    };
    RuntimeError::InvalidRequest {
        kind: "query",
        reason,
    }
}

pub(super) fn query_response(response: ApiResponse) -> QueryResponse {
    if !(200..300).contains(&response.status_code) {
        return QueryResponse::rejected("query rejected by app");
    }
    let payload = if response.payload.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_slice(&response.payload).unwrap_or_else(
            |_| serde_json::json!({ "payload": String::from_utf8_lossy(&response.payload) }),
        )
    };
    QueryResponse::ok(payload)
}
