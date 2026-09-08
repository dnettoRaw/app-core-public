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
use std::sync::Arc;

/// Maximum UTF-8 bytes accepted in one process-local metric name.
pub const MAX_METRIC_NAME_BYTES: usize = 128;
/// Maximum distinct counters retained by one process-local registry.
pub const MAX_IN_MEMORY_METRICS: usize = 4_096;
/// Absolute aggregate retained-byte ceiling for process-local metric names.
pub const MAX_IN_MEMORY_METRIC_BYTES: usize = 1024 * 1024;
const METRIC_FIXED_BYTES: usize =
    std::mem::size_of::<(Arc<str>, u64)>() + std::mem::size_of::<usize>() * 4;

/// Named monotonic counter snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricCounter {
    /// Stable metric name.
    pub name: String,
    /// Current counter value.
    pub value: u64,
}

/// Immutable shared view of sorted process-local counters.
#[derive(Debug, Clone, Default)]
pub struct MetricSnapshot {
    counters: Arc<BTreeMap<Arc<str>, u64>>,
}

impl MetricSnapshot {
    /// Returns the number of counters in the snapshot.
    pub fn len(&self) -> usize {
        self.counters.len()
    }

    /// Reports whether the snapshot contains no counters.
    pub fn is_empty(&self) -> bool {
        self.counters.is_empty()
    }

    /// Iterates over sorted borrowed names and their values.
    pub fn iter(&self) -> impl Iterator<Item = (&str, u64)> {
        self.counters
            .iter()
            .map(|(name, value)| (name.as_ref(), *value))
    }
}

/// Point-in-time capacity and retained-memory pressure for metric names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricRegistryPressure {
    /// Current distinct counter count.
    pub entries: usize,
    /// Configured distinct counter ceiling.
    pub max_entries: usize,
    /// Estimated bytes retained by counter keys and map nodes.
    pub used_bytes: usize,
    /// Highest estimated retained byte count observed since creation.
    pub peak_bytes: usize,
    /// Configured aggregate retained-byte ceiling.
    pub max_bytes: usize,
    /// New names rejected because the counter ceiling was full.
    pub count_rejections: u64,
    /// New names rejected because the byte ceiling was full.
    pub byte_rejections: u64,
    /// Empty, oversized or NUL-containing names rejected at admission.
    pub name_rejections: u64,
}

#[derive(Debug)]
struct MetricState {
    counters: Arc<BTreeMap<Arc<str>, u64>>,
    pressure: MetricRegistryPressure,
}

/// Process-local monotonic counter registry.
#[derive(Debug)]
pub struct InMemoryMetrics {
    state: Mutex<MetricState>,
}

impl InMemoryMetrics {
    /// Creates an empty counter registry.
    pub fn new() -> Self {
        Self::with_limits(MAX_IN_MEMORY_METRICS, MAX_IN_MEMORY_METRIC_BYTES)
    }

    /// Creates a registry under explicit limits clamped to public safety ceilings.
    pub fn with_limits(max_entries: usize, max_bytes: usize) -> Self {
        let max_entries = max_entries.clamp(1, MAX_IN_MEMORY_METRICS);
        let max_bytes = max_bytes.clamp(1, MAX_IN_MEMORY_METRIC_BYTES);
        Self {
            state: Mutex::new(MetricState {
                counters: Arc::new(BTreeMap::new()),
                pressure: MetricRegistryPressure {
                    max_entries,
                    max_bytes,
                    ..MetricRegistryPressure::default()
                },
            }),
        }
    }

    /// Saturating-increments a named counter and returns its new value.
    ///
    /// Invalid or over-capacity new names return zero and increment the
    /// corresponding [`Self::pressure`] rejection counter. Existing admitted
    /// counters can always be incremented.
    pub fn increment(&self, name: &str) -> u64 {
        self.try_increment(name).unwrap_or(0)
    }

    /// Attempts to increment a counter under the configured admission bounds.
    pub fn try_increment(&self, name: &str) -> Option<u64> {
        let mut state = self.state.lock();
        if !metric_name_is_valid(name) {
            state.pressure.name_rejections = state.pressure.name_rejections.saturating_add(1);
            return None;
        }
        if state.counters.contains_key(name) {
            if let Some(value) = Arc::make_mut(&mut state.counters).get_mut(name) {
                *value = value.saturating_add(1);
                return Some(*value);
            }
            return None;
        }
        if state.pressure.entries >= state.pressure.max_entries {
            state.pressure.count_rejections = state.pressure.count_rejections.saturating_add(1);
            return None;
        }
        let retained_bytes = metric_retained_bytes(name);
        if state.pressure.used_bytes.saturating_add(retained_bytes) > state.pressure.max_bytes {
            state.pressure.byte_rejections = state.pressure.byte_rejections.saturating_add(1);
            return None;
        }
        Arc::make_mut(&mut state.counters).insert(Arc::from(name), 1);
        state.pressure.entries = state.pressure.entries.saturating_add(1);
        state.pressure.used_bytes = state.pressure.used_bytes.saturating_add(retained_bytes);
        state.pressure.peak_bytes = state.pressure.peak_bytes.max(state.pressure.used_bytes);
        Some(1)
    }

    /// Returns counters ordered by name.
    pub fn snapshot(&self) -> Vec<MetricCounter> {
        self.shared_snapshot()
            .iter()
            .map(|(name, value)| MetricCounter {
                name: name.to_string(),
                value,
            })
            .collect()
    }

    /// Returns an immutable sorted snapshot without cloning counter names.
    ///
    /// Retaining this view across an update makes that update clone the map
    /// nodes. Each retained generation remains alive until its last snapshot
    /// clone is dropped. Consumers must bound retained generations; registry
    /// pressure reports the current generation, not all consumer-owned views.
    pub fn shared_snapshot(&self) -> MetricSnapshot {
        MetricSnapshot {
            counters: Arc::clone(&self.state.lock().counters),
        }
    }

    /// Returns current counter cardinality and retained-memory pressure.
    pub fn pressure(&self) -> MetricRegistryPressure {
        self.state.lock().pressure
    }
}

impl Default for InMemoryMetrics {
    fn default() -> Self {
        Self::new()
    }
}

fn metric_name_is_valid(name: &str) -> bool {
    !name.is_empty() && name.len() <= MAX_METRIC_NAME_BYTES && !name.contains('\0')
}

fn metric_retained_bytes(name: &str) -> usize {
    METRIC_FIXED_BYTES.saturating_add(name.len())
}

#[cfg(test)]
#[path = "metrics_tests.rs"]
mod tests;
