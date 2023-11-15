// =============================================================================
//        #######
//     ###       ###     F: trace.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/20 23:03:47 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Distributed trace context export.

pub use appcore_types::TraceContext;

#[cfg(test)]
#[path = "trace_tests.rs"]
mod tests;
