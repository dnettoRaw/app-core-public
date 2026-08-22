// =============================================================================
//        #######
//     ###       ###     F: residency_metrics.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

/// Low-cardinality model-residency counters and current gauges.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResidencyMetrics {
    /// Hot-model reuse decisions.
    pub reuses: u64,
    /// Requests that joined an existing in-flight load.
    pub in_flight: u64,
    /// Successful two-phase reservations.
    pub reservations: u64,
    /// Failed or cancelled loads rolled back without eviction.
    pub rollbacks: u64,
    /// Residents evicted after a successful replacement load.
    pub evictions: u64,
    /// Current resident records across every tier.
    pub residents: usize,
    /// Current resident bytes across every tier.
    pub resident_bytes: u64,
    /// Current in-flight reservations.
    pub pending: usize,
    /// Current speculative prefetch reservations.
    pub active_prefetch: usize,
}

impl crate::ResidencyPlanner {
    /// Returns bounded counters and current aggregate residency gauges.
    pub fn metrics(&self) -> crate::AiResult<ResidencyMetrics> {
        let state = self
            .state
            .lock()
            .map_err(|_| crate::AiError::InternalState)?;
        let mut metrics = state.metrics;
        metrics.residents = state.residents.len();
        metrics.resident_bytes = state
            .residents
            .values()
            .fold(0u64, |sum, record| sum.saturating_add(record.size_bytes));
        metrics.pending = state.pending.len();
        metrics.active_prefetch = state.active_prefetch;
        Ok(metrics)
    }
}
