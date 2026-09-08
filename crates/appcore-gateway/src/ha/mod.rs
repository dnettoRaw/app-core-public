// =============================================================================
//        #######
//     ###       ###     F: mod.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.6-rc
// =============================================================================

//! Opt-in Gateway HA registry contracts.

pub mod coordinator;
pub mod coordinator_live;
pub mod coordinator_request;
pub mod coordinator_support;
pub mod lifecycle;
pub mod limits;
pub mod ownership;
pub mod provider;
pub mod redis;
pub mod types;

pub use coordinator::*;
pub use coordinator_request::*;
pub use lifecycle::*;
pub use limits::*;
pub use ownership::*;
pub use provider::*;
pub use types::*;
