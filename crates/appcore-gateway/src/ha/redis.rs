// =============================================================================
//        #######
//     ###       ###     F: redis.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 00:00:00 by dnettoRaw
//      ###########      S: 1.0.6-rc
// =============================================================================

//! Feature-gated Redis implementation of the provider-independent HA contract.

#[cfg(feature = "ha-redis")]
#[path = "redis_config.rs"]
pub mod redis_config;
#[cfg(feature = "ha-redis")]
#[path = "redis_keys.rs"]
pub mod redis_keys;
#[cfg(feature = "ha-redis")]
#[path = "redis_operations.rs"]
pub mod redis_operations;
#[cfg(feature = "ha-redis")]
#[path = "redis_provider.rs"]
pub mod redis_provider;
#[cfg(feature = "ha-redis")]
#[path = "redis_provider_impl.rs"]
pub mod redis_provider_impl;
#[cfg(feature = "ha-redis")]
#[path = "redis_scripts.rs"]
pub mod redis_scripts;
#[cfg(feature = "ha-redis")]
#[path = "redis_validation.rs"]
pub mod redis_validation;

#[cfg(feature = "ha-redis")]
pub use redis_config::*;
#[cfg(feature = "ha-redis")]
pub use redis_provider::*;
