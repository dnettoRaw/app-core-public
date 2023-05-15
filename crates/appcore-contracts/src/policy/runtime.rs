// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/22 15:41:18 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

/// Determines whether a runtime is isolated or participates in coordination.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    /// Runs without remote coordination, discovery, election or queues.
    #[default]
    Standalone,
    /// Registers with a control plane and can coordinate with peers.
    Cluster,
}

/// How a capability is invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityMode {
    /// Side-effect-free request.
    Query,
    /// State-changing or important action.
    Command,
    /// Long-lived or event-oriented flow.
    Stream,
}
