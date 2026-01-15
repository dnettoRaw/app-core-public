// =============================================================================
//        #######
//     ###       ###     F: resolver.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/26 08:53:09 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/26 08:53:09 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Capability resolver to locate appropriate workers.

use crate::connection::WorkerConnectionKey;
use crate::registry::CapabilityRegistry;
use appcore_types::CapabilityName;

/// Strategy to choose one worker from multiple candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionPolicy {
    /// Always picks the first available candidate.
    FirstAvailable,
}

/// Resolves worker targets for capability requests within a tenant partition.
#[derive(Debug, Clone)]
pub struct CapabilityResolver {
    policy: SelectionPolicy,
}

impl Default for CapabilityResolver {
    fn default() -> Self {
        Self {
            policy: SelectionPolicy::FirstAvailable,
        }
    }
}

impl CapabilityResolver {
    /// Creates a resolver with the default selection policy.
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolves a worker connection key for a given capability using the registry.
    pub fn resolve(
        &self,
        capability: &CapabilityName,
        registry: &CapabilityRegistry,
    ) -> Option<WorkerConnectionKey> {
        let candidates = registry.resolve(capability)?;
        match self.policy {
            SelectionPolicy::FirstAvailable => candidates.iter().next().cloned(),
        }
    }
}
