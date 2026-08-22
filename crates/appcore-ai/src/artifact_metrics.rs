// =============================================================================
//        #######
//     ###       ###     F: artifact_metrics.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

/// Low-cardinality peer artifact-transfer counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PeerArtifactMetrics {
    /// Successful verified peer fetches.
    pub fetches: u64,
    /// Verified bytes received from peers.
    pub transferred_bytes: u64,
    /// Rejected, cancelled, missing or corrupt peer fetches.
    pub failures: u64,
}
