// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/20 23:03:47 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Direct peer RPC contracts for private AppCore-to-AppCore calls.

#![deny(missing_docs)]

#[cfg(all(feature = "insecure-testing", not(debug_assertions)))]
compile_error!("insecure-testing cannot be enabled in release builds");

use appcore_core::{
    validate_identifier, ClusterId, CoreId, CoreIdentity, DistributedCoreManifest, ProtocolVersion,
    TenantId,
};
use appcore_security::{
    CommandTokenError, CommandTokenFactory, CommandTokenValidator, HashTokenProvider, TokenClaims,
    TokenProvider, LOCAL_ADMIN_SUBJECT,
};
pub use appcore_transport::CancellationToken;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Stable peer RPC wire contracts.
pub mod v1 {
    pub use appcore_distributed_contracts::peer_rpc::v1::*;
}

/// Explicit chunked peer RPC wire contracts.
pub mod v2 {
    pub use appcore_distributed_contracts::peer_rpc::v2::*;
}

pub use v1::{
    PeerAdvertisementV1, PeerCapabilityV1, PeerEndpointV1, PeerHealthResponse, PeerIdentityV1,
    PeerManifestResponse, PeerRpcCallKind, PeerRpcClientExecutor, PeerRpcEnvelope, PeerRpcError,
    PeerRpcOutboundRequest, PeerRpcRemoteErrorCodeV1, PeerRpcRemoteErrorV1, PeerRpcResponse,
    PEER_COMMAND_PATH, PEER_HEALTH_PATH, PEER_MANIFEST_PATH, PEER_QUERY_PATH,
};

const MAX_NONCE_CACHE_ENTRIES: usize = 65_536;
const MAX_HTTP_HEADER_BYTES: usize = 32_768;
const COMPRESSION_THRESHOLD_BYTES: usize = 1_024;
const MAX_ENVELOPE_OVERHEAD_BYTES: usize = 65_536;

mod advertisement;
mod authentication;
mod client;
mod host;
mod nonce;
mod replay;
mod stream;
mod stream_client;
mod stream_host;
mod stream_registry;
mod stream_registry_protocol;
mod stream_registry_types;
mod stream_session;
mod stream_signing;
mod stream_spool;
mod stream_spool_security;
mod transport;
mod validation;

#[cfg(any(test, feature = "insecure-testing"))]
pub use authentication::{AllowPeerAuthenticator, StaticPeerRpcTokenIssuer};
pub use authentication::{
    HashTokenPeerAuthenticator, HashTokenPeerTokenIssuer, PeerRpcAuthenticator, PeerRpcDispatcher,
    PeerRpcTokenIssuer,
};
pub use client::{
    PeerRpcClient, PeerRpcClientConfig, PeerRpcHttpRequest, PeerRpcHttpResponse,
    PeerRpcRetryPolicy, PeerTransportProvider,
};
pub use host::{PeerRpcHttpHost, PeerRpcHttpState};
pub use nonce::{FilePeerNonceStore, InMemoryPeerNonceStore, PeerNonceStore};
pub use replay::{BoundedReplayStore, ReplayStore, ReplayStoreConfig, ReplayStoreMetrics};
pub use stream::{PeerRpcChunkAssembler, PeerRpcChunkEncoder, PeerRpcChunkLimits};
pub use stream_client::{PeerRpcStreamClientErrorV2, PeerRpcStreamRequestV2};
pub use stream_registry::PeerRpcStreamRegistry;
pub use stream_registry_types::{
    PeerRpcStreamDispatcherV2, PeerRpcStreamRegistryConfig, PeerRpcStreamRegistrySnapshot,
    PeerRpcStreamResponseSourceV2,
};
pub use stream_signing::{stream_frame_signing_hash, stream_frame_signing_hash_with_codec};
pub use stream_spool::PeerRpcStreamPayload;
pub use transport::{PooledPeerRpcTransport, StdPeerRpcTransport};
pub use validation::{
    envelope_signing_hash, payload_hash, route_for_command, route_for_query,
    PeerRpcValidationConfig, PeerRpcValidator,
};

#[cfg(test)]
mod stream_client_tests;
#[cfg(test)]
mod stream_registry_tests;
#[cfg(test)]
mod stream_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod transport_tests;
