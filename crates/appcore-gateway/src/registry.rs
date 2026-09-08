// =============================================================================
//        #######
//     ###       ###     F: registry.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/26 08:53:09 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:48:56 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Capability advertisement registry.

use crate::connection::WorkerConnectionKey;
use appcore_types::CapabilityName;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Payload-free accounting for one tenant's capability registry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CapabilityRegistryStats {
    /// Distinct capability names retained by the tenant.
    pub unique_capabilities: usize,
    /// Worker registrations retained by the reverse index.
    pub workers: usize,
    /// Worker-to-capability advertisements retained by the registry.
    pub advertisements: usize,
    /// UTF-8 bytes owned by the distinct interned capability names.
    pub capability_name_bytes: usize,
}

/// Tracks which workers advertise which capabilities within a tenant partition.
#[derive(Debug, Default, Clone)]
pub struct CapabilityRegistry {
    capability_to_workers: HashMap<CapabilityName, HashSet<WorkerConnectionKey>>,
    worker_to_capabilities: HashMap<WorkerConnectionKey, Arc<[Arc<CapabilityName>]>>,
}

impl CapabilityRegistry {
    /// Creates an empty capability registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers capabilities for a specific worker.
    pub fn register(&mut self, worker: WorkerConnectionKey, mut capabilities: Vec<CapabilityName>) {
        self.deregister(&worker);
        capabilities.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        capabilities.dedup_by(|left, right| left.as_str() == right.as_str());
        let mut shared = Vec::with_capacity(capabilities.len());
        for cap in capabilities {
            let shared_name = self
                .shared_name(&cap)
                .unwrap_or_else(|| Arc::new(cap.clone()));
            self.capability_to_workers
                .entry(cap)
                .or_default()
                .insert(worker.clone());
            shared.push(shared_name);
        }
        self.worker_to_capabilities.insert(worker, shared.into());
    }

    /// Deregisters all capabilities associated with a specific worker connection.
    pub fn deregister(&mut self, worker: &WorkerConnectionKey) {
        if let Some(caps) = self.worker_to_capabilities.remove(worker) {
            for cap in caps.iter() {
                if let Some(workers) = self.capability_to_workers.get_mut(cap.as_ref()) {
                    workers.remove(worker);
                    if workers.is_empty() {
                        self.capability_to_workers.remove(cap.as_ref());
                    }
                }
            }
        }
    }

    /// Returns all workers advertising a specific capability.
    #[inline]
    pub fn resolve(&self, capability: &CapabilityName) -> Option<&HashSet<WorkerConnectionKey>> {
        self.capability_to_workers.get(capability)
    }

    /// Returns all capabilities currently advertised by any worker.
    pub fn all_capabilities(&self) -> Vec<CapabilityName> {
        self.capability_to_workers.keys().cloned().collect()
    }

    /// Iterates over one worker's capabilities in stable identity order
    /// without cloning their names.
    pub fn capabilities_for_iter<'a>(
        &'a self,
        worker: &WorkerConnectionKey,
    ) -> impl ExactSizeIterator<Item = &'a CapabilityName> + 'a {
        self.worker_to_capabilities
            .get(worker)
            .map(Arc::as_ref)
            .unwrap_or(&[])
            .iter()
            .map(Arc::as_ref)
    }

    /// Returns one worker's advertised capabilities in stable identity order.
    pub fn capabilities_for(&self, worker: &WorkerConnectionKey) -> Vec<CapabilityName> {
        self.capabilities_for_iter(worker).cloned().collect()
    }

    /// Returns bounded, payload-free ownership accounting for this registry.
    pub fn stats(&self) -> CapabilityRegistryStats {
        CapabilityRegistryStats {
            unique_capabilities: self.capability_to_workers.len(),
            workers: self.worker_to_capabilities.len(),
            advertisements: self
                .worker_to_capabilities
                .values()
                .fold(0_usize, |total, capabilities| {
                    total.saturating_add(capabilities.len())
                }),
            capability_name_bytes: self
                .capability_to_workers
                .keys()
                .fold(0_usize, |total, capability| {
                    total.saturating_add(capability.as_str().len())
                }),
        }
    }

    fn shared_name(&self, capability: &CapabilityName) -> Option<Arc<CapabilityName>> {
        self.capability_to_workers
            .get(capability)?
            .iter()
            .filter_map(|worker| self.worker_to_capabilities.get(worker))
            .find_map(|capabilities| {
                capabilities
                    .binary_search_by(|candidate| candidate.as_str().cmp(capability.as_str()))
                    .ok()
                    .map(|index| Arc::clone(&capabilities[index]))
            })
    }
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
