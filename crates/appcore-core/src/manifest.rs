// =============================================================================
//        #######
//     ###       ###     F: manifest.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Runtime peer-manifest contracts.

pub use appcore_types::{
    CapabilityDescriptor, CapabilityMode, CapabilityRequirements, CapabilityVisibility,
    DistributedCoreManifest, PeerEndpoint,
};

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
