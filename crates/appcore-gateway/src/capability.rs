// =============================================================================
//        #######
//     ###       ###     F: capability.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/26 08:53:09 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/26 08:53:09 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Capability definitions inside the Gateway context.

use appcore_types::{
    CapabilityDescriptor, CapabilityMode, CapabilityName, CapabilityRequirements,
    CapabilityVisibility,
};
use serde::{Deserialize, Serialize};

/// Stable Runtime infrastructure capability implemented by the Gateway.
pub const GATEWAY_RUNTIME_CAPABILITY: &str = "runtime.gateway";

/// Builds the descriptor consumed by the Runtime capability policy when the
/// Gateway deployment adapter is selected.
pub fn gateway_capability_descriptor() -> crate::GatewayResult<CapabilityDescriptor> {
    Ok(CapabilityDescriptor::new(
        CapabilityName::new(GATEWAY_RUNTIME_CAPABILITY)?,
        env!("CARGO_PKG_VERSION"),
        CapabilityMode::Stream,
        CapabilityVisibility::Tenant,
    )
    .with_requirements(CapabilityRequirements {
        requires_leader: false,
        read_only: false,
        idempotency_required: false,
    }))
}

/// Simplified descriptor for a capability registered at the gateway.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayCapability {
    /// Unique capability name.
    pub name: CapabilityName,
    /// SemVer compatibility version string.
    pub version: String,
}

impl GatewayCapability {
    /// Creates a new gateway capability.
    pub fn new(name: CapabilityName, version: impl Into<String>) -> Self {
        Self {
            name,
            version: version.into(),
        }
    }
}
