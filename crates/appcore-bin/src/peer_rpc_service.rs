// =============================================================================
//        #######
//     ###       ###     F: peer_rpc_service.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/20 23:03:47 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 11:51:10 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Peer RPC host wiring for appcore-bin.

use crate::bootstrap::BootstrapError;
use crate::capability_policy::RuntimeCapabilityPolicy;
use crate::server::RuntimeServer;
use appcore_api::{ApiMethod, ApiRequest, ApiRouter, QueryName};
use appcore_capabilities::CapabilityError;
use appcore_core::{
    AppFamily, AppId, CapabilityMode, CommandEnvelope, CommandName, DistributedCoreManifest,
    NodeId, RuntimeContext, RuntimeContractVersion, RuntimeController, RuntimeError,
    RuntimeIdentity, SyncGroup, TraceContext,
};
use appcore_peer_rpc::{
    FilePeerNonceStore, HashTokenPeerAuthenticator, PeerRpcDispatcher, PeerRpcEnvelope,
    PeerRpcError, PeerRpcHttpHost, PeerRpcResponse, PeerRpcValidationConfig, PeerRpcValidator,
};
use appcore_security::TokenClaims;
use appcore_supervisor::ManagedService;
use parking_lot::Mutex;
use std::sync::Arc;

#[derive(Debug, Clone)]
struct PeerRuntimeContext {
    app_id: AppId,
    app_family: AppFamily,
    sync_group: SyncGroup,
    runtime_contract: RuntimeContractVersion,
    node_id: NodeId,
}

struct RuntimePeerDispatcher {
    controller: Arc<Mutex<RuntimeController>>,
    manifest: DistributedCoreManifest,
    operation_mode: Arc<Mutex<appcore_core::RuntimeOperationalMode>>,
    capability_policy: Arc<RuntimeCapabilityPolicy>,
    app_query_router: Option<Arc<Mutex<ApiRouter>>>,
    max_payload_bytes: usize,
}

impl RuntimeContext for PeerRuntimeContext {
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

impl PeerRuntimeContext {
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

impl PeerRpcDispatcher for RuntimePeerDispatcher {
    fn dispatch_peer_query(
        &self,
        envelope: PeerRpcEnvelope,
    ) -> Result<PeerRpcResponse, PeerRpcError> {
        match envelope.capability.as_str() {
            "runtime.status" => self.runtime_status(envelope),
            "runtime.manifest" => self.runtime_manifest(envelope),
            _ => self.dispatch_application_query(envelope),
        }
    }

    fn dispatch_peer_command(
        &self,
        envelope: PeerRpcEnvelope,
    ) -> Result<PeerRpcResponse, PeerRpcError> {
        if envelope.payload.len() > self.max_payload_bytes {
            return Err(PeerRpcError::PayloadTooLarge);
        }
        self.capability_policy
            .authorize(
                envelope.capability.as_str(),
                CapabilityMode::Command,
                envelope.idempotency_key.as_deref(),
                crate::bootstrap::now_ms(),
            )
            .map_err(map_capability_error)?;

        let (command, context, instance, pre_dispatch_outcome) = {
            let guard = self.controller.lock();
            let identity = guard.instance().identity().clone();
            let mut command = CommandEnvelope::new(
                CommandName::new(envelope.capability.as_str())
                    .map_err(|error| PeerRpcError::InvalidEnvelope(format!("{error:?}")))?,
                envelope.request_id.clone(),
                identity.app_id.clone(),
                identity.node_id.clone(),
                envelope.timestamp_ms,
                envelope.idempotency_key.clone(),
                envelope.payload.clone(),
            )
            .map_err(|error| PeerRpcError::InvalidEnvelope(format!("{error:?}")))?;
            if let Some(trace) = trace_from_envelope(&envelope) {
                command = command.with_trace(trace);
            }
            let context = PeerRuntimeContext::from_identity(&identity);
            let instance = guard.instance_arc();
            let outcome = guard
                .pre_dispatch(&command)
                .map_err(|error| PeerRpcError::InvalidEnvelope(format!("{error:?}")))?;
            (command, context, instance, outcome)
        };

        if let Some(outcome) = pre_dispatch_outcome {
            let result =
                outcome.map_err(|error| PeerRpcError::InvalidEnvelope(format!("{error:?}")))?;
            return command_result_response(command.command_id, result);
        }

        let dispatch_result = instance.dispatch_command(&command, &context);

        {
            let guard = self.controller.lock();
            guard
                .post_dispatch(&command, &dispatch_result)
                .map_err(|error| PeerRpcError::InvalidEnvelope(format!("{error:?}")))?;
        }

        let result =
            dispatch_result.map_err(|error| PeerRpcError::InvalidEnvelope(format!("{error:?}")))?;
        command_result_response(command.command_id, result)
    }
}

fn command_result_response(
    command_id: String,
    result: appcore_core::CommandResult,
) -> Result<PeerRpcResponse, PeerRpcError> {
    let payload = serde_json::to_vec(&serde_json::json!({
        "accepted": result.is_accepted(),
        "message": result.message(),
        "events": result.events().iter().map(|event| {
            serde_json::json!({
                "event_name": event.event_name.as_str(),
                "event_id": event.event_id,
                "trace_id": event.trace.as_ref().map(|trace| trace.trace_id.clone())
            })
        }).collect::<Vec<_>>()
    }))
    .map_err(|error| PeerRpcError::InvalidEnvelope(error.to_string()))?;
    Ok(PeerRpcResponse {
        ok: result.is_accepted(),
        request_id: command_id,
        payload,
        error: result.message().map(str::to_string),
    })
}

impl RuntimePeerDispatcher {
    fn runtime_status(&self, envelope: PeerRpcEnvelope) -> Result<PeerRpcResponse, PeerRpcError> {
        let lifecycle = self.controller.lock().lifecycle().current();
        let payload = serde_json::to_vec(&serde_json::json!({
            "app_id": self.manifest.identity.runtime.app_id.as_str(),
            "node_id": self.manifest.identity.runtime.node_id.as_str(),
            "tenant_id": self.manifest.identity.tenant_id.as_str(),
            "cluster_id": self.manifest.identity.cluster_id.as_str(),
            "core_id": self.manifest.identity.core_id.as_str(),
            "operation_mode": self.operation_mode.lock().as_str(),
            "lifecycle": format!("{lifecycle:?}")
        }))
        .map_err(|error| PeerRpcError::InvalidEnvelope(error.to_string()))?;
        Ok(PeerRpcResponse::ok(envelope.request_id, payload))
    }

    fn runtime_manifest(&self, envelope: PeerRpcEnvelope) -> Result<PeerRpcResponse, PeerRpcError> {
        let payload = serde_json::to_vec(&self.manifest)
            .map_err(|error| PeerRpcError::InvalidEnvelope(error.to_string()))?;
        Ok(PeerRpcResponse::ok(envelope.request_id, payload))
    }

    fn dispatch_application_query(
        &self,
        envelope: PeerRpcEnvelope,
    ) -> Result<PeerRpcResponse, PeerRpcError> {
        if envelope.payload.len() > self.max_payload_bytes {
            return Err(PeerRpcError::PayloadTooLarge);
        }
        self.capability_policy
            .authorize(
                envelope.capability.as_str(),
                CapabilityMode::Query,
                None,
                crate::bootstrap::now_ms(),
            )
            .map_err(map_capability_error)?;
        let Some(router) = &self.app_query_router else {
            return Ok(PeerRpcResponse::rejected(
                envelope.request_id,
                "query_not_available",
            ));
        };
        let name = QueryName::new(envelope.capability.as_str().to_string())
            .map_err(|error| PeerRpcError::InvalidEnvelope(format!("{error:?}")))?;
        let response = router
            .lock()
            .dispatch_query(
                &name,
                ApiRequest {
                    method: ApiMethod::Query,
                    path: envelope.capability.as_str().to_string(),
                    payload: envelope.payload,
                },
            )
            .map_err(map_query_dispatch_error)?;
        Ok(PeerRpcResponse {
            ok: (200..300).contains(&response.status_code),
            request_id: envelope.request_id,
            payload: response.payload,
            error: (response.status_code >= 300).then(|| "query_rejected".to_string()),
        })
    }
}

fn map_capability_error(error: CapabilityError) -> PeerRpcError {
    match error {
        CapabilityError::CapabilityNotDeclared(_)
        | CapabilityError::ProviderUnavailable(_)
        | CapabilityError::WritesDisabled(_) => PeerRpcError::Forbidden,
        CapabilityError::RequiresLeader(_)
        | CapabilityError::LeaseExpired(_)
        | CapabilityError::StaleEpoch(_) => PeerRpcError::EndpointUnavailable,
        CapabilityError::HandlerRejected(reason) => PeerRpcError::InvalidEnvelope(reason),
        _ => PeerRpcError::InvalidEnvelope("capability_policy_rejected".to_string()),
    }
}

fn map_query_dispatch_error(error: RuntimeError) -> PeerRpcError {
    match error {
        RuntimeError::RegistryItemNotFound { .. } => PeerRpcError::Forbidden,
        _ => PeerRpcError::InvalidEnvelope("query_dispatch_failed".to_string()),
    }
}

pub(super) fn peer_rpc_service_if_enabled(
    server: &RuntimeServer,
) -> Result<Option<Arc<dyn ManagedService>>, BootstrapError> {
    if !server.app.config.peer_rpc_enabled {
        return Ok(None);
    }
    let claims = TokenClaims {
        issuer: server.app.config.token_issuer.clone(),
        audience: server.app.config.token_audience.clone(),
        salt: "peer".to_string(),
        ttl_ms: 60_000,
    };
    let authenticator = Arc::new(HashTokenPeerAuthenticator::new(
        server.app.security_provider.clone(),
        claims,
    ));
    let nonce_store = FilePeerNonceStore::open(
        std::path::PathBuf::from(&server.app.config.storage_path)
            .join("security/peer-rpc-nonces.json"),
    )
    .map_err(|error| {
        BootstrapError::Runtime(format!(
            "peer RPC nonce store initialization failed: {error}"
        ))
    })?;
    let validator = PeerRpcValidator::new(PeerRpcValidationConfig {
        local_tenant_id: server.app.core_identity.tenant_id.clone(),
        local_cluster_id: server.app.core_identity.cluster_id.clone(),
        local_core_id: server.app.core_identity.core_id.clone(),
        max_payload_bytes: server.app.config.api_max_payload_bytes,
        nonce_window_ms: 60_000,
    })
    .with_protocol_version(server.app.core_identity.protocol_version)
    .with_nonce_store(Arc::new(nonce_store));
    let dispatcher = Arc::new(RuntimePeerDispatcher {
        controller: Arc::clone(&server.app.controller),
        manifest: server.app.core_manifest.clone(),
        operation_mode: Arc::clone(&server.app.operation_mode),
        capability_policy: Arc::clone(&server.app.capability_policy),
        app_query_router: server.app.app_query_router.clone(),
        max_payload_bytes: server.app.config.api_max_payload_bytes,
    });
    let host = Arc::new(PeerRpcHttpHost::new(
        server.app.config.peer_rpc_bind_host.clone(),
        server.app.config.peer_rpc_bind_port,
        server.app.core_manifest.clone(),
        validator,
        dispatcher,
        authenticator,
    ));
    let descriptor = crate::runtime_services::service_descriptor(
        crate::runtime_services::PEER_RPC_SERVICE,
        appcore_supervisor::ManagedResource::PeerRpc,
        &[crate::runtime_services::SECURITY_SERVICE],
    )?;
    Ok(Some(Arc::new(
        appcore_supervisor::ManagedThreadService::new(descriptor, move |shutdown| {
            let host = Arc::clone(&host);
            std::thread::Builder::new()
                .name("appcore-peer-rpc".to_string())
                .spawn(move || {
                    host.run_until_shutdown(shutdown)
                        .map_err(|error| format!("peer rpc failed: {error}"))
                })
                .map_err(|error| error.to_string())
        }),
    )))
}

fn trace_from_envelope(envelope: &PeerRpcEnvelope) -> Option<TraceContext> {
    envelope
        .trace
        .as_ref()
        .and_then(|trace| {
            trace
                .child_span(
                    format!("{}-peer", envelope.request_id),
                    envelope.target_core_id.clone(),
                )
                .ok()
        })
        .or_else(|| {
            TraceContext::new(
                envelope.trace_id.clone(),
                envelope.request_id.clone(),
                envelope.source_core_id.clone(),
                envelope.target_core_id.clone(),
                envelope.tenant_id.clone(),
            )
            .ok()
            .and_then(|trace| trace.with_command_id(envelope.request_id.clone()).ok())
        })
}
