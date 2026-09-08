// =============================================================================
//        #######
//     ###       ###     F: limits.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 00:00:00 by dnettoRaw
//      ###########      S: 1.0.6-rc
// =============================================================================

//! Provider-independent bounds for Gateway HA ownership and discovery.

/// Maximum simultaneous shared-registry operations.
pub const MAX_GATEWAY_REGISTRY_CONCURRENCY: usize = 64;
/// Maximum accepted instance or worker ownership TTL.
pub const MAX_GATEWAY_INSTANCE_LEASE_TTL_MS: u64 = 60_000;
/// Maximum number of workers returned by one shared resolution.
pub const MAX_GATEWAY_RESOLVE_CANDIDATES: usize = 1_024;
