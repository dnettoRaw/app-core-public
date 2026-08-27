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

/// Tracks which workers advertise which capabilities within a tenant partition.
#[derive(Debug, Default, Clone)]
pub struct CapabilityRegistry {
    capability_to_workers: HashMap<CapabilityName, HashSet<WorkerConnectionKey>>,
    worker_to_capabilities: HashMap<WorkerConnectionKey, HashSet<CapabilityName>>,
}

impl CapabilityRegistry {
    /// Creates an empty capability registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers capabilities for a specific worker.
    pub fn register(&mut self, worker: WorkerConnectionKey, capabilities: Vec<CapabilityName>) {
        self.deregister(&worker);
        let mut caps_set = HashSet::new();
        for cap in capabilities {
            self.capability_to_workers
                .entry(cap.clone())
                .or_default()
                .insert(worker.clone());
            caps_set.insert(cap);
        }
        self.worker_to_capabilities.insert(worker, caps_set);
    }

    /// Deregisters all capabilities associated with a specific worker connection.
    pub fn deregister(&mut self, worker: &WorkerConnectionKey) {
        if let Some(caps) = self.worker_to_capabilities.remove(worker) {
            for cap in caps {
                if let Some(workers) = self.capability_to_workers.get_mut(&cap) {
                    workers.remove(worker);
                    if workers.is_empty() {
                        self.capability_to_workers.remove(&cap);
                    }
                }
            }
        }
    }

    /// Returns all workers advertising a specific capability.
    pub fn resolve(&self, capability: &CapabilityName) -> Option<&HashSet<WorkerConnectionKey>> {
        self.capability_to_workers.get(capability)
    }

    /// Returns all capabilities currently advertised by any worker.
    pub fn all_capabilities(&self) -> Vec<CapabilityName> {
        self.capability_to_workers.keys().cloned().collect()
    }

    /// Returns one worker's advertised capabilities in stable identity order.
    pub fn capabilities_for(&self, worker: &WorkerConnectionKey) -> Vec<CapabilityName> {
        let mut capabilities = self
            .worker_to_capabilities
            .get(worker)
            .into_iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        capabilities.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        capabilities
    }
}
