// =============================================================================
//        #######
//     ###       ###     F: resolver.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::policy::enforce_requirements;
use crate::{
    CapabilityError, CapabilityProvider, CapabilityRegistry, CapabilityRequest, CapabilityResponse,
    CapabilityResult, CapabilitySelectionPolicy, DefaultCapabilitySelectionPolicy,
    RemoteCapabilityInvoker,
};
use appcore_contracts::ServiceId;
use appcore_core::{
    CapabilityDescriptor, CapabilityRequirements, CoreCompatibilityPolicy, CoreIdentity,
};
use appcore_distributed_contracts::{PeerRecord, ServiceLeadershipGuard};
use std::sync::Arc;

/// Resolves local and discovered providers and enforces capability requirements.
pub struct CapabilityResolver {
    registry: CapabilityRegistry,
    peers: Vec<PeerRecord>,
    selector: Arc<dyn CapabilitySelectionPolicy>,
}

impl CapabilityResolver {
    /// Creates a resolver backed by a local registry.
    pub fn new(registry: CapabilityRegistry) -> Self {
        Self {
            registry,
            peers: Vec::new(),
            selector: Arc::new(DefaultCapabilitySelectionPolicy::default()),
        }
    }

    /// Adds the current discovery snapshot used for remote resolution.
    pub fn with_peers(mut self, peers: Vec<PeerRecord>) -> Self {
        self.peers = peers;
        self
    }

    /// Replaces the default provider selection policy.
    pub fn with_selector(mut self, selector: Arc<dyn CapabilitySelectionPolicy>) -> Self {
        self.selector = selector;
        self
    }

    /// Resolves a provider and checks leadership for the declared service.
    pub fn resolve(
        &self,
        identity: &CoreIdentity,
        service_id: &ServiceId,
        request: &CapabilityRequest,
        leadership: Option<&dyn ServiceLeadershipGuard>,
        now_ms: u64,
    ) -> CapabilityResult<CapabilityProvider> {
        let candidates = self.candidates(identity, request);
        let Some(provider) = self.selector.select(&candidates) else {
            return Err(CapabilityError::ProviderUnavailable(
                request.capability.clone(),
            ));
        };
        enforce_requirements(
            identity,
            service_id,
            request,
            provider.descriptor(),
            provider.core_id(),
            leadership,
            true,
            now_ms,
        )?;
        Ok(provider)
    }

    /// Handles a local capability using service-scoped leadership.
    pub fn handle_local(
        &self,
        identity: &CoreIdentity,
        service_id: &ServiceId,
        request: &CapabilityRequest,
        leadership: Option<&dyn ServiceLeadershipGuard>,
        now_ms: u64,
    ) -> CapabilityResult<CapabilityResponse> {
        let provider = self.resolve(identity, service_id, request, leadership, now_ms)?;
        match provider {
            CapabilityProvider::Local { .. } => {
                let Some(local) = self.registry.get(&request.capability) else {
                    return Err(CapabilityError::HandlerNotFound(request.capability.clone()));
                };
                local.handle(request)
            }
            CapabilityProvider::Remote { peer, .. } => Ok(CapabilityResponse::accepted(
                Vec::new(),
                Some(peer.identity.core_id.clone()),
            )),
        }
    }

    /// Handles a local or remote capability using service-scoped leadership.
    pub fn handle(
        &self,
        identity: &CoreIdentity,
        service_id: &ServiceId,
        request: &CapabilityRequest,
        leadership: Option<&dyn ServiceLeadershipGuard>,
        remote_invoker: Option<&dyn RemoteCapabilityInvoker>,
        now_ms: u64,
    ) -> CapabilityResult<CapabilityResponse> {
        let provider = self.resolve(identity, service_id, request, leadership, now_ms)?;
        match provider {
            CapabilityProvider::Local { .. } => {
                let Some(local) = self.registry.get(&request.capability) else {
                    return Err(CapabilityError::HandlerNotFound(request.capability.clone()));
                };
                local.handle(request)
            }
            CapabilityProvider::Remote { peer, .. } => {
                let Some(invoker) = remote_invoker else {
                    return Err(CapabilityError::RemoteEndpointUnavailable(
                        request.capability.clone(),
                    ));
                };
                invoker.invoke_remote(&peer, request)
            }
        }
    }

    fn candidates(
        &self,
        identity: &CoreIdentity,
        request: &CapabilityRequest,
    ) -> Vec<CapabilityProvider> {
        let mut candidates = Vec::new();
        if let Some(local) = self.registry.get(&request.capability) {
            if local.is_healthy() && local.descriptor().mode == request.mode {
                candidates.push(CapabilityProvider::Local {
                    core_id: identity.core_id.clone(),
                    descriptor: local.descriptor().clone(),
                });
            }
        }

        for peer in &self.peers {
            if !peer.healthy {
                continue;
            }
            if let Some(descriptor) = peer.capabilities.iter().find(|descriptor| {
                descriptor.name == request.capability && descriptor.mode == request.mode
            }) {
                if !remote_descriptor_is_compatible(identity, peer, descriptor) {
                    continue;
                }
                let preferred = peer
                    .metadata
                    .get("preferred")
                    .map(|value| value == "true")
                    .unwrap_or(false);
                candidates.push(CapabilityProvider::Remote {
                    peer: Box::new(peer.clone()),
                    descriptor: descriptor.clone(),
                    preferred,
                });
            }
        }
        candidates
    }
}

fn remote_descriptor_is_compatible(
    identity: &CoreIdentity,
    peer: &PeerRecord,
    descriptor: &CapabilityDescriptor,
) -> bool {
    let require_same_cluster = match descriptor.visibility {
        appcore_core::CapabilityVisibility::Local => return false,
        appcore_core::CapabilityVisibility::Cluster => true,
        appcore_core::CapabilityVisibility::Tenant => false,
    };
    let policy = CoreCompatibilityPolicy {
        require_same_cluster,
        required_capability: Some(descriptor.name.clone()),
    };
    identity
        .ensure_compatible(
            &peer.identity,
            &policy,
            &peer
                .capabilities
                .iter()
                .map(|capability| capability.name.clone())
                .collect::<Vec<_>>(),
        )
        .is_ok()
}

/// Returns the standard requirements for a side-effect-free capability.
pub fn requirements_for_read_only() -> CapabilityRequirements {
    CapabilityRequirements {
        requires_leader: false,
        read_only: true,
        idempotency_required: false,
    }
}
