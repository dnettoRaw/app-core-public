// =============================================================================
//        #######
//     ###       ###     F: verbosity.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: unknown by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Raise detail for one component without changing global logging.
//!
//! This policy can be passed to a dispatcher when a single subsystem needs
//! deeper inspection without increasing the volume of every component.

use appcore_log::{LogPolicy, Verbosity};

fn main() {
    // V4 remains the default for application-wide operational messages.
    let mut policy = LogPolicy::new(Verbosity::V4);

    // Only synchronization is allowed to emit V5 through V9 details.
    policy.set_component("sync", Verbosity::V9);
}
