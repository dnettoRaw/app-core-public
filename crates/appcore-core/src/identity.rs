// =============================================================================
//        #######
//     ###       ###     F: identity.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Foundational Runtime identity exports.

pub use appcore_types::{
    CompatibilityStatus, CoreCompatibilityPolicy, CoreCompatibilityStatus, CoreIdentity, CoreKind,
    RuntimeIdentity,
};

#[cfg(test)]
#[path = "identity_tests.rs"]
mod tests;
