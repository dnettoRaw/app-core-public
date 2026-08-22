// =============================================================================
//        #######
//     ###       ###     F: runtime_health.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{BackendRegistrySnapshot, ExecutionQueueSnapshot, ModelRegistrySnapshot};

/// Aggregate component health used by composition and supervisor adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AiRuntimeHealth {
    /// Backend adapter state.
    pub backends: BackendRegistrySnapshot,
    /// Model registry state.
    pub models: ModelRegistrySnapshot,
    /// Execution admission state.
    pub execution: ExecutionQueueSnapshot,
}

impl AiRuntimeHealth {
    /// Reports whether at least one non-unavailable backend and routable model exist.
    #[must_use]
    pub fn is_available(self) -> bool {
        self.backends.healthy.saturating_add(self.backends.degraded) > 0
            && self.models.available.saturating_add(self.models.ready) > 0
    }
}

impl crate::AiRuntime {
    /// Returns bounded model-load single-flight counters and gauges.
    #[must_use]
    pub fn model_loads(&self) -> crate::ModelLoadSnapshot {
        self.model_loads.snapshot()
    }
}
