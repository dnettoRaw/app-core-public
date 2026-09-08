// =============================================================================
//        #######
//     ###       ###     F: component_filter.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: unknown by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Raise one component to V8 while keeping the global default at V4.
//!
//! Component policy decides whether an event is visible. Severity still tells
//! operators how serious the visible event is.

use appcore_log::{ConsoleSink, LogDispatcher, LogPolicy, Verbosity};
use std::sync::Arc;

fn main() {
    // All components start at V4, keeping normal output concise.
    let mut policy = LogPolicy::new(Verbosity::V4);

    // Synchronization may need temporary I/O and timing diagnostics.
    policy.set_component("sync", Verbosity::V8);

    let log = LogDispatcher::new(policy, vec![Arc::new(ConsoleSink::new())]);

    let event = log.event(0, "sync");

    // This V7 event is selected only because the sync override allows it.
    event.verbosity(7).debug("batch details");
}
