// =============================================================================
//        #######
//     ###       ###     F: resolver.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Defines bounded resolver contracts and behavior for this crate.

use crate::policy::enforce_requirements;
use crate::{
    CapabilityError, CapabilityProvider, CapabilityRegistry, CapabilityRequest, CapabilityResponse,
    CapabilityResult, CapabilitySelectionPolicy, LocalCapabilityProvider, RemoteCapabilityInvoker,
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
    selector: Option<Arc<dyn CapabilitySelectionPolicy>>,
}

enum ExecutionProvider<'a> {
    Local(&'a LocalCapabilityProvider),
    Remote {
        peer: &'a PeerRecord,
        descriptor: &'a CapabilityDescriptor,
    },
    Selected(CapabilityProvider),
}

impl ExecutionProvider<'_> {
    fn descriptor(&self) -> &CapabilityDescriptor {
        match self {
            Self::Local(provider) => provider.descriptor(),
            Self::Remote { descriptor, .. } => descriptor,
            Self::Selected(provider) => provider.descriptor(),
        }
    }

    fn core_id<'a>(&'a self, identity: &'a CoreIdentity) -> &'a appcore_core::CoreId {
        match self {
            Self::Local(_) => &identity.core_id,
            Self::Remote { peer, .. } => &peer.identity.core_id,
            Self::Selected(provider) => provider.core_id(),
        }
    }
}

impl CapabilityResolver {
    /// Creates a resolver backed by a local registry.
    pub fn new(registry: CapabilityRegistry) -> Self {
        Self {
            registry,
            peers: Vec::new(),
            selector: None,
        }
    }

    /// Adds the current discovery snapshot used for remote resolution.
    pub fn with_peers(mut self, peers: Vec<PeerRecord>) -> Self {
        self.peers = peers;
        self
    }

    /// Replaces the default provider selection policy.
    pub fn with_selector(mut self, selector: Arc<dyn CapabilitySelectionPolicy>) -> Self {
        self.selector = Some(selector);
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
        let provider = match &self.selector {
            Some(selector) => selector.select(&self.candidates(identity, request)),
            None => self.select_default(identity, request),
        };
        let Some(provider) = provider else {
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
        let provider =
            self.resolve_for_execution(identity, service_id, request, leadership, now_ms)?;
        match provider {
            ExecutionProvider::Local(local) => local.handle(request),
            ExecutionProvider::Remote { peer, .. } => Ok(CapabilityResponse::accepted(
                Vec::new(),
                Some(peer.identity.core_id.clone()),
            )),
            ExecutionProvider::Selected(CapabilityProvider::Local { .. }) => {
                let Some(local) = self.registry.get(&request.capability) else {
                    return Err(CapabilityError::HandlerNotFound(request.capability.clone()));
                };
                local.handle(request)
            }
            ExecutionProvider::Selected(CapabilityProvider::Remote { peer, .. }) => Ok(
                CapabilityResponse::accepted(Vec::new(), Some(peer.identity.core_id.clone())),
            ),
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
        let provider =
            self.resolve_for_execution(identity, service_id, request, leadership, now_ms)?;
        match provider {
            ExecutionProvider::Local(local) => local.handle(request),
            ExecutionProvider::Remote { peer, .. } => {
                let Some(invoker) = remote_invoker else {
                    return Err(CapabilityError::RemoteEndpointUnavailable(
                        request.capability.clone(),
                    ));
                };
                invoker.invoke_remote(peer, request)
            }
            ExecutionProvider::Selected(CapabilityProvider::Local { .. }) => {
                let Some(local) = self.registry.get(&request.capability) else {
                    return Err(CapabilityError::HandlerNotFound(request.capability.clone()));
                };
                local.handle(request)
            }
            ExecutionProvider::Selected(CapabilityProvider::Remote { peer, .. }) => {
                let Some(invoker) = remote_invoker else {
                    return Err(CapabilityError::RemoteEndpointUnavailable(
                        request.capability.clone(),
                    ));
                };
                invoker.invoke_remote(&peer, request)
            }
        }
    }

    /// Handles an owned local or remote request using service-scoped leadership.
    ///
    /// Local handlers retain their borrowed contract. A remote invoker may
    /// transfer the request fields directly into its transport.
    pub fn handle_owned(
        &self,
        identity: &CoreIdentity,
        service_id: &ServiceId,
        request: CapabilityRequest,
        leadership: Option<&dyn ServiceLeadershipGuard>,
        remote_invoker: Option<&dyn RemoteCapabilityInvoker>,
        now_ms: u64,
    ) -> CapabilityResult<CapabilityResponse> {
        let provider =
            self.resolve_for_execution(identity, service_id, &request, leadership, now_ms)?;
        match provider {
            ExecutionProvider::Local(local) => local.handle(&request),
            ExecutionProvider::Remote { peer, .. } => {
                let Some(invoker) = remote_invoker else {
                    return Err(CapabilityError::RemoteEndpointUnavailable(
                        request.capability.clone(),
                    ));
                };
                invoker.invoke_remote_owned(peer, request)
            }
            ExecutionProvider::Selected(CapabilityProvider::Local { .. }) => {
                let Some(local) = self.registry.get(&request.capability) else {
                    return Err(CapabilityError::HandlerNotFound(request.capability.clone()));
                };
                local.handle(&request)
            }
            ExecutionProvider::Selected(CapabilityProvider::Remote { peer, .. }) => {
                let Some(invoker) = remote_invoker else {
                    return Err(CapabilityError::RemoteEndpointUnavailable(
                        request.capability.clone(),
                    ));
                };
                invoker.invoke_remote_owned(&peer, request)
            }
        }
    }

    fn resolve_for_execution<'a>(
        &'a self,
        identity: &CoreIdentity,
        service_id: &ServiceId,
        request: &CapabilityRequest,
        leadership: Option<&dyn ServiceLeadershipGuard>,
        now_ms: u64,
    ) -> CapabilityResult<ExecutionProvider<'a>> {
        let provider = match &self.selector {
            Some(selector) => selector
                .select(&self.candidates(identity, request))
                .map(ExecutionProvider::Selected),
            None => self.select_default_for_execution(identity, request),
        };
        let Some(provider) = provider else {
            return Err(CapabilityError::ProviderUnavailable(
                request.capability.clone(),
            ));
        };
        enforce_requirements(
            identity,
            service_id,
            request,
            provider.descriptor(),
            provider.core_id(identity),
            leadership,
            true,
            now_ms,
        )?;
        Ok(provider)
    }

    fn select_default_for_execution<'a>(
        &'a self,
        identity: &CoreIdentity,
        request: &CapabilityRequest,
    ) -> Option<ExecutionProvider<'a>> {
        if let Some(local) = self.registry.get(&request.capability) {
            if local.is_healthy() && local.descriptor().mode == request.mode {
                return Some(ExecutionProvider::Local(local));
            }
        }

        let mut fallback = None;
        for peer in &self.peers {
            let Some((descriptor, preferred)) = compatible_remote(identity, request, peer) else {
                continue;
            };
            if preferred {
                return Some(ExecutionProvider::Remote { peer, descriptor });
            }
            fallback.get_or_insert((peer, descriptor));
        }
        fallback.map(|(peer, descriptor)| ExecutionProvider::Remote { peer, descriptor })
    }

    fn select_default(
        &self,
        identity: &CoreIdentity,
        request: &CapabilityRequest,
    ) -> Option<CapabilityProvider> {
        if let Some(local) = self.registry.get(&request.capability) {
            if local.is_healthy() && local.descriptor().mode == request.mode {
                return Some(CapabilityProvider::Local {
                    core_id: identity.core_id.clone(),
                    descriptor: local.descriptor().clone(),
                });
            }
        }

        let mut fallback = None;
        for peer in &self.peers {
            let Some((descriptor, preferred)) = compatible_remote(identity, request, peer) else {
                continue;
            };
            if preferred {
                return Some(remote_provider(peer, descriptor, true));
            }
            fallback.get_or_insert((peer, descriptor));
        }
        fallback.map(|(peer, descriptor)| remote_provider(peer, descriptor, false))
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
            if let Some((descriptor, preferred)) = compatible_remote(identity, request, peer) {
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

fn compatible_remote<'a>(
    identity: &CoreIdentity,
    request: &CapabilityRequest,
    peer: &'a PeerRecord,
) -> Option<(&'a CapabilityDescriptor, bool)> {
    if !peer.healthy {
        return None;
    }
    let descriptor = peer.capabilities.iter().find(|descriptor| {
        descriptor.name == request.capability && descriptor.mode == request.mode
    })?;
    if !remote_descriptor_is_compatible(identity, peer, descriptor) {
        return None;
    }
    let preferred = peer
        .metadata
        .get("preferred")
        .is_some_and(|value| value == "true");
    Some((descriptor, preferred))
}

fn remote_provider(
    peer: &PeerRecord,
    descriptor: &CapabilityDescriptor,
    preferred: bool,
) -> CapabilityProvider {
    CapabilityProvider::Remote {
        peer: Box::new(peer.clone()),
        descriptor: descriptor.clone(),
        preferred,
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
        required_capability: None,
    };
    identity
        .ensure_compatible(&peer.identity, &policy, &[])
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
