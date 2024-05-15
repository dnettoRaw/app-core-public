// =============================================================================
//        #######
//     ###       ###     F: metrics.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 17:10:40 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Minimal in-memory counters for local runtime diagnostics.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Named monotonic counter snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricCounter {
    /// Stable metric name.
    pub name: String,
    /// Current counter value.
    pub value: u64,
}

/// Process-local monotonic counter registry.
#[derive(Debug, Default)]
pub struct InMemoryMetrics {
    counters: Mutex<BTreeMap<String, u64>>,
}

impl InMemoryMetrics {
    /// Creates an empty counter registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Saturating-increments a named counter and returns its new value.
    pub fn increment(&self, name: &str) -> u64 {
        let mut counters = self.counters.lock();
        let value = counters.entry(name.to_string()).or_insert(0);
        *value = value.saturating_add(1);
        *value
    }

    /// Returns counters ordered by name.
    pub fn snapshot(&self) -> Vec<MetricCounter> {
        let counters = self.counters.lock();
        counters
            .iter()
            .map(|(name, value)| MetricCounter {
                name: name.clone(),
                value: *value,
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "metrics_tests.rs"]
mod tests;
