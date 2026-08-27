// =============================================================================
//        #######
//     ###       ###     F: mod.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.4-rc
// =============================================================================

//! Opt-in Gateway HA registry contracts.

pub mod coordinator;
pub mod coordinator_live;
pub mod coordinator_request;
pub mod coordinator_support;
pub mod lifecycle;
pub mod ownership;
pub mod provider;
pub mod redis_config;
pub mod redis_keys;
pub mod redis_operations;
pub mod redis_provider;
pub mod redis_provider_impl;
pub mod redis_scripts;
pub mod redis_validation;
pub mod types;

pub use coordinator::*;
pub use coordinator_request::*;
pub use lifecycle::*;
pub use ownership::*;
pub use provider::*;
pub use redis_config::*;
pub use redis_provider::*;
pub use types::*;
