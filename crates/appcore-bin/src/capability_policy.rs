// =============================================================================
//        #######
//     ###       ###     F: capability_policy.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/20 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/20 00:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use appcore_api::{CommandCapabilityPolicy, CommandCapabilityPolicyError};
use appcore_capabilities::{
    CapabilityCatalog, CapabilityEnforcementContext, CapabilityError, CapabilityRequest,
    CapabilityResult,
};
use appcore_contracts::ServiceId;
use appcore_control_plane::{ServiceLeaderLease, StaticServiceLeadershipGuard};
use appcore_core::{
    CapabilityMode, CapabilityName, CoreIdentity, DistributedCoreManifest, RuntimeError,
    RuntimeOperationalMode, RuntimeResult,
};
use parking_lot::Mutex;
use std::sync::Arc;

pub(crate) struct RuntimeCapabilityPolicy {
    catalog: CapabilityCatalog,
    identity: CoreIdentity,
    operation_mode: Arc<Mutex<RuntimeOperationalMode>>,
    service_id: ServiceId,
    leader_lease: Arc<Mutex<Option<ServiceLeaderLease>>>,
}

impl RuntimeCapabilityPolicy {
    pub(crate) fn from_manifest(
        manifest: &DistributedCoreManifest,
        operation_mode: Arc<Mutex<RuntimeOperationalMode>>,
        service_id: ServiceId,
        leader_lease: Arc<Mutex<Option<ServiceLeaderLease>>>,
    ) -> CapabilityResult<Self> {
        Ok(Self {
            catalog: CapabilityCatalog::from_descriptors(manifest.capabilities.clone())?,
            identity: manifest.identity.clone(),
            operation_mode,
            service_id,
            leader_lease,
        })
    }

    pub(crate) fn authorize(
        &self,
        capability: &str,
        mode: CapabilityMode,
        idempotency_key: Option<&str>,
        now_ms: u64,
    ) -> CapabilityResult<()> {
        let capability = CapabilityName::new(capability)
            .map_err(|_| CapabilityError::HandlerRejected("invalid_capability_name".to_string()))?;
        let request = CapabilityRequest {
            request_id: "host-capability-policy".to_string(),
            capability,
            mode,
            payload: Vec::new(),
            idempotency_key: idempotency_key.map(str::to_string),
            trace: None,
        };
        let leases = self.leader_lease.lock().clone().into_iter();
        let leadership = StaticServiceLeadershipGuard::new(leases);
        let context = CapabilityEnforcementContext::new(&self.identity, &self.service_id, now_ms)
            .with_leadership(&leadership)
            .with_writes_allowed(*self.operation_mode.lock() == RuntimeOperationalMode::ReadWrite);
        self.catalog.authorize_local(&request, context)
    }

    pub(crate) fn authorize_runtime_command(
        &self,
        command_name: &str,
        idempotency_key: Option<&str>,
        now_ms: u64,
    ) -> RuntimeResult<()> {
        self.authorize(
            command_name,
            CapabilityMode::Command,
            idempotency_key,
            now_ms,
        )
        .map_err(|error| runtime_policy_error(error, command_name, "command"))
    }

    pub(crate) fn authorize_runtime_query(
        &self,
        query_name: &str,
        now_ms: u64,
    ) -> RuntimeResult<()> {
        self.authorize(query_name, CapabilityMode::Query, None, now_ms)
            .map_err(|error| runtime_policy_error(error, query_name, "query"))
    }
}

impl CommandCapabilityPolicy for RuntimeCapabilityPolicy {
    fn authorize_command(
        &self,
        command_name: &str,
        idempotency_key: Option<&str>,
        now_ms: u64,
    ) -> Result<(), CommandCapabilityPolicyError> {
        self.authorize(
            command_name,
            CapabilityMode::Command,
            idempotency_key,
            now_ms,
        )
        .map_err(http_policy_error)
    }

    fn authorize_query(
        &self,
        query_name: &str,
        now_ms: u64,
    ) -> Result<(), CommandCapabilityPolicyError> {
        self.authorize(query_name, CapabilityMode::Query, None, now_ms)
            .map_err(http_policy_error)
    }
}

pub(crate) fn http_policy_error(error: CapabilityError) -> CommandCapabilityPolicyError {
    match error {
        CapabilityError::CapabilityNotDeclared(_) | CapabilityError::ProviderUnavailable(_) => {
            CommandCapabilityPolicyError::CapabilityNotDeclared
        }
        CapabilityError::HandlerRejected(reason) if reason == "missing_idempotency_key" => {
            CommandCapabilityPolicyError::MissingIdempotencyKey
        }
        CapabilityError::RequiresLeader(_) => CommandCapabilityPolicyError::RequiresLeader,
        CapabilityError::LeaseExpired(_) => CommandCapabilityPolicyError::LeaseExpired,
        CapabilityError::StaleEpoch(_) => CommandCapabilityPolicyError::StaleEpoch,
        CapabilityError::WritesDisabled(_) => CommandCapabilityPolicyError::ReadOnly,
        CapabilityError::HandlerRejected(reason) => CommandCapabilityPolicyError::Rejected(reason),
        _ => CommandCapabilityPolicyError::Rejected("capability_policy_rejected".to_string()),
    }
}

fn runtime_policy_error(
    error: CapabilityError,
    capability: &str,
    request_kind: &'static str,
) -> RuntimeError {
    match error {
        CapabilityError::CapabilityNotDeclared(_) | CapabilityError::ProviderUnavailable(_) => {
            RuntimeError::MissingCapabilityNamed {
                capability: capability.to_string(),
            }
        }
        CapabilityError::HandlerRejected(reason) => RuntimeError::InvalidRequest {
            kind: request_kind,
            reason: stable_rejection_reason(&reason),
        },
        CapabilityError::WritesDisabled(_)
        | CapabilityError::RequiresLeader(_)
        | CapabilityError::LeaseExpired(_)
        | CapabilityError::StaleEpoch(_) => RuntimeError::Forbidden,
        other => RuntimeError::RegistryError(format!("capability policy failed: {other:?}")),
    }
}

fn stable_rejection_reason(reason: &str) -> &'static str {
    match reason {
        "capability_mode_mismatch" => "capability_mode_mismatch",
        "read_only_capability" => "read_only_capability",
        "missing_idempotency_key" => "missing_idempotency_key",
        "invalid_capability_name" => "invalid_capability_name",
        _ => "capability_rejected",
    }
}

#[cfg(test)]
#[path = "capability_policy_tests.rs"]
mod tests;
