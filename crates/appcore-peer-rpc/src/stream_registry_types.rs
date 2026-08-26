// =============================================================================
//        #######
//     ###       ###     F: stream_registry_types.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Public configuration and dispatch contracts for bounded V2 stream sessions.

use super::*;
use crate::v2::{PeerRpcStreamErrorV2, PeerRpcStreamOpenV2};
use std::fmt::{Debug, Formatter};
use std::io::Read;
use std::path::PathBuf;

/// Bounded partial-state and spool configuration for V2 streams.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerRpcStreamRegistryConfig {
    /// Maximum request, dispatch, and response sessions combined.
    pub max_sessions: usize,
    /// Maximum decoded bytes reserved across all sessions.
    pub max_reserved_payload_bytes: u64,
    /// Existing owner-only directory for automatically removed spool files.
    ///
    /// Unix requires the effective owner and mode `0700`; Windows requires the
    /// current process owner SID and no allow ACE for another principal.
    pub spool_directory: PathBuf,
    /// Per-stream and per-chunk bounds.
    pub chunk_limits: PeerRpcChunkLimits,
}

/// Read-only bounded registry observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerRpcStreamRegistrySnapshot {
    /// Current request, dispatch, and response session count.
    pub active_sessions: usize,
    /// Decoded bytes reserved by active sessions.
    pub reserved_payload_bytes: u64,
    /// Saturating number of rejected admissions.
    pub saturation_count: u64,
    /// Saturating number of sessions released by completion, error, cancel, or expiry.
    pub cleanup_count: u64,
}

/// Bounded response reader returned by a V2 stream dispatcher.
pub struct PeerRpcStreamResponseSourceV2 {
    pub(crate) payload_bytes: u64,
    pub(crate) reader: Box<dyn Read + Send>,
}

impl PeerRpcStreamResponseSourceV2 {
    /// Creates a response source with its exact decoded size.
    pub fn new(payload_bytes: u64, reader: Box<dyn Read + Send>) -> Self {
        Self {
            payload_bytes,
            reader,
        }
    }
}

impl Debug for PeerRpcStreamResponseSourceV2 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PeerRpcStreamResponseSourceV2")
            .field("payload_bytes", &self.payload_bytes)
            .finish()
    }
}

/// Executes one complete, verified V2 request from a file-backed bounded reader.
pub trait PeerRpcStreamDispatcherV2: Send + Sync {
    /// Dispatches request metadata and a rewound payload, producing a bounded response source.
    fn dispatch_peer_stream(
        &self,
        open: PeerRpcStreamOpenV2,
        payload: PeerRpcStreamPayload,
        cancellation: CancellationToken,
    ) -> Result<PeerRpcStreamResponseSourceV2, PeerRpcStreamErrorV2>;
}
