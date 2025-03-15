// =============================================================================
//        #######
//     ###       ###     F: role.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

/// Runtime infrastructure role fulfilled by a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProviderRole {
    /// Durable runtime or application storage.
    Storage,
    /// Distributed presence, discovery and leadership coordination.
    ControlPlane,
    /// Runtime coordination persistence owned by a control-plane deployment.
    CoordinationStore,
    /// Durable distributed job coordination.
    Job,
    /// Installation secret resolution.
    Secret,
    /// Generic peer discovery source.
    Discovery,
    /// Direct peer transport.
    PeerTransport,
    /// Command transport.
    CommandTransport,
    /// Application artifact and update source.
    Update,
    /// Application-owned database adapter.
    Database,
    /// Named installation adapter.
    Adapter,
}
