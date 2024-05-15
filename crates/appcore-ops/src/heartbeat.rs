// =============================================================================
//        #######
//     ###       ###     F: heartbeat.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Heartbeat contracts for runtime liveness signals.

use appcore_core::NodeId;

/// Snapshot of node heartbeat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heartbeat {
    /// Node that emitted the heartbeat.
    pub node_id: NodeId,
    /// Emission timestamp in Unix milliseconds.
    pub timestamp_ms: u64,
}

/// Contract for components that can produce a heartbeat.
pub trait HeartbeatSource {
    /// Produces the latest heartbeat snapshot.
    fn heartbeat(&self) -> Heartbeat;
}

/// Static heartbeat source used in local runtime composition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticHeartbeatSource {
    heartbeat: Heartbeat,
}

impl StaticHeartbeatSource {
    /// Creates a source that always returns the supplied heartbeat.
    pub fn new(node_id: NodeId, timestamp_ms: u64) -> Self {
        Self {
            heartbeat: Heartbeat {
                node_id,
                timestamp_ms,
            },
        }
    }
}

impl HeartbeatSource for StaticHeartbeatSource {
    fn heartbeat(&self) -> Heartbeat {
        self.heartbeat.clone()
    }
}

#[cfg(test)]
#[path = "heartbeat_tests.rs"]
mod tests;
