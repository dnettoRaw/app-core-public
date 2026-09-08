// =============================================================================
//        #######
//     ###       ###     F: prelude.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/04 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/04 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Small, stable imports for a new `AppCore` application.
//!
//! Keep this module deliberately narrow. Commands, query endpoints and
//! capability-specific types remain explicit imports so application code does
//! not acquire an ambiguous global namespace as it grows.

pub use crate::{run, App, AppResult, Application};
