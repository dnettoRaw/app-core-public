// =============================================================================
//        #######
//     ###       ###     F: selection.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/22 15:41:18 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use appcore_core::{CapabilityDescriptor, CoreId};
use appcore_distributed_contracts::PeerRecord;

/// A local or discovered remote provider selected for a capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityProvider {
    /// Provider hosted by the current runtime process.
    Local {
        /// Identity of the local core.
        core_id: CoreId,
        /// Capability contract advertised by the handler.
        descriptor: CapabilityDescriptor,
    },
    /// Provider advertised by a discovered peer.
    Remote {
        /// Peer identity, endpoints, and advertised capabilities.
        peer: Box<PeerRecord>,
        /// Capability contract advertised by the peer.
        descriptor: CapabilityDescriptor,
        /// Whether discovery metadata marks this peer as preferred.
        preferred: bool,
    },
}

impl CapabilityProvider {
    /// Returns the selected provider's capability descriptor.
    pub fn descriptor(&self) -> &CapabilityDescriptor {
        match self {
            Self::Local { descriptor, .. } | Self::Remote { descriptor, .. } => descriptor,
        }
    }

    /// Returns the selected provider's core identity.
    pub fn core_id(&self) -> &CoreId {
        match self {
            Self::Local { core_id, .. } => core_id,
            Self::Remote { peer, .. } => &peer.identity.core_id,
        }
    }

    /// Returns `true` when the provider must be invoked through peer RPC.
    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Remote { .. })
    }
}

/// Basic locality policy used by the default provider selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolutionPolicy {
    /// Prefer a healthy local provider over remote candidates.
    pub prefer_local: bool,
    /// Permit discovered remote providers to be selected.
    pub allow_remote: bool,
}

impl Default for ResolutionPolicy {
    fn default() -> Self {
        Self {
            prefer_local: true,
            allow_remote: true,
        }
    }
}

/// Pluggable policy for selecting one provider from compatible candidates.
pub trait CapabilitySelectionPolicy: Send + Sync {
    /// Selects a provider or returns `None` when no candidate is acceptable.
    fn select(&self, candidates: &[CapabilityProvider]) -> Option<CapabilityProvider>;
}

/// Deterministic selector that applies locality and discovery preference flags.
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultCapabilitySelectionPolicy {
    /// Locality constraints applied during selection.
    pub policy: ResolutionPolicy,
}

impl CapabilitySelectionPolicy for DefaultCapabilitySelectionPolicy {
    fn select(&self, candidates: &[CapabilityProvider]) -> Option<CapabilityProvider> {
        if self.policy.prefer_local {
            if let Some(local) = candidates
                .iter()
                .find(|candidate| matches!(candidate, CapabilityProvider::Local { .. }))
            {
                return Some(local.clone());
            }
        }

        if self.policy.allow_remote {
            if let Some(remote) = candidates.iter().find(|candidate| {
                matches!(
                    candidate,
                    CapabilityProvider::Remote {
                        preferred: true,
                        ..
                    }
                )
            }) {
                return Some(remote.clone());
            }

            if let Some(remote) = candidates
                .iter()
                .find(|candidate| matches!(candidate, CapabilityProvider::Remote { .. }))
            {
                return Some(remote.clone());
            }
        }

        candidates
            .iter()
            .find(|candidate| matches!(candidate, CapabilityProvider::Local { .. }))
            .cloned()
    }
}
