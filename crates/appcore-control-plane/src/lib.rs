// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/20 23:03:47 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Generic control-plane contracts for distributed AppCore deployments.
//!
//! The control plane stores presence, leases, and routing metadata. It must not
//! transport business payloads or contain tenant-specific business rules.

#![deny(missing_docs)]

use appcore_contracts::ServiceId;
use appcore_core::{
    ClusterId, CoreId, CoreIdentity, RuntimeOperationalMode, TenantId, TraceContext,
};
use appcore_distributed_contracts::control_plane::v1::{
    EmptyResponse, ServiceLeaseRequest, CONTROL_HEARTBEAT_PATH, CONTROL_PEERS_PATH,
    CONTROL_REGISTER_PATH, CONTROL_SERVICE_LEASE_PATH, CONTROL_SERVICE_LEASE_RELEASE_PATH,
};
pub use appcore_transport::CancellationToken;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{mpsc, Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::thread;
use std::time::Duration;

/// Stable control-plane wire and provider contracts.
pub mod v1 {
    pub use appcore_distributed_contracts::control_plane::v1::*;
}

pub use v1::{
    ControlPlaneError, ControlPlaneFuture, ControlPlaneProvider, ControlPlaneResult, CorePresence,
    CoreRegistration, DiscoveryProvider, HeartbeatRequest, HeartbeatResponse, LeadershipDecision,
    PeerDirectory, PeerRecord, ServiceLeaderLease, ServiceLeadershipGuard,
};

const DEFAULT_MAX_HTTP_RESPONSE_BYTES: usize = 1_048_576;
const MAX_HTTP_HEADER_BYTES: usize = 32_768;
const MAX_CONTROL_PLANE_WORK_ITEMS: usize = 64;

mod client;
mod coordinator;
mod file;
mod leadership;
mod memory;
mod offline;
mod transport;
mod worker;

pub use client::{
    ControlPlaneHttpConfig, HttpControlPlaneClient, HttpControlPlaneRequest,
    HttpControlPlaneResponse, HttpTransport, RetryPolicy,
};
pub use coordinator::{ControlPlaneCoordinator, HeartbeatPolicy};
pub use file::FileControlPlane;
pub use leadership::StaticServiceLeadershipGuard;
pub use memory::InMemoryControlPlane;
pub use offline::OfflineControlPlaneClient;
pub use transport::{
    require_secure_remote_endpoint, BearerHttpTransport, SecretString, StdHttpTransport,
};

#[cfg(test)]
mod tests;
